use super::*;
struct TestB;
struct TestBPlus;
struct TestMaster;
impl Board for TestB {
    const MODEL: BbcModel = BbcModel::ModelB;
    const EXTRA_RAM_LEN: usize = 0;
    const SIDEWAYS_RAM: u16 = 0;
    fn new_cpu() -> M6502 {
        M6502::new()
    }
}
impl Board for TestBPlus {
    const MODEL: BbcModel = BbcModel::ModelBPlus;
    const EXTRA_RAM_LEN: usize = 0x8000;
    const SIDEWAYS_RAM: u16 = 0;
    fn new_cpu() -> M6502 {
        M6502::new()
    }
}
impl Board for TestMaster {
    const MODEL: BbcModel = BbcModel::Master128;
    const EXTRA_RAM_LEN: usize = 0x8000;
    const SIDEWAYS_RAM: u16 = 0xF0;
    fn new_cpu() -> M6502 {
        M6502::new_65c12()
    }
}
type BbcMicro = BbcCore<TestB>;
type BPlus = BbcCore<TestBPlus>;
type Master = BbcCore<TestMaster>;

#[test]
fn native_frame_follows_the_programmed_crtc_field_length() {
    let mut durations = Vec::new();
    for rows in [38, 19] {
        let mut machine = BbcMicro::new(trap_rom());
        let mut regs = MODE0_CRTC;
        regs[4] = rows;
        program_mode(&mut machine, regs, 0x9C);
        for _ in 0..37 {
            machine.tick_cpu_cycle();
        }
        assert!(machine.run_native_frame().1);
        let (ticks, completed) = machine.run_native_frame();
        assert!(completed && ticks > 0);
        assert!(
            !machine.state.video_frame_ended,
            "completed field must have been presented"
        );
        durations.push(ticks);
    }
    assert!(
        durations[1] < durations[0] * 3 / 4,
        "R4 must change the boundary, not merely a host budget: {durations:?}"
    );
}

fn trap_rom() -> Vec<u8> {
    // 16 KB MOS ROM with JMP self at $C000 + reset / IRQ / NMI
    // vectors pointing there.
    let mut rom = vec![0xEA_u8; 0x4000];
    rom[0x0000] = 0x4C;
    rom[0x0001] = 0x00;
    rom[0x0002] = 0xC0;
    rom[0x3FFA] = 0x00;
    rom[0x3FFB] = 0xC0;
    rom[0x3FFC] = 0x00;
    rom[0x3FFD] = 0xC0;
    rom[0x3FFE] = 0x00;
    rom[0x3FFF] = 0xC0;
    rom
}

/// Save-state must capture LIVE machine state (6502, 6845 CRTC, Video ULA,
/// both 6522 VIAs, SN76489 PSG, 32 KB RAM, ADC, ACIA, latch), not cold-boot
/// from the MOS ROM. Serialise, advance (so the state differs), then
/// deserialise the first snapshot and confirm re-serialising it is
/// byte-identical: every stateful field across all chips round-trips,
/// including the 32 KB RAM via BigArray.
#[test]
fn snapshot_round_trips_live_state() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.run_frame();
    sys.poke(0x0100, 0xA5); // a low-RAM byte to carry across the snapshot
    assert_eq!(sys.peek(0x0100), 0xA5, "poke lands in BBC main RAM");
    sys.run_frame();
    let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

    sys.run_frame(); // advance past the snapshot point
    let s2 = postcard::to_allocvec(&sys).expect("encode again");
    assert_ne!(s1, s2, "running a frame should change the serialised state");

    let restored: BbcMicro = postcard::from_bytes(&s1).expect("decode snapshot");
    let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
    assert_eq!(
        s1, s3,
        "restore should reproduce the snapshot state exactly"
    );
}

#[test]
fn frame_runs_expected_cycles() {
    let mut sys = BbcMicro::new(trap_rom());
    let t = sys.run_frame();
    assert_eq!(t, CYCLES_PER_FRAME);
    assert_eq!(sys.frame_count(), 1);
}

#[test]
fn many_frames_complete_without_panic() {
    let mut sys = BbcMicro::new(trap_rom());
    for _ in 0..10 {
        sys.run_frame();
    }
    assert_eq!(sys.frame_count(), 10);
}

#[test]
fn one_mhz_bus_accesses_cost_two_or_three_ticks_rest_one() {
    // Even and odd starts: a 1 MHz access costs two ticks when it starts
    // as a 1 MHz cycle begins (odd here) and three half-way through one.
    for start in [10u64, 11] {
        let slow = if BbcMicro::ends_one_mhz_cycle(start) {
            3
        } else {
            2
        };
        // Fast (2 MHz):
        for addr in [
            0x0000, 0xC000, 0x8000, 0xFE20, 0xFE30, 0xFE80, 0xFEA0, 0xFEE0,
        ] {
            assert_eq!(BbcMicro::access_master_ticks(addr, start), 1, "{addr:04X}");
        }
        // Slow (1 MHz bus): FRED, JIM, CRTC, ACIA, serial ULA, both
        // VIAs, ADC.
        for addr in [
            0xFC00, 0xFD00, 0xFE00, 0xFE08, 0xFE10, 0xFE40, 0xFE5F, 0xFE60, 0xFEC0,
        ] {
            assert_eq!(
                BbcMicro::access_master_ticks(addr, start),
                slow,
                "{addr:04X}"
            );
        }
    }
    assert_eq!(BbcMicro::access_master_ticks(0xFE40, 11), 2);
    assert_eq!(BbcMicro::access_master_ticks(0xFE40, 10), 3);
}

/// A 16 KB MOS ROM whose reset vector runs `program` at `$C000`.
fn rom_running(program: &[u8]) -> Vec<u8> {
    let mut rom = trap_rom();
    rom[..program.len()].copy_from_slice(program);
    rom
}

/// Master ticks per pass of a loop of `instructions` instructions, once
/// the loop has settled onto its steady phase against the 1 MHz clock.
fn ticks_per_pass(program: &[u8], instructions: usize) -> u64 {
    const PASSES: u64 = 64;
    let mut sys = BbcMicro::new(rom_running(program));
    // Reset, then a few passes to settle the phase.
    for _ in 0..8 * instructions {
        sys.step_instruction();
    }
    let start = sys.state.master_ticks;
    for _ in 0..PASSES as usize * instructions {
        sys.step_instruction();
    }
    let total = sys.state.master_ticks - start;
    assert_eq!(
        total % PASSES,
        0,
        "the loop should settle to a fixed period"
    );
    total / PASSES
}

/// The stretched cycle ends on a falling edge of the 1 MHz clock, and
/// a cycle that starts while that clock is high must wait out the low
/// half and a whole further high half before it can (Advanced User Guide
/// §28.4 pin 4 and §28.5.2). So a 1 MHz access costs two master ticks
/// when it starts as the 1 MHz cycle begins, and three when it starts
/// half-way through one.
///
/// `LDA &FE4F : JMP loop` has six 2 MHz cycles between VIA reads, an even
/// number, so every read starts in phase: 6 + 2 = 8 ticks a pass. Add a
/// three-cycle `BIT &00` and the gap is odd, so every read starts half a
/// microsecond out and costs three: 9 + 3 = 12, not 11.
#[test]
fn one_mhz_access_waits_for_the_next_whole_one_mhz_cycle() {
    // LDA &FE4F : JMP &C000
    let in_phase = [0xAD, 0x4F, 0xFE, 0x4C, 0x00, 0xC0];
    assert_eq!(ticks_per_pass(&in_phase, 2), 8);
    // LDA &FE4F : BIT &00 : JMP &C000
    let out_of_phase = [0xAD, 0x4F, 0xFE, 0x24, 0x00, 0x4C, 0x00, 0xC0];
    assert_eq!(ticks_per_pass(&out_of_phase, 3), 12);
    // The User VIA, the CRTC and FRED stretch the same way.
    for page_and_offset in [[0x6F, 0xFE], [0x01, 0xFE], [0x00, 0xFC]] {
        let program = [
            0xAD,
            page_and_offset[0],
            page_and_offset[1],
            0x24,
            0x00,
            0x4C,
            0x00,
            0xC0,
        ];
        assert_eq!(ticks_per_pass(&program, 3), 12, "{page_and_offset:02X?}");
    }
    // RAM, ROM and the fast SHEILA devices never stretch: LDA &FE30
    // (the ROM latch) : BIT &00 : JMP is 4 + 3 + 3 = 10.
    let fast = [0xAD, 0x30, 0xFE, 0x24, 0x00, 0x4C, 0x00, 0xC0];
    assert_eq!(ticks_per_pass(&fast, 3), 10);
}

/// Whatever the phase it starts in, a cycle that touches the 1 MHz bus
/// ends as a 1 MHz cycle ends (Advanced User Guide §28.4: "The trailing
/// edges of the 1MHzE and 2MHz processor clock are then coincidental").
/// Exercise every phase with a read-modify-write, whose three VIA
/// accesses follow back to back, and an odd-length gap between passes.
#[test]
fn every_one_mhz_cycle_ends_on_the_one_mhz_clock() {
    // INC &FE4F : BIT &00 : JMP &C000
    let program = [0xEE, 0x4F, 0xFE, 0x24, 0x00, 0x4C, 0x00, 0xC0];
    let mut sys = BbcMicro::new(rom_running(&program));
    let mut slow_cycles = 0;
    for _ in 0..2_000 {
        let start = sys.state.master_ticks;
        sys.tick_cpu_cycle();
        let addr = sys.state.cpu.addr;
        if BbcMicro::is_one_mhz(addr) {
            slow_cycles += 1;
            assert_eq!(
                sys.state.master_ticks % 2,
                1,
                "a 1 MHz access at {addr:04X} from tick {start} ended mid-way \
                 through a 1 MHz cycle"
            );
        }
    }
    assert!(slow_cycles > 100, "the loop should hit the VIA");
}

