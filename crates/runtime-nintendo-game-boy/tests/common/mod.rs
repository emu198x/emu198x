//! Shared helpers for the Game Boy runtime integration-test suite.
//!
//! Mirrors the helpers that used to live inside the `runtime.rs`
//! `#[cfg(test)] mod tests` block, exposed `pub` so each per-topic
//! integration-test file can pull them in via `mod common;`.

#![allow(dead_code)]

use emu198x_shell::{NullAudioSink, NullFrameSink, NullTraceSink};

/// Build a 32 KiB ROM that loops forever at $0100 with a valid header.
pub fn loop_rom() -> Vec<u8> {
    let mut rom = vec![0x00; 0x8000];
    rom[0x0100] = 0x18; // JR
    rom[0x0101] = 0xFE; // -2 → tight loop
    rom[0x0147] = 0x00; // ROM only
    rom[0x0148] = 0x00; // ROM size code 0 → 32 KiB
    rom[0x0149] = 0x00; // RAM size code 0
    let mut checksum: u8 = 0;
    for &byte in &rom[0x0134..=0x014C] {
        checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
    }
    rom[0x014D] = checksum;
    rom
}

/// Build a 32 KiB ROM with a valid MBC1 + RAM + battery header so the
/// runtime exposes 8 KiB of external RAM.
pub fn battery_ram_rom() -> Vec<u8> {
    let mut rom = loop_rom();
    rom[0x0147] = 0x03; // MBC1 + RAM + battery
    rom[0x0149] = 0x02; // 8 KiB RAM
    let mut checksum: u8 = 0;
    for &byte in &rom[0x0134..=0x014C] {
        checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
    }
    rom[0x014D] = checksum;
    rom
}

/// Returns the trio of null sinks used by tests that don't care about
/// frame / audio / trace output.
pub fn null_host_buffers() -> (NullFrameSink, NullAudioSink, NullTraceSink) {
    (NullFrameSink, NullAudioSink, NullTraceSink)
}
