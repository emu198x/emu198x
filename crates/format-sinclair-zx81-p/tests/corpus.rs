//! The parser against a real preservation corpus.
//!
//! The rules in this crate were derived from the TOSEC ZX81 `[P]` set rather
//! than from a format document, so they are checked back against it. Point
//! `EMU198X_ZX81_P_CORPUS` at a directory of extracted `.p` / `.p81` / `.81`
//! files; the test skips when it is unset.

use std::{env, fs, path::PathBuf};

use format_sinclair_zx81_p::{ParseError, Zx81Image};

fn corpus() -> Option<Vec<PathBuf>> {
    let dir = PathBuf::from(env::var("EMU198X_ZX81_P_CORPUS").ok()?);
    let mut images: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e.to_ascii_lowercase().as_str(), "p" | "p81" | "81"))
                .unwrap_or(false)
        })
        .collect();
    images.sort();
    (!images.is_empty()).then_some(images)
}

/// Nearly all of a real corpus parses, and every rejection is a structural
/// one the format itself defines.
///
/// The point is not the exact count — a corpus can be re-dumped — but that
/// nothing is rejected for a reason the parser invented. An earlier draft
/// required `E_LINE` to equal the file length, as the ZX80 sibling does, and
/// that alone rejected 827 of 1,206 perfectly good images.
#[test]
#[ignore = "needs a .p corpus — set EMU198X_ZX81_P_CORPUS and run with --ignored"]
fn a_real_corpus_parses_except_for_structurally_broken_images() {
    let Some(images) = corpus() else {
        emu198x_test_skip::skip!(
            "no .p corpus — set EMU198X_ZX81_P_CORPUS to a directory of extracted images"
        );
    };

    let total = images.len();
    let mut parsed = 0usize;
    let mut ends_past = 0usize;
    let mut too_long = 0usize;
    let mut not_a_program = 0usize;
    let mut unexpected = Vec::new();

    for path in &images {
        let bytes = fs::read(path).expect("read image");
        match Zx81Image::parse(&bytes) {
            Ok(image) => {
                parsed += 1;
                assert!(
                    image.program().len() <= image.bytes().len(),
                    "{}: program cannot exceed the image",
                    path.display(),
                );
                assert!(
                    image.required_ram_bytes() <= 16 * 1024,
                    "{}: a parsed image must fit a 16 KB machine",
                    path.display(),
                );
            }
            Err(ParseError::EndsPastImage { .. }) => ends_past += 1,
            Err(ParseError::TooLong { .. }) => too_long += 1,
            // A `.p` extension on something that is not a program. The set
            // carries three 512-byte `ZZZ-UNK-karset*.p` files whose `E_LINE`
            // reads below `$4009`: they are character-set dumps (64 glyphs of
            // 8 bytes), not saved programs, and rejecting them is correct.
            Err(ParseError::EndsBeforeStart { .. } | ParseError::TooShort { .. }) => {
                not_a_program += 1;
            }
            Err(other) => unexpected.push(format!("{}: {other}", path.display())),
        }
    }

    println!(
        "corpus: {total} images, {parsed} parsed, {ends_past} truncated, \
         {too_long} over 16 KB, {not_a_program} not programs"
    );

    assert!(
        unexpected.is_empty(),
        "images rejected for reasons the format does not define: {unexpected:#?}",
    );
    assert!(
        parsed * 100 / total >= 98,
        "only {parsed} of {total} parsed; the rules have drifted from the corpus",
    );
}
