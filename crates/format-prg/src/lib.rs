//! PRG file parser and loader.
//!
//! A PRG file is the simplest C64 binary format: a 2-byte little-endian
//! load address followed by the data bytes. The data is loaded into RAM
//! starting at the given address.

#![allow(clippy::cast_possible_truncation)]

/// Trait for accessing raw RAM (bypassing ROM overlays and I/O).
pub trait RamAccess {
    fn ram_read(&self, addr: u16) -> u8;
    fn ram_write(&mut self, addr: u16, val: u8);
}

/// A parsed PRG file.
pub struct PrgFile {
    /// Load address (first two bytes of the file).
    pub load_address: u16,
    /// Data payload (everything after the two-byte header).
    pub data: Vec<u8>,
}

/// BASIC start-of-variables pointer (low byte).
const BASIC_VARTAB_LO: u16 = 0x2D;
/// BASIC start-of-variables pointer (high byte).
const BASIC_VARTAB_HI: u16 = 0x2E;
/// Default BASIC start address.
const BASIC_START: u16 = 0x0801;

impl PrgFile {
    /// Parse a PRG file from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is too short to contain a valid PRG header.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 3 {
            return Err("PRG file too short (need at least 3 bytes)".to_string());
        }

        let load_address = u16::from(data[0]) | (u16::from(data[1]) << 8);
        let payload = data[2..].to_vec();

        Ok(Self {
            load_address,
            data: payload,
        })
    }

    /// Load the PRG data into RAM and return the load address.
    ///
    /// When the load address is `$0801` (the standard BASIC start), the BASIC
    /// program text is relinked and the start-of-variables pointer (`$2D`/`$2E`)
    /// is set to one byte past the end-of-program marker. This matches the real
    /// KERNAL LOAD routine's behaviour, letting `RUN` work immediately.
    pub fn load_into(self, ram: &mut impl RamAccess) -> u16 {
        for (i, &byte) in self.data.iter().enumerate() {
            ram.ram_write(self.load_address.wrapping_add(i as u16), byte);
        }

        if self.load_address == BASIC_START {
            relink_basic(ram);
        }

        self.load_address
    }
}

/// Load a PRG file into RAM.
///
/// Convenience wrapper combining `parse` and `load_into`.
///
/// # Errors
///
/// Returns an error if the data is too short to contain a valid PRG header.
pub fn load_prg(ram: &mut impl RamAccess, data: &[u8]) -> Result<u16, String> {
    let prg = PrgFile::parse(data)?;
    Ok(prg.load_into(ram))
}

/// Walk the BASIC program text, recalculate next-line pointers, and set
/// the start-of-variables pointer (`$2D`/`$2E`) to the byte after the
/// end-of-program marker.
///
/// This replicates the KERNAL LINKPRG routine at `$A533`.
fn relink_basic(ram: &mut impl RamAccess) {
    let mut addr = BASIC_START;

    loop {
        // Read the existing next-line pointer (we'll recalculate it).
        let lo = ram.ram_read(addr);
        let hi = ram.ram_read(addr.wrapping_add(1));

        // A null pointer means end of program.
        if lo == 0 && hi == 0 {
            // $2D/$2E = first byte after the two-byte null terminator.
            let end = addr.wrapping_add(2);
            ram.ram_write(BASIC_VARTAB_LO, (end & 0xFF) as u8);
            ram.ram_write(BASIC_VARTAB_HI, (end >> 8) as u8);
            return;
        }

        // Skip the 2-byte pointer and 2-byte line number.
        let mut scan = addr.wrapping_add(4);

        // Scan forward until the end-of-line $00 byte.
        let limit = 1000u16; // safety limit
        let mut count = 0u16;
        while ram.ram_read(scan) != 0 && count < limit {
            scan = scan.wrapping_add(1);
            count += 1;
        }
        scan = scan.wrapping_add(1); // skip the $00 terminator

        // Write the corrected next-line pointer.
        ram.ram_write(addr, (scan & 0xFF) as u8);
        ram.ram_write(addr.wrapping_add(1), (scan >> 8) as u8);

        addr = scan;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simple 64K RAM for testing.
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
        let prg = vec![0x00, 0xC0, 0x0A, 0x0B];
        let addr = load_prg(&mut ram, &prg).expect("load should succeed");
        assert_eq!(addr, 0xC000);
        assert_eq!(ram.ram_read(0xC000), 0x0A);
        assert_eq!(ram.ram_read(0xC001), 0x0B);
    }

    #[test]
    fn load_prg_too_short() {
        let mut ram = TestRam::new();
        let result = load_prg(&mut ram, &[0x01, 0x08]);
        assert!(result.is_err());
    }

    #[test]
    fn load_prg_relinks_basic_stub() {
        let mut ram = TestRam::new();

        // Standard BASIC stub: 10 SYS 2061
        let prg = vec![
            0x01, 0x08, // load address $0801
            0x0C, 0x08, // next-line pointer (wrong: $080C)
            0x0A, 0x00, // line number 10
            0x9E,       // SYS token
            0x32, 0x30, 0x36, 0x31, // "2061"
            0x00,       // end of line
            0x00, 0x00, // end of program
            0xA9, 0x00, // LDA #$00 (machine code at $080D)
            0x8D,       // padding
        ];

        let addr = load_prg(&mut ram, &prg).expect("load should succeed");
        assert_eq!(addr, 0x0801);

        // After relink, the next-line pointer should be $080B.
        let ptr_lo = ram.ram_read(0x0801);
        let ptr_hi = ram.ram_read(0x0802);
        let ptr = u16::from(ptr_lo) | (u16::from(ptr_hi) << 8);
        assert_eq!(ptr, 0x080B, "next-line pointer should point to end-of-program");

        // $2D/$2E should point to $080D.
        let vartab_lo = ram.ram_read(0x2D);
        let vartab_hi = ram.ram_read(0x2E);
        let vartab = u16::from(vartab_lo) | (u16::from(vartab_hi) << 8);
        assert_eq!(vartab, 0x080D, "start-of-variables should be past end marker");
    }
}
