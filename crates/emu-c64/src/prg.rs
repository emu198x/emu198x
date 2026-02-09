//! PRG file loader.
//!
//! A PRG file is the simplest C64 binary format: a 2-byte little-endian
//! load address followed by the data bytes. The data is loaded into RAM
//! starting at the given address.

use crate::memory::C64Memory;

/// Load a PRG file into C64 RAM.
///
/// Returns the load address on success.
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

    Ok(load_addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_memory() -> C64Memory {
        C64Memory::new(&vec![0; 8192], &vec![0; 8192], &vec![0; 4096])
    }

    #[test]
    fn load_prg_basic() {
        let mut mem = make_memory();
        // PRG: load at $0801, data = [0x0A, 0x0B]
        let prg = vec![0x01, 0x08, 0x0A, 0x0B];
        let addr = load_prg(&mut mem, &prg).expect("load should succeed");
        assert_eq!(addr, 0x0801);
        assert_eq!(mem.ram_read(0x0801), 0x0A);
        assert_eq!(mem.ram_read(0x0802), 0x0B);
    }

    #[test]
    fn load_prg_too_short() {
        let mut mem = make_memory();
        let result = load_prg(&mut mem, &[0x01, 0x08]);
        assert!(result.is_err());
    }
}
