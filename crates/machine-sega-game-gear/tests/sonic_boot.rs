//! Identified Sonic Rev 0 reaches its title and attract map after the split.
//!
//! MAME `hash/gamegear.xml`: 262144 bytes, CRC32 3e31cb8c,
//! SHA-1 8923295c96b2e05da0a83a827fd709b033fc7332. The local input was also
//! checked independently with SHA-256 (see the frozen-state README).
//! No commercial data is committed. Set EMU198X_GG_SONIC_REV0_ROM and run
//! `cargo test -p machine-sega-game-gear --test sonic_boot -- --ignored`.
use machine_sega_game_gear::Sms;
use sha2::{Digest, Sha256};
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[test]
#[ignore = "FIXTURE: needs identified Sonic Rev 0 at EMU198X_GG_SONIC_REV0_ROM"]
fn sonic_rev0_reaches_the_title_and_attract_map() {
    let path =
        std::env::var_os("EMU198X_GG_SONIC_REV0_ROM").expect("explicit original cartridge path");
    let rom = std::fs::read(path).expect("read immutable cartridge");
    assert_eq!(rom.len(), 262144, "pinned Sonic Rev 0 size");
    assert_eq!(
        sha256(&rom),
        "dd605c17c8d24b2db9928beff6d8294fd84184b31c2fa92760ae691db84fd569",
        "complete original Sonic Rev 0 identity"
    );
    let mut board = Sms::new(rom);
    for frame in 1..=1200 {
        board.run_frame();
        if let Some(expected) = match frame {
            600 => Some("db8c9f067ade6129c8694c14b225d1da89648012f9dcdc252437015f88bc734b"),
            1200 => Some("b0eab2e156a7a6fab54d17fece4ebbd5c62077ba3b0812036640b96609399a1a"),
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
            assert_eq!(sha256(&bytes), expected, "original pixels at frame {frame}");
        }
        drop(board.take_audio_buffer());
    }
}
