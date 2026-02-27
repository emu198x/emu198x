//! Code Like It's 198x integration tests.
//!
//! Verify that the emulator can run actual lesson code from the Code198x
//! curriculum. These tests load compiled binaries (SNA snapshots) and
//! confirm visible output — proving the emulators are teaching-ready.

#![allow(clippy::cast_possible_truncation)]

use emu_spectrum::capture::save_screenshot;
use emu_spectrum::sna::load_sna;
use emu_spectrum::{Spectrum, SpectrumConfig, SpectrumModel};

const ROM_48K: &[u8] = include_bytes!("../../../roms/48.rom");
const OUTPUT_DIR: &str = "../../test_output";

fn make_spectrum() -> Spectrum {
    let config = SpectrumConfig {
        model: SpectrumModel::Spectrum48K,
        rom: ROM_48K.to_vec(),
    };
    Spectrum::new(&config)
}

fn ensure_output_dir() {
    let _ = std::fs::create_dir_all(OUTPUT_DIR);
}

/// Path to Code198x lesson code. Returns None if the repo isn't checked out.
fn code198x_path(relative: &str) -> Option<std::path::PathBuf> {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../Code198x/code-samples")
        .join(relative);
    if base.exists() {
        Some(base)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Shadowkeep Unit 1: coloured blocks drawn via attribute memory
// ---------------------------------------------------------------------------

#[test]
#[ignore] // Requires Code198x repo adjacent to Emu198x
fn test_shadowkeep_unit01() {
    let sna_path = match code198x_path(
        "sinclair-zx-spectrum/game-01-shadowkeep/unit-01/shadowkeep.sna",
    ) {
        Some(p) => p,
        None => {
            eprintln!("Skipping: Code198x repo not found");
            return;
        }
    };

    ensure_output_dir();
    let mut spectrum = make_spectrum();

    // Load the SNA snapshot
    let sna_data = std::fs::read(&sna_path).expect("read SNA");
    load_sna(&mut spectrum, &sna_data).expect("load SNA");

    // Run 50 frames — enough for the program to write attributes and halt
    for _ in 0..50 {
        spectrum.run_frame();
    }

    // Verify: border should be black (the program sets it to 0)
    assert_eq!(
        spectrum.bus().ula.border_colour(),
        0,
        "Shadowkeep sets border to black"
    );

    // Verify: attribute memory has non-zero values in the room area.
    // The program writes walls (blue=$09), floor (white=$38), treasure ($70),
    // hazard ($90) to attribute rows 10-14, cols 14-18.
    let wall_attr = spectrum.bus().memory.peek(0x594E); // Row 10, col 14
    assert_eq!(wall_attr, 0x09, "Wall attribute should be $09 (blue)");

    let floor_attr = spectrum.bus().memory.peek(0x596F); // Row 11, col 15
    assert_eq!(floor_attr, 0x38, "Floor attribute should be $38 (white)");

    let treasure_attr = spectrum.bus().memory.peek(0x5990); // Row 12, col 16
    assert_eq!(treasure_attr, 0x70, "Treasure attribute should be $70 (bright yellow)");

    let hazard_attr = spectrum.bus().memory.peek(0x59B1); // Row 13, col 17
    assert_eq!(hazard_attr, 0x90, "Hazard attribute should be $90 (flash red)");

    // Save screenshot for visual verification
    let path = format!("{OUTPUT_DIR}/code198x_shadowkeep_unit01.png");
    save_screenshot(&spectrum, path.as_ref()).expect("save screenshot");
    eprintln!("Saved {path}");
}

// ---------------------------------------------------------------------------
// Shadowkeep Unit 3: room drawn with loops (if available)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_shadowkeep_unit03() {
    let sna_path = match code198x_path(
        "sinclair-zx-spectrum/game-01-shadowkeep/unit-03/shadowkeep.sna",
    ) {
        Some(p) => p,
        None => {
            eprintln!("Skipping: Code198x repo not found or unit-03 missing");
            return;
        }
    };

    ensure_output_dir();
    let mut spectrum = make_spectrum();

    let sna_data = std::fs::read(&sna_path).expect("read SNA");
    load_sna(&mut spectrum, &sna_data).expect("load SNA");

    // Run enough frames for the room to be drawn
    for _ in 0..100 {
        spectrum.run_frame();
    }

    // Verify screen has content — at least the attribute area should be populated
    let mut attr_count = 0u32;
    for addr in 0x5800..=0x5AFFu16 {
        if spectrum.bus().memory.peek(addr) != 0 {
            attr_count += 1;
        }
    }
    assert!(
        attr_count > 10,
        "Unit 3 should draw a room with multiple attribute cells, found {attr_count}"
    );

    let path = format!("{OUTPUT_DIR}/code198x_shadowkeep_unit03.png");
    save_screenshot(&spectrum, path.as_ref()).expect("save screenshot");
    eprintln!("Saved {path}");
}
