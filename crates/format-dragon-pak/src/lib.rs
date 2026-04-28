//! Dragon cartridge, DGN, and PC-Dragon PAK image normalisation.
//!
//! XRoar treats non-256-byte-aligned ROM images as having a small leading
//! header whose length is `file_size % 256`. Cartridge parsing applies the
//! same skip. PC-Dragon `.pak` files are snapshots, not cartridges, and are
//! parsed through the snapshot path.

use thiserror::Error;

/// Maximum size XRoar keeps in a single cartridge ROM image.
pub const MAX_CART_ROM_SIZE: usize = 0x40000;
const ROM_CART_LIMIT: usize = 0x4000;

/// Dragon cartridge hardware model inferred from an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragonCartridgeKind {
    /// Plain ROM cartridge mapped at `$C000-$FEFF`.
    Rom,
    /// Games Master Cartridge style banked cartridge.
    GamesMaster,
}

/// Parsed Dragon cartridge image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragonPakImage {
    /// Inferred cartridge hardware model.
    pub kind: DragonCartridgeKind,
    /// Offset skipped before ROM data.
    pub header_offset: usize,
    /// Normalised ROM bytes.
    pub rom: Box<[u8]>,
}

/// PC-Dragon snapshot register state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcDragonRegisters {
    /// Program counter.
    pub pc: u16,
    /// X index register.
    pub x: u16,
    /// Y index register.
    pub y: u16,
    /// U stack pointer.
    pub u: u16,
    /// S stack pointer.
    pub s: u16,
    /// Direct page register.
    pub dp: u8,
    /// B accumulator.
    pub b: u8,
    /// A accumulator.
    pub a: u8,
    /// Condition-code register.
    pub cc: u8,
}

/// Snapshot peripheral bytes captured from PC-Dragon's V1.4 display state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcDragonPeripherals {
    /// Stored value for `$FF02`, Dragon PIA0 port B.
    pub ff02: u8,
    /// Stored value for `$FF03`, Dragon PIA0 port B control.
    pub ff03: u8,
    /// Stored value for `$FF22`, Dragon PIA1 port B.
    pub ff22: u8,
}

/// Parsed PC-Dragon `.pak` snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcDragonSnapshot {
    /// RAM bytes starting at `load_address`.
    pub ram: Box<[u8]>,
    /// Address where the RAM dump starts.
    pub load_address: u16,
    /// Register state captured by the snapshot.
    pub registers: PcDragonRegisters,
    /// PIA/display control state captured by the snapshot when present.
    pub peripherals: Option<PcDragonPeripherals>,
    /// Display base captured by the snapshot when present.
    pub display_base: Option<u16>,
}

/// Error returned when a cartridge image cannot be normalised.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DragonPakParseError {
    /// No cartridge data was present after any header skip.
    #[error("cartridge image is empty")]
    Empty,
    /// The cartridge image is larger than the supported XRoar-compatible limit.
    #[error("cartridge image is too large: {size} bytes")]
    TooLarge {
        /// Image size in bytes before header stripping.
        size: usize,
    },
    /// Snapshot header or payload is truncated.
    #[error("snapshot image is truncated")]
    TruncatedSnapshot,
    /// Snapshot compression method is not supported.
    #[error("unsupported snapshot compression method {method}")]
    UnsupportedCompression {
        /// Compression method byte.
        method: u8,
    },
}

/// Parse a Dragon cartridge or PAK image.
///
/// # Errors
///
/// Returns an error when the image is empty after header stripping or exceeds
/// XRoar's 256 KiB automatic cartridge size limit.
pub fn parse_dragon_pak(bytes: &[u8]) -> Result<DragonPakImage, DragonPakParseError> {
    if bytes.len() > MAX_CART_ROM_SIZE {
        return Err(DragonPakParseError::TooLarge { size: bytes.len() });
    }

    let header_offset = xroar_header_offset(bytes.len());
    let rom = bytes
        .get(header_offset..)
        .filter(|rom| !rom.is_empty())
        .ok_or(DragonPakParseError::Empty)?;
    let kind = if bytes.len() > ROM_CART_LIMIT {
        DragonCartridgeKind::GamesMaster
    } else {
        DragonCartridgeKind::Rom
    };

    Ok(DragonPakImage {
        kind,
        header_offset,
        rom: rom.into(),
    })
}

