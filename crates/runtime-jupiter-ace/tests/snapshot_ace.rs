//! `.ace` snapshot loading for the Jupiter Ace (#279).
//!
//! Format: `reference/by-system/jupiter-ace/jupiter-ace-ace-snapshot-format.md`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet};
use runtime_jupiter_ace::{JupiterAceRuntime, Model};

fn bios() -> Option<Vec<u8>> {
    if let Ok(p) = env::var("EMU198X_JUPITER_ACE_BIOS") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(fs::read(p).expect("read BIOS"));
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/jupiter-ace/ace.rom");
    p.exists().then(|| fs::read(p).expect("read BIOS"))
}

/// Encode a decoded image back into the container, so a test can build one
/// without shipping a copyrighted snapshot.
fn encode(image: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < image.len() {
        let byte = image[i];
        let mut run = 1;
        while i + run < image.len() && image[i + run] == byte && run < 240 {
            run += 1;
        }
        if run >= 3 || byte == 0xED {
            out.extend_from_slice(&[0xED, run as u8, byte]);
        } else {
            out.extend(std::iter::repeat_n(byte, run));
        }
        i += run;
    }
    out.extend_from_slice(&[0xED, 0x00]);
    out
}

fn runtime_with(image: &[u8]) -> JupiterAceRuntime {
    let Some(rom) = bios() else {
        panic!("Jupiter Ace ROM not found at ~/.emu198x/roms/jupiter-ace/ace.rom");
    };
    let mut runtime = JupiterAceRuntime::new(Model::Ace16k, rom).expect("build runtime");
    let ace = encode(image);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("snapshot-1", MediaKind::Snapshot, &ace));
    runtime.load_media(&media).expect("snapshot is accepted");
    runtime
}

#[test]
#[ignore = "FIXTURE: needs the Jupiter Ace ROM — run with --ignored"]
fn a_snapshot_lands_in_the_right_banks() {
    // $2000-$7FFF. Distinct markers per bank, including in the shadow ranges
    // the file carries but the hardware aliases away.
    let mut image = vec![0u8; 0x6000];
    image[0x0000] = 0xC0; // $2000 — ACE32 config, must NOT survive
    image[0x0400] = 0x41; // $2400 — real video RAM
    image[0x0800] = 0xC1; // $2800 — char shadow, must NOT survive
    image[0x0C00] = 0x42; // $2C00 — real character RAM
    image[0x1000] = 0xC2; // $3000 — mirror, must NOT survive
    image[0x1C00] = 0x43; // $3C00 — real working RAM
    image[0x2000] = 0x44; // $4000 — expansion RAM
    // PC into ROM so the resumed machine runs somewhere sane.
    image[0x0100 + 7 * 4..0x0100 + 7 * 4 + 4].copy_from_slice(&[0xBD, 0x04, 0x07, 0x07]);

    let runtime = runtime_with(&image);
    let machine = runtime.machine().expect("machine");

    assert_eq!(machine.peek(0x2400), 0x41, "video RAM");
    assert_eq!(
        machine.peek(0x2000),
        0x41,
        "the $2000 alias reads video RAM, not the config block it held in the file"
    );
    assert_eq!(machine.peek(0x2C00), 0x42, "character RAM");
    assert_eq!(machine.peek(0x3C00), 0x43, "working RAM");
    assert_eq!(
        machine.peek(0x3000),
        0x43,
        "the $3000 mirror collapses onto $3C00"
    );
    assert_eq!(machine.peek(0x4000), 0x44, "expansion RAM");
}

#[test]
#[ignore = "FIXTURE: needs the Jupiter Ace ROM — run with --ignored"]
fn the_register_block_is_restored() {
    let mut image = vec![0u8; 0x6000];
    // Upper halves non-zero, as real files have them.
    for (slot, value) in [(5usize, 0x04C8u16), (7, 0x04BD), (6, 0x7FFE), (4, 0x3C00)] {
        let at = 0x0100 + slot * 4;
        image[at..at + 2].copy_from_slice(&value.to_le_bytes());
        image[at + 2..at + 4].copy_from_slice(&[0x07, 0x07]);
    }
    let runtime = runtime_with(&image);
    let regs = &runtime.machine().expect("machine").cpu().regs;

    assert_eq!(regs.iy, 0x04C8, "IY is the FORTH inner interpreter");
    assert_eq!(regs.pc, 0x04BD);
    assert_eq!(regs.sp, 0x7FFE);
    assert_eq!(regs.ix, 0x3C00, "IX is the system-variable base");
}

#[test]
#[ignore = "FIXTURE: needs the Jupiter Ace ROM — run with --ignored"]
fn a_malformed_image_is_rejected_at_the_slot_boundary() {
    let Some(rom) = bios() else {
        panic!("Jupiter Ace ROM not found");
    };
    let mut runtime = JupiterAceRuntime::new(Model::Ace16k, rom).expect("build runtime");
    let mut media = MediaSet::new();
    // Decodes to 3 bytes, which is not a legal .ace length.
    media.push(MediaImage::new(
        "snapshot-1",
        MediaKind::Snapshot,
        &[0x01, 0x02, 0x03, 0xED, 0x00],
    ));
    let error = runtime.load_media(&media).expect_err("illegal length");
    assert!(
        matches!(error, emu198x_shell::MachineError::InvalidMedia { ref slot, .. } if slot == "snapshot-1")
    );
}
