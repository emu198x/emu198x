use super::*;
struct TestPal;
struct TestNtsc;
impl Board for TestPal {
    const MODEL: Vic20Model = Vic20Model::Pal;
    const PAL: bool = true;
}
impl Board for TestNtsc {
    const MODEL: Vic20Model = Vic20Model::Ntsc;
    const PAL: bool = false;
}
type Vic20 = Vic20Core<TestPal>;

fn make_vic20() -> Vic20 {
    make_vic20_model::<TestPal>()
}

fn make_vic20_model<B: Board>() -> Vic20Core<B> {
    let mut kernal = vec![0xEAu8; 0x2000];
    // Reset vector at $FFFC/$FFFD → $E000.
    kernal[0x1FFC] = 0x00;
    kernal[0x1FFD] = 0xE0;
    Vic20Core::<B>::new(
        kernal,
        vec![0u8; 0x2000],
        vec![0u8; 0x1000],
        Vic20RamExpansion::NONE,
    )
}

fn check_io_decode_mirrors_and_overlaps_reach_every_selected_chip<B: Board>() {
    // VICE's VIC/VIA I/O callbacks, with independently seeded register C:
    // VIC=$F3, VIA1=$CF, VIA2=$AF. Each 64-byte group repeats the selects.
    let reads = [[0xf3, 0xc3, 0xa3, 0x83], [0xff, 0xcf, 0xaf, 0x8f]];
    let selected = [[false, false], [true, false], [false, true], [true, true]];
    let mut cases = 0;
    {
        let model = B::MODEL;
        for page in 0x90_u16..=0x93 {
            for block in 0..16 {
                let mut sys = make_vic20_model::<B>();
                sys.state.vic.write(0x0c, 0xf3);
                sys.state.via1.write(0x0c, 0xcf);
                sys.state.via2.write(0x0c, 0xaf);
                let addr = page * 256 + block * 16 + 12;
                let group = usize::from(block % 4);
                assert_eq!(
                    sys.peek(addr),
                    reads[usize::from(page != 0x90)][group],
                    "{model:?} ${addr:04x}"
                );
                sys.poke(addr, 0x55);
                let [via1, via2] = selected[group];
                assert_eq!(
                    sys.state.vic.read(0x0c),
                    if page == 0x90 { 0x55 } else { 0xf3 },
                    "VIC ${addr:04x}"
                );
                assert_eq!(
                    sys.state.via1.peek(0x0c),
                    if via1 { 0x55 } else { 0xcf },
                    "VIA1 ${addr:04x}"
                );
                assert_eq!(
                    sys.state.via2.peek(0x0c),
                    if via2 { 0x55 } else { 0xaf },
                    "VIA2 ${addr:04x}"
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 64);
}

fn check_io_decode_cpu_reads_acknowledge_all_selected_vias_but_peeks_do_not<B: Board>() {
    let reads = [[0xf3, 0xc3, 0xa3, 0x83], [0xff, 0xcf, 0xaf, 0x8f]];
    let flags = [[0x40, 0x40], [0, 0x40], [0x40, 0], [0, 0]];
    let mut cases = 0;
    {
        let model = B::MODEL;
        for page in 0x90_u16..=0x93 {
            for block in 0..16 {
                let mut sys = make_vic20_model::<B>();
                sys.state.vic.write(0x0c, 0xf3);
                sys.state.via1.write(0x0c, 0xcf);
                sys.state.via2.write(0x0c, 0xaf);
                for via in [&mut sys.state.via1, &mut sys.state.via2] {
                    via.write(4, 2);
                    via.write(5, 0);
                    for _ in 0..4 {
                        via.tick();
                    }
                    assert_eq!(via.peek(13), 0x40, "timer must fire before access");
                }
                let addr = page * 256 + block * 16;
                let _ = sys.peek(addr + 4);
                assert_eq!(
                    [sys.state.via1.peek(13), sys.state.via2.peek(13)],
                    [0x40; 2],
                    "peek acknowledged timer"
                );
                let [low, high] = addr.to_le_bytes();
                let code = [
                    0xad,
                    low + 12,
                    high,
                    0x8d,
                    0,
                    0x10, // combined register read
                    0xad,
                    low + 4,
                    high, // acknowledge selected timers
                    0xad,
                    0x1d,
                    0x91,
                    0x8d,
                    1,
                    0x10, // VIA1 IFR
                    0xad,
                    0x2d,
                    0x91,
                    0x8d,
                    2,
                    0x10, // VIA2 IFR
                    0xa9,
                    0x5a,
                    0x8d,
                    3,
                    0x10, // completion marker
                    0x4c,
                    26,
                    0xe0,
                ];
                sys.state.kernal_rom[..code.len()].copy_from_slice(&code);
                for _ in 0..100 {
                    sys.tick_cycle();
                }
                assert_eq!(sys.peek(0x1003), 0x5a, "guest did not complete");
                let group = usize::from(block % 4);
                assert_eq!(
                    sys.peek(0x1000),
                    reads[usize::from(page != 0x90)][group],
                    "CPU data {model:?} ${addr:04x}"
                );
                assert_eq!(
                    [sys.peek(0x1001), sys.peek(0x1002)],
                    flags[group],
                    "CPU side effects {model:?} ${addr:04x}"
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 64);
}

/// Save-state must capture LIVE machine state (6502 + both 6522 VIAs +
/// VIC-I + keyboard + all RAM banks + colour RAM + ROMs), not cold-boot
/// from the ROMs. Run a frame, poke a work-RAM byte, advance, serialise;
/// advance again and confirm the serialised state changed; then deserialise
/// the first snapshot and confirm re-serialising it is byte-identical.
#[test]
fn snapshot_round_trips_live_state() {
    let mut sys = make_vic20();
    sys.run_frame();
    sys.poke(0x1000, 0xA5); // a main-RAM byte to carry across the snapshot
    assert_eq!(sys.peek(0x1000), 0xA5);
    sys.run_frame();
    let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

    sys.run_frame(); // advance past the snapshot point
    let s2 = postcard::to_allocvec(&sys).expect("encode again");
    assert_ne!(s1, s2, "running a frame should change the serialised state");

    let restored: Vic20 = postcard::from_bytes(&s1).expect("decode snapshot");
    let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
    assert_eq!(
        s1, s3,
        "restore should reproduce the snapshot state exactly"
    );
}

#[test]
fn ram_round_trips() {
    let mut sys = make_vic20();
    sys.mem_write(0x0000, 0x42);
    assert_eq!(sys.mem_read(0x0000), 0x42);
}

#[test]
fn main_ram_round_trips() {
    let mut sys = make_vic20();
    sys.mem_write(0x1000, 0xAB);
    assert_eq!(sys.mem_read(0x1000), 0xAB);
}

#[test]
fn colour_ram_masks_to_nibble() {
    let mut sys = make_vic20();
    sys.mem_write(0x9400, 0xFF);
    assert_eq!(sys.mem_read(0x9400), 0x0F);
}

#[test]
fn vic_address_bus_reaches_rom_colour_and_each_visible_ram_bank() {
    let mut sys = make_vic20();
    sys.state.char_rom[0x0123] = 0x11;
    sys.state.colour_ram[0x0234] = 0xF2;
    sys.state.ram_low[0x0345] = 0x33;
    sys.state.ram_main[0x0456] = 0x44;

    let read = |addr| {
        read_vic_memory(
            addr,
            &sys.state.ram_low,
            &sys.state.ram_exp_low,
            &sys.state.ram_main,
            sys.state.expansion.exp_3k,
            &sys.state.colour_ram,
            &sys.state.char_rom,
        )
    };
    assert_eq!(read(0x0123), 0x11, "VIC $0123 maps to character ROM");
    assert_eq!(read(0x1634), 0x02, "VIC $1634 maps to colour RAM");
    assert_eq!(read(0x2345), 0x33, "VIC $2345 maps to CPU $0345");
    assert_eq!(read(0x3456), 0x44, "VIC $3456 maps to CPU $1456");
    assert_eq!(read(0x2456), 0xFF, "absent 3 KiB expansion is open bus");

    let mut expanded = Vic20::new(
        vec![0; 0x2000],
        vec![0; 0x2000],
        vec![0; 0x1000],
        Vic20RamExpansion::EXP_3K,
    );
    expanded.state.ram_exp_low[0x0567] = 0x55;
    assert_eq!(
        read_vic_memory(
            0x2967,
            &expanded.state.ram_low,
            &expanded.state.ram_exp_low,
            &expanded.state.ram_main,
            expanded.state.expansion.exp_3k,
            &expanded.state.colour_ram,
            &expanded.state.char_rom,
        ),
        0x55,
        "VIC $2967 maps to expanded CPU $0967"
    );
}

#[test]
fn rom_writes_ignored() {
    let mut sys = make_vic20();
    sys.mem_write(0xE000, 0xFF);
    assert_eq!(sys.mem_read(0xE000), 0xEA);
}

#[test]
fn frame_advances_count() {
    let mut sys = make_vic20();
    let _ = sys.run_frame();
    assert_eq!(sys.frame_count(), 1);
}

#[test]
fn ntsc_runs() {
    let mut kernal = vec![0xEAu8; 0x2000];
    kernal[0x1FFC] = 0x00;
    kernal[0x1FFD] = 0xE0;
    let mut sys = Vic20Core::<TestNtsc>::new(
        kernal,
        vec![0u8; 0x2000],
        vec![0u8; 0x1000],
        Vic20RamExpansion::NONE,
    );
    let _ = sys.run_frame();
    assert_eq!(sys.frame_count(), 1);
    assert_eq!(sys.model(), Vic20Model::Ntsc);
}

#[test]
fn keyboard_scan_reads_through_via2() {
    // ROM-free guard for the VIA #2 keyboard path: configure port B as
    // the column-drive output and port A as the row-read input (as the
    // KERNAL does), press a key, drive its column low, and confirm the
    // key's row line reads low on port A.
    let mut sys = make_vic20();
    sys.mem_write(0x9122, 0xFF); // VIA2 DDRB: port B all outputs (columns)
    sys.mem_write(0x9123, 0x00); // VIA2 DDRA: port A all inputs (rows)

    // 'A' is matrix (row 1, col 2): pressing it shorts PA1 to PB2.
    sys.press_key(Vic20Key::A);
    sys.mem_write(0x9120, !(1 << 2)); // drive column 2 low, others high
    let _ = sys.step_instruction(); // let tick refresh the row input

    let port_a = sys.mem_read(0x9121);
    assert_eq!(
        port_a & (1 << 1),
        0,
        "PA1 should read low for 'A'; got {port_a:#04X}"
    );

    // A column that no pressed key occupies leaves every row line high.
    sys.mem_write(0x9120, !(1 << 5)); // drive an unrelated column low
    let _ = sys.step_instruction();
    assert_eq!(
        sys.mem_read(0x9121),
        0xFF,
        "no key on column 5 → all rows high"
    );
}

#[test]
fn joystick_reads_through_the_vias() {
    // Up/down/left/fire are VIA #1 PA2-PA5 (read at $9111, port A is input
    // by default); right is VIA #2 PB7 (read at $9120). All active low.
    let mut sys = make_vic20();

    sys.set_joystick(true, false, true, false, true); // up + left + fire
    let _ = sys.step_instruction(); // let tick merge the switches

    let pa = sys.mem_read(0x9111);
    assert_eq!(pa & (1 << 2), 0, "up → PA2 low");
    assert_eq!(pa & (1 << 4), 0, "left → PA4 low");
    assert_eq!(pa & (1 << 5), 0, "fire → PA5 low");
    assert_eq!(pa & (1 << 3), 1 << 3, "down idle → PA3 high");

    // Right lives on the awkward VIA #2 PB7 line, not PA.
    sys.set_joystick(false, false, false, true, false);
    let _ = sys.step_instruction();
    assert_eq!(sys.mem_read(0x9120) & (1 << 7), 0, "right → PB7 low");
    // PA directions all idle again.
    assert_eq!(sys.mem_read(0x9111) & 0x3C, 0x3C, "no PA directions held");

    // Release everything → all control lines idle high.
    sys.set_joystick(false, false, false, false, false);
    let _ = sys.step_instruction();
    assert_eq!(sys.mem_read(0x9111) & 0x3C, 0x3C, "PA2-PA5 idle high");
    assert_eq!(sys.mem_read(0x9120) & (1 << 7), 1 << 7, "PB7 idle high");
}

#[test]
fn user_port_pb0_external_level_reaches_via1_through_ddr() {
    let mut sys = make_vic20();
    sys.mem_write(0x9112, 0x00); // VIA1 DDRB: PB0 input

    sys.set_user_port_pb0(false);
    sys.tick_cycle();
    assert_eq!(sys.mem_read(0x9110) & 0x01, 0, "external PB0 low");
    assert!(!sys.user_port_pb0());

    sys.set_user_port_pb0(true);
    sys.tick_cycle();
    assert_eq!(sys.mem_read(0x9110) & 0x01, 1, "external PB0 high");
    assert!(sys.user_port_pb0());

    // Output mode must expose the VIA latch, not the external input.
    sys.mem_write(0x9112, 0x01);
    sys.mem_write(0x9110, 0x00);
    assert!(!sys.user_port_pb0(), "DDRB0 output low wins");
    sys.mem_write(0x9110, 0x01);
    assert!(sys.user_port_pb0(), "DDRB0 output high wins");
}

#[test]
fn user_port_cb2_exposes_via1_pin_transitions() {
    let mut sys = make_vic20();

    sys.mem_write(0x911C, 0xC0); // PCR: CB2 manual output low
    assert!(!sys.user_port_cb2());

    sys.mem_write(0x911C, 0xE0); // PCR: CB2 manual output high
    assert!(sys.user_port_cb2());
}

#[test]
fn frame_user_port_exchange_runs_at_cycle_granularity() {
    let mut sys = make_vic20();
    sys.mem_write(0x9112, 0x00); // PB0 input
    let mut calls = 0u64;
    let ticks = sys.run_frame_with_user_port(&mut |cb2| {
        assert!(cb2, "unattached CB2 should idle high");
        calls += 1;
        calls & 1 == 0
    });
    assert_eq!(calls, ticks);
    assert_eq!(sys.user_port_pb0(), calls & 1 == 0);
}

#[test]
fn expansion_ram_low() {
    let mut sys = Vic20::new(
        vec![0xEA; 0x2000],
        vec![0u8; 0x2000],
        vec![0u8; 0x1000],
        Vic20RamExpansion::EXP_3K,
    );
    sys.mem_write(0x0400, 0x55);
    assert_eq!(sys.mem_read(0x0400), 0x55);
}

fn machine_with(expansion: Vic20RamExpansion) -> Vic20 {
    Vic20::new(
        vec![0xEA; 0x2000],
        vec![0u8; 0x2000],
        vec![0u8; 0x1000],
        expansion,
    )
}

#[test]
fn an_8k_cartridge_alone_leaves_the_3k_area_as_open_bus() {
    // A VIC-1110 fills BLK1 only. The 3K expander is a separate product,
    // so $0400-$0FFF must stay unpopulated.
    let mut sys = machine_with(Vic20RamExpansion::EXP_8K);
    sys.mem_write(0x0400, 0x55);
    sys.mem_write(0x0FFF, 0x55);
    assert_eq!(sys.mem_read(0x0400), 0xFF, "$0400 has no RAM behind it");
    assert_eq!(sys.mem_read(0x0FFF), 0xFF, "$0FFF has no RAM behind it");

    sys.mem_write(0x2000, 0x42);
    sys.mem_write(0x3FFF, 0x43);
    assert_eq!(sys.mem_read(0x2000), 0x42, "BLK1 is fitted");
    assert_eq!(sys.mem_read(0x3FFF), 0x43, "BLK1 runs to $3FFF");
    assert_eq!(sys.mem_read(0x4000), 0xFF, "BLK2 is not fitted");
}

#[test]
fn a_3k_expander_alone_leaves_the_blocks_as_open_bus() {
    let mut sys = machine_with(Vic20RamExpansion::EXP_3K);
    sys.mem_write(0x0400, 0x55);
    assert_eq!(sys.mem_read(0x0400), 0x55, "the 3K expander is fitted");
    sys.mem_write(0x2000, 0x42);
    assert_eq!(sys.mem_read(0x2000), 0xFF, "BLK1 is not fitted");
}

#[test]
fn a_dip_switched_cartridge_populates_only_its_own_block() {
    // "8k@4000": BLK2 answers, BLK1 and BLK3 stay empty.
    let expansion = Vic20RamExpansion::parse("8k@4000").expect("valid spec");
    let mut sys = machine_with(expansion);
    for (addr, fitted) in [(0x2000u16, false), (0x4000, true), (0x6000, false)] {
        sys.mem_write(addr, 0x42);
        let expected = if fitted { 0x42 } else { 0xFF };
        assert_eq!(sys.mem_read(addr), expected, "${addr:04X}");
    }
}

#[test]
fn blk5_ram_answers_when_no_rom_cartridge_claims_it() {
    let expansion = Vic20RamExpansion::parse("8k@a000").expect("valid spec");
    let mut sys = machine_with(expansion);
    sys.mem_write(0xA000, 0x37);
    sys.mem_write(0xBFFF, 0x38);
    assert_eq!(sys.mem_read(0xA000), 0x37);
    assert_eq!(sys.mem_read(0xBFFF), 0x38);

    let mut bare = machine_with(Vic20RamExpansion::NONE);
    bare.mem_write(0xA000, 0x37);
    assert_eq!(bare.mem_read(0xA000), 0xFF, "empty BLK5 is open bus");
}

#[test]
fn expansion_spec_round_trips_and_rejects_a_double_claim() {
    for (spec, expected) in [
        ("none", Vic20RamExpansion::NONE),
        ("3k", Vic20RamExpansion::EXP_3K),
        ("8k", Vic20RamExpansion::EXP_8K),
        ("16k", Vic20RamExpansion::EXP_16K),
        ("24k", Vic20RamExpansion::EXP_24K),
    ] {
        assert_eq!(Vic20RamExpansion::parse(spec), Ok(expected), "{spec}");
        assert_eq!(expected.to_string(), spec, "display of {spec}");
    }

    let module = Vic20RamExpansion::parse("3k+8k").expect("VIC-1010 combination");
    assert!(module.exp_3k && module.blk1);
    assert_eq!(module.to_string(), "3k+8k");
    assert_eq!(module.total_kib(), 11);

    let split = Vic20RamExpansion::parse("8k@2000+8k@6000").expect("two DIP-switched packs");
    assert_eq!(split.to_string(), "8k+8k@6000");
    assert!(
        !split.blk2,
        "BLK2 stays empty, so BASIC cannot follow past $3FFF"
    );

    let clash = Vic20RamExpansion::parse("8k+8k@2000").expect_err("same block twice");
    assert!(clash.contains("BLK1"), "{clash}");
    let unknown = Vic20RamExpansion::parse("32k").expect_err("no such cartridge");
    assert!(unknown.contains("unknown RAM expansion term"), "{unknown}");
}

#[test]
fn basic_start_follows_the_fitted_cartridges() {
    assert_eq!(Vic20RamExpansion::NONE.basic_start(), 0x1000);
    assert_eq!(Vic20RamExpansion::EXP_3K.basic_start(), 0x0400);
    assert_eq!(Vic20RamExpansion::EXP_8K.basic_start(), 0x1200);
    assert_eq!(Vic20RamExpansion::EXP_24K.basic_start(), 0x1200);
    // With both fitted the 3K area is outside the BASIC program area, so
    // BASIC still starts at $1200.
    let module = Vic20RamExpansion::parse("3k+8k").expect("valid spec");
    assert_eq!(module.basic_start(), 0x1200);
}

#[test]
fn cartridge_blocks_overlay_ram_and_blk5_and_can_be_removed() {
    fn crt(chips: &[(u16, u8)]) -> Vec<u8> {
        let mut bytes = Vec::from(*b"VIC20 CARTRIDGE ");
        bytes.extend_from_slice(&0x40u32.to_be_bytes());
        bytes.extend_from_slice(&0x0100u16.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&[0; 8 + 32]);
        for (address, fill) in chips {
            bytes.extend_from_slice(b"CHIP");
            bytes.extend_from_slice(&0x2010u32.to_be_bytes());
            bytes.extend_from_slice(&[0; 4]);
            bytes.extend_from_slice(&address.to_be_bytes());
            bytes.extend_from_slice(&0x2000u16.to_be_bytes());
            bytes.extend(std::iter::repeat_n(*fill, 0x2000));
        }
        bytes
    }

    let mut sys = make_vic20();
    sys.mem_write(0x2000, 0x11);
    let image = crt(&[(0x2000, 0x22), (0xA000, 0x33)]);
    sys.insert_cartridge_bytes(&image).expect("generic CRT");
    assert_eq!(sys.peek(0x2000), 0x22, "BLK1 cartridge overlays RAM");
    assert_eq!(sys.peek(0xA000), 0x33, "BLK5 cartridge replaces open bus");

    sys.remove_cartridge();
    assert_eq!(
        sys.peek(0x2000),
        0xFF,
        "unexpanded BLK1 returns to open bus"
    );
    assert_eq!(sys.peek(0xA000), 0xFF, "BLK5 returns to open bus");
}

#[test]
fn raw_blk5_autostart_signature_is_visible_to_the_kernal() {
    let mut sys = make_vic20();
    let mut image = vec![0; 0x2000];
    image[0..4].copy_from_slice(&[0x09, 0xA0, 0x09, 0xA0]);
    image[4..9].copy_from_slice(&[0x41, 0x30, 0xC3, 0xC2, 0xCD]);
    sys.insert_cartridge_bytes(&image).expect("raw BLK5 ROM");
    assert!(sys.cartridge_is_autostart());
    assert_eq!(&[sys.peek(0xA004), sys.peek(0xA005)], &[0x41, 0x30]);
}
#[test]
fn io_decode_mirrors_and_overlaps_reach_every_selected_chip() {
    check_io_decode_mirrors_and_overlaps_reach_every_selected_chip::<TestPal>();
    check_io_decode_mirrors_and_overlaps_reach_every_selected_chip::<TestNtsc>();
}

#[test]
fn io_decode_cpu_reads_acknowledge_all_selected_vias_but_peeks_do_not() {
    check_io_decode_cpu_reads_acknowledge_all_selected_vias_but_peeks_do_not::<TestPal>();
    check_io_decode_cpu_reads_acknowledge_all_selected_vias_but_peeks_do_not::<TestNtsc>();
}

#[test]
fn fixed_boards_reject_a_different_model_or_chip_raster() {
    let pal = make_vic20_model::<TestPal>();
    let bytes = postcard::to_allocvec(&pal).expect("PAL state");
    assert!(postcard::from_bytes::<Vic20Core<TestNtsc>>(&bytes).is_err());
    let ntsc = make_vic20_model::<TestNtsc>();
    let bytes = postcard::to_allocvec(&ntsc).expect("NTSC state");
    assert!(postcard::from_bytes::<Vic20Core<TestPal>>(&bytes).is_err());
    let mut wrong_chip = make_vic20_model::<TestPal>();
    wrong_chip.state.vic = Vic6560::new(false);
    let bytes = postcard::to_allocvec(&wrong_chip).expect("inconsistent state");
    assert!(postcard::from_bytes::<Vic20Core<TestPal>>(&bytes).is_err());
}