/// Parse a PC-Dragon `.pak` snapshot.
///
/// # Errors
///
/// Returns an error when the memory dump, compressed stream, or register block
/// is truncated, or when the snapshot uses an unknown compression algorithm.
pub fn parse_pcdragon_snapshot(bytes: &[u8]) -> Result<PcDragonSnapshot, DragonPakParseError> {
    let mut offset = 0;
    let dump_len = read_u16_le(bytes, &mut offset)? as usize;
    let mut load_address = read_u16_le(bytes, &mut offset)?;
    let dump_len = if dump_len == 0 { 0x10000 } else { dump_len };
    let mut compressed = false;
    let mut zerocode = 0;
    let mut ffcode = 0;
    let mut othercode = 0;

    if load_address == 0xfff5 {
        compressed = true;
        load_address = read_u16_le(bytes, &mut offset)?;
        let _version = read_u8(bytes, &mut offset)?;
        let algorithm = read_u8(bytes, &mut offset)?;
        if algorithm != 0 {
            return Err(DragonPakParseError::UnsupportedCompression { method: algorithm });
        }
        zerocode = read_u8(bytes, &mut offset)?;
        ffcode = read_u8(bytes, &mut offset)?;
        othercode = read_u8(bytes, &mut offset)?;
    }

    let mut ram = Vec::with_capacity(dump_len.min(0x10000));
    while ram.len() < dump_len {
        let byte = read_u8(bytes, &mut offset)?;
        if compressed && (byte == zerocode || byte == ffcode || byte == othercode) {
            let repeat = usize::from(read_u8(bytes, &mut offset)?) + 1;
            let value = if byte == zerocode {
                0
            } else if byte == ffcode {
                0xff
            } else {
                read_u8(bytes, &mut offset)?
            };
            let remaining = dump_len - ram.len();
            ram.extend(std::iter::repeat_n(value, repeat.min(remaining)));
        } else {
            ram.push(byte);
        }
    }

    let info = bytes
        .get(offset..)
        .ok_or(DragonPakParseError::TruncatedSnapshot)?;
    let registers = parse_snapshot_registers(info)?;
    let peripherals = parse_snapshot_peripherals(info);
    let display_base = parse_snapshot_display_base(info);

    Ok(PcDragonSnapshot {
        ram: ram.into(),
        load_address,
        registers,
        peripherals,
        display_base,
    })
}

fn xroar_header_offset(file_size: usize) -> usize {
    if file_size > 256 && !file_size.is_multiple_of(256) {
        file_size % 256
    } else {
        0
    }
}

fn parse_snapshot_registers(info: &[u8]) -> Result<PcDragonRegisters, DragonPakParseError> {
    const NAME_LEN: usize = 33;
    const REG_LEN: usize = 14;
    let regs = info
        .get(NAME_LEN..NAME_LEN + REG_LEN)
        .ok_or(DragonPakParseError::TruncatedSnapshot)?;
    let cc = info.get(54).copied().unwrap_or(0);
    Ok(PcDragonRegisters {
        pc: u16::from_le_bytes([regs[0], regs[1]]),
        x: u16::from_le_bytes([regs[2], regs[3]]),
        y: u16::from_le_bytes([regs[4], regs[5]]),
        u: u16::from_le_bytes([regs[6], regs[7]]),
        s: u16::from_le_bytes([regs[8], regs[9]]),
        dp: regs[11],
        b: regs[12],
        a: regs[13],
        cc,
    })
}

fn parse_snapshot_display_base(info: &[u8]) -> Option<u16> {
    // Archived PC-Dragon V1.4 PAKs store the display page in 256-byte units,
    // not a byte address.
    let offset = info.get(62..64)?;
    Some(u16::from_le_bytes([offset[0], offset[1]]) << 8)
}

fn parse_snapshot_peripherals(info: &[u8]) -> Option<PcDragonPeripherals> {
    // This matches the observed V1.4 layout in the local PC-Dragon archive:
    // the useful display control bytes straddle p5c/p5d as FF02, FF03, FF22.
    let display_state = info.get(58..61)?;
    Some(PcDragonPeripherals {
        ff02: display_state[0],
        ff03: display_state[1],
        ff22: display_state[2],
    })
}

