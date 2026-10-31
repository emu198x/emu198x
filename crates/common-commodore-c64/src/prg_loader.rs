//! Loading a PRG straight into RAM, as a completed KERNAL `LOAD` leaves it.
//!
//! `format198x-commodore-c64-prg` reads the file; what the KERNAL does after
//! the bytes land (the `$AE/$AF` end pointer, relinking BASIC text at `$0801`)
//! is machine behaviour, so it lives here.

#![allow(clippy::cast_possible_truncation)]

use format198x_commodore_c64_prg::PrgFile;

/// Raw RAM, bypassing ROM overlays and I/O.
pub(crate) trait RamAccess {
    /// Reads one byte of raw RAM.
    fn ram_read(&self, addr: u16) -> u8;

    /// Writes one byte of raw RAM.
    fn ram_write(&mut self, addr: u16, val: u8);
}

const BASIC_VARTAB_LO: u16 = 0x2D;
const BASIC_VARTAB_HI: u16 = 0x2E;
const BASIC_START: u16 = 0x0801;

/// Parses a PRG and loads it into RAM, returning its load address.
///
/// When the load address is `$0801`, the BASIC text is relinked and BASIC's
/// start-of-variables pointer is updated so `RUN` works immediately.
///
/// # Errors
///
/// Returns an error if the file is too short to hold its load address.
pub(crate) fn load_prg(ram: &mut impl RamAccess, data: &[u8]) -> Result<u16, String> {
    let prg = PrgFile::parse(data).map_err(|error| error.to_string())?;
    for (index, byte) in prg.data.iter().copied().enumerate() {
        ram.ram_write(prg.load_address.wrapping_add(index as u16), byte);
    }

    // Preserve the KERNAL LOAD end pointer used by PRG unpackers.
    // VICE 3.10: src/c64/c64memsc.c::mem_set_basic_text and
    // src/autostart-prg.c::autostart_prg_perform_injection.
    // This is the complete payload end, not the BASIC stub's end.
    let [lo, hi] = prg.end_address().to_le_bytes();
    ram.ram_write(0xAE, lo);
    ram.ram_write(0xAF, hi);

    if prg.load_address == BASIC_START {
        relink_basic(ram);
    }

    Ok(prg.load_address)
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
        assert!(load_prg(&mut ram, &[0x01]).is_err());
    }

    /// A file holding only its load address is a valid, empty PRG: nothing
    /// loads and the end pointer is the load address.
    #[test]
    fn load_prg_accepts_an_empty_payload() {
        let mut ram = TestRam::new();
        let addr = load_prg(&mut ram, &[0x00, 0xC0]).expect("an empty PRG loads");
        assert_eq!(addr, 0xC000);
        let end = u16::from_le_bytes([ram.ram_read(0xAE), ram.ram_read(0xAF)]);
        assert_eq!(end, 0xC000);
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
    fn basic_stub_keeps_full_payload_end_for_unpackers() {
        let mut ram = TestRam::new();
        // One BASIC line followed by machine code: the loaded file extends
        // beyond the end of BASIC text. No commercial software is required.
        let prg = [
            0x01, 0x08, 0x0B, 0x08, 0x0A, 0x00, 0x9E, b'2', b'0', b'6', b'1', 0x00, 0x00, 0x00,
            0x60, 0xEA, 0xEA,
        ];
        let start = load_prg(&mut ram, &prg).expect("load synthetic PRG");
        let payload_end = start.wrapping_add((prg.len() - 2) as u16);
        let kernal_end = u16::from_le_bytes([ram.ram_read(0xAE), ram.ram_read(0xAF)]);
        let basic_end = u16::from_le_bytes([ram.ram_read(0x2D), ram.ram_read(0x2E)]);
        assert_eq!(kernal_end, payload_end, "unpackers need the full file end");
        assert!(
            basic_end < kernal_end,
            "BASIC relinking must retain its own end"
        );
    }

    #[test]
    fn load_end_wraps_with_the_payload() {
        let mut ram = TestRam::new();
        load_prg(&mut ram, &[0xFF, 0xFF, 0x11, 0x22, 0x33]).expect("load wrapping PRG");
        let end = u16::from_le_bytes([ram.ram_read(0xAE), ram.ram_read(0xAF)]);
        assert_eq!(end, 2);
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
