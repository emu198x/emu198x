//! CPU-executed mapper-66 smoke. Original synthetic ROM, no external firmware.
use common_nintendo_nes::Nes;
use nintendo_nes_cartridge::parse_ines;

#[path = "common/gxrom_fixture.rs"]
mod fixture;
use fixture::cartridge;

#[test]
fn guest_reads_every_prg_chr_pair_and_conflicted_write() {
    let parsed = parse_ines(&cartridge()).expect("mapper 66 loads");
    let mut machine = Nes::new(parsed.mapper);
    for _ in 0..2_000_000 {
        machine.tick();
        if machine.peek(0x3FF) == 0xA5 {
            break;
        }
    }
    assert_eq!(machine.peek(0x3FF), 0xA5, "guest did not finish");
    for prg in 0..4u8 {
        for chr in 0..4u8 {
            let slot = 0x300 + u16::from(prg * 4 + chr);
            assert_eq!(machine.peek(slot), 0x80 + prg, "PRG {prg}, CHR {chr}");
            assert_eq!(
                machine.peek(slot + 0x40),
                0xA0 + chr,
                "PRG {prg}, CHR {chr}"
            );
        }
    }
    assert_eq!([machine.peek(0x320), machine.peek(0x360)], [0x81, 0xA1]);
    assert_eq!([machine.peek(0x321), machine.peek(0x361)], [0x82, 0xA2]);
}