#[test]
fn frame_is_a_fixed_master_tick_budget() {
    // However the CPU's access mix falls out, the frame is exactly
    // 312 × 128 master ticks; the CPU just fits fewer cycles in when
    // it hits the 1 MHz bus.
    let mut sys = BbcMicro::new(trap_rom());
    sys.run_frame();
    assert_eq!(sys.state.master_ticks, CYCLES_PER_FRAME);
    // The trap loop runs entirely in ROM (2 MHz), so it fits one CPU
    // cycle per master tick — the maximum.
    assert_eq!(sys.cpu_cycles(), CYCLES_PER_FRAME);
}

#[test]
fn memory_map_routes_pages() {
    let mut rom = trap_rom();
    rom[0x0100] = 0x99;
    let mut sys = BbcMicro::new(rom);
    sys.insert_rom(0, vec![0x77; 0x4000]);
    // MOS at $C000.
    assert_eq!(sys.mem_read(0xC100), 0x99);
    // Sideways ROM at $8000.
    assert_eq!(sys.mem_read(0x8000), 0x77);
    // RAM round-trip.
    sys.mem_write(0x4000, 0x42);
    assert_eq!(sys.mem_read(0x4000), 0x42);
    // ROM writes ignored.
    sys.mem_write(0xC100, 0x00);
    assert_eq!(sys.mem_read(0xC100), 0x99);
}

#[test]
fn rom_bank_register_at_fe30() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.insert_rom(0, vec![0xAA; 0x4000]);
    sys.insert_rom(7, vec![0xBB; 0x4000]);
    sys.mem_write(0xFE30, 7);
    assert_eq!(sys.rom_bank(), 7);
    assert_eq!(sys.mem_read(0x8000), 0xBB);
    sys.mem_write(0xFE30, 0);
    assert_eq!(sys.mem_read(0x8000), 0xAA);
}

/// SHEILA devices repeat across their blocks (the Advanced User Guide's
/// SHEILA map): the Video ULA at `&20-&2F`, the ROM select at
/// `&30-&3F`, the VIAs at `&40-&5F` and `&60-&7F`. Only the first
/// address of each used to answer.
#[test]
fn sheila_devices_answer_across_their_whole_blocks() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.mem_write(0xFE2E, 0x9D);
    assert_eq!(
        sys.state.video_ula.control, 0x9D,
        "&FE2E is the control register"
    );
    sys.mem_write(0xFE2F, 0x53);
    assert_eq!(sys.state.video_ula.palette[5], 3, "&FE2F is the palette");
    sys.mem_write(0xFE3F, 0x07);
    assert_eq!(sys.rom_bank(), 7, "&FE3F selects a ROM");
    sys.mem_write(0xFE52, 0xFF); // system VIA DDRB, at its mirror
    assert_eq!(sys.mem_read(0xFE42), 0xFF);
    sys.mem_write(0xFE73, 0xA5); // user VIA DDRA, at its mirror
    assert_eq!(sys.mem_read(0xFE63), 0xA5);
}

#[test]
fn video_ula_palette_write_decodes_logical_and_physical() {
    let mut sys = BbcMicro::new(trap_rom());
    // Set logical entry 5 to physical 3 ($53 → logical=5, phys=3).
    sys.mem_write(0xFE21, 0x53);
    assert_eq!(sys.state.video_ula.palette[5], 3);
}

/// The control values the MOS writes for each mode, from the Advanced
/// User Guide's own table (§19.1.7), against the pixels each byte has to
/// produce for the mode to come out the documented width.
///
/// The old decode read bits 3-2 as a bit-depth field, which the register
/// does not have — they set the pixel rate. It got MODE 1 and MODE 2
/// backwards and doubled MODE 4 and MODE 6, and the test that covered it
/// asserted the same wrong answer (#1195).
#[test]
fn video_ula_pixels_per_byte_matches_the_documented_modes() {
    let mut sys = BbcMicro::new(trap_rom());
    for (mode, control, expected, width) in [
        (0u8, 0x9Cu8, 8usize, 640usize),
        (1, 0xD8, 4, 320),
        (2, 0xF4, 2, 160),
        (3, 0x9C, 8, 640),
        (4, 0x88, 8, 320),
        (5, 0xC4, 4, 160),
        (6, 0x88, 8, 320),
    ] {
        sys.mem_write(0xFE20, control);
        assert_eq!(
            sys.state.video_ula.pixels_per_byte(),
            expected,
            "MODE {mode} (control ${control:02X})"
        );
        // Bytes per line comes from the 6845, so pair each mode with its
        // own to confirm the geometry lands on the documented width.
        let bytes_per_line = if mode <= 3 { 80 } else { 40 };
        assert_eq!(bytes_per_line * expected, width, "MODE {mode} pixel width");
    }
}

/// The guide's `*FX154,224` worked example (§19.3): a 16-colour mode with
/// ten characters per line, which its own listing documents as two pixels
/// per byte.
#[test]
fn video_ula_matches_the_guides_mode_8_example() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.mem_write(0xFE20, 0xE0);
    assert!(!sys.state.video_ula.fast_clock());
    assert_eq!(sys.state.video_ula.pixels_per_byte(), 2);
}

#[test]
fn system_via_writes_round_trip() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.mem_write(0xFE43, 0xFF); // DDRA
    sys.mem_write(0xFE41, 0x77); // ORA
    assert_eq!(sys.state.system_via.ora(), 0x77);
}

/// Both VIAs are clocked by the 1 MHz clock, not the 2 MHz CPU clock
/// (Advanced User Guide §28.5: "All 1MHz peripherals are clocked by a
/// 1MHz 50% duty cycle square wave … to allow chips such as 6522 VIAs to
/// use their internal timing elements correctly"). A one-shot T1 loaded
/// with N times out N + 1.5 µs later — about 2N + 3 master ticks. Clocked
/// at 2 MHz it fired in half that, and the MOS's 100 Hz tick ran at 200.
#[test]
fn via_timers_count_at_one_megahertz() {
    const LATCH: u64 = 100;
    let mut sys = BbcMicro::new(trap_rom());
    for via_base in [0xFE40u16, 0xFE60] {
        sys.mem_write(via_base + 0x04, LATCH as u8); // T1 latch low
        sys.mem_write(via_base + 0x05, 0); // T1 counter high: start
    }
    let start = sys.state.master_ticks;
    let mut fired = [None, None];
    while fired.contains(&None) && sys.state.master_ticks - start < 1_000 {
        sys.tick_cpu_cycle();
        for (slot, via_base) in fired.iter_mut().zip([0xFE40u16, 0xFE60]) {
            if slot.is_none() && sys.mem_read(via_base + 0x0D) & 0x40 != 0 {
                *slot = Some(sys.state.master_ticks - start);
            }
        }
    }
    for (name, ticks) in ["System VIA", "User VIA"].iter().zip(fired) {
        let ticks = ticks.unwrap_or(u64::MAX);
        assert!(
            (2 * LATCH..=2 * LATCH + 6).contains(&ticks),
            "{name} T1 = {LATCH} should time out after ~{} master ticks \
             (1 MHz), took {ticks}",
            2 * LATCH + 3
        );
    }
}

#[test]
fn joystick_fire_buttons_pull_system_via_pb4_pb5_low() {
    let mut sys = BbcMicro::new(trap_rom());
    // Idle: both fire lines read high.
    sys.update_joystick_fire();
    assert_eq!(sys.state.system_via.pb_in & 0x30, 0x30);

    // Joy 1 fire → PB4 low, PB5 still high.
    sys.set_fire_button(1, true);
    sys.update_joystick_fire();
    assert_eq!(sys.state.system_via.pb_in & 0x10, 0, "joy1 fire → PB4 low");
    assert_eq!(
        sys.state.system_via.pb_in & 0x20,
        0x20,
        "joy2 idle → PB5 high"
    );

    // Joy 2 fire as well → both low.
    sys.set_fire_button(2, true);
    sys.update_joystick_fire();
    assert_eq!(
        sys.state.system_via.pb_in & 0x30,
        0,
        "both fire → PB4+PB5 low"
    );

    // Release joy 1 → PB4 high again, PB5 held low.
    sys.set_fire_button(1, false);
    sys.update_joystick_fire();
    assert_eq!(
        sys.state.system_via.pb_in & 0x10,
        0x10,
        "joy1 released → PB4 high"
    );
    assert_eq!(
        sys.state.system_via.pb_in & 0x20,
        0,
        "joy2 still held → PB5 low"
    );

    // It must reach the CPU through the IRB read with port B as input.
    let pb = sys.mem_read(0xFE40);
    assert_eq!(pb & 0x20, 0, "PB5 low visible at $FE40");
}

