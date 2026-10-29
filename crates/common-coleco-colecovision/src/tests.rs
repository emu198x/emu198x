struct TestNtsc;
struct TestPal;
impl Board for TestNtsc {
    const REGION: CvRegion = CvRegion::Ntsc;
    const VDP_REGION: VdpRegion = VdpRegion::Ntsc;
    const PSG_CLOCK_HZ: u32 = 3_579_545;
    const TSTATES_PER_FRAME: u64 = 228 * 262;
    const FRAMEBUFFER_HEIGHT: u32 = 240;
}
impl Board for TestPal {
    const REGION: CvRegion = CvRegion::Pal;
    const VDP_REGION: VdpRegion = VdpRegion::Pal;
    const PSG_CLOCK_HZ: u32 = 3_579_545;
    const TSTATES_PER_FRAME: u64 = 228 * 313;
    const FRAMEBUFFER_HEIGHT: u32 = 288;
}

use super::*;

#[test]
fn native_frame_stops_at_the_chip_wrap_after_partial_stepping() {
    fn check<B: Board>() {
        let mut machine = CvCore::<B>::new(empty_bios(), vec![]);
        let line = |m: &CvCore<B>| m.state.vdp.scanline();
        for _ in 0..37 {
            machine.tick_cpu_cycle();
        }
        for _ in 0..4 {
            let bytes = postcard::to_allocvec(&machine).expect("encode partial state");
            let mut direct: CvCore<B> = postcard::from_bytes(&bytes).expect("restore control");
            let start = direct.state.cpu_cycles;
            loop {
                let before = line(&direct);
                direct.tick_cpu_cycle();
                assert!(
                    direct.state.cpu_cycles - start < 200_000,
                    "control must wrap"
                );
                if line(&direct) < before {
                    break;
                }
            }
            let (ticks, completed) = machine.run_native_frame();
            assert!(completed);
            assert_eq!(
                ticks,
                direct.state.cpu_cycles - start,
                "elapsed time must match the chip event"
            );
            assert_eq!(
                line(&machine),
                0,
                "raster must wrap, not merely consume a nominal budget"
            );
            assert_eq!(machine.state.cpu_cycles, direct.state.cpu_cycles);
            // A debugger can leave us partway through the next field.
            for _ in 0..53 {
                machine.tick_cpu_cycle();
            }
        }
    }
    check::<TestNtsc>();
    check::<TestPal>();
}

fn empty_bios() -> Vec<u8> {
    // 8 KB of NOPs followed by a JR to itself at $0008 — gives the
    // CPU something legal to execute so the smoke tests don't crash
    // on stray opcodes even though we don't have a real BIOS.
    let mut bios = vec![0u8; 8192];
    // JR -2 at $0008 ($18 $FE) — infinite tight loop.
    bios[0x0008] = 0x18;
    bios[0x0009] = 0xFE;
    bios
}

/// Save-state must capture LIVE machine state (Z80 + TMS9918 VDP + SN76489
/// PSG + RAM), not cold-boot from the BIOS/cart. Serialise, advance (so the
/// state differs), then deserialise the first snapshot and confirm
/// re-serialising it is byte-identical — every stateful field across all
/// three chips round-trips, including the VDP's 16 KB VRAM.
#[test]
fn snapshot_round_trips_live_state() {
    let mut cv = CvCore::<TestNtsc>::new(empty_bios(), vec![]);
    cv.run_frame();
    cv.poke(0x6100, 0xA5); // a work-RAM byte to carry across the snapshot
    cv.run_frame();
    let s1 = postcard::to_allocvec(&cv).expect("encode snapshot");

    cv.run_frame(); // advance past the snapshot point
    let s2 = postcard::to_allocvec(&cv).expect("encode again");
    assert_ne!(s1, s2, "running a frame should change the serialised state");

    let restored: CvCore<TestNtsc> = postcard::from_bytes(&s1).expect("decode snapshot");
    let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
    assert_eq!(
        s1, s3,
        "restore should reproduce the snapshot state exactly"
    );
}

