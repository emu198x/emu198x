/// .SNA snapshot format parser.
///
/// The simplest Spectrum snapshot format — fixed layout, no compression.
///
/// 48K: 27-byte header + 49152 bytes RAM = 49179 bytes total.
/// 128K: 49179 bytes (as 48K) + 4 bytes (PC + port $7FFD) + 5 × 16384 remaining banks.
///
/// The 48K format stores PC on the stack (SP points to it). The loader
/// must pop it to get the real PC and adjust SP += 2.
use super::{SnapshotModel, Z80Snapshot};

/// Parse a .SNA file.
pub fn parse_sna(data: &[u8]) -> Result<Z80Snapshot, String> {
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
        // 128K format: extra data after the 48K block
        let pc = read_u16(data, 49179);
        let port_7ffd = data[49181];
        // data[49182] = TR-DOS paged flag (ignored)

        // The 48K RAM block contains: bank 5 ($4000), bank 2 ($8000),
        // and the currently paged bank at $C000.
        let current_bank = (port_7ffd & 0x07) as usize;

        // Build pages: the first 48K gives us banks 5, 2, and current_bank
        let mut pages = Vec::new();
        pages.push((8, ram[..16384].to_vec())); // page 8 = bank 5 ($4000)
        pages.push((5, ram[16384..32768].to_vec())); // page 5 = bank 2 ($8000)
        pages.push(((current_bank as u8) + 3, ram[32768..49152].to_vec())); // current bank

        // Remaining 5 banks follow (all except 5, 2, and current_bank)
        let mut offset = 49183;
        for bank in 0..8u8 {
            if bank == 5 || bank == 2 || bank == current_bank as u8 {
                continue;
            }
            if offset + 16384 > data.len() {
                break;
            }
            pages.push((bank + 3, data[offset..offset + 16384].to_vec()));
            offset += 16384;
        }

        // AY registers: not stored in .SNA format
        Ok(Z80Snapshot {
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
        // 48K format: PC is on the stack
        let pc_lo = ram[(sp.wrapping_sub(0x4000)) as usize];
        let pc_hi = ram[(sp.wrapping_sub(0x4000).wrapping_add(1)) as usize];
        let pc = u16::from_le_bytes([pc_lo, pc_hi]);
        sp = sp.wrapping_add(2);

        let pages = vec![
            (8, ram[..16384].to_vec()),      // $4000-$7FFF
            (4, ram[16384..32768].to_vec()), // $8000-$BFFF
            (5, ram[32768..49152].to_vec()), // $C000-$FFFF
        ];

        Ok(Z80Snapshot {
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

        let snap = parse_sna(&data).unwrap();
        assert_eq!(snap.pc, 0x6000);
        assert_eq!(snap.sp, 0x8002); // SP adjusted +2
        assert_eq!(snap.i, 0x3F);
        assert_eq!(snap.af, 0xFF00);
        assert_eq!(snap.border, 7);
        assert_eq!(snap.model, SnapshotModel::Spectrum48K);
    }
}
