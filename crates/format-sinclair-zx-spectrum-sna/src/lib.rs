/// .SNA snapshot format parser.
///
/// The simplest Spectrum snapshot format — fixed layout, no compression.
///
/// 48K: 27-byte header + 49152 bytes RAM = 49179 bytes total.
/// 128K: 49179 bytes (as 48K) + 4 bytes (PC, port $7FFD, TR-DOS flag)
/// plus 5 × 16384 remaining banks, or 6 banks when bank 2 or 5 is paged at $C000.
/// See <https://worldofspectrum.org/faq/reference/formats.htm>.
///
/// The 48K format stores PC on the stack (SP points to it). The loader
/// must pop it to get the real PC and adjust SP += 2.
use format_sinclair_zx_spectrum_snapshot::{Snapshot, SnapshotModel};

/// Parse a .SNA file.
pub fn parse_sna(data: &[u8]) -> Result<Snapshot, String> {
    if data.len() < 49179 {
        return Err(format!(
            ".SNA too short: {} bytes (need at least 49179)",
            data.len()
        ));
    }

    let i = data[0];
    let hl_alt = read_u16(data, 1);
    let de_alt = read_u16(data, 3);
    let bc_alt = read_u16(data, 5);
    let af_alt = read_u16(data, 7);
    let hl = read_u16(data, 9);
    let de = read_u16(data, 11);
    let bc = read_u16(data, 13);
    let iy = read_u16(data, 15);
    let ix = read_u16(data, 17);
    let iff2 = data[19] & 0x04 != 0;
    let r = data[20];
    let af = read_u16(data, 21);
    let mut sp = read_u16(data, 23);
    let im = data[25];
    let border = data[26];

    let ram = &data[27..27 + 49152];

    if data.len() > 49179 {
        // Check the entire extension before reading PC and the paging register.
        if data.len() < 49183 {
            return Err(format!(
                ".SNA 128K extension too short: {} bytes (need at least 49183)",
                data.len()
            ));
        }
        let pc = read_u16(data, 49179);
        let port_7ffd = data[49181];
        // data[49182] = TR-DOS paged flag (ignored)

        // The 48K RAM block contains: bank 5 ($4000), bank 2 ($8000),
        // and the currently paged bank at $C000.
        let current_bank = (port_7ffd & 0x07) as usize;
        // Bank 2 or 5 appears twice in the first 48K, leaving six other banks.
        let remaining_banks = if current_bank == 2 || current_bank == 5 {
            6
        } else {
            5
        };
        let required_length = 49183 + remaining_banks * 16384;
        if data.len() < required_length {
            return Err(format!(
                ".SNA 128K RAM too short: {} bytes (need at least {required_length} for current bank {current_bank})",
                data.len()
            ));
        }

        // Build pages: the first 48K gives us banks 5, 2, and current_bank
        let mut pages = Vec::new();
        pages.push((8, ram[..16384].to_vec())); // page 8 = bank 5 ($4000)
        pages.push((5, ram[16384..32768].to_vec())); // page 5 = bank 2 ($8000)
        pages.push(((current_bank as u8) + 3, ram[32768..49152].to_vec())); // current bank

        // Remaining banks follow (all except 5, 2, and current_bank)
        let mut offset = 49183;
        for bank in 0..8u8 {
            if bank == 5 || bank == 2 || bank == current_bank as u8 {
                continue;
            }
            pages.push((bank + 3, data[offset..offset + 16384].to_vec()));
            offset += 16384;
        }

        // AY registers: not stored in .SNA format
        Ok(Snapshot {
            af,
            bc,
            de,
            hl,
            af_alt,
            bc_alt,
            de_alt,
            hl_alt,
            ix,
            iy,
            sp,
            pc,
            i,
            r,
            im,
            iff1: iff2, // .SNA only stores IFF2
            iff2,
            border,
            model: SnapshotModel::Spectrum128K,
            port_7ffd,
            port_1ffd: 0,
            ay_register: 0,
            ay_regs: [0; 16],
            pages,
        })
    } else {
        // 48K format: both stacked PC bytes must be present in the RAM dump.
        if !(0x4000..=0xFFFE).contains(&sp) {
            return Err(format!(
                ".SNA 48K stacked PC outside saved RAM: SP=${sp:04X}"
            ));
        }
        let stack_offset = usize::from(sp - 0x4000);
        let pc_lo = ram[stack_offset];
        let pc_hi = ram[stack_offset + 1];
        let pc = u16::from_le_bytes([pc_lo, pc_hi]);
        sp = sp.wrapping_add(2);

        let pages = vec![
            (8, ram[..16384].to_vec()),      // $4000-$7FFF
            (4, ram[16384..32768].to_vec()), // $8000-$BFFF
            (5, ram[32768..49152].to_vec()), // $C000-$FFFF
        ];

        Ok(Snapshot {
            af,
            bc,
            de,
            hl,
            af_alt,
            bc_alt,
            de_alt,
            hl_alt,
            ix,
            iy,
            sp,
            pc,
            i,
            r,
            im,
            iff1: iff2,
            iff2,
            border,
            model: SnapshotModel::Spectrum48K,
            port_7ffd: 0,
            port_1ffd: 0,
            ay_register: 0,
            ay_regs: [0; 16],
            pages,
        })
    }
}

