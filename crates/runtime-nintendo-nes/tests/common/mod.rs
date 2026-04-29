//! Shared helpers for the NES runtime integration-test suite.
//!
//! Mirrors the helpers that used to live inside the `runtime.rs`
//! `#[cfg(test)] mod tests` block, exposed `pub` so each per-topic
//! integration-test file can pull them in via `mod common;`.

#![allow(dead_code)]

pub const NTSC_FRAME_TICKS: u64 = 341 * 262;

/// 16 KiB of NOP (0xEA) PRG plus an 8 KiB blank CHR, wrapped in a
/// minimal iNES header with mapper 0 (NROM). Reset vector points at
/// $8000 so the CPU keeps NOPing forever and the PPU/APU run alongside.
pub fn minimal_ines() -> Vec<u8> {
    let mut prg = vec![0xea; 16 * 1024];
    prg[0x3ffc] = 0x00;
    prg[0x3ffd] = 0x80;
    let chr = vec![0u8; 8 * 1024];
    let mut data = vec![0u8; 16 + prg.len() + chr.len()];
    data[0..4].copy_from_slice(b"NES\x1a");
    data[4] = 1;
    data[5] = 1;
    data[16..16 + prg.len()].copy_from_slice(&prg);
    data[16 + prg.len()..].copy_from_slice(&chr);
    data
}

/// 16 KiB PRG with a startup sequence that writes a blargg result
/// block at $6000–$6004+: signature bytes at $6001–$6003, status at
/// $6000, then `text` followed by a null terminator at $6004+. Ends
/// in an infinite `JMP self` so the CPU keeps the result block stable
/// once written.
pub fn blargg_ines(status: u8, text: &[u8]) -> Vec<u8> {
    let mut prg = vec![0xea; 16 * 1024];
    let mut cursor = 0usize;
    for (addr, value) in [
        (0x6001, 0xDE),
        (0x6002, 0xB0),
        (0x6003, 0x61),
        (0x6000, status),
    ] {
        emit_store(&mut prg, &mut cursor, addr, value);
    }
    for (index, &byte) in text.iter().enumerate() {
        emit_store(&mut prg, &mut cursor, 0x6004 + index as u16, byte);
    }
    emit_store(&mut prg, &mut cursor, 0x6004 + text.len() as u16, 0);
    let loop_addr = 0x8000 + cursor as u16;
    prg[cursor] = 0x4C;
    prg[cursor + 1] = (loop_addr & 0x00FF) as u8;
    prg[cursor + 2] = (loop_addr >> 8) as u8;

    prg[0x3ffc] = 0x00;
    prg[0x3ffd] = 0x80;
    let chr = vec![0u8; 8 * 1024];
    let mut data = vec![0u8; 16 + prg.len() + chr.len()];
    data[0..4].copy_from_slice(b"NES\x1a");
    data[4] = 1;
    data[5] = 1;
    data[16..16 + prg.len()].copy_from_slice(&prg);
    data[16 + prg.len()..].copy_from_slice(&chr);
    data
}

fn emit_store(prg: &mut [u8], cursor: &mut usize, addr: u16, value: u8) {
    prg[*cursor] = 0xA9;
    prg[*cursor + 1] = value;
    prg[*cursor + 2] = 0x8D;
    prg[*cursor + 3] = (addr & 0x00FF) as u8;
    prg[*cursor + 4] = (addr >> 8) as u8;
    *cursor += 5;
}