#[test]
fn adc_converts_a_channel_and_reports_completion() {
    let mut sys = BbcMicro::new(trap_rom());
    // Park a known value on channel 1 (joystick 1 Y): 0x0ABC.
    sys.set_adc_channel(1, 0x0ABC);

    // Start a 12-bit conversion on channel 1 (bit 3 = 12-bit, mux = 01).
    sys.mem_write(0xFEC0, 0b0000_1001);
    // Immediately busy: status bit 6 (busy_n) low, bit 7 (completed_n) high.
    let status = sys.mem_read(0xFEC0);
    assert_eq!(status & 0x40, 0, "busy_n low while converting");
    assert_eq!(status & 0x80, 0x80, "completed_n high while converting");

    // Run the conversion to completion (12-bit = 20000 cycles).
    for _ in 0..ADC_CONVERT_12BIT {
        sys.state.adc.tick();
    }
    let status = sys.mem_read(0xFEC0);
    assert_eq!(status & 0x80, 0, "completed_n low once finished");
    assert_eq!(status & 0x40, 0x40, "busy_n high once finished");
    assert_eq!(status & 0x03, 0x01, "mux echoes channel 1");
    // Top two value bits (0x0ABC >> 10 = 0b10) appear in status bits 5-4.
    assert_eq!((status >> 4) & 0x03, 0b10, "value[11:10] in status");

    // Result registers: high = value[11:4], low = value[3:0] << 4.
    assert_eq!(sys.mem_read(0xFEC1), 0xAB, "high byte = value[11:4]");
    assert_eq!(sys.mem_read(0xFEC2), 0xC0, "low byte = value[3:0] << 4");
}

#[test]
fn adc_completion_raises_the_system_via_cb1_interrupt() {
    let mut sys = BbcMicro::new(trap_rom());
    // Configure System VIA CB1 for a negative-edge interrupt and enable it:
    // PCR bit 4 = 0 (CB1 negative edge); IER bit 4 + bit 7 (set-enable).
    sys.mem_write(0xFE4C, 0x00); // PCR: CB1 negative edge
    sys.mem_write(0xFE4E, 0x90); // IER: enable CB1 (bit 4) with set bit (7)

    // A conversion in flight presents CB1 high (no edge yet).
    sys.mem_write(0xFEC0, 0b0000_1000); // 12-bit, channel 0
    assert_eq!(sys.mem_read(0xFE4D) & 0x10, 0, "no CB1 flag mid-conversion");

    // Drive it to completion through the real per-cycle tick so the
    // EOC→CB1 falling edge is delivered the same way the engine does it.
    for _ in 0..ADC_CONVERT_12BIT {
        sys.tick_cpu_cycle();
    }
    assert_ne!(
        sys.mem_read(0xFE4D) & 0x10,
        0,
        "CB1 (ADC end-of-conversion) interrupt flag set"
    );
}

/// The BBC's SN76489 runs off a 4 MHz clock: "frequency = 4 000 000/32 x
/// 10 bit binary number" (Advanced User Guide §23.3.1). A tone of N = 125
/// is 1 kHz, so one emulated second holds 48,000 samples crossing zero
/// 2,000 times. Ticked once per 2 MHz master tick it produced half the
/// samples, each pitched an octave low.
#[test]
fn psg_runs_off_a_four_megahertz_clock() {
    const N: u16 = 125;
    let mut sys = BbcMicro::new(trap_rom());
    sys.state.psg.write(0x80 | (N & 0x0F) as u8); // tone 0, low four bits
    sys.state.psg.write((N >> 4) as u8); // tone 0, high six bits
    sys.state.psg.write(0x90); // tone 0 at full volume
    for silence in [0xBF, 0xDF, 0xFF] {
        sys.state.psg.write(silence);
    }
    // Let the DC blocker settle, then take one second.
    for _ in 0..10 {
        sys.run_frame();
    }
    sys.take_audio_buffer();
    for _ in 0..50 {
        sys.run_frame();
    }
    let samples = sys.take_audio_buffer();
    assert!(
        (47_900..=48_100).contains(&samples.len()),
        "one second should hold 48,000 samples; got {}",
        samples.len()
    );
    let crossings = samples
        .windows(2)
        .filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0))
        .count();
    assert!(
        (1_980..=2_020).contains(&crossings),
        "a 1 kHz tone crosses zero 2,000 times a second; got {crossings}"
    );
}

#[test]
fn ic32_falling_edge_on_bit_0_writes_psg() {
    let mut sys = BbcMicro::new(trap_rom());
    // Set ORA = $80 (PSG tone latch byte for ch0).
    sys.mem_write(0xFE43, 0xFF);
    sys.mem_write(0xFE41, 0x80);
    // Raise latch bit 0 (write port B with addr=0, data=1).
    sys.mem_write(0xFE40, 0b0000_1000);
    // Drop latch bit 0 — should latch ORA into PSG.
    sys.mem_write(0xFE40, 0b0000_0000);
    // PSG sweep / mute behaviour is verified inside emu198x-ti-sn76489;
    // here we just confirm the write path didn't panic.
}

#[test]
fn acia_idle_does_not_signal_an_interrupt() {
    // The MOS IRQ handler reads $FE08 to decide whether the 6850 ACIA
    // interrupted. An idle ACIA must report TDRE set and the interrupt bit
    // ($80) CLEAR — the old open-bus $FF read set bit 7 and the MOS serviced
    // a phantom serial interrupt forever, never clearing the System VIA
    // timer (the storm that kept BASIC from printing `>`).
    let mut sys = BbcMicro::new(trap_rom());
    let status = sys.mem_read(0xFE08);
    assert_eq!(status & 0x80, 0, "idle ACIA must not assert IRQ (bit 7)");
    assert_eq!(
        status & 0x02,
        0x02,
        "idle ACIA reports TDRE (ready to send)"
    );
    assert!(!sys.state.acia.irq(), "idle ACIA drives no CPU interrupt");

    // Faithful detail: enabling the transmit interrupt (control bits 6-5 =
    // 01) does make TDRE assert the interrupt, matching b-em.
    sys.mem_write(0xFE08, 0x20);
    assert!(sys.state.acia.irq(), "TX-interrupt mode + TDRE asserts IRQ");
    assert_eq!(sys.mem_read(0xFE08) & 0x80, 0x80, "and shows in the status");
}

/// Program the 6845 the way the MOS does for a mode (Advanced User Guide
/// §18 register tables) and the Video ULA's control register.
fn program_mode<B: Board>(sys: &mut BbcCore<B>, crtc: [u8; 14], ula_control: u8) {
    for (reg, value) in crtc.into_iter().enumerate() {
        sys.mem_write(0xFE00, reg as u8);
        sys.mem_write(0xFE01, value);
    }
    sys.mem_write(0xFE20, ula_control);
}

const MODE0_CRTC: [u8; 14] = [
    127, 80, 98, 0x28, 38, 0, 32, 34, 0x01, 7, 0x67, 8, 0x06, 0x00,
];
const MODE4_CRTC: [u8; 14] = [
    63, 40, 49, 0x24, 38, 0, 32, 34, 0x01, 7, 0x67, 8, 0x0B, 0x00,
];
const MODE7_CRTC: [u8; 14] = [
    63, 40, 51, 0x24, 30, 2, 25, 27, 0x93, 18, 0x72, 19, 0x28, 0x00,
];

/// MODE 0's two-colour palette: logical 0-7 black, 8-15 white, the way
/// the MOS sets it so a shifted-in `1` cannot change the colour.
fn mode0_palette<B: Board>(sys: &mut BbcCore<B>) {
    for logical in 0..16u8 {
        let physical = if logical >= 8 { 7 } else { 0 };
        sys.mem_write(0xFE21, (logical << 4) | (physical ^ 7));
    }
}

/// Set addressable-latch output `bit` to `on` through System VIA port B.
fn set_latch<B: Board>(sys: &mut BbcCore<B>, bit: u8, on: bool) {
    sys.mem_write(0xFE40, bit | if on { 0x08 } else { 0 });
}

const WHITE: u32 = 0xFFFF_FFFF;

fn pixel<B: Board>(sys: &BbcCore<B>, x: usize, y: usize) -> u32 {
    sys.framebuffer()[y * FB_WIDTH as usize + x]
}

/// A screen scrolled so its first row sits at the top of RAM carries on
/// from `$3000`, not from ROM: the wrap circuit takes MODE 0's 20K off
/// any address past `$7FFF` (Advanced User Guide §18.10) (#164). Before,
/// the second row read past RAM and came out black.
#[test]
fn a_scrolled_screen_wraps_past_the_top_of_ram() {
    let mut sys = BbcMicro::new(trap_rom());
    let mut crtc = MODE0_CRTC;
    // Start one row (640 bytes) below the top of RAM.
    let start = (0x8000u16 - 640) / 8;
    crtc[12] = (start >> 8) as u8;
    crtc[13] = start as u8;
    program_mode(&mut sys, crtc, 0x9C);
    mode0_palette(&mut sys);
    // MODE 0's wrap: B5 = 1, B4 = 0, as the MOS sets it.
    set_latch(&mut sys, 5, true);
    set_latch(&mut sys, 4, false);
    for offset in 0..8 {
        sys.state.ram[0x7D80 + offset] = 0xFF; // row 0, first character
        sys.state.ram[0x3000 + offset] = 0xFF; // row 1 after the wrap
    }
    for _ in 0..3 {
        sys.run_frame();
    }
    // Two character rows of eight lines, each line on two rows.
    for y in 0..32 {
        for x in 0..8 {
            assert_eq!(pixel(&sys, x, y), WHITE, "lit cell at ({x}, {y})");
        }
        assert_eq!(pixel(&sys, 8, y), BLANK, "unlit neighbour at (8, {y})");
    }
}