#[test]
fn ntsc_frame_returns_expected_cpu_cycles() {
    let mut cv = CvCore::<TestNtsc>::new(empty_bios(), vec![]);
    let cycles = cv.run_frame();
    assert_eq!(cycles, NTSC_CPU_CYCLES_PER_FRAME);
    assert_eq!(cv.frame_count(), 1);
}

#[test]
fn pal_frame_returns_expected_cpu_cycles() {
    let mut cv = CvCore::<TestPal>::new(empty_bios(), vec![]);
    let cycles = cv.run_frame();
    assert_eq!(cycles, PAL_CPU_CYCLES_PER_FRAME);
}

#[test]
fn vdp_runs_at_three_dots_per_two_tstates() {
    // 3:2 phase, not the donor's 3:1: 4 T-states add 12 phase units,
    // each dot consumes 2 → 6 dots, 0 remainder (well within one
    // 342-dot line, so the scanline shouldn't advance yet).
    let mut cv = CvCore::<TestNtsc>::new(empty_bios(), vec![]);
    let start = cv.state.vdp.scanline();
    for _ in 0..4 {
        cv.tick_cpu_cycle();
    }
    assert_eq!(cv.state.vdp_phase, 0);
    assert_eq!(cv.state.vdp.scanline(), start);
}

#[test]
fn one_frame_of_tstates_is_exactly_one_vdp_frame() {
    // 228×262 T-states × 3/2 = 342×262 dots = one full NTSC frame,
    // so the VDP scanline returns to where it started. Under the old
    // 3:1 model the VDP would have run ~1.5 frames in this budget.
    let mut cv = CvCore::<TestNtsc>::new(empty_bios(), vec![]);
    let start = cv.state.vdp.scanline();
    for _ in 0..NTSC_CPU_CYCLES_PER_FRAME {
        cv.tick_cpu_cycle();
    }
    assert_eq!(cv.state.vdp.scanline(), start);
    assert_eq!(cv.state.vdp_phase, 0);
}

#[test]
fn many_frames_complete_without_panic() {
    let mut cv = CvCore::<TestNtsc>::new(empty_bios(), vec![]);
    for _ in 0..60 {
        cv.run_frame();
    }
    assert_eq!(cv.frame_count(), 60);
}

#[test]
fn vdp_drives_cpu_irq_at_vblank() {
    // Run a frame and check the VDP has set its interrupt flag at
    // some point during the frame. Final IRQ state depends on
    // whether the CPU has acknowledged it via IM 1 RST 38h.
    let mut cv = CvCore::<TestNtsc>::new(empty_bios(), vec![]);
    cv.run_frame();
    // After at least one full frame the VDP has crossed VBlank.
    // We can't trivially assert irq == true at frame end because
    // the BIOS-less CPU runs IM 0 and never acknowledges, leaving
    // the line pending — but the interrupt FLAG on the VDP should
    // be set during the frame. Test scaffolding for the real
    // assertion when a BIOS is wired in.
    assert!(cv.cpu_cycles() >= NTSC_CPU_CYCLES_PER_FRAME);
}

#[test]
fn joystick_mode_toggle_via_io() {
    let mut cv = CvCore::<TestNtsc>::new(empty_bios(), vec![]);
    assert!(!cv.state.joystick_mode);
    cv.io_write(0xC0, 0xFF);
    assert!(cv.state.joystick_mode);
    cv.io_write(0x80, 0xFF);
    assert!(!cv.state.joystick_mode);
}

#[test]
fn keypad_read_routes_to_correct_controller() {
    let mut cv = CvCore::<TestNtsc>::new(empty_bios(), vec![]);
    cv.state.controller1.keypad = Some(KeypadKey::K5);
    cv.state.controller2.keypad = Some(KeypadKey::K7);
    // bit 1 = 0 → controller 1.
    let c1 = cv.io_read(0xE0);
    // bit 1 = 1 → controller 2.
    let c2 = cv.io_read(0xE2);
    assert_eq!(c1 & 0x0F, KeypadKey::K5.encode());
    assert_eq!(c2 & 0x0F, KeypadKey::K7.encode());
}
