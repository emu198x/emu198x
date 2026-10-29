struct TestNtsc;
struct TestPal;
impl Board for TestNtsc {
    const REGION: SviRegion = SviRegion::Ntsc;
    const VDP_REGION: VdpRegion = VdpRegion::Ntsc;
    const TSTATES_PER_FRAME: u64 = 228 * 262;
    const FRAMEBUFFER_HEIGHT: u32 = 240;
}
impl Board for TestPal {
    const REGION: SviRegion = SviRegion::Pal;
    const VDP_REGION: VdpRegion = VdpRegion::Pal;
    const TSTATES_PER_FRAME: u64 = 228 * 313;
    const FRAMEBUFFER_HEIGHT: u32 = 288;
}

use super::*;

#[test]
fn native_frame_stops_at_the_chip_wrap_after_partial_stepping() {
    fn check<B: Board>() {
        let mut machine = booted::<B>(trap_rom());
        let line = |m: &SviCore<B>| m.state.vdp.scanline();
        for _ in 0..37 {
            machine.tick_tstate();
        }
        for _ in 0..4 {
            let bytes = postcard::to_allocvec(&machine).expect("encode partial state");
            let mut direct: SviCore<B> = postcard::from_bytes(&bytes).expect("restore control");
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

/// An SVI whose 8255 has the BIOS's "initial set" mode, `$92`: port A
/// and port B in, port C out (service manual, § 8255 Control Word).
/// Test ROMs never run that code.
fn booted<B: Board>(rom: Vec<u8>) -> SviCore<B> {
    let mut sys = SviCore::<B>::new(rom);
    sys.io_write(0x97, 0x92);
    sys
}

fn trap_rom() -> Vec<u8> {
    // 32 KB ROM: NOPs with JR -2 trap at $0008.
    let mut rom = vec![0u8; 0x8000];
    rom[0x0008] = 0x18;
    rom[0x0009] = 0xFE;
    rom
}

/// Save-state must capture LIVE machine state (Z80 + TMS9918 VDP +
/// AY-3-8910 PSG + 8255 PPI + RAM + banking), not cold-boot from the ROM.
/// Serialise, advance (so the state differs), then deserialise the first
/// snapshot and confirm re-serialising it is byte-identical — every
/// stateful field round-trips, including the VDP's 16 KB VRAM.
#[test]
fn snapshot_round_trips_live_state() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    sys.run_frame();
    sys.poke(0xC100, 0xA5); // a work-RAM byte to carry across the snapshot
    assert_eq!(sys.peek(0xC100), 0xA5, "0xC100 is RAM and accepts the poke");
    sys.run_frame();
    let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

    sys.run_frame(); // advance past the snapshot point
    let s2 = postcard::to_allocvec(&sys).expect("encode again");
    assert_ne!(s1, s2, "running a frame should change the serialised state");

    let restored: SviCore<TestNtsc> = postcard::from_bytes(&s1).expect("decode snapshot");
    let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
    assert_eq!(
        s1, s3,
        "restore should reproduce the snapshot state exactly"
    );
}

#[test]
fn ntsc_frame_returns_expected_tstates() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    let t = sys.run_frame();
    assert_eq!(t, NTSC_TSTATES_PER_FRAME);
    assert_eq!(sys.frame_count(), 1);
}

#[test]
fn pal_frame_returns_expected_tstates() {
    let mut sys = booted::<TestPal>(trap_rom());
    let t = sys.run_frame();
    assert_eq!(t, PAL_TSTATES_PER_FRAME);
}

#[test]
fn many_frames_complete_without_panic() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    for _ in 0..60 {
        sys.run_frame();
    }
    assert_eq!(sys.frame_count(), 60);
}