fn read_u16(data: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([data[pos], data[pos + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_48k_sna() {
        let mut data = vec![0u8; 49179];
        // I register
        data[0] = 0x3F;
        // AF at offset 21-22
        data[21] = 0x00; // F
        data[22] = 0xFF; // A
        // SP at offset 23-24 = $8000
        data[23] = 0x00;
        data[24] = 0x80;
        // IM at offset 25
        data[25] = 1;
        // Border at offset 26
        data[26] = 7;
        // RAM starts at 27. Put PC=$6000 on the stack at $8000 (offset $8000-$4000=0x4000 in RAM)
        data[27 + 0x4000] = 0x00; // PC low
        data[27 + 0x4001] = 0x60; // PC high

        let snap = parse_sna(&data).expect("the sna fixture parses");
        assert_eq!(snap.pc, 0x6000);
        assert_eq!(snap.sp, 0x8002); // SP adjusted +2
        assert_eq!(snap.i, 0x3F);
        assert_eq!(snap.af, 0xFF00);
        assert_eq!(snap.border, 7);
        assert_eq!(snap.model, SnapshotModel::Spectrum48K);
    }

    #[test]
    fn parse_sna_rejects_short_buffer() {
        let data = vec![0u8; 49178];
        let err = parse_sna(&data).expect_err("the malformed sna fixture is rejected");
        assert!(err.contains("too short"));
    }

    #[test]
    fn parse_48k_sna_decodes_full_register_file() {
        let mut data = vec![0u8; 49179];
        data[0] = 0x12; // I
        // HL'=0x3344
        data[1] = 0x44;
        data[2] = 0x33;
        // DE'=0x5566
        data[3] = 0x66;
        data[4] = 0x55;
        // BC'=0x7788
        data[5] = 0x88;
        data[6] = 0x77;
        // AF'=0x99AA (LE in .SNA, unlike .Z80)
        data[7] = 0xAA;
        data[8] = 0x99;
        // HL=0xBBCC
        data[9] = 0xCC;
        data[10] = 0xBB;
        // DE=0xDDEE
        data[11] = 0xEE;
        data[12] = 0xDD;
        // BC=0xFF11
        data[13] = 0x11;
        data[14] = 0xFF;
        // IY=0x2233
        data[15] = 0x33;
        data[16] = 0x22;
        // IX=0x4455
        data[17] = 0x55;
        data[18] = 0x44;
        // IFF (only IFF2 stored): bit 2 set → iff2=true
        data[19] = 0x04;
        // R
        data[20] = 0xC0;
        // AF=0x6677
        data[21] = 0x77;
        data[22] = 0x66;
        // SP at $4002 → PC bytes are at ram[2], ram[3]
        data[23] = 0x02;
        data[24] = 0x40;
        // IM=2, border=3
        data[25] = 2;
        data[26] = 3;
        // PC bytes at SP=$4002 → ram[2..4]
        data[27 + 2] = 0xCD;
        data[27 + 3] = 0xAB;

        let snap = parse_sna(&data).expect("the sna fixture parses");
        assert_eq!(snap.i, 0x12);
        assert_eq!(snap.hl_alt, 0x3344);
        assert_eq!(snap.de_alt, 0x5566);
        assert_eq!(snap.bc_alt, 0x7788);
        assert_eq!(snap.af_alt, 0x99AA);
        assert_eq!(snap.hl, 0xBBCC);
        assert_eq!(snap.de, 0xDDEE);
        assert_eq!(snap.bc, 0xFF11);
        assert_eq!(snap.iy, 0x2233);
        assert_eq!(snap.ix, 0x4455);
        assert!(snap.iff2);
        assert!(snap.iff1); // .SNA only stores IFF2; iff1 mirrors it.
        assert_eq!(snap.r, 0xC0);
        assert_eq!(snap.af, 0x6677);
        assert_eq!(snap.pc, 0xABCD);
        assert_eq!(snap.sp, 0x4004); // bumped by 2 after popping PC
        assert_eq!(snap.im, 2);
        assert_eq!(snap.border, 3);
        assert_eq!(snap.model, SnapshotModel::Spectrum48K);
        // 48K split into pages 8/4/5
        assert_eq!(snap.pages.len(), 3);
        assert_eq!(snap.pages[0].0, 8);
        assert_eq!(snap.pages[1].0, 4);
        assert_eq!(snap.pages[2].0, 5);
        // No 128K state on a 48K snapshot.
        assert_eq!(snap.port_7ffd, 0);
        assert_eq!(snap.port_1ffd, 0);
        assert_eq!(snap.ay_register, 0);
        assert_eq!(snap.ay_regs, [0; 16]);
    }

    #[test]
    fn parse_128k_sna_layout_and_paging() {
        // 128K .SNA = 49179 + 4 + 5*16384 = 131103 bytes total.
        let total = 49179 + 4 + 5 * 16384;
        let mut data = vec![0u8; total];
        // Mark the three 48K-block banks with sentinel values so we can
        // verify the bank-5 / bank-2 / current-bank assignment.
        data[27] = 0x55; // first byte of $4000 → bank 5
        data[27 + 16384] = 0x22; // first byte of $8000 → bank 2
        data[27 + 32768] = 0xCC; // first byte of $C000 → current bank

        // PC at offset 49179 = $5678
        data[49179] = 0x78;
        data[49180] = 0x56;
        // port_7ffd at 49181 — current bank = 4 (bits 0-2)
        data[49181] = 0b0000_0100;
        // 49182 = TR-DOS flag (ignored)

        // Five remaining banks at offset 49183, 16384 bytes each. Tag the
        // first byte of each so we can assert ordering.
        let banks = [0, 1, 3, 6, 7]; // banks NOT in {2, 5, current=4}
        for (i, b) in banks.iter().enumerate() {
            data[49183 + i * 16384] = 0xB0 | b;
        }

        let snap = parse_sna(&data).expect("the sna fixture parses");
        assert_eq!(snap.model, SnapshotModel::Spectrum128K);
        assert_eq!(snap.pc, 0x5678);
        assert_eq!(snap.port_7ffd, 0b0000_0100);
        assert_eq!(snap.sp, 0x0000); // 48K-block SP path is skipped on 128K
        // 128K format gives us 3 from the 48K block + 5 trailing = 8 pages.
        assert_eq!(snap.pages.len(), 8);
        // Page numbering: 48K block contributes (8, bank5), (5, bank2), (current+3, currentbank).
        assert_eq!(snap.pages[0].0, 8);
        assert_eq!(snap.pages[0].1[0], 0x55);
        assert_eq!(snap.pages[1].0, 5);
        assert_eq!(snap.pages[1].1[0], 0x22);
        // current_bank=4 → page = 4+3 = 7
        assert_eq!(snap.pages[2].0, 7);
        assert_eq!(snap.pages[2].1[0], 0xCC);
        // Trailing five pages are bank+3 in iteration order, skipping 5/2/4.
        let trailing: Vec<u8> = snap.pages[3..].iter().map(|(p, _)| *p).collect();
        assert_eq!(trailing, vec![3, 4, 6, 9, 10]); // (0,1,3,6,7) + 3
    }

    #[test]
    fn parse_128k_sna_rejects_incomplete_extension() {
        for length in 49180..=49183 {
            let data = vec![0; length];
            assert!(parse_sna(&data).is_err(), "accepted {length} bytes");
        }
    }

    #[test]
    fn parse_128k_sna_rejects_incomplete_banks() {
        for current_bank in 0..8u8 {
            let data = sna_128k(current_bank);
            // Check the first byte and final byte of every trailing bank.
            for boundary in (49183..data.len()).step_by(16384) {
                for length in [boundary, boundary + 16383] {
                    assert!(
                        parse_sna(&data[..length]).is_err(),
                        "accepted {length} bytes with current bank {current_bank}"
                    );
                }
            }
        }
    }

    #[test]
    fn parse_48k_sna_rejects_pc_outside_saved_ram() {
        for sp in [0u16, 0x3FFF, 0xFFFF] {
            let mut data = vec![0; 49179];
            data[23..25].copy_from_slice(&sp.to_le_bytes());
            assert!(parse_sna(&data).is_err(), "accepted SP {sp:04X}");
        }
    }

    #[test]
    fn parse_48k_sna_accepts_stack_boundaries() {
        for sp in [0x4000u16, 0xFFFE] {
            let mut data = vec![0; 49179];
            data[23..25].copy_from_slice(&sp.to_le_bytes());
            let offset = 27 + usize::from(sp - 0x4000);
            data[offset..offset + 2].copy_from_slice(&0xABCDu16.to_le_bytes());
            let snap = parse_sna(&data).expect("both PC bytes are in RAM");
            assert_eq!(snap.pc, 0xABCD);
            assert_eq!(snap.sp, sp.wrapping_add(2));
        }
    }

    #[test]
    fn parse_128k_sna_preserves_all_banks_for_every_page_selection() {
        for current_bank in 0..8u8 {
            let data = sna_128k(current_bank);
            let snap = parse_sna(&data).expect("complete 128K snapshot");
            assert_eq!(snap.model, SnapshotModel::Spectrum128K);
            assert_eq!(snap.port_7ffd, current_bank);
            assert_eq!(snap.pc, 0x8765);
            for bank in 0..8u8 {
                let pages: Vec<_> = snap
                    .pages
                    .iter()
                    .filter(|(page, _)| *page == bank + 3)
                    .collect();
                assert!(!pages.is_empty(), "missing bank {bank}");
                for (_, bytes) in pages {
                    assert_eq!(bytes.as_slice(), vec![bank; 16384]);
                }
            }
        }
    }

    #[test]
    fn parse_128k_sna_keeps_trailing_byte_tolerance() {
        let mut data = sna_128k(0);
        data.extend_from_slice(&[0xAA, 0x55]);
        let snap = parse_sna(&data).expect("existing trailing-byte tolerance");
        assert_eq!(snap.pages.len(), 8);
    }

    fn sna_128k(current_bank: u8) -> Vec<u8> {
        let mut data = vec![0; 27];
        for bank in [5, 2, current_bank] {
            data.extend(std::iter::repeat_n(bank, 16384));
        }
        data.extend_from_slice(&[0x65, 0x87, current_bank, 0]);
        for bank in 0..8u8 {
            if bank != 5 && bank != 2 && bank != current_bank {
                data.extend(std::iter::repeat_n(bank, 16384));
            }
        }
        data
    }
}