/// Each latch setting wraps by its mode's screen size. Decoded from the
/// bits the MOS writes; see `screen_wrap_size`.
#[test]
fn the_latch_selects_each_modes_screen_size() {
    let mut sys = BbcMicro::new(trap_rom());
    for (b5, b4, size, mode) in [
        (true, false, 0x5000u16, "0-2"),
        (false, false, 0x4000, "3"),
        (true, true, 0x2800, "4-5"),
        (false, true, 0x2000, "6"),
    ] {
        set_latch(&mut sys, 5, b5);
        set_latch(&mut sys, 4, b4);
        // MA $1000 is RAM address $8000: the first byte past the top.
        assert_eq!(
            sys.video_address(0x1000, 0),
            0x8000 - size,
            "MODE {mode} wraps to its own start"
        );
    }
}

/// The displayed rows come from the 6845's R6, not from the framebuffer's
/// height: a 16-row display blanks everything below it (#163).
#[test]
fn rows_past_r6_are_not_displayed() {
    let mut sys = BbcMicro::new(trap_rom());
    let mut crtc = MODE0_CRTC;
    crtc[6] = 16;
    program_mode(&mut sys, crtc, 0x9C);
    mode0_palette(&mut sys);
    sys.state.ram[0x3000..0x8000].fill(0xFF);
    for _ in 0..3 {
        sys.run_frame();
    }
    assert_eq!(pixel(&sys, 0, 255), WHITE, "row 15 is displayed");
    assert_eq!(pixel(&sys, 0, 256), BLANK, "row 16 is past R6");
    assert_eq!(pixel(&sys, 320, 511), BLANK);
}

/// The slow-clock modes run the 6845 at 1 MHz, so their 64-character
/// lines take the same 64 µs as MODE 0's 128 and the frame is still
/// 50 Hz. Clocked at 2 MHz they produced VSYNC at 100 Hz (#163).
#[test]
fn slow_clock_modes_still_sync_at_fifty_hertz() {
    for (crtc, control, name) in [(MODE0_CRTC, 0x9C, "MODE 0"), (MODE4_CRTC, 0x88, "MODE 4")] {
        let mut sys = BbcMicro::new(trap_rom());
        program_mode(&mut sys, crtc, control);
        sys.run_frame();
        let mut edges = 0;
        let mut was = sys.crtc().vsync;
        let start = sys.state.master_ticks;
        while sys.state.master_ticks - start < CYCLES_PER_FRAME * 10 {
            sys.tick_cpu_cycle();
            let now = sys.crtc().vsync;
            edges += u32::from(now && !was);
            was = now;
        }
        assert_eq!(edges, 10, "{name}: one VSYNC per 20 ms frame");
    }
}

/// Run until the picture holds a whole MODE 7 frame: both of its fields,
/// each started after the 6845 was programmed.
fn run_both_fields<B: Board>(sys: &mut BbcCore<B>) {
    for _ in 0..4 {
        sys.run_frame();
    }
}

/// A glyph ROM whose `A` lights every pixel of every row and whose other
/// glyphs are empty.
fn font_with_a_lit() -> Vec<u8> {
    let mut font = vec![0u8; 96 * 10];
    let a = usize::from(b'A' - 0x20) * 10;
    font[a..a + 10].fill(0x3F);
    font
}

/// MODE 7 reads the characters the 6845 addresses. Its hardware scroll
/// moves the start address, so the first character on screen is wherever
/// R12/R13 point; it used to be read from `$7C00` regardless (#163).
#[test]
fn mode7_follows_the_start_address() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.set_teletext_font(font_with_a_lit());
    let mut crtc = MODE7_CRTC;
    crtc[13] = 40; // scrolled up one row: the screen starts at $7C28
    program_mode(&mut sys, crtc, 0x4B);
    sys.state.ram[0x7C00..0x8000].fill(b' ');
    sys.state.ram[0x7C28] = b'A';
    run_both_fields(&mut sys);
    let x = 0;
    assert_eq!(pixel(&sys, x, 0), WHITE, "$7C28 is the top-left cell");
    assert_eq!(pixel(&sys, x, 20), BLANK, "and not the second row's");
}

/// MODE 7's 1K screen wraps on its own: MA0-9 address it, so the row
/// after `$7FFF` is `$7C00` (Advanced User Guide §18.11.3).
#[test]
fn mode7_wraps_within_its_kilobyte() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.set_teletext_font(font_with_a_lit());
    let mut crtc = MODE7_CRTC;
    // Start at $7FD8: forty bytes short of the top of RAM, one row.
    crtc[12] = 0x2B;
    crtc[13] = 0xD8;
    program_mode(&mut sys, crtc, 0x4B);
    sys.state.ram[0x7C00..0x8000].fill(b' ');
    sys.state.ram[0x7C00] = b'A';
    run_both_fields(&mut sys);
    assert_eq!(pixel(&sys, 0, 20), WHITE, "row 1 is $7C00");
}

/// MODE 7's interlace sync and video mode scans the even raster addresses
/// in one field and the odd ones in the next (Advanced User Guide figure
/// 18.2c), and the picture holds both: framebuffer row 0 is the even
/// field's first line and row 1 the odd field's. Before, one 256-line
/// field overwrote the other, so a glyph's first line filled one row.
#[test]
fn mode7_weaves_its_two_fields() {
    let mut sys = BbcMicro::new(trap_rom());
    let mut font = vec![0u8; 96 * 10];
    font[usize::from(b'A' - 0x20) * 10] = 0x3F; // only the glyph's top line
    sys.set_teletext_font(font);
    program_mode(&mut sys, MODE7_CRTC, 0x4B);
    sys.state.ram[0x7C00..0x8000].fill(b' ');
    sys.state.ram[0x7C00] = b'A';
    run_both_fields(&mut sys);
    let x = 0;
    assert_eq!(pixel(&sys, x, 0), WHITE, "the even field's line");
    assert_eq!(pixel(&sys, x, 1), WHITE, "the odd field's line");
    assert_eq!(pixel(&sys, x, 2), BLANK, "the glyph's second line");
}

/// Only D0-D6 reach the SAA5050, so the MOS's `CHR$129` is the
/// red-alphanumerics control code and the text after it is red.
#[test]
fn teletext_control_codes_ignore_bit_7() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.set_teletext_font(font_with_a_lit());
    program_mode(&mut sys, MODE7_CRTC, 0x4B);
    sys.state.ram[0x7C00..0x8000].fill(b' ');
    sys.state.ram[0x7C00] = 0x81;
    sys.state.ram[0x7C01] = b'A';
    run_both_fields(&mut sys);
    assert_eq!(
        pixel(&sys, TELETEXT_CELL_WIDTH, 0),
        0xFFFF_0000,
        "the A after CHR$129 is red"
    );
}

/// MODE 7 with `rows` of teletext, one string of codes per screen row,
/// and a font whose `A` has the dot rows `a`. Both fields are drawn.
fn teletext_screen(a: [u8; 10], rows: &[&[u8]]) -> BbcMicro {
    let mut sys = BbcMicro::new(trap_rom());
    let mut font = vec![0u8; 96 * 10];
    let glyph = usize::from(b'A' - 0x20) * 10;
    font[glyph..glyph + 10].copy_from_slice(&a);
    sys.set_teletext_font(font);
    program_mode(&mut sys, MODE7_CRTC, 0x4B);
    sys.state.ram[0x7C00..0x8000].fill(b' ');
    for (row, codes) in rows.iter().enumerate() {
        let start = 0x7C00 + row * 40;
        sys.state.ram[start..start + codes.len()].copy_from_slice(codes);
    }
    run_both_fields(&mut sys);
    sys
}

/// Framebuffer rows of MODE 7 cell (`column`, `row`) that have any
/// pixel in `colour`, as offsets 0-19 from the top of the cell.
fn cell_rows_in<B: Board>(sys: &BbcCore<B>, column: usize, row: usize, colour: u32) -> Vec<usize> {
    let x0 = column * TELETEXT_CELL_WIDTH;
    (0..20)
        .filter(|&y| (x0..x0 + TELETEXT_CELL_WIDTH).any(|x| pixel(sys, x, row * 20 + y) == colour))
        .collect()
}

/// How much of each pixel of one framebuffer row of a MODE 7 cell white
/// text covers on black, in thirds.
fn cell_line<B: Board>(sys: &BbcCore<B>, column: usize, y: usize) -> Vec<u8> {
    let x0 = column * TELETEXT_CELL_WIDTH;
    (0..TELETEXT_CELL_WIDTH)
        .map(|x| {
            let green = (pixel(sys, x0 + x, y) >> 8) as u8;
            let thirds = TELETEXT_COVERAGE_LEVELS.iter().position(|&l| l == green);
            thirds.expect("a white-on-black level") as u8
        })
        .collect()
}

