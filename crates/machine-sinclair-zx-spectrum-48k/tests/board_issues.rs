//! The 48K's board issues, told apart the way software could tell them apart.
//!
//! - Issue 1: 5C ULA, no "spider", so every I/O cycle contends.
//! - Issue 2: 5C ULA, with the spider.
//! - Issue 3 (`Spectrum48k`): 6C ULA, with the spider.
//!
//! Smith, *The ZX Spectrum ULA*: ch. 24 p. 250 for the spider, Table 20-2
//! p. 220 for the EAR input's bias on the two ULA families.

use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use common_sinclair_zx_spectrum::memory::{MemoryBus, Spectrum48kMemory};
use common_sinclair_zx_spectrum_48k_class::{SpectrumMachineCore, Variant48kClass};
use machine_sinclair_zx_spectrum_48k::{
    Spectrum48k, Spectrum48kIssue1, Spectrum48kIssue2, UlaRevision,
};

const FRAME_TSTATES: u32 = 69_888;
/// Uncontended RAM, so the only contention an `IN` meets is the I/O kind.
const CODE: u16 = 0x8000;

/// T-states one frame's worth of `IN A,(C)` takes with `BC` = `port`,
/// starting at the frame boundary, with the code in uncontended RAM.
fn frame_of_ins<V: Variant48kClass>(
    mut machine: SpectrumMachineCore<Spectrum48kMemory, V>,
    port: u16,
) -> (u32, u32) {
    machine.reset();
    for i in 0..0x4000u16 / 2 {
        // ED 78 — IN A,(C). The port's high byte comes from B, which the
        // read doesn't change, so every pass reads the same port class.
        machine.memory_mut().write(CODE + 2 * i, 0xED);
        machine.memory_mut().write(CODE + 2 * i + 1, 0x78);
    }
    while machine.tstate_in_frame() != 0 {
        machine.advance_tstates(1);
    }
    machine.z80_mut().regs.pc = CODE;
    machine.z80_mut().regs.bc = port;
    let start = machine.z80().instructions_retired();
    let mut spent = 0;
    while spent < FRAME_TSTATES {
        machine.advance_tstates(1);
        spent += 1;
    }
    let ins = (machine.z80().instructions_retired() - start) as u32;
    (ins, spent)
}

/// How many `IN A,(C)`s fit in one frame on each board, for the even ULA
/// port `$00FE` and the odd port `$00FF`, which nothing on a bare 48K
/// answers.
#[test]
fn without_the_spider_every_port_is_held_up_like_the_ulas() {
    let issue1 = |p| frame_of_ins(Spectrum48kIssue1::with_revision(UlaRevision::Ferranti5C), p).0;
    let issue2 = |p| frame_of_ins(Spectrum48kIssue2::with_revision(UlaRevision::Ferranti5C), p).0;
    let issue3 = |p| frame_of_ins(Spectrum48k::with_revision(UlaRevision::Ferranti6C), p).0;

    // With the spider, an odd port is never held up: 12 T-states each.
    assert_eq!(issue2(0x00FF), FRAME_TSTATES / 12, "Issue 2, odd port");
    assert_eq!(issue3(0x00FF), FRAME_TSTATES / 12, "Issue 3, odd port");
    // The ULA's own port is held up during the picture on every board.
    assert!(issue2(0x00FE) < issue2(0x00FF), "Issue 2: $FE is contended");
    assert_eq!(
        issue1(0x00FE),
        issue2(0x00FE),
        "the ULA's port costs the same on both 5C boards"
    );
    // Without the spider, the odd port costs exactly what the ULA's does.
    assert_eq!(
        issue1(0x00FF),
        issue1(0x00FE),
        "Issue 1: $FF is held up like $FE"
    );
    assert!(
        issue1(0x00FF) < issue2(0x00FF),
        "Issue 1: $FF is slower than on the Issue 2"
    );
}

/// Port 254 bit 6 after `OUT (254),8`, MIC alone set and no tape. The 5C ULA's
/// EAR input then sits above its switching point and reads 1; the 6C's sits
/// below it and reads 0.
#[test]
fn bit_6_follows_mic_on_the_issue_1_and_2_not_the_issue_3() {
    let bit6 = |value: u8| value & 0x40;
    let mut issue1 = Spectrum48kIssue1::with_revision(UlaRevision::Ferranti5C);
    let mut issue2 = Spectrum48kIssue2::with_revision(UlaRevision::Ferranti5C);
    let mut issue3 = Spectrum48k::with_revision(UlaRevision::Ferranti6C);
    issue1.write_fe(0x08);
    issue2.write_fe(0x08);
    issue3.write_fe(0x08);
    assert_eq!(bit6(issue1.read_fe(0x00FE)), 0x40, "Issue 1");
    assert_eq!(bit6(issue2.read_fe(0x00FE)), 0x40, "Issue 2");
    assert_eq!(bit6(issue3.read_fe(0x00FE)), 0x00, "Issue 3");
}

/// What *Sinclair User* 26 (May 1984) told Issue 3 owners about a type-in
/// game that tested `IN 65278<>255`: "alter all the 255s to 191". From the
/// real ROM, once a key has been pressed and let go, with no tape, the Issue 1
/// and Issue 2 read 255 from that port and the Issue 3 reads 191, bit 6 clear.
///
/// The key matters: the key click goes through the ROM's BEEPER routine
/// (03B5), whose `OR +08` leaves MIC's bit set in the last write to port 254
/// (Logan and O'Hara, *The Complete Spectrum ROM Disassembly*), and the 5C's
/// bit 6 follows MIC. Straight after power-on, before any key, all three
/// boards read 191.
#[test]
#[ignore = "FIXTURE: needs EMU198X_SPECTRUM_48K_ROM"]
fn after_a_key_the_rom_reads_255_on_the_5c_boards_and_191_on_the_issue_3() {
    let rom = std::fs::read(std::env::var("EMU198X_SPECTRUM_48K_ROM").expect("ROM path"))
        .expect("48K ROM");
    fn booted<V: Variant48kClass>(
        mut m: SpectrumMachineCore<Spectrum48kMemory, V>,
        rom: &[u8],
    ) -> u8 {
        m.load_rom_bytes(rom).expect("48K ROM should load");
        m.reset();
        m.advance_tstates(FRAME_TSTATES * 200);
        let untouched = m.read_fe(65278);
        assert_eq!(untouched, 191, "before any key, every board reads 191");
        m.keyboard_mut().set_key(SpectrumKey::A, true);
        m.advance_tstates(FRAME_TSTATES * 5);
        m.keyboard_mut().set_key(SpectrumKey::A, false);
        m.advance_tstates(FRAME_TSTATES * 5);
        m.read_fe(65278)
    }
    assert_eq!(
        booted(
            Spectrum48kIssue1::with_revision(UlaRevision::Ferranti5C),
            &rom
        ),
        255
    );
    assert_eq!(
        booted(
            Spectrum48kIssue2::with_revision(UlaRevision::Ferranti5C),
            &rom
        ),
        255
    );
    assert_eq!(
        booted(Spectrum48k::with_revision(UlaRevision::Ferranti6C), &rom),
        191
    );
}
