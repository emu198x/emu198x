use super::*;

struct TestNtsc;
struct TestPal;
impl Board for TestNtsc {
    const REGION: M5Region = M5Region::Ntsc;
    const VDP_REGION: VdpRegion = VdpRegion::Ntsc;
    const PSG_CLOCK_HZ: u32 = 3_579_545;
    const TSTATES_PER_FRAME: u64 = 228 * 262;
    const FRAMEBUFFER_HEIGHT: u32 = 240;
}
impl Board for TestPal {
    const REGION: M5Region = M5Region::Pal;
    const VDP_REGION: VdpRegion = VdpRegion::Pal;
    const PSG_CLOCK_HZ: u32 = 3_579_545;
    const TSTATES_PER_FRAME: u64 = 228 * 313;
    const FRAMEBUFFER_HEIGHT: u32 = 288;
}
const NTSC_TSTATES_PER_FRAME: u64 = 228 * 262;
const PAL_TSTATES_PER_FRAME: u64 = 228 * 313;

#[test]
fn native_frame_stops_at_the_chip_wrap_after_partial_stepping() {
    fn check<B: Board>() {
        let mut machine = M5Core::<B>::new(trap_rom(), vec![]);
        let line = |m: &M5Core<B>| m.state.vdp.scanline();
        for _ in 0..37 {
            machine.tick_tstate();
        }
        for _ in 0..4 {
            let bytes = postcard::to_allocvec(&machine).expect("encode partial state");
            let mut direct: M5Core<B> = postcard::from_bytes(&bytes).expect("restore control");
            let start = direct.state.cpu_tstates;
            loop {
                let before = line(&direct);
                direct.tick_tstate();
                assert!(
                    direct.state.cpu_tstates - start < 200_000,
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
                direct.state.cpu_tstates - start,
                "elapsed time must match the chip event"
            );
            assert_eq!(
                line(&machine),
                0,
                "raster must wrap, not merely consume a nominal budget"
            );
            assert_eq!(machine.state.cpu_tstates, direct.state.cpu_tstates);
            // A debugger can leave us partway through the next field.
            for _ in 0..53 {
                machine.tick_tstate();
            }
        }
    }
    check::<TestNtsc>();
    check::<TestPal>();
}

fn trap_rom() -> Vec<u8> {
    // 8 KB Monitor-ROM stub: NOPs with a JR -2 trap at $0008.
    let mut rom = vec![0u8; 8192];
    rom[0x0008] = 0x18;
    rom[0x0009] = 0xFE;
    rom
}

/// Save-state must capture LIVE machine state (Z80 + TMS9918 VDP + SN76489
/// PSG + Z80 CTC + RAM), not cold-boot from the ROM. Serialise, advance (so
/// the state differs), then deserialise the first snapshot and confirm
/// re-serialising it is byte-identical — every stateful field across all
/// chips round-trips, including the VDP's 16 KB VRAM and the CTC channels.
#[test]
fn snapshot_round_trips_live_state() {
    let mut sys = M5Core::<TestNtsc>::new(trap_rom(), vec![]);
    sys.run_frame();
    sys.poke(0x7100, 0xA5); // a system-RAM byte ($7000-$7FFF) to carry across
    sys.run_frame();
    let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

    sys.run_frame(); // advance past the snapshot point
    let s2 = postcard::to_allocvec(&sys).expect("encode again");
    assert_ne!(s1, s2, "running a frame should change the serialised state");

    let restored: M5Core<TestNtsc> = postcard::from_bytes(&s1).expect("decode snapshot");
    let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
    assert_eq!(
        s1, s3,
        "restore should reproduce the snapshot state exactly"
    );
}

#[test]
fn ntsc_frame_returns_expected_tstates() {
    let mut sys = M5Core::<TestNtsc>::new(trap_rom(), vec![]);
    let t = sys.run_frame();
    assert_eq!(t, NTSC_TSTATES_PER_FRAME);
    assert_eq!(sys.frame_count(), 1);
}

#[test]
fn pal_frame_returns_expected_tstates() {
    let mut sys = M5Core::<TestPal>::new(trap_rom(), vec![]);
    let t = sys.run_frame();
    assert_eq!(t, PAL_TSTATES_PER_FRAME);
}

#[test]
fn many_frames_complete_without_panic() {
    let mut sys = M5Core::<TestNtsc>::new(trap_rom(), vec![]);
    for _ in 0..60 {
        sys.run_frame();
    }
    assert_eq!(sys.frame_count(), 60);
}

#[test]
fn memory_map_routes_pages() {
    let mut sys = M5Core::<TestNtsc>::new(trap_rom(), vec![0xCAu8; 0x4000]);
    // BIOS trap byte.
    assert_eq!(sys.mem_read(0x0008), 0x18);
    // Cart ROM mirror.
    assert_eq!(sys.mem_read(0x2000), 0xCA);
    // RAM round-trip.
    sys.mem_write(0x7100, 0x42);
    assert_eq!(sys.mem_read(0x7100), 0x42);
    // Cart RAM round-trip after allocating.
    sys.set_cart_ram_size(0x4000);
    sys.mem_write(0x8000, 0x77);
    assert_eq!(sys.mem_read(0x8000), 0x77);
    // Unmapped region.
    assert_eq!(sys.mem_read(0xC000), 0xFF);
}

#[test]
fn keyboard_rows_read_directly_at_0x30_block() {
    let mut sys = M5Core::<TestNtsc>::new(trap_rom(), vec![]);
    // Each row Y0-Y6 is read directly at its own port $30+row — no strobe.
    sys.state.key_matrix[3] = 0x77;
    assert_eq!(sys.io_read(0x33), 0x77, "row 3 reads at $33");
    sys.state.key_matrix[6] = 0xAA;
    assert_eq!(sys.io_read(0x36), 0xAA, "row 6 reads at $36");
    // A3 mirror: $38-$3E fold onto $30-$36.
    assert_eq!(sys.io_read(0x3B), 0x77, "$3B mirrors $33");
    // $37 in the same block is the joystick, not a keyboard row.
    sys.set_joystick(1, true, false, false, false);
    assert_eq!(sys.io_read(0x37) & 0x02, 0x02, "$37 is JOY (P1 up)");
    // $40 read is unmapped open bus (Centronics is write-only there).
    assert_eq!(sys.io_read(0x40), 0xFF);
}

#[test]
fn joystick_reads_active_high_at_0x37() {
    let mut sys = M5Core::<TestNtsc>::new(trap_rom(), vec![]);
    // Idle: nothing pressed → all lines low (active high).
    assert_eq!(sys.io_read(0x37), 0x00);

    // Player 1 right + up → bits 0 and 1 high. (up, down, left, right)
    sys.set_joystick(1, true, false, false, true);
    let v = sys.io_read(0x37);
    assert_eq!(v & 0x01, 0x01, "P1 right → bit 0 high");
    assert_eq!(v & 0x02, 0x02, "P1 up → bit 1 high");
    assert_eq!(v & 0xF0, 0x00, "P2 nibble idle low");

    // Player 2 down → bit 7; independent of P1.
    sys.set_joystick(2, false, true, false, false);
    let v = sys.io_read(0x37);
    assert_eq!(v & 0x80, 0x80, "P2 down → bit 7 high");
    assert_eq!(v & 0x03, 0x03, "P1 right+up still held");

    // Mirror: $3F (A3 set) reads the same byte.
    assert_eq!(sys.io_read(0x3F), sys.io_read(0x37));
}

#[test]
fn vdp_io_routes_at_0x10_block() {
    let mut sys = M5Core::<TestNtsc>::new(trap_rom(), vec![]);
    sys.io_write(0x11, 0x00); // VDP control low byte
    sys.io_write(0x11, 0x40); // VDP control high (set VRAM write addr $0000)
    sys.io_write(0x10, 0x42); // VDP data write
    // Reading status ($11) doesn't panic.
    let _ = sys.io_read(0x11);
}

#[test]
fn ctc_routes_at_0x00_block() {
    let mut sys = M5Core::<TestNtsc>::new(trap_rom(), vec![]);
    // Channel 0: vector base, then a ÷16 timer with time constant 5.
    sys.io_write(0x00, 0x40); // vector base $40 (D0=0 → vector)
    sys.io_write(0x00, 0x05); // control: TC follows + control word
    sys.io_write(0x00, 5); // time constant
    assert!(sys.ctc().running(0));
    assert_eq!(sys.io_read(0x00), 5, "read returns the live counter");
    assert_eq!(sys.ctc().vector_base(), 0x40);
}

#[test]
fn vdp_dot_ratio_is_three_per_two_tstates() {
    let mut sys = M5Core::<TestNtsc>::new(trap_rom(), vec![]);
    let start = sys.state.vdp.scanline();
    for _ in 0..4 {
        sys.tick_tstate();
    }
    assert_eq!(sys.state.vdp.scanline(), start);
    assert_eq!(sys.state.vdp_phase, 0);
}

#[test]
fn key_press_and_release_round_trip() {
    let mut sys = M5Core::<TestNtsc>::new(trap_rom(), vec![]);
    // Active-high: pressing sets the cell, releasing clears it. Reading the
    // row's port shows the BIOS-visible bit.
    sys.press_key(2, 5);
    assert_eq!(sys.state.key_matrix[2] & 0b0010_0000, 0b0010_0000);
    assert_eq!(sys.io_read(0x32) & 0b0010_0000, 0b0010_0000);
    sys.release_key(2, 5);
    assert_eq!(sys.state.key_matrix[2] & 0b0010_0000, 0);
}
