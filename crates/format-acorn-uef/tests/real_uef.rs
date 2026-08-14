//! Demodulate a real UEF tape end to end (parser → shared receiver).
//!
//! Defaults to the `Welcome_B.uef` that ships with the vendored b-em, so it
//! runs without setup wherever the 198x umbrella is checked out. Override with
//! `ACORN_UEF` to point at a different tape. Run with `--ignored --nocapture`
//! to see the recovered Acorn CFS byte stream.

use std::path::PathBuf;

use common_acorn_cassette::{CassetteEvent, CassetteReceiver};

/// The vendored tape, four directories up: `emulators/` is a sibling of the
/// whole Emu198x org container at the 198x umbrella level, so the walk is
/// crate -> crates -> emu198x -> Emu198x -> 198x. `machine-acorn-bbc-micro`
/// reads the same file, and got this wrong by two levels until 2026-08-14 —
/// which is why the count is spelled out here.
fn default_uef() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../emulators/bbc-micro/b-em/tapes/Welcome_B.uef")
}

#[test]
#[ignore = "reads a real .uef from the vendored emulators tree — run with --ignored"]
fn demodulates_a_real_uef() {
    let path = std::env::var("ACORN_UEF").map_or_else(|_| default_uef(), PathBuf::from);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "read the UEF file at {}: {e}. Set ACORN_UEF to override.",
            path.display()
        )
    });

    let tape = format_acorn_uef::parse(&bytes).expect("parse the UEF");
    eprintln!(
        "parsed {} pulse spans; skipped chunk ids: {:04X?}",
        tape.pulses.len(),
        tape.skipped_chunks
    );

    let mut receiver = CassetteReceiver::new();
    receiver.load(tape.pulses);

    let mut recovered = Vec::new();
    let mut carriers = 0u32;
    let mut guard = 0u32;
    while !receiver.finished() {
        receiver.advance(10_000_000, &mut |event| match event {
            CassetteEvent::ByteReady(byte) => recovered.push(byte),
            CassetteEvent::HighTone => carriers += 1,
        });
        guard += 1;
        assert!(guard < 2_000_000, "tape did not finish");
    }

    eprintln!(
        "recovered {} bytes across {} carrier leaders",
        recovered.len(),
        carriers
    );
    let preview = &recovered[..recovered.len().min(48)];
    eprintln!("first bytes: {preview:02X?}");

    // Acorn CFS blocks are introduced by the sync byte 0x2A ('*'), immediately
    // followed by the ASCII filename — a real tape must contain them.
    let sync = recovered
        .iter()
        .position(|&b| b == 0x2A)
        .expect("no CFS sync byte (0x2A) recovered");
    let name: String = recovered[sync + 1..]
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    eprintln!("first CFS file name: {name:?}");
    assert!(!recovered.is_empty());
}
