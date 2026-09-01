//! Booting the 800XL from a disk in D1:.
//!
//! The OS's cold start sends a Status command to D1:, then reads sector 1 and
//! follows its boot header: the first six bytes are flags, a sector count, a
//! two-byte load address and a two-byte run address. It loads that many
//! sectors from sector 1 onward into memory at the load address and jumps.
//!
//! The disk here is built rather than fetched, so the test asserts on what the
//! program does rather than on a title's screen. Needs an OS ROM, so it is
//! gated like the other boot tests.

use std::path::PathBuf;

use format_atari_8bit_atr::AtrImage;
use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

const LOAD_ADDRESS: u16 = 0x0700;

/// A single-density disk whose boot sectors hold a program that writes a
/// signature into memory and then loops.
fn bootable_disk() -> AtrImage {
    let mut sectors = vec![0u8; 720 * 128];

    // The OS reads sector 1, loads `count` sectors to the load address, JSRs
    // to the init vector while loading, and finally JSRs to load + 6. So the
    // program proper starts at offset 6, and the init vector points at an RTS
    // parked past the end of it.
    let code: [u8; 8] = [
        0xA9, 0x2A, // LDA #$2A
        0x8D, 0x00, 0x06, // STA $0600
        0x4C, 0x06, 0x07, // JMP $0706 — back to the top, so it stays put
    ];
    const INIT_OFFSET: u16 = 20;

    sectors[0] = 0x00; // flags
    sectors[1] = 1; // one sector to load
    sectors[2] = LOAD_ADDRESS as u8;
    sectors[3] = (LOAD_ADDRESS >> 8) as u8;
    sectors[4] = (LOAD_ADDRESS + INIT_OFFSET) as u8;
    sectors[5] = ((LOAD_ADDRESS + INIT_OFFSET) >> 8) as u8;
    sectors[6..6 + code.len()].copy_from_slice(&code);
    sectors[usize::from(INIT_OFFSET)] = 0x60; // RTS

    let mut image = vec![0u8; 16];
    image[0..2].copy_from_slice(&0x0296u16.to_le_bytes());
    image[2..4].copy_from_slice(&((sectors.len() / 16) as u16).to_le_bytes());
    image[4..6].copy_from_slice(&128u16.to_le_bytes());
    image.extend(sectors);
    AtrImage::parse(&image).expect("the built disk parses")
}

fn os_rom() -> Option<Vec<u8>> {
    let root = std::env::var_os("EMU198X_ROMS_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms")))?;
    let dir = root.join("atari-800xl");
    for name in ["atarixl.rom", "altirraos_xl.rom"] {
        let path = dir.join(name);
        if path.exists() {
            return std::fs::read(path).ok();
        }
    }
    None
}

#[test]
#[ignore = "FIXTURE: needs an 800XL OS ROM at <EMU198X_ROMS_ROOT>/atari-800xl/"]
fn the_os_boots_a_disk_in_d1() {
    let Some(os) = os_rom() else {
        emu198x_test_skip::skip!("no 800XL OS ROM staged");
    };

    let mut sys = Atari800xl::new(Some(os), None, None, Atari800xlRegion::Ntsc, false)
        .expect("machine should initialise");
    sys.sio_mut().insert_disk(1, bootable_disk());

    for _ in 0..1_500 {
        sys.run_frame();
        if sys.peek(0x0600) == 0x2A {
            break;
        }
    }

    assert_eq!(
        sys.peek(0x0600),
        0x2A,
        "the OS should have read the boot sector over SIO, loaded it to ${LOAD_ADDRESS:04X} and run it"
    );
    assert_eq!(
        sys.peek(LOAD_ADDRESS + 6),
        0xA9,
        "and the program should be where the boot header asked for it"
    );
}