fn read_u8(bytes: &[u8], offset: &mut usize) -> Result<u8, DragonPakParseError> {
    let value = bytes
        .get(*offset)
        .copied()
        .ok_or(DragonPakParseError::TruncatedSnapshot)?;
    *offset += 1;
    Ok(value)
}

fn read_u16_le(bytes: &[u8], offset: &mut usize) -> Result<u16, DragonPakParseError> {
    let lo = read_u8(bytes, offset)?;
    let hi = read_u8(bytes, offset)?;
    Ok(u16::from_le_bytes([lo, hi]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_short_image_is_plain_rom_without_header() {
        let bytes = vec![0x42; 0x4000];
        let image = parse_dragon_pak(&bytes).expect("aligned cart should parse");

        assert_eq!(image.kind, DragonCartridgeKind::Rom);
        assert_eq!(image.header_offset, 0);
        assert_eq!(image.rom.len(), 0x4000);
        assert_eq!(image.rom[0], 0x42);
    }

    #[test]
    fn odd_sized_pak_skips_xroar_header_and_uses_gmc() {
        let bytes = vec![0x35; 0x7f7a];
        let image = parse_dragon_pak(&bytes).expect("headered PAK should parse");

        assert_eq!(image.kind, DragonCartridgeKind::GamesMaster);
        assert_eq!(image.header_offset, 0x7a);
        assert_eq!(image.rom.len(), 0x7f00);
    }

    #[test]
    fn empty_image_is_rejected() {
        let err = parse_dragon_pak(&[]).expect_err("empty image should fail");

        assert_eq!(err, DragonPakParseError::Empty);
    }

    #[test]
    fn pcdragon_snapshot_decodes_uncompressed_ram_and_registers() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&0x2000u16.to_le_bytes());
        bytes.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd]);
        bytes.extend_from_slice(&[0; 33]);
        bytes.extend_from_slice(&0x1234u16.to_le_bytes());
        bytes.extend_from_slice(&0x5678u16.to_le_bytes());
        bytes.extend_from_slice(&0x9abcu16.to_le_bytes());
        bytes.extend_from_slice(&0xdef0u16.to_le_bytes());
        bytes.extend_from_slice(&0x2468u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0x12, 0x34, 0x56]);
        bytes.extend_from_slice(&[0; 7]);
        bytes.extend_from_slice(&[0x87]);

        let snapshot = parse_pcdragon_snapshot(&bytes).expect("snapshot should parse");

        assert_eq!(snapshot.load_address, 0x2000);
        assert_eq!(snapshot.ram.as_ref(), &[0xaa, 0xbb, 0xcc, 0xdd]);
        assert_eq!(snapshot.registers.pc, 0x1234);
        assert_eq!(snapshot.registers.x, 0x5678);
        assert_eq!(snapshot.registers.y, 0x9abc);
        assert_eq!(snapshot.registers.u, 0xdef0);
        assert_eq!(snapshot.registers.s, 0x2468);
        assert_eq!(snapshot.registers.dp, 0x12);
        assert_eq!(snapshot.registers.b, 0x34);
        assert_eq!(snapshot.registers.a, 0x56);
        assert_eq!(snapshot.registers.cc, 0x87);
        assert_eq!(snapshot.peripherals, None);
    }

    #[test]
    fn pcdragon_snapshot_decodes_v14_display_state() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&0x2000u16.to_le_bytes());
        bytes.push(0xaa);
        let mut info = [0; 65];
        info[33..35].copy_from_slice(&0x1234u16.to_le_bytes());
        info[54] = 0x87;
        info[58] = 0x7f;
        info[59] = 0xb5;
        info[60] = 0xfc;
        info[62..64].copy_from_slice(&0x000cu16.to_le_bytes());
        bytes.extend_from_slice(&info);

        let snapshot = parse_pcdragon_snapshot(&bytes).expect("snapshot should parse");

        assert_eq!(snapshot.registers.cc, 0x87);
        assert_eq!(
            snapshot.peripherals,
            Some(PcDragonPeripherals {
                ff02: 0x7f,
                ff03: 0xb5,
                ff22: 0xfc,
            })
        );
        assert_eq!(snapshot.display_base, Some(0x0c00));
    }
}
