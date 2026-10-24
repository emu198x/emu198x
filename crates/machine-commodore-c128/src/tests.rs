//! Machine-level tests on made-up ROMs; the real ROMs boot in `tests/`.

use super::*;

/// A ROM set whose Z80 BIOS hands the bus to the 8502, and whose KERNAL
/// stores `$42` at `$0400` and spins.
fn handover_roms() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut kernal = vec![0xEA; 0x4000];
    // Z80 BIOS, at the KERNAL ROM's second 4 KB: LD BC,$D505; LD A,1;
    // OUT (C),A; then spin.
    kernal[0x1000..0x1009].copy_from_slice(&[0x01, 0x05, 0xD5, 0x3E, 0x01, 0xED, 0x79, 0x18, 0xFE]);
    // KERNAL at $E000: LDA #$42; STA $0400; JMP $E005.
    kernal[0x2000..0x2008].copy_from_slice(&[0xA9, 0x42, 0x8D, 0x00, 0x04, 0x4C, 0x05, 0xE0]);
    kernal[0x3FFC] = 0x00;
    kernal[0x3FFD] = 0xE0;
    (kernal, vec![0; 0x4000], vec![0; 0x4000], vec![0; 0x2000])
}

fn machine(roms: &(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)) -> C128 {
    C128::new(&C128Roms {
        kernal: &roms.0,
        basic_lo: &roms.1,
        basic_hi: &roms.2,
        chargen: &roms.3,
    })
    .expect("ROMs")
}

#[test]
fn the_z80_boots_first_and_hands_the_bus_to_the_8502() {
    let mut c128 = machine(&handover_roms());
    assert!(!c128.running_8502(), "the Z80 has the bus at power-on");
    for _ in 0..200 {
        c128.tick();
    }
    assert!(c128.running_8502(), "the BIOS's OUT to $D505 hands it over");
    for _ in 0..200 {
        c128.tick();
    }
    assert_eq!(c128.ram(0x0400), 0x42, "the 8502 ran from its reset vector");
}

#[test]
fn a_rom_of_the_wrong_size_is_refused() {
    let mut roms = handover_roms();
    roms.0.truncate(0x2000);
    let err = C128::new(&C128Roms {
        kernal: &roms.0,
        basic_lo: &roms.1,
        basic_hi: &roms.2,
        chargen: &roms.3,
    })
    .err();
    assert_eq!(
        err,
        Some(RomError::WrongSize {
            name: "KERNAL",
            got: 0x2000,
            expected: 0x4000
        })
    );
}

#[test]
fn the_port_selects_colour_ram_banks_and_hides_the_character_rom() {
    let mut c128 = machine(&handover_roms());
    c128.memory.port_direction = 0x07;
    c128.memory.port_data = 0x00; // CPU bank 1, VIC bank 1, character ROM on
    c128.write_colour(0x10, 0x05);
    assert_eq!(c128.memory.colour[0x410], 0x05);
    assert_eq!(c128.memory.read_colour(0x10), 0x05);
    assert!(c128.memory.is_character_rom(0x1000));
    c128.memory.port_data = 0x04;
    assert!(!c128.memory.is_character_rom(0x1000));
}

#[test]
fn screen_codes_read_as_text() {
    assert_eq!(screen_code_char(0x12), 'R');
    assert_eq!(screen_code_char(0x2E), '.');
    assert_eq!(screen_code_char(0x37), '7');
}

#[test]
fn a_snapshot_mid_boot_resumes_in_lockstep() {
    let mut c128 = machine(&handover_roms());
    for _ in 0..37 {
        c128.tick(); // part-way through a Z80 instruction
    }
    let bytes = postcard::to_allocvec(&c128).expect("encode");
    let mut restored: C128 = postcard::from_bytes(&bytes).expect("decode");
    restored.finish_restore();
    for _ in 0..400 {
        c128.tick();
        restored.tick();
    }
    assert!(restored.running_8502());
    assert_eq!(restored.ram(0x0400), 0x42);
    assert_eq!(
        postcard::to_allocvec(&restored).expect("encode"),
        postcard::to_allocvec(&c128).expect("encode"),
        "the two machines ran identically"
    );
}

#[test]
fn an_ntsc_machine_runs_263_lines_of_65_cycles() {
    let roms = handover_roms();
    let mut c128 = C128::with_model(
        &C128Roms {
            kernal: &roms.0,
            basic_lo: &roms.1,
            basic_hi: &roms.2,
            chargen: &roms.3,
        },
        C128Model::Ntsc,
    )
    .expect("ROMs");
    assert_eq!(c128.model(), C128Model::Ntsc);
    assert_eq!(c128.timing(), C128Model::Ntsc.timing());
    c128.run_frame();
    assert_eq!(c128.run_frame(), 263 * 65);
    let mut pal = machine(&roms);
    pal.run_frame();
    assert_eq!(pal.run_frame(), 312 * 63);
}

#[test]
fn a_poke_lands_in_ram_under_io_without_touching_the_chips() {
    let mut c128 = machine(&handover_roms());
    let border = c128.vic_register(0x20);
    c128.poke(0xD020, border ^ 0x0F);
    assert_eq!(c128.vic_register(0x20), border, "the VIC never saw it");
    assert_eq!(c128.ram(0xD020), border ^ 0x0F, "the RAM underneath did");
    c128.poke(0x2000, 0x99);
    assert_eq!(c128.peek(0x2000), 0x99);
}

#[test]
fn a_basic_prg_moves_the_end_of_text_pointer() {
    let mut c128 = machine(&handover_roms());
    assert_eq!(c128.load_prg(&[0x01, 0x1C, 0xAA, 0xBB, 0xCC]), Ok(0x1C01));
    assert_eq!(c128.ram(0x1C01), 0xAA);
    assert_eq!(c128.ram(0x1C03), 0xCC);
    assert_eq!(
        (c128.ram(0x1210), c128.ram(0x1211)),
        (0x04, 0x1C),
        "end of text at $1C04"
    );
    assert_eq!(c128.load_prg(&[0x00, 0xC0, 0x01]), Ok(0xC000));
    assert_eq!(c128.ram(0x1210), 0x04, "not BASIC text, so untouched");
    assert!(c128.load_prg(&[0x01]).is_err());
    assert!(c128.load_prg(&[0xFF, 0xFF, 1, 2]).is_err());
}

#[test]
fn a_step_runs_one_8502_instruction() {
    let mut c128 = machine(&handover_roms());
    for _ in 0..400 {
        c128.tick();
    }
    assert!(c128.running_8502());
    let pc = c128.cpu().regs.pc;
    let cycles = c128.step_instruction();
    assert!((2..=8).contains(&cycles), "{cycles} cycles");
    assert_ne!(c128.cpu().regs.pc, pc);
    c128.cpu_mut().regs.a = 0x5A;
    assert_eq!(c128.cpu().regs.a, 0x5A);
    assert_eq!(c128.vic().raster_line(), c128.raster_line());
    assert_eq!(c128.vic().raster_cycle(), c128.cycle_in_line());
    assert_eq!(c128.audio_sample_rate(), 48_000);
    assert_eq!(c128.cia1().ddr_a(), c128.cia1().ddr_a());
    assert_eq!(c128.cia2().ddr_a(), c128.cia2().ddr_a());
}
