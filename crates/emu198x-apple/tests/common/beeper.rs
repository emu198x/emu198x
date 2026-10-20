//! A 16 KiB stand-in for the Spectrum ROM that makes sound and moves the
//! picture, so parity checks compare real audio and changing frames without
//! Sinclair's firmware. The Game Boy plate is silent and static, so equal
//! results from it say little about either path.
//!
//! ```text
//! 0000  3E 00     LD   A,0
//! 0002  D3 FE     OUT  ($FE),A    ; beeper (bit 4) and border (bits 0-2)
//! 0004  EE 17     XOR  $17        ; flip both
//! 0006  06 FF     LD   B,255
//! 0008  10 FE     DJNZ $0008      ; about 3,300 T-states
//! 000A  18 F6     JR   $0002
//! ```
//!
//! That is a square wave near 500 Hz, and border stripes that drift from
//! one frame to the next. The rest of the image is zero.

/// The program at address 0, padded to the 48K ROM's size.
#[must_use]
pub fn beeper_rom() -> Vec<u8> {
    let program = [
        0x3E, 0x00, 0xD3, 0xFE, 0xEE, 0x17, 0x06, 0xFF, 0x10, 0xFE, 0x18, 0xF6,
    ];
    let mut rom = vec![0; 16 * 1024];
    rom[..program.len()].copy_from_slice(&program);
    rom
}