#[test]
fn joystick_directions_read_through_psg_port_a() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    sys.io_write(0x88, 14); // select PSG register 14 (port A)

    // Idle: both sticks read all-high.
    assert_eq!(sys.io_read(0x90), 0xFF);

    // Player 1 up+left → low-nibble bits 0 and 2 low; P2 nibble idle high.
    sys.set_joystick(1, true, false, true, false, false);
    let v = sys.io_read(0x90);
    assert_eq!(v & 0x01, 0, "P1 up → bit 0 low");
    assert_eq!(v & 0x04, 0, "P1 left → bit 2 low");
    assert_eq!(v & 0x0A, 0x0A, "P1 down/right idle high");
    assert_eq!(v & 0xF0, 0xF0, "P2 nibble untouched");

    // Player 2 down → high-nibble bit 5 low, independent of P1.
    sys.set_joystick(2, false, true, false, false, false);
    let v = sys.io_read(0x90);
    assert_eq!(v & 0x20, 0, "P2 down → bit 5 low");
    assert_eq!(v & 0x01, 0, "P1 up still held");
}

#[test]
fn joystick_fire_buttons_read_on_ppi_port_a() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    // Idle PPI port A: buttons high, no-cassette bit set → 0x7F.
    assert_eq!(sys.io_read(0x98), 0x7F);

    sys.set_joystick(1, false, false, false, false, true); // P1 fire
    assert_eq!(sys.io_read(0x98) & 0x10, 0, "P1 fire → PPI PA4 low");
    assert_eq!(sys.io_read(0x98) & 0x20, 0x20, "P2 fire idle high");

    sys.set_joystick(2, false, false, false, false, true); // P2 fire
    assert_eq!(sys.io_read(0x98) & 0x30, 0, "both fire → PA4+PA5 low");
}

#[test]
fn ppi_cassette_lines_report_media_and_follow_motor_control() {
    let mut sys = booted::<TestNtsc>(vec![0; 0x8000]);
    assert_eq!(sys.io_read(0x98) & 0xC0, 0x40, "empty deck: absent and low");
    sys.insert_cassette(&[vec![0x80]]);
    assert_eq!(sys.io_read(0x98) & 0xC0, 0x00, "inserted tape starts low");

    // PC4 low starts the deck. Run through MAME's 200-sample initial
    // silence and observe the first high half-wave of the leader on PA7.
    sys.io_write(0x96, 0x00);
    for _ in 0..17_000 {
        sys.tick_tstate();
    }
    assert_eq!(sys.io_read(0x98) & 0xC0, 0x80, "leader reaches PA7");

    sys.io_write(0x96, 0x10);
    let level = sys.io_read(0x98) & 0x80;
    for _ in 0..10_000 {
        sys.tick_tstate();
    }
    assert_eq!(sys.io_read(0x98) & 0x80, level, "PC4 high stops tape");
}

#[test]
fn rom_visible_at_reset() {
    let sys = booted::<TestNtsc>(trap_rom());
    // BIOS-trap byte at $0008.
    assert_eq!(sys.mem_read(0x0008), 0x18);
    // Read past ROM end → 0xFF from the unmapped fall-through.
    assert_eq!(sys.mem_read(0xFFFE), 0x00); // RAM defaults zero.
}

#[test]
fn ram_overlay_replaces_rom_at_low_window() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    // Bank the lower RAM in via PSG R15 (port B), bit 1 low.
    sys.io_write(0x88, 0x0F); // select R15
    sys.io_write(0x8C, 0x01); // CART=1, BK21=0 → RAM low
    assert_eq!(sys.memory_control(), (true, false));
    sys.mem_write(0x0100, 0x42);
    assert_eq!(sys.mem_read(0x0100), 0x42);
}

#[test]
fn psg_port_b_cart_bit_controls_the_cart_overlay() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    sys.insert_cart(vec![0xAA; 0x4000]);
    assert_eq!(sys.memory_control(), (false, false));
    assert_eq!(sys.mem_read(0x8000), 0x00, "cart starts disabled");

    sys.io_write(0x88, 0x0F); // select PSG R15 (port B)
    sys.io_write(0x8C, 0x02); // CART=0, BK21=1
    assert_eq!(sys.memory_control(), (false, true));
    assert_eq!(sys.mem_read(0x8000), 0xAA);
    assert_eq!(sys.mem_read(0xBFFF), 0xAA);

    sys.io_write(0x8C, 0x03); // CART=1, BK21=1
    assert_eq!(sys.memory_control(), (false, false));
    assert_eq!(sys.mem_read(0x8000), 0x00, "RAM returns when CART is high");
}

