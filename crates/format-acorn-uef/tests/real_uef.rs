//! Demodulate a real UEF tape end to end (parser → shared receiver).
//!
//! Ignored by default; point `ACORN_UEF` at a `.uef` file and run with
//! `--ignored --nocapture` to see the recovered Acorn CFS byte stream.

use common_acorn_cassette::{CassetteEvent, CassetteReceiver};

#[test]
#[ignore = "needs a real .uef via the ACORN_UEF env var"]
fn demodulates_a_real_uef() {
    let path = std::env::var("ACORN_UEF").expect("set ACORN_UEF to a .uef path");
    let bytes = std::fs::read(&path).expect("read the UEF file");

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
