//! The final odd input cell can carry a Copper word, including across wrap.
use machine_commodore_amiga_ocs::{AmigaOcs, RamConfig};

fn parked_rom() -> Vec<u8> {
    let mut rom = vec![0; 512 * 1024];
    rom[..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8..10].copy_from_slice(&0x60FEu16.to_be_bytes());
    rom
}

#[test]
fn last_odd_copper_request_retires_before_or_after_wrap_by_instruction_phase() {
    // Registered generate_copper admits requests on every free odd cell.
    // WAIT $DC: compare $DD, IR1 $DF, IR2 $E1, MOVE service $E2.
    // WAIT $DE: compare $DF, IR1 $E1, IR2 next-line $01, service $02.
    for (ntsc, wait_hp, expected) in [
        (false, 0xD8, (1, 0xDE)),
        (false, 0xDC, (1, 0xE2)),
        (false, 0xDE, (2, 2)),
        (true, 0xD8, (1, 0xDE)),
        (true, 0xDC, (1, 0xE2)),
        (true, 0xDE, (2, 0)),
    ] {
        let mut amiga = if ntsc {
            AmigaOcs::with_ram_config_ntsc(parked_rom(), RamConfig::default())
        } else {
            AmigaOcs::new(parked_rom())
        };
        let words = [0x0101 | wait_hp, 0xFFFE, 0x0180, 0x0F00, 0xFFFF, 0xFFFE];
        for (index, word) in words.into_iter().enumerate() {
            amiga.poke_word(0x1000 + index as u32 * 2, word);
        }
        amiga.poke_word(0x00DF_F080, 0);
        amiga.poke_word(0x00DF_F082, 0x1000);
        amiga.poke_word(0x00DF_F088, 0);
        amiga.poke_word(0x00DF_F096, 0x8280);
        for _ in 0..1_400 {
            if amiga.color(0) == 0x0F00 {
                break;
            }
            amiga.tick();
        }
        assert_eq!(amiga.color(0), 0x0F00, "MOVE must execute");
        let writes: Vec<_> = amiga
            .debug_copper_move_log
            .iter()
            .filter(|entry| entry.3 == 0x180)
            .map(|entry| (entry.1, entry.2))
            .collect();
        assert_eq!(writes, [expected], "NTSC={ntsc}, WAIT HP={wait_hp:02x}");
    }
}