#[test]
fn ppi_control_port_does_not_change_memory_banks() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    sys.insert_cart(vec![0xAA; 0x4000]);
    sys.io_write(0x97, 0x00);
    assert_eq!(
        sys.memory_control(),
        (false, false),
        "$97 is the 8255 control register, not memory control"
    );
}

#[test]
fn psg_port_b_drives_lower_ram_bank() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    sys.io_write(0x88, 0x0F); // select R15
    sys.io_write(0x8C, 0x01); // CART=1, BK21=0 → RAM low
    assert!(sys.memory_control().0, "bk21 low should bank RAM in");
    sys.io_write(0x88, 0x0F);
    sys.io_write(0x8C, 0x03); // CART=1, BK21=1 → ROM low
    assert!(!sys.memory_control().0, "bk21 high should restore the ROM");
}

#[test]
fn keyboard_io_returns_selected_row() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    sys.state.keyboard[5] = 0xCD;
    // Select row 5 via PPI port C ($96), read the column data at port B ($99).
    sys.io_write(0x96, 0x05);
    assert_eq!(sys.io_read(0x99), 0xCD);
}

#[test]
fn vdp_dot_ratio_is_three_per_two_tstates() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    let start = sys.state.vdp.scanline();
    for _ in 0..4 {
        sys.tick_tstate();
    }
    assert_eq!(sys.state.vdp.scanline(), start);
    assert_eq!(sys.state.vdp_phase, 0);
}

#[test]
fn key_press_and_release_round_trip() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    sys.press_key(3, 2);
    assert_eq!(sys.state.keyboard[3] & 0b0000_0100, 0);
    sys.release_key(3, 2);
    assert_eq!(sys.state.keyboard[3] & 0b0000_0100, 0b0000_0100);
}

#[test]
fn ay_watch_captures_psg_data_writes() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    assert!(sys.ay_write_watch_records().is_none());
    let cap = sys.start_ay_write_watch();
    assert!(cap > 0);
    sys.io_write(0x88, 7); // select R7
    sys.io_write(0x8C, 0x38); // data
    sys.io_write(0x88, 8); // select R8
    sys.io_write(0x8C, 0x0F); // data
    let records = sys.ay_write_watch_records().expect("armed");
    assert_eq!(records.len(), 2);
    assert_eq!((records[0].register, records[0].value), (7, 0x38));
    assert_eq!((records[1].register, records[1].value), (8, 0x0F));
    sys.clear_ay_write_watch_records();
    assert_eq!(sys.ay_write_watch_records().expect("armed").len(), 0);
    sys.stop_ay_write_watch();
    assert!(sys.ay_write_watch_records().is_none());
}

/// #254: the PSG was ticking, but its generated samples had no path out
/// of the machine. Exercise the real SVI I/O ports so this covers both the
/// register wiring and the host-facing buffer.
#[test]
fn psg_tone_reaches_the_audio_buffer() {
    let mut sys = booted::<TestNtsc>(trap_rom());
    for (register, value) in [(0, 100), (1, 0), (7, 0x3E), (8, 15)] {
        sys.io_write(0x88, register);
        sys.io_write(0x8C, value);
    }

    sys.run_frame();
    let audio = sys.take_audio_buffer();
    let (minimum, maximum) = audio.iter().copied().fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(minimum, maximum), sample| (minimum.min(sample), maximum.max(sample)),
    );

    assert_eq!(audio.len(), AY_SAMPLES_PER_FRAME);
    assert!(
        maximum - minimum > 0.01,
        "an enabled tone should produce a varying waveform"
    );
}