/// Character rounding: a diagonal step from dot 2 on glyph row 1 to dot 3
/// on row 2 is smoothed by half a dot, the odd field's line of row 1
/// reaching towards row 2 and the even field's line of row 2 reaching
/// back. Before, each dot row was drawn square on whichever field came
/// last.
///
/// The half-dots are 5-7 and 4-6 of twelve, four thirds of a pixel each,
/// so the steps fall a third and two thirds of the way into pixels.
#[test]
fn mode7_rounds_diagonals_across_the_two_fields() {
    let sys = teletext_screen([0, 0x04, 0x08, 0, 0, 0, 0, 0, 0, 0], &[b"A"]);
    #[rustfmt::skip]
    let expected: [(usize, [u8; 16], &str); 4] = [
        (2, [0, 0, 0, 0, 0, 0, 0, 0, 3, 3, 2, 0, 0, 0, 0, 0], "row 1, even field"),
        (3, [0, 0, 0, 0, 0, 0, 1, 3, 3, 3, 2, 0, 0, 0, 0, 0], "row 1, odd field"),
        (4, [0, 0, 0, 0, 0, 2, 3, 3, 3, 1, 0, 0, 0, 0, 0, 0], "row 2, even field"),
        (5, [0, 0, 0, 0, 0, 2, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0], "row 2, odd field"),
    ];
    for (y, thirds, line) in expected {
        assert_eq!(cell_line(&sys, 0, y), thirds, "{line}");
    }
}

/// The SAA5050 puts out a character a microsecond, the 6845's clock in
/// MODE 7, so a cell is the sixteen pixels a microsecond fills on the
/// 16 MHz framebuffer, and MODE 7's forty columns fill the 640 as MODE
/// 0-6 do. They were twelve pixels each, centred in 480 (#1623).
#[test]
fn mode7_cells_are_a_microsecond_wide_and_fill_the_window() {
    assert_eq!(TELETEXT_CELL_WIDTH, 16);
    let line = |columns: &[usize]| {
        let mut row = [b' '; 40];
        for &column in columns {
            row[column] = b'A';
        }
        white_run(&teletext_screen([0x3F; 10], &[&row]), 0)
    };
    assert_eq!(line(&[0]), Some((0, 16)), "the first cell");
    assert_eq!(line(&[39]), Some((624, 640)), "the fortieth cell");
    let all: Vec<usize> = (0..40).collect();
    assert_eq!(line(&all), Some((0, FB_WIDTH as usize)), "forty cells");
}

/// Each half-dot covers four thirds of a pixel, from `4h/3` to
/// `4(h + 1)/3`, so the thirds of each pixel it covers are the overlap of
/// that span with the pixel's. The resampler must give every pattern the
/// sum of its half-dots' overlaps: no light lost or moved.
#[test]
fn mode7_resampling_keeps_each_half_dot_where_it_falls() {
    let overlap = |half_dot: usize, pixel: usize| -> u8 {
        let start = (4 * half_dot).max(3 * pixel);
        let end = (4 * half_dot + 4).min(3 * pixel + 3);
        end.saturating_sub(start) as u8
    };
    for pattern in 0..0x1000u16 {
        let expected: Vec<u8> = (0..16)
            .map(|pixel| {
                (0..12)
                    .filter(|&h| pattern & (0x800 >> h) != 0)
                    .map(|h| overlap(h, pixel))
                    .sum()
            })
            .collect();
        assert_eq!(
            teletext_coverage(pattern).to_vec(),
            expected,
            "{pattern:012b}"
        );
    }
}

