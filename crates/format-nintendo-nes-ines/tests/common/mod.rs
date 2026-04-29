//! Shared helpers for the integration tests.

use format_nintendo_nes_ines::ParsedCartridge;

/// Build a minimal iNES 1.0 file with the given bank counts and
/// flags6. PRG bytes are filled with their offset mod 256 so
/// tests can identify which byte was served by a read.
pub fn make_ines(prg_banks: u8, chr_banks: u8, flags6: u8) -> Vec<u8> {
    let prg_size = usize::from(prg_banks) * 16384;
    let chr_size = usize::from(chr_banks) * 8192;
    let mut data = vec![0u8; 16 + prg_size + chr_size];
    data[0..4].copy_from_slice(b"NES\x1a");
    data[4] = prg_banks;
    data[5] = chr_banks;
    data[6] = flags6;
    for i in 0..prg_size {
        data[16 + i] = (i & 0xFF) as u8;
    }
    for i in 0..chr_size {
        data[16 + prg_size + i] = ((i + 0x80) & 0xFF) as u8;
    }
    data
}

/// Build a minimal NES 2.0 file with the given bank counts and
/// 12-bit mapper number.
pub fn make_nes2(prg_banks: u16, chr_banks: u16, mapper: u16) -> Vec<u8> {
    let prg_lo = (prg_banks & 0xFF) as u8;
    let prg_hi = ((prg_banks >> 8) & 0x0F) as u8;
    let chr_lo = (chr_banks & 0xFF) as u8;
    let chr_hi = ((chr_banks >> 8) & 0x0F) as u8;

    let mapper_lo = (mapper & 0x0F) as u8;
    let mapper_mid = ((mapper >> 4) & 0x0F) as u8;
    let mapper_hi = ((mapper >> 8) & 0x0F) as u8;

    let flags6 = mapper_lo << 4;
    let flags7 = (mapper_mid << 4) | 0x08; // NES 2.0 signature
    let byte8 = mapper_hi;

    let prg_size = prg_banks as usize * 16384;
    let chr_size = chr_banks as usize * 8192;

    let mut data = vec![0u8; 16 + prg_size + chr_size];
    data[0..4].copy_from_slice(b"NES\x1a");
    data[4] = prg_lo;
    data[5] = chr_lo;
    data[6] = flags6;
    data[7] = flags7;
    data[8] = byte8;
    data[9] = (chr_hi << 4) | prg_hi;

    for i in 0..prg_size {
        data[16 + i] = (i & 0xFF) as u8;
    }
    for i in 0..chr_size {
        data[16 + prg_size + i] = ((i + 0x80) & 0xFF) as u8;
    }
    data
}

/// `ParsedCartridge` contains a `Box<dyn Mapper>` which does
/// not implement `Debug`, so `.expect_err()` can't be used in
/// these negative tests. This helper unwraps the error arm
/// with a custom message and drops the `Ok` side.
pub fn expect_err(result: Result<ParsedCartridge, String>, ctx: &str) -> String {
    match result {
        Ok(_) => panic!("{ctx}: expected error, got Ok"),
        Err(e) => e,
    }
}
