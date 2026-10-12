use emu198x_spectrum_web::basic_tape;
use format198x_sinclair_zx_spectrum_tap::decode;

#[test]
fn shared_basic_tape_preserves_rom_editor_bytes() {
    let sources: Vec<_> = include_str!("fixtures/rom-editor/cases.bas")
        .lines()
        .collect();
    let expected: Vec<_> = include_str!("fixtures/rom-editor/cases.hex")
        .lines()
        .collect();
    assert_eq!(sources.len(), 13, "keep the complete captured corpus");
    assert_eq!(sources.len(), expected.len());
    let mut mismatches = Vec::new();
    for (source, hex) in sources.iter().zip(expected) {
        let want: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("captured hex"))
            .collect();
        match basic_tape(source, "rom-editor") {
            Ok(tape) => {
                let blocks = decode(&tape).expect("TAP checksums and framing");
                assert_eq!(blocks.len(), 2);
                if blocks[1].data != want {
                    mismatches.push(format!("{source}: stored bytes differ"));
                }
            }
            Err(error) => mismatches.push(format!("{source}: {error}")),
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn shared_basic_tape_refuses_rom_rejected_numbers() {
    let sources: Vec<_> = include_str!("fixtures/rom-editor/refused.bas")
        .lines()
        .collect();
    assert!(!sources.is_empty());
    for source in sources {
        assert!(basic_tape(source, "refused").is_err(), "accepted {source}");
    }
}