/// The levels are coverage mixed in linear light and sRGB-encoded.
#[test]
fn mode7_coverage_levels_are_linear_light_in_srgb() {
    for (thirds, &level) in TELETEXT_COVERAGE_LEVELS.iter().enumerate() {
        let linear = thirds as f64 / 3.0;
        let encoded = if linear <= 0.003_130_8 {
            12.92 * linear
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        assert_eq!(level, (encoded * 255.0).round() as u8, "{thirds} thirds");
    }
}

/// The cursor inverts the SAA5050's colours before they are mixed, so an
/// inverted pixel is the same mix of the inverted foreground and
/// background.
#[test]
fn the_cursor_inverts_a_mixed_mode7_pixel_to_the_inverted_mix() {
    for fg in 0..8 {
        for bg in 0..8 {
            for coverage in 0..=3 {
                assert_eq!(
                    invert_rgb(teletext_pixel(fg, bg, coverage)),
                    teletext_pixel(fg ^ 7, bg ^ 7, coverage),
                    "fg {fg}, bg {bg}, {coverage} thirds"
                );
            }
        }
    }
}

/// Double height (code 141, `$8D`) is set-after: the code's own cell is a
/// space, the characters after it draw their top half across the row,
/// each dot row four lines tall, and the row below draws the bottom
/// halves of its own double-height characters. The MOS repeats the line
/// to get that, as the Advanced Teletext System User Guide describes.
#[test]
fn double_height_draws_top_and_bottom_halves_on_two_rows() {
    // Dot rows 1 and 6 lit: the top half shows row 1, the bottom row 6.
    let a = [0, 0x3F, 0, 0, 0, 0, 0x3F, 0, 0, 0];
    let sys = teletext_screen(a, &[b"\x8DA", b"\x8DA", b"A"]);
    assert_eq!(
        cell_rows_in(&sys, 0, 0, WHITE),
        Vec::<usize>::new(),
        "the code's cell"
    );
    assert_eq!(
        cell_rows_in(&sys, 1, 0, WHITE),
        [4, 5, 6, 7],
        "row 1, doubled"
    );
    assert_eq!(
        cell_rows_in(&sys, 1, 1, WHITE),
        [4, 5, 6, 7],
        "row 6, doubled"
    );
    assert_eq!(
        cell_rows_in(&sys, 0, 2, WHITE),
        [2, 3, 12, 13],
        "the next row is normal"
    );
}

/// The row after a double-height row shows only double-height characters.
/// If it has no double-height code at all, it is blank, whatever it
/// holds; and normal-height characters on it are blank too.
#[test]
fn the_row_under_double_height_shows_only_double_height_characters() {
    let a = [0x3F; 10];
    let sys = teletext_screen(a, &[b"\x8DA", b"A A\x8DA"]);
    assert_eq!(
        cell_rows_in(&sys, 0, 1, WHITE),
        Vec::<usize>::new(),
        "normal height"
    );
    assert_eq!(
        cell_rows_in(&sys, 2, 1, WHITE),
        Vec::<usize>::new(),
        "normal height"
    );
    assert_eq!(cell_rows_in(&sys, 4, 1, WHITE).len(), 20, "double height");

    let sys = teletext_screen(a, &[b"\x8DA", b"A"]);
    assert_eq!(
        cell_rows_in(&sys, 0, 1, WHITE),
        Vec::<usize>::new(),
        "no code: blank"
    );
}

/// Flash (136, `$88`) hides the text after it for 16 fields in every 64;
/// steady (137, `$89`) stops it at its own cell. Watched on the even
/// field's rows, which change once a frame pair.
#[test]
fn flashing_teletext_hides_for_16_fields_in_64() {
    let mut sys = teletext_screen([0x3F; 10], &[b"\x88A\x89A"]);
    let x = TELETEXT_CELL_WIDTH;
    let steady = 3 * TELETEXT_CELL_WIDTH;
    let mut shown = Vec::new();
    for _ in 0..150 {
        sys.run_frame();
        shown.push(pixel(&sys, x, 0) == WHITE);
        assert_eq!(pixel(&sys, steady, 0), WHITE, "the steady A never flashes");
    }
    // Runs of frames shown or hidden, without the partial first and last.
    let mut runs = Vec::new();
    let mut run = 1;
    for pair in shown.windows(2) {
        if pair[0] == pair[1] {
            run += 1;
        } else {
            runs.push((pair[0], run));
            run = 1;
        }
    }
    let whole = runs.get(1..).unwrap_or_default();
    assert!(whole.len() >= 2, "it flashes: {shown:?}");
    for &(lit, frames) in whole {
        assert_eq!(frames, if lit { 48 } else { 16 }, "{runs:?}");
    }
}

/// Conceal (152, `$98`) hides what follows until a colour code, which
/// reveals the text after it.
#[test]
fn conceal_hides_text_until_a_colour_code() {
    let sys = teletext_screen([0x3F; 10], &[b"\x98A\x82A"]);
    assert_eq!(
        cell_rows_in(&sys, 1, 0, WHITE),
        Vec::<usize>::new(),
        "concealed"
    );
    assert_eq!(
        cell_rows_in(&sys, 3, 0, 0xFF00_FF00).len(),
        20,
        "green, shown"
    );
}

/// Hold graphics (158, `$9E`) is set-at, so its own cell repeats the last
/// mosaic; and a colour change under hold is set-after, so its cell
/// repeats the mosaic in the old colour. Before, the hold cell was a
/// space and the colour cell took the new colour.
#[test]
fn hold_graphics_repeats_the_mosaic_in_its_own_colour() {
    let sys = teletext_screen([0; 10], &[b"\x97\x7F\x9E\x91\x7F"]);
    assert_eq!(cell_rows_in(&sys, 2, 0, WHITE).len(), 20, "the hold cell");
    assert_eq!(
        cell_rows_in(&sys, 3, 0, WHITE).len(),
        20,
        "the colour cell, still white"
    );
    assert_eq!(cell_rows_in(&sys, 4, 0, 0xFFFF_0000).len(), 20, "then red");
}

/// Physical colour `&08` flashes black and white: the palette stores it
/// EOR 7 as `&0F`, and Video ULA control bit 0 picks which of the pair is
/// showing (Advanced User Guide §19.1.1, §19.2.3). Before, the flash bit
/// was ignored and the colour was black whatever bit 0 said (#384).
#[test]
fn control_bit_0_selects_which_flashing_colour_shows() {
    for (control, expected, phase) in [(0x9C, BLANK, "first"), (0x9D, WHITE, "second")] {
        let mut sys = BbcMicro::new(trap_rom());
        program_mode(&mut sys, MODE0_CRTC, control);
        for logical in 0..16u8 {
            let physical = if logical >= 8 { 0x08 } else { 0 };
            sys.mem_write(0xFE21, (logical << 4) | (physical ^ 7));
        }
        sys.state.ram[0x3000..0x3008].fill(0xFF);
        for _ in 0..3 {
            sys.run_frame();
        }
        assert_eq!(
            pixel(&sys, 0, 0),
            expected,
            "flashing black-white shows its {phase} colour"
        );
    }
}

/// A steady cursor on raster 7 of the character at `column` of row 0.
fn mode0_with_cursor(r8: u8, control: u8, column: u16) -> BbcMicro {
    let mut sys = BbcMicro::new(trap_rom());
    let mut crtc = MODE0_CRTC;
    crtc[8] = r8;
    crtc[10] = 0x07; // steady, starting on raster 7
    crtc[11] = 7;
    program_mode(&mut sys, crtc, control);
    mode0_palette(&mut sys);
    let cursor = 0x0600 + column;
    sys.mem_write(0xFE00, 14);
    sys.mem_write(0xFE01, (cursor >> 8) as u8);
    sys.mem_write(0xFE00, 15);
    sys.mem_write(0xFE01, cursor as u8);
    for _ in 0..3 {
        sys.run_frame();
    }
    sys
}

/// The pixels of framebuffer row `y` that are white, as a run `start..end`.
fn white_run<B: Board>(sys: &BbcCore<B>, y: usize) -> Option<(usize, usize)> {
    let lit: Vec<usize> = (0..FB_WIDTH as usize)
        .filter(|&x| pixel(sys, x, y) == WHITE)
        .collect();
    let (&first, &last) = (lit.first()?, lit.last()?);
    assert_eq!(last + 1 - first, lit.len(), "the lit pixels are one run");
    Some((first, last + 1))
}

/// The 6845's CURSOR is one character wide; control bits 7, 6 and 5
/// widen it to the one, two or four bytes a mode's character takes
/// (Advanced User Guide §19.1.5-§19.1.7). It inverts the picture, so over
/// black it is white, and only on the rasters R10-R11 select. Before,
/// the BBC drew no cursor at all (#384).
#[test]
fn the_cursor_is_as_wide_as_control_bits_5_to_7_make_it() {
    for (control, width, mode) in [
        (0x9C, 8, "bit 7 alone: MODE 0's one byte"),
        (0xDC, 16, "bits 7 and 6: MODE 1's two bytes"),
        (0xFC, 32, "bits 7, 6 and 5: MODE 2's four bytes"),
    ] {
        let sys = mode0_with_cursor(0x00, control, 2);
        // Raster 7 of the first row is framebuffer rows 14 and 15.
        assert_eq!(white_run(&sys, 14), Some((16, 16 + width)), "{mode}");
        assert_eq!(white_run(&sys, 15), Some((16, 16 + width)), "{mode}");
        assert_eq!(white_run(&sys, 13), None, "{mode}: raster 6 has none");
    }
    let sys = mode0_with_cursor(0x00, 0x1C, 2);
    assert_eq!(white_run(&sys, 14), None, "bits 5-7 clear hide it");
}

/// R8 bits 6-7 delay the cursor by none, one or two characters, or turn
/// it off (Advanced User Guide §18.6.3).
#[test]
fn r8_skews_the_cursor_or_turns_it_off() {
    for (skew, start) in [(0u8, 16usize), (1, 24), (2, 32)] {
        let sys = mode0_with_cursor(skew << 6, 0x9C, 2);
        assert_eq!(white_run(&sys, 14), Some((start, start + 8)), "skew {skew}");
    }
    let sys = mode0_with_cursor(0xC0, 0x9C, 2);
    assert_eq!(white_run(&sys, 14), None, "skew 3 disables the cursor");
}

/// R10 = `&67`, the MOS's bitmap-mode cursor, blinks with a 32-field
/// period: sixteen frames on, sixteen off (Advanced User Guide §18.8).
#[test]
fn the_mos_cursor_blinks_every_sixteen_fields() {
    let mut sys = mode0_with_cursor(0x00, 0x9C, 2);
    sys.mem_write(0xFE00, 10);
    sys.mem_write(0xFE01, 0x67);
    let shown: Vec<bool> = (0..96)
        .map(|_| {
            sys.run_frame();
            white_run(&sys, 14).is_some()
        })
        .collect();
    let changes: Vec<usize> = (1..shown.len())
        .filter(|&i| shown[i] != shown[i - 1])
        .collect();
    assert!(changes.len() >= 5, "blinks: {shown:?}");
    // The first change can come early: the phase was running while the
    // cursor was steady.
    for pair in changes[1..].windows(2) {
        assert_eq!(pair[1] - pair[0], 16, "a 16-field half-period: {shown:?}");
    }
}

/// MODE 7's cursor: the MOS sets a two-character skew and enables only
/// the second segment (`&4B`), which together land it on the character
/// the 6845 matched, three clocks on, once the SAA5050 has drawn it. With
/// R10/R11 = 18-19 it underlines the cell's tenth line: raster 18 in the
/// even field and 19 in the odd, framebuffer rows 18 and 19.
#[test]
fn the_mode7_cursor_underlines_the_matched_cell() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.set_teletext_font(font_with_a_lit());
    let mut crtc = MODE7_CRTC;
    crtc[10] = 0x12; // steady, from raster 18
    program_mode(&mut sys, crtc, 0x4B);
    sys.state.ram[0x7C00..0x8000].fill(b' ');
    sys.mem_write(0xFE00, 14);
    sys.mem_write(0xFE01, 0x28);
    sys.mem_write(0xFE00, 15);
    sys.mem_write(0xFE01, 0x05); // the sixth cell of row 0
    run_both_fields(&mut sys);
    let x = 5 * TELETEXT_CELL_WIDTH;
    assert_eq!(white_run(&sys, 18), Some((x, x + TELETEXT_CELL_WIDTH)));
    assert_eq!(white_run(&sys, 19), Some((x, x + TELETEXT_CELL_WIDTH)));
    assert_eq!(white_run(&sys, 17), None, "only the bottom line");
}

// Kansas-City encoding for the cassette wiring tests.
const T_ZERO_HALF: u32 = 416_667;
const T_ONE_HALF: u32 = 208_333;

fn push_tape_byte(pulses: &mut Vec<TapePulse>, byte: u8) {
    let mut push_bit = |set: bool| {
        pulses.push(if set {
            TapePulse::Cycles {
                half_period_ns: T_ONE_HALF,
                count: 2,
            }
        } else {
            TapePulse::Cycles {
                half_period_ns: T_ZERO_HALF,
                count: 1,
            }
        });
    };
    push_bit(false); // start
    for i in 0..8 {
        push_bit((byte >> i) & 1 == 1);
    }
    push_bit(true); // stop
}

/// A long carrier leader then one framed byte.
fn carrier_then_byte(byte: u8) -> Vec<TapePulse> {
    let mut pulses = vec![TapePulse::Cycles {
        half_period_ns: T_ONE_HALF,
        count: 256,
    }];
    push_tape_byte(&mut pulses, byte);
    pulses
}

#[test]
fn cassette_does_not_play_while_the_motor_is_off() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.insert_tape(carrier_then_byte(0xA5));
    // The serial ULA motor bit ($FE10 bit 7) defaults off.
    assert!(!sys.cassette_motor_on());
    for _ in 0..10 {
        sys.run_frame();
    }
    assert!(
        !sys.state.acia.rx_full,
        "no byte should arrive with the motor off"
    );
    assert!(sys.tape_loaded());
}

/// The motor line says whether the machine wants the tape to move; the
/// deck gate says whether it is running at all. A script could not stop
/// the tape at all before, which made a stalled load hard to inspect
/// (#1198).
/// Stepping instructions and running frames have to drive the same
/// hardware. The scanline loop lived in `run_frame`, so a stepped BBC
/// never had VSYNC driven into the System VIA and painted nothing --
/// no 50 Hz interrupt, and a stale framebuffer (#1202).
#[test]
fn stepping_a_frames_worth_of_instructions_is_a_frame() {
    let mut stepped = BbcMicro::new(trap_rom());
    let mut run = BbcMicro::new(trap_rom());

    while stepped.state.master_ticks < CYCLES_PER_FRAME {
        stepped.step_instruction();
    }
    run.run_frame();

    assert_eq!(stepped.frame_count(), run.frame_count());
    assert_eq!(
        stepped.framebuffer(),
        run.framebuffer(),
        "a stepped frame has to paint what a run frame paints"
    );
}

#[test]
fn a_stopped_deck_does_not_advance_even_with_the_motor_on() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.insert_tape(carrier_then_byte(0xA5));
    sys.mem_write(0xFE08, 0x80); // ACIA: enable RX interrupt
    sys.mem_write(0xFE10, 0x80); // serial ULA: motor on
    assert!(sys.cassette_motor_on());

    sys.set_deck_running(false);
    for _ in 0..12 {
        sys.run_frame();
    }
    assert!(
        !sys.state.acia.rx_full,
        "the motor is on, but the deck is stopped, so no byte can arrive"
    );

    sys.set_deck_running(true);
    let mut arrived = false;
    for _ in 0..12 {
        sys.run_frame();
        if sys.state.acia.rx_full {
            arrived = true;
            break;
        }
    }
    assert!(arrived, "starting the deck lets the same tape play");
}

