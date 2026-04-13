//! PRG file parser and loader.
//!
//! A PRG file is the simplest C64 binary format: a 2-byte little-endian load
//! address followed by data bytes loaded directly into RAM.

#![allow(clippy::cast_possible_truncation)]

/// Trait for accessing raw RAM, bypassing ROM overlays and I/O.
pub trait RamAccess {
    /// Reads one byte of raw RAM.
    fn ram_read(&self, addr: u16) -> u8;

    /// Writes one byte of raw RAM.
    fn ram_write(&mut self, addr: u16, val: u8);
}

/// A parsed PRG file.
pub struct PrgFile {
    /// Load address from the PRG header.
    pub load_address: u16,
    /// Payload bytes after the 2-byte header.
    pub data: Vec<u8>,
}

const BASIC_VARTAB_LO: u16 = 0x2D;
const BASIC_VARTAB_HI: u16 = 0x2E;
const BASIC_START: u16 = 0x0801;

impl PrgFile {
    /// Parses one PRG file from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the file is too short to contain a valid header.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 3 {
            return Err("PRG file too short (need at least 3 bytes)".to_owned());
        }

        Ok(Self {
            load_address: u16::from(data[0]) | (u16::from(data[1]) << 8),
            data: data[2..].to_vec(),
        })
    }

    /// Loads this PRG into RAM and returns the load address.
    ///
    /// When the load address is `$0801`, the BASIC text is relinked and BASIC's
    /// start-of-variables pointer is updated so `RUN` works immediately.
    pub fn load_into(self, ram: &mut impl RamAccess) -> u16 {
        for (index, byte) in self.data.iter().copied().enumerate() {
            ram.ram_write(self.load_address.wrapping_add(index as u16), byte);
        }

        if self.load_address == BASIC_START {
            relink_basic(ram);
        }

        self.load_address
    }
}

/// Convenience wrapper combining PRG parse and load.
///
/// # Errors
///
/// Returns an error if the file header is malformed.
pub fn load_prg(ram: &mut impl RamAccess, data: &[u8]) -> Result<u16, String> {
    Ok(PrgFile::parse(data)?.load_into(ram))
}

fn relink_basic(ram: &mut impl RamAccess) {
    let mut addr = BASIC_START;

    loop {
        let lo = ram.ram_read(addr);
        let hi = ram.ram_read(addr.wrapping_add(1));

        if lo == 0 && hi == 0 {
            let end = addr.wrapping_add(2);
            ram.ram_write(BASIC_VARTAB_LO, (end & 0xFF) as u8);
            ram.ram_write(BASIC_VARTAB_HI, (end >> 8) as u8);
            return;
        }

        let mut scan = addr.wrapping_add(4);
        let mut count = 0u16;
        while ram.ram_read(scan) != 0 && count < 1000 {
            scan = scan.wrapping_add(1);
            count = count.wrapping_add(1);
        }
        scan = scan.wrapping_add(1);

        ram.ram_write(addr, (scan & 0xFF) as u8);
        ram.ram_write(addr.wrapping_add(1), (scan >> 8) as u8);
        addr = scan;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRam([u8; 65536]);

    impl TestRam {
        fn new() -> Self {
            Self([0; 65536])
        }
    }

    impl RamAccess for TestRam {
        fn ram_read(&self, addr: u16) -> u8 {
            self.0[addr as usize]
        }

        fn ram_write(&mut self, addr: u16, val: u8) {
            self.0[addr as usize] = val;
        }
    }

    #[test]
    fn parse_prg() {
        let prg = PrgFile::parse(&[0x00, 0xC0, 0x0A, 0x0B]).expect("parse should succeed");
        assert_eq!(prg.load_address, 0xC000);
        assert_eq!(prg.data, &[0x0A, 0x0B]);
    }

    #[test]
    fn load_prg_non_basic_address() {
        let mut ram = TestRam::new();
        let addr = load_prg(&mut ram, &[0x00, 0xC0, 0x0A, 0x0B]).expect("load should succeed");
        assert_eq!(addr, 0xC000);
        assert_eq!(ram.ram_read(0xC000), 0x0A);
        assert_eq!(ram.ram_read(0xC001), 0x0B);
    }

    #[test]
    fn load_prg_too_short() {
        let mut ram = TestRam::new();
        assert!(load_prg(&mut ram, &[0x01, 0x08]).is_err());
    }

    #[test]
    fn load_prg_relinks_basic_stub() {
        let mut ram = TestRam::new();
        let prg = [
            0x01, 0x08, 0x0C, 0x08, 0x0A, 0x00, 0x9E, 0x32, 0x30, 0x36, 0x31, 0x00, 0x00, 0x00,
            0xA9, 0x00, 0x8D,
        ];

        let addr = load_prg(&mut ram, &prg).expect("load should succeed");
        assert_eq!(addr, 0x0801);

        let ptr = u16::from(ram.ram_read(0x0801)) | (u16::from(ram.ram_read(0x0802)) << 8);
        assert_eq!(ptr, 0x080B);

        let vartab = u16::from(ram.ram_read(0x2D)) | (u16::from(ram.ram_read(0x2E)) << 8);
        assert_eq!(vartab, 0x080D);
    }

    #[test]
    fn non_basic_load_does_not_rewrite_basic_pointers() {
        let mut ram = TestRam::new();
        ram.ram_write(0x2D, 0xAA);
        ram.ram_write(0x2E, 0xBB);

        let addr = load_prg(&mut ram, &[0x00, 0xC0, 0x11, 0x22]).expect("load should succeed");

        assert_eq!(addr, 0xC000);
        assert_eq!(ram.ram_read(0x2D), 0xAA);
        assert_eq!(ram.ram_read(0x2E), 0xBB);
    }

    #[test]
    fn load_prg_wraps_at_end_of_address_space() {
        let mut ram = TestRam::new();
        let addr =
            load_prg(&mut ram, &[0xFF, 0xFF, 0x11, 0x22, 0x33]).expect("load should succeed");

        assert_eq!(addr, 0xFFFF);
        assert_eq!(ram.ram_read(0xFFFF), 0x11);
        assert_eq!(ram.ram_read(0x0000), 0x22);
        assert_eq!(ram.ram_read(0x0001), 0x33);
    }
}
