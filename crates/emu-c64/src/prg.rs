//! PRG file loader.
//!
//! A PRG file is the simplest C64 binary format: a 2-byte little-endian
//! load address followed by the data bytes. The data is loaded into RAM
//! starting at the given address.

use crate::memory::C64Memory;

/// BASIC start-of-variables pointer (low byte).
const BASIC_VARTAB_LO: u16 = 0x2D;
/// BASIC start-of-variables pointer (high byte).
const BASIC_VARTAB_HI: u16 = 0x2E;
/// Default BASIC start address.
const BASIC_START: u16 = 0x0801;

/// Load a PRG file into C64 RAM.
///
/// Returns the load address on success.
///
/// When the load address is `$0801` (the standard BASIC start), the BASIC
/// program text is relinked and the start-of-variables pointer (`$2D`/`$2E`)
/// is set to one byte past the end-of-program marker. This matches the real
/// KERNAL LOAD routine's behaviour, letting `RUN` work immediately.
///
/// # Errors
///
/// Returns an error if the data is too short to contain a valid PRG header.
pub fn load_prg(memory: &mut C64Memory, data: &[u8]) -> Result<u16, String> {
    if data.len() < 3 {
        return Err("PRG file too short (need at least 3 bytes)".to_string());
    }

    let load_addr = u16::from(data[0]) | (u16::from(data[1]) << 8);

    for (i, &byte) in data[2..].iter().enumerate() {
        memory.ram_write(load_addr.wrapping_add(i as u16), byte);
    }

    // Relink BASIC and fix up pointers so RUN works after loading.
    if load_addr == BASIC_START {
        relink_basic(memory);
    }

    Ok(load_addr)
}

/// Walk the BASIC program text, recalculate next-line pointers, and set
/// the start-of-variables pointer (`$2D`/`$2E`) to the byte after the
/// end-of-program marker.
///
/// This replicates the KERNAL LINKPRG routine at `$A533`.
fn relink_basic(memory: &mut C64Memory) {
    let mut addr = BASIC_START;

    loop {
        // Read the existing next-line pointer (we'll recalculate it).
        let lo = memory.ram_read(addr);
        let hi = memory.ram_read(addr.wrapping_add(1));

        // A null pointer means end of program.
        if lo == 0 && hi == 0 {
            // $2D/$2E = first byte after the two-byte null terminator.
            let end = addr.wrapping_add(2);
            memory.ram_write(BASIC_VARTAB_LO, (end & 0xFF) as u8);
            memory.ram_write(BASIC_VARTAB_HI, (end >> 8) as u8);
            return;
        }

        // Skip the 2-byte pointer and 2-byte line number.
        let mut scan = addr.wrapping_add(4);

        // Scan forward until the end-of-line $00 byte.
        let limit = 1000u16; // safety limit
        let mut count = 0u16;
        while memory.ram_read(scan) != 0 && count < limit {
            scan = scan.wrapping_add(1);
            count += 1;
        }
        scan = scan.wrapping_add(1); // skip the $00 terminator

        // Write the corrected next-line pointer.
        memory.ram_write(addr, (scan & 0xFF) as u8);
        memory.ram_write(addr.wrapping_add(1), (scan >> 8) as u8);

        addr = scan;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_memory() -> C64Memory {
        C64Memory::new(&vec![0; 8192], &vec![0; 8192], &vec![0; 4096])
    }

    #[test]
    fn load_prg_non_basic_address() {
        let mut mem = make_memory();
        // PRG: load at $C000 (not BASIC), data = [0x0A, 0x0B]
        let prg = vec![0x00, 0xC0, 0x0A, 0x0B];
        let addr = load_prg(&mut mem, &prg).expect("load should succeed");
        assert_eq!(addr, 0xC000);
        assert_eq!(mem.ram_read(0xC000), 0x0A);
        assert_eq!(mem.ram_read(0xC001), 0x0B);
    }

    #[test]
    fn load_prg_too_short() {
        let mut mem = make_memory();
        let result = load_prg(&mut mem, &[0x01, 0x08]);
        assert!(result.is_err());
    }

    #[test]
    fn load_prg_relinks_basic_stub() {
        let mut mem = make_memory();

        // Standard BASIC stub: 10 SYS 2061
        // With a deliberately wrong next-line pointer ($080C instead of $080B)
        // followed by machine code at $080D.
        let mut prg = vec![
            0x01, 0x08, // load address $0801
            0x0C, 0x08, // next-line pointer (wrong: $080C)
            0x0A, 0x00, // line number 10
            0x9E,       // SYS token
            0x32, 0x30, 0x36, 0x31, // "2061"
            0x00,       // end of line
            0x00, 0x00, // end of program
            0xA9, 0x00, // LDA #$00 (machine code at $080D)
        ];
        // Pad to avoid short PRG check
        prg.push(0x8D);

        let addr = load_prg(&mut mem, &prg).expect("load should succeed");
        assert_eq!(addr, 0x0801);

        // After relink, the next-line pointer should be $080B (not $080C).
        let ptr_lo = mem.ram_read(0x0801);
        let ptr_hi = mem.ram_read(0x0802);
        let ptr = u16::from(ptr_lo) | (u16::from(ptr_hi) << 8);
        assert_eq!(ptr, 0x080B, "next-line pointer should point to end-of-program");

        // At $080B, the end-of-program marker should be $0000.
        assert_eq!(mem.ram_read(0x080B), 0x00);
        assert_eq!(mem.ram_read(0x080C), 0x00);

        // $2D/$2E should point to $080D (one past the end-of-program marker).
        let vartab_lo = mem.ram_read(0x2D);
        let vartab_hi = mem.ram_read(0x2E);
        let vartab = u16::from(vartab_lo) | (u16::from(vartab_hi) << 8);
        assert_eq!(vartab, 0x080D, "start-of-variables should be past end marker");
    }
}