#[test]
fn cassette_byte_fills_the_acia_and_clears_on_read() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.insert_tape(carrier_then_byte(0xA5));
    sys.mem_write(0xFE08, 0x80); // ACIA control: enable RX interrupt (CR7)
    sys.mem_write(0xFE10, 0x80); // serial ULA: motor on
    assert!(sys.cassette_motor_on());

    let mut arrived = false;
    for _ in 0..12 {
        sys.run_frame();
        if sys.state.acia.rx_full {
            arrived = true;
            break;
        }
    }
    assert!(arrived, "the ACIA never received a byte");
    // RDRF + RX-int enable raises the ACIA interrupt and the status bit.
    assert!(
        sys.state.acia.irq(),
        "received byte must assert the ACIA IRQ"
    );
    assert_eq!(sys.mem_read(0xFE08) & 0x01, 0x01, "status shows RDRF");
    // Reading the data register ($FE09) returns the byte and clears RDRF.
    assert_eq!(sys.mem_read(0xFE09), 0xA5);
    assert!(!sys.state.acia.rx_full, "reading $FE09 clears RDRF");
    assert!(!sys.state.acia.irq(), "and drops the interrupt");
}

#[test]
fn cassette_carrier_raises_dcd_and_clears_on_data_read() {
    let mut sys = BbcMicro::new(trap_rom());
    // A sustained carrier tone with no data.
    sys.insert_tape(vec![TapePulse::Cycles {
        half_period_ns: T_ONE_HALF,
        count: 256,
    }]);
    sys.mem_write(0xFE08, 0x80); // ACIA control: enable RX interrupt
    sys.mem_write(0xFE10, 0x80); // serial ULA: motor on

    let mut dcd = false;
    for _ in 0..6 {
        sys.run_frame();
        if sys.mem_read(0xFE08) & 0x04 != 0 {
            dcd = true;
            break;
        }
    }
    // The MOS waits on Data Carrier Detect (status bit 2) before reading a
    // tape block; sustained carrier must raise it and the interrupt.
    assert!(dcd, "sustained carrier must raise DCD");
    assert!(sys.state.acia.irq(), "DCD raises the ACIA interrupt");
    // Reading the data register clears the latched DCD.
    let _ = sys.mem_read(0xFE09);
    assert_eq!(sys.mem_read(0xFE08) & 0x04, 0, "reading data clears DCD");
}

/// Run `program` from `&C000` (the MOS's VDU-driver range) on a B+.
fn b_plus_running(program: &[u8], instructions: usize) -> BPlus {
    let mut sys = BPlus::new(rom_running(program));
    for _ in 0..instructions + 2 {
        sys.step_instruction();
    }
    sys
}

#[test]
fn the_master_runs_a_65c12_and_says_so_after_a_restore() {
    let mut sys = Master::new(trap_rom());
    assert_eq!(sys.state.cpu.variant(), emu198x_mos_6502::Variant::Cmos);
    sys.state.cpu.set_variant(emu198x_mos_6502::Variant::Nmos);
    sys.finish_restore();
    assert_eq!(sys.state.cpu.variant(), emu198x_mos_6502::Variant::Cmos);
    let mut b = BbcMicro::new(trap_rom());
    b.finish_restore();
    assert_eq!(b.state.cpu.variant(), emu198x_mos_6502::Variant::Nmos);
}

#[test]
fn the_master_pages_andy_hazel_and_sideways_ram() {
    let mut sys = Master::new(trap_rom());
    // ANDY: 4 KB at &8000-&8FFF with ROMSEL bit 7.
    sys.mem_write(0xFE30, 0x80);
    sys.mem_write(0x8000, 0xA1);
    assert_eq!(sys.mem_read(0x8000), 0xA1);
    assert_eq!(sys.mem_read(0xFE30), 0x80, "ROMSEL reads back");
    sys.mem_write(0xFE30, 0x09);
    assert_eq!(sys.mem_read(0x8000), 0xFF, "the empty bank 9 again");
    // Sideways RAM in banks 4-7, and only there.
    sys.mem_write(0xFE30, 0x05);
    sys.mem_write(0x9000, 0x5B);
    assert_eq!(sys.mem_read(0x9000), 0x5B);
    sys.mem_write(0xFE30, 0x09);
    sys.mem_write(0x9000, 0x77);
    assert_eq!(sys.mem_read(0x9000), 0xFF, "bank 9 is ROM");
    // HAZEL: 8 KB over the MOS at &C000-&DFFF with ACCCON bit 3.
    let mos = sys.mem_read(0xC000);
    sys.mem_write(0xFE34, 0x08);
    sys.mem_write(0xC000, 0x3C);
    sys.mem_write(0xDFFF, 0xC3);
    assert_eq!([sys.mem_read(0xC000), sys.mem_read(0xDFFF)], [0x3C, 0xC3]);
    assert_eq!(sys.mem_read(0xFE34), 0x08, "ACCCON reads back");
    sys.mem_write(0xFE34, 0x00);
    assert_eq!(sys.mem_read(0xC000), mos, "the MOS again");
    // TST: I/O reads come from the MOS.
    sys.mem_write(0xFE34, 0x40);
    assert_eq!(sys.mem_read(0xFE40), sys.state.mos_rom[0x3E40]);
}

#[test]
fn the_master_shadow_follows_x_for_all_code_and_e_for_the_vdu() {
    let mut sys = Master::new(trap_rom());
    sys.state.ram[0x3000] = 0x11;
    sys.state.extra_ram[0x3000] = 0x99;
    let read_from = |sys: &mut Master, pc: u16| {
        sys.latch_shadow_access(pc);
        sys.mem_read(0x3000)
    };
    // E (bit 1): only the VDU drivers see the shadow screen.
    sys.mem_write(0xFE34, 0x02);
    assert_eq!(read_from(&mut sys, 0xC000), 0x99);
    assert_eq!(read_from(&mut sys, 0x0200), 0x11);
    // X (bit 2): everything else does too, and so does VDU code with E.
    sys.mem_write(0xFE34, 0x06);
    assert_eq!(read_from(&mut sys, 0x0200), 0x99);
    assert_eq!(read_from(&mut sys, 0xD000), 0x99);
    // X without E: VDU code goes back to main RAM.
    sys.mem_write(0xFE34, 0x04);
    assert_eq!(read_from(&mut sys, 0xD000), 0x11);
    // With HAZEL paged in, &C000-&DFFF is not VDU code, so X decides.
    sys.mem_write(0xFE34, 0x0C);
    assert_eq!(read_from(&mut sys, 0xD000), 0x99);
    // D (bit 0) is the display alone.
    sys.mem_write(0xFE34, 0x01);
    assert_eq!(sys.video_byte(0x3000), 0x99);
    assert_eq!(read_from(&mut sys, 0x0200), 0x11);
}

#[test]
fn the_master_moves_the_adc_and_puts_the_1770_at_fe28() {
    let mut sys = Master::new(trap_rom());
    sys.insert_disk(0, BbcMicro::dfs_disk(&ssd_image(), 1, false));
    sys.mem_write(0xFE24, 0x25); // drive 0, single density, out of reset
    settle_fdc(&mut sys);
    sys.mem_read(0xFE28); // clear the Restore's INTRQ
    sys.mem_write(0xFE2A, 0x01);
    assert_eq!(sys.mem_read(0xFE2A), 0x01, "the sector register");
    assert_eq!(sys.mem_read(0xFE86), 0xFF, "not the B+ address");
    sys.mem_write(0xFE18, 0x00); // ADC conversion on channel 0
    assert!(sys.state.adc.busy, "the ADC answers at &FE18");
}

#[test]
fn the_master_reads_its_cmos_clock_over_the_system_via() {
    let mut sys = Master::new(trap_rom());
    // The MOS's sequence: put the register on port A and pulse PB7 with
    // PB6 selecting the chip, then release port A, raise R/W (IC32 bit 1)
    // and the strobe (bit 2), and read port A.
    sys.mem_write(0xFE43, 0xFF); // DDRA: output
    sys.mem_write(0xFE42, 0xCF); // DDRB: PB0-3, PB6-7 out
    sys.mem_write(0xFE4F, 19); // the language/filing-system byte
    sys.mem_write(0xFE40, 0xC0); // PB7 latches the address, PB6 selects
    sys.mem_write(0xFE40, 0x40);
    sys.mem_write(0xFE43, 0x00); // DDRA: input
    sys.mem_write(0xFE40, 0x4B); // IC32 bit 3 high: keyboard off
    sys.mem_write(0xFE40, 0x49); // IC32 bit 1 high: read
    sys.mem_write(0xFE40, 0x4A); // IC32 bit 2 high: strobe
    sys.tick_cpu_cycle();
    assert_eq!(sys.mem_read(0xFE4F), 0xC9, "BASIC (12) and the DFS (9)");
}

