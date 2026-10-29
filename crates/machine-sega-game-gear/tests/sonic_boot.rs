//! Identified Sonic Rev 0 reaches its title and attract map after the split.
//!
//! MAME `hash/gamegear.xml`: 262144 bytes, CRC32 3e31cb8c,
//! SHA-1 8923295c96b2e05da0a83a827fd709b033fc7332. The local input was also
//! checked independently with SHA-256 (see the frozen-state README).
//! No commercial data is committed. Set EMU198X_GG_SONIC_REV0_ROM and run
//! `cargo test -p machine-sega-game-gear --test sonic_boot -- --ignored`.
use machine_sega_game_gear::Sms;
fn fingerprint(bytes: &[u8]) -> u64 {
    // Explicit FNV-1a, independent of Rust's changeable DefaultHasher.
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
#[test]
#[ignore = "FIXTURE: needs identified Sonic Rev 0 at EMU198X_GG_SONIC_REV0_ROM"]
fn sonic_rev0_reaches_the_title_and_attract_map() {
    let path =
        std::env::var_os("EMU198X_GG_SONIC_REV0_ROM").expect("explicit original cartridge path");
    let rom = std::fs::read(path).expect("read immutable cartridge");
    assert_eq!(rom.len(), 262144, "pinned Sonic Rev 0 size");
    assert_eq!(crc32(&rom), 0x3E31_CB8C, "MAME Sonic Rev 0 identity");
    assert_eq!(
        fingerprint(&rom),
        0x51b6_96f6_d353_0163,
        "complete original cartridge"
    );
    let mut board = Sms::new(rom);
    for frame in 1..=1200 {
        board.run_frame();
        if let Some(expected) = match frame {
            600 => Some(0x7d5a_8333_71e0_8a73),
            1200 => Some(0x4a1e_062f_0dd0_c72a),
            _ => None,
        } {
            assert_eq!(
                (board.framebuffer_width(), board.framebuffer_height()),
                (160, 144)
            );
            let bytes: Vec<_> = board
                .framebuffer()
                .iter()
                .flat_map(|p| p.to_le_bytes())
                .collect();
            assert_eq!(bytes.len(), 160 * 144 * 4);
            assert_eq!(
                fingerprint(&bytes),
                expected,
                "original pixels at frame {frame}"
            );
        }
        drop(board.take_audio_buffer());
    }
}
