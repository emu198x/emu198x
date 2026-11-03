//! Strict opt-in original-ROM regressions. No firmware is bundled.
use emu198x_shell::{FirmwareImage, FirmwareSet};
use runtime_oric_atmos::{Model, OricRuntime};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
const BASIC10: &str = "83d341dab126092f7a6d56f6a92bca0643ff3aa6529d896085004f958c9be2ff";
const BASIC11: &str = "ed28568574716eef5d7c0fde2568d7a47a6e4b1fbca81daff3be05e45723466d";
fn identity(bytes: &[u8], expected: &str) -> Result<(), String> {
    if bytes.len() != 16384 {
        return Err(format!("expected 16384 ROM bytes, got {}", bytes.len()));
    }
    let actual = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual != expected {
        return Err(format!(
            "reference ROM SHA-256 mismatch: {actual}, expected {expected}"
        ));
    }
    Ok(())
}
fn fixture(name: &str, expected: &str) -> Vec<u8> {
    let root = PathBuf::from(std::env::var_os("EMU198X_ORIC_REFERENCE_ROM_DIR").expect(
        "set EMU198X_ORIC_REFERENCE_ROM_DIR to the verified basic10.rom/basic11b.rom directory",
    ));
    let bytes = std::fs::read(root.join(name)).expect("reference ROM must exist");
    identity(&bytes, expected).expect("complete reference ROM identity must match before boot");
    bytes
}
fn screen(runtime: &OricRuntime) -> String {
    let sys = runtime.machine().expect("live original firmware");
    (0..28)
        .map(|row| {
            (0..40)
                .map(|col| {
                    let c = sys.peek(0xbb80 + row * 40 + col) & 0x7f;
                    if (32..127).contains(&c) {
                        char::from(c)
                    } else {
                        ' '
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn boot_and_type(model: Model, rom: &[u8]) -> (String, String) {
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(model.firmware_id(), rom));
    let mut runtime = OricRuntime::from_firmware(model, &firmware)
        .expect("selected model's explicit firmware ID");
    for _ in 0..300 {
        runtime.machine_mut().expect("live").run_frame();
    }
    let boot = screen(&runtime);
    for c in "PRINTASC(STR$(1))\n".chars() {
        let (col, row, shift) = match c {
            'P' => (5, 3, false),
            'R' => (1, 2, false),
            'I' => (5, 1, false),
            'N' => (0, 1, false),
            'T' => (1, 1, false),
            'A' => (6, 5, false),
            'S' => (6, 6, false),
            'C' => (2, 7, false),
            '(' => (3, 1, true),
            ')' => (7, 2, true),
            '$' => (2, 3, true),
            '1' => (0, 5, false),
            '\n' => (7, 5, false),
            _ => panic!("unmapped reference probe key"),
        };
        let sys = runtime.machine_mut().expect("live");
        if shift {
            sys.press_key(4, 4);
        }
        sys.press_key(col, row);
        for _ in 0..4 {
            sys.run_frame();
        }
        sys.release_key(col, row);
        if shift {
            sys.release_key(4, 4);
        }
        for _ in 0..4 {
            sys.run_frame();
        }
    }
    for _ in 0..50 {
        runtime.machine_mut().expect("live").run_frame();
    }
    (boot, screen(&runtime))
}
fn str_result_matches(text: &str, expected: &str) -> bool {
    text.contains("PRINTASC(STR$(1))")
        && !text.contains("ERROR")
        && text.matches("Ready").count() == 2
        && text.lines().any(|line| line.trim() == expected)
}
#[test]
#[ignore = "FIXTURE: needs verified original BASIC 1.0 and 1.1 ROMs; set EMU198X_ORIC_REFERENCE_ROM_DIR"]
fn original_rom_profiles_boot_and_preserve_their_distinct_str_behaviour() {
    for (model, name, hash, banner, result) in [
        (
            Model::Oric1,
            "basic10.rom",
            BASIC10,
            "ORIC EXTENDED BASIC V1.0",
            "2",
        ),
        (
            Model::Atmos,
            "basic11b.rom",
            BASIC11,
            "ORIC EXTENDED BASIC V1.1",
            "32",
        ),
    ] {
        let rom = fixture(name, hash);
        let (boot, text) = boot_and_type(model, &rom);
        assert!(
            boot.contains(banner) && boot.contains("Ready"),
            "{name}: wrong boot screen\n{boot}"
        );
        assert!(
            str_result_matches(&text, result),
            "{name}: wrong ROM-specific STR$ result\n{text}"
        );
        println!("{name}: exact identity, {banner}, Ready and STR$ result {result} verified");
    }
}
#[test]
#[ignore = "FIXTURE: needs verified original BASIC 1.1 ROM; set EMU198X_ORIC_REFERENCE_ROM_DIR"]
fn wrong_rom_and_mutated_images_cannot_pass_the_oric1_reference() {
    let rom = fixture("basic11b.rom", BASIC11);
    assert!(
        identity(&rom, BASIC10).is_err(),
        "Atmos bytes must not pass the Oric-1 identity gate"
    );
    let mut mutated = rom.clone();
    mutated[8192] ^= 1;
    assert!(
        identity(&mutated, BASIC11).is_err(),
        "a changed byte must fail before boot"
    );
    // Deliberately bypass the reference identity gate for this behaviour control.
    // Production still accepts explicitly supplied custom/localised images.
    let (_, text) = boot_and_type(Model::Oric1, &rom);
    assert!(
        str_result_matches(&text, "32"),
        "wrong-ROM control must actually execute Atmos BASIC"
    );
    assert!(
        !str_result_matches(&text, "2"),
        "Atmos must not pass the Oric-1 STR$ regression"
    );
    println!("wrong-ROM identity, one-byte mutation and incorrect STR$ expectation all rejected");
}