#[test]
fn b_plus_romsel_bit_7_pages_ram_in_over_the_sideways_rom() {
    let mut sys = BPlus::new(trap_rom());
    assert_eq!(sys.model(), BbcModel::ModelBPlus);
    sys.mem_write(0xFE30, 0x80);
    sys.mem_write(0x8000, 0x42);
    sys.mem_write(0xAFFF, 0x24);
    assert_eq!([sys.mem_read(0x8000), sys.mem_read(0xAFFF)], [0x42, 0x24]);
    assert_eq!(sys.mem_read(0xB000), 0xFF, "paged RAM ends at &AFFF");
    sys.mem_write(0xFE38, 0x00); // ROMSEL repeats at +8
    assert_eq!(sys.mem_read(0x8000), 0xFF, "the empty sideways slot again");
    sys.mem_write(0xFE30, 0x80);
    assert_eq!(sys.mem_read(0x8000), 0x42, "paged RAM kept its contents");
}

#[test]
fn a_model_b_ignores_romsel_bit_7_and_acccon() {
    let mut sys = BbcMicro::new(trap_rom());
    sys.mem_write(0xFE30, 0x80);
    sys.mem_write(0x8000, 0x42);
    assert_eq!(sys.mem_read(0x8000), 0xFF);
    sys.mem_write(0xFE34, 0x80); // on a Model B this is ROMSEL again
    assert_eq!(sys.state.rom_bank, 0);
    assert!(!sys.state.shadow_screen);
}

#[test]
fn vdu_driver_code_writes_the_shadow_screen_and_other_code_main_ram() {
    // At &C000, inside the VDU drivers' range: select the shadow screen,
    // then store &55 at &3000.
    let program = [
        0xA9, 0x80, // LDA #&80
        0x8D, 0x34, 0xFE, // STA &FE34 (ACCCON: shadow)
        0xA9, 0x55, // LDA #&55
        0x8D, 0x00, 0x30, // STA &3000
        0x4C, 0x0A, 0xC0, // JMP &C00A (spin)
    ];
    let mut sys = b_plus_running(&program, 4);
    assert_eq!(sys.state.ram[0x3000], 0x00, "main RAM untouched");
    assert_eq!(
        sys.state.extra_ram[0x3000], 0x55,
        "the shadow screen took it"
    );
    // Code outside &C000-&DFFF sees main RAM.
    sys.latch_shadow_access(0x0200);
    assert_eq!(sys.mem_read(0x3000), 0x00);
    sys.latch_shadow_access(0xC000);
    assert_eq!(sys.mem_read(0x3000), 0x55);
    // Code in paged RAM between &A000 and &AFFF reaches it only with paged
    // RAM selected.
    sys.latch_shadow_access(0xA000);
    assert_eq!(sys.mem_read(0x3000), 0x00);
    sys.mem_write(0xFE30, 0x80);
    sys.latch_shadow_access(0xA000);
    assert_eq!(sys.mem_read(0x3000), 0x55);
}

#[test]
fn the_display_shows_the_shadow_screen_when_selected() {
    let mut sys = BPlus::new(trap_rom());
    sys.state.ram[0x3000] = 0x11;
    sys.state.extra_ram[0x3000] = 0x99;
    assert_eq!(sys.video_byte(0x3000), 0x11);
    sys.mem_write(0xFE34, 0x80);
    assert_eq!(sys.video_byte(0x3000), 0x99, "the shadow screen is shown");
    sys.mem_write(0xFE3C, 0x00); // ACCCON repeats at +8
    assert_eq!(sys.video_byte(0x3000), 0x11);
}

/// A 40-track single-sided DFS image whose every byte names its track,
/// sector and offset.
fn ssd_image() -> Vec<u8> {
    (0..40 * 10 * 256)
        .map(|i: usize| ((i / 2560) as u8) ^ (((i / 256) % 10) as u8) << 4 ^ (i as u8))
        .collect()
}

/// A DFS catalogue for a side of `sectors` sectors holding `files` files.
fn catalogue(image: &mut [u8], at: usize, files: u8, sectors: u16) {
    image[at + 256 + 5] = files * 8;
    image[at + 256 + 6] = (sectors >> 8) as u8;
    image[at + 256 + 7] = sectors as u8;
}

#[test]
fn dfs_sides_finds_the_second_catalogue_of_a_dsd() {
    let mut dsd = vec![0u8; 40 * 2 * DFS_TRACK_BYTES];
    catalogue(&mut dsd, 0, 2, 400);
    catalogue(&mut dsd, DFS_TRACK_BYTES, 0, 400);
    assert_eq!(BbcMicro::dfs_sides(&dsd), 2);

    let mut ssd = vec![0u8; 80 * DFS_TRACK_BYTES];
    catalogue(&mut ssd, 0, 1, 800);
    assert_eq!(BbcMicro::dfs_sides(&ssd), 1, "track 1 holds no catalogue");
    assert_eq!(BbcMicro::dfs_sides(&[]), 1);
    assert_eq!(BbcMicro::dfs_sides(&vec![0; 81 * DFS_TRACK_BYTES]), 2);
}

#[test]
fn a_short_dfs_image_is_padded_to_its_tracks() {
    let disk = BbcMicro::dfs_disk(&[0xAA; 3 * DFS_TRACK_BYTES], 1, true);
    assert_eq!(disk.data().len(), 40 * DFS_TRACK_BYTES);
    assert_eq!(disk.data()[0], 0xAA);
    let long = BbcMicro::dfs_disk(&vec![0; 41 * DFS_TRACK_BYTES], 1, true);
    assert_eq!(long.data().len(), 80 * DFS_TRACK_BYTES);
}

fn settle_fdc<B: Board>(sys: &mut BbcCore<B>) {
    for _ in 0..10_000 {
        sys.state.fdc.tick();
        if sys.state.fdc.peek_status() & 0x01 == 0 {
            return;
        }
    }
    panic!("the 1770 stayed busy");
}

#[test]
fn the_b_plus_1770_decodes_at_fe84_and_repeats_to_fe9f() {
    let mut sys = BPlus::new(trap_rom());
    sys.mem_write(0xFE85, 0x27); // track register
    assert_eq!(sys.mem_read(0xFE85), 0x27);
    assert_eq!(sys.mem_read(0xFE9D), 0x27, "repeats every four bytes");
    let mut model_b = BbcMicro::new(trap_rom());
    model_b.mem_write(0xFE85, 0x27);
    assert_eq!(model_b.mem_read(0xFE85), 0xFF, "no 1770 on a Model B");
    model_b.insert_disk(0, BbcMicro::dfs_disk(&ssd_image(), 1, false));
    assert!(model_b.disk(0).is_none());
}

#[test]
fn releasing_the_drive_latch_reset_restores_the_head() {
    let mut sys = BPlus::new(trap_rom());
    sys.insert_disk(0, BbcMicro::dfs_disk(&ssd_image(), 1, false));
    sys.mem_write(0xFE85, 9);
    sys.mem_write(0xFE80, 0x09); // drive 0, single density, held in reset
    assert_eq!(sys.state.fdc.peek_status() & 0x01, 0, "no reset pulse yet");
    sys.mem_write(0xFE80, 0x29); // release reset
    assert_ne!(sys.state.fdc.peek_status() & 0x01, 0, "the Restore runs");
    settle_fdc(&mut sys);
    assert_eq!(sys.mem_read(0xFE85), 0, "the head is back on track 0");
}

#[test]
fn a_sector_read_raises_nmi_for_each_byte() {
    let mut sys = BPlus::new(trap_rom());
    let image = ssd_image();
    sys.insert_disk(1, BbcMicro::dfs_disk(&image, 1, false));
    sys.mem_write(0xFE80, 0x2A); // drive 1, single density, out of reset
    settle_fdc(&mut sys);
    sys.mem_read(0xFE84); // clear the Restore's INTRQ
    sys.mem_write(0xFE86, 3); // sector 3, track 0
    sys.mem_write(0xFE84, 0x80); // Read Sector
    let mut data = Vec::new();
    for _ in 0..200_000 {
        sys.state.fdc.tick();
        sys.state.cpu.nmi = sys.state.fdc.intrq() || sys.state.fdc.drq();
        if sys.state.fdc.drq() {
            assert!(sys.state.cpu.nmi, "DRQ drives NMI");
            data.push(sys.mem_read(0xFE87));
        } else if sys.state.fdc.intrq() {
            break;
        }
    }
    assert_eq!(data, image[3 * 256..4 * 256]);
    assert_eq!(sys.mem_read(0xFE84) & 0x1C, 0, "no errors");
}

#[test]
fn restored_board_rejects_foreign_hardware_and_restores_its_cpu_variant() {
    let master = Master::new(trap_rom());
    let bytes = postcard::to_allocvec(&master).expect("Master state");
    let restored: Master = postcard::from_bytes(&bytes).expect("Master restore");
    assert_eq!(restored.cpu().variant(), emu198x_mos_6502::Variant::Cmos);
    assert!(postcard::from_bytes::<BPlus>(&bytes).is_err());
    assert!(postcard::from_bytes::<BbcMicro>(&bytes).is_err());
    let mut forged = Master::new(trap_rom());
    forged.state.model = BbcModel::ModelBPlus;
    let bytes = postcard::to_allocvec(&forged).expect("forged state");
    assert!(
        postcard::from_bytes::<BPlus>(&bytes).is_err(),
        "Master sideways RAM is not a B+"
    );
    let b = postcard::to_allocvec(&BbcMicro::new(trap_rom())).expect("Model B state");
    assert!(postcard::from_bytes::<Master>(&b).is_err());
    assert!(postcard::from_bytes::<BPlus>(&b).is_err());
}
