//! Classify a real `.mtx` corpus against the parser.
//!
//! Point `EMU198X_MTX_CORPUS_DIR` at a directory of `.mtx` files. The parser is
//! expected to *reject* a meaningful share of them: the extension is not the
//! format, and roughly a quarter of the TOSEC set is something else. This test
//! guards the ratio rather than demanding a clean sweep, so a change that
//! quietly starts accepting malformed images fails here.
//!
//! See `reference/by-system/memotech-mtx/memotech-mtx-tape-and-run-formats.md` §3.

use std::env;
use std::fs;
use std::path::PathBuf;

use format_memotech_mtx::TapeImage;

#[test]
#[ignore = "FIXTURE: set EMU198X_MTX_CORPUS_DIR to a directory of .mtx files"]
fn the_corpus_splits_the_way_the_reference_records() {
    let Ok(dir) = env::var("EMU198X_MTX_CORPUS_DIR") else {
        panic!("set EMU198X_MTX_CORPUS_DIR");
    };
    let mut full = 0;
    let mut headerless = 0;
    let mut rejected = 0;
    for entry in fs::read_dir(PathBuf::from(&dir)).expect("read corpus") {
        let path = entry.expect("entry").path();
        if !path.is_file() {
            continue;
        }
        let bytes = fs::read(&path).expect("read image");
        match TapeImage::parse(&bytes) {
            Ok(image) if image.name.is_some() => full += 1,
            Ok(_) => headerless += 1,
            Err(_) => rejected += 1,
        }
    }
    let total = full + headerless + rejected;
    println!("full={full} headerless={headerless} rejected={rejected} total={total}");
    assert!(total > 0, "corpus directory held no files");

    let accepted = full + headerless;
    assert!(
        accepted * 100 / total >= 60,
        "only {accepted}/{total} accepted; the parser has become too strict"
    );
    assert!(
        rejected > 0,
        "nothing was rejected; the parser has stopped validating, and a quarter \
         of files carrying this extension are a different format"
    );
    assert!(
        headerless > 0,
        "no headerless images recognised; the F2 F8 variant is ~14% of the set"
    );
}
