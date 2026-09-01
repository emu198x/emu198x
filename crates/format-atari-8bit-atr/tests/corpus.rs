//! Classify a directory of `.atr` files against the parser.
//!
//! Provisioned by hand from the TOSEC `[ATR]` set: point `EMU198X_ATR_CORPUS`
//! at a directory of extracted images. The set is not redistributable, so this
//! is a fixture like the ROM-gated boot tests.

use std::collections::BTreeMap;
use std::path::PathBuf;

use format_atari_8bit_atr::{AtrImage, BootSectorLayout};

#[test]
#[ignore = "FIXTURE: needs a directory of .atr images at EMU198X_ATR_CORPUS"]
fn the_tosec_set_parses() {
    let Some(dir) = std::env::var_os("EMU198X_ATR_CORPUS").map(PathBuf::from) else {
        emu198x_test_skip::skip!("EMU198X_ATR_CORPUS is not set");
    };

    let mut parsed = 0usize;
    let mut rejected: BTreeMap<String, usize> = BTreeMap::new();
    let mut layouts: BTreeMap<String, usize> = BTreeMap::new();
    let mut sector_sizes: BTreeMap<u16, usize> = BTreeMap::new();
    let mut size_field_agrees = 0usize;

    for entry in std::fs::read_dir(&dir).expect("corpus directory should read") {
        let path = entry.expect("entry").path();
        if path
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("atr"))
        {
            continue;
        }
        let bytes = std::fs::read(&path).expect("image should read");
        match AtrImage::parse(&bytes) {
            Ok(image) => {
                parsed += 1;
                *sector_sizes.entry(image.sector_size()).or_default() += 1;
                let layout = match image.boot_sector_layout() {
                    BootSectorLayout::Logical => "logical",
                    BootSectorLayout::Physical => "physical",
                    BootSectorLayout::Padded => "padded",
                };
                *layouts.entry(layout.to_owned()).or_default() += 1;
                if image.declared_size_agrees() {
                    size_field_agrees += 1;
                }
                assert!(image.sector_count() > 0);
                assert!(image.sector(1).is_some(), "{}", path.display());
                assert!(
                    image.sector(image.sector_count()).is_some(),
                    "{}",
                    path.display()
                );
                assert!(image.sector(image.sector_count() + 1).is_none());
            }
            Err(e) => {
                let kind = e.to_string();
                let kind = kind.split(':').next().unwrap_or(&kind).to_owned();
                *rejected.entry(kind).or_default() += 1;
            }
        }
    }

    let total = parsed + rejected.values().sum::<usize>();
    println!(
        "{total} images: {parsed} parsed, {} rejected",
        total - parsed
    );
    println!("  sector sizes    {sector_sizes:?}");
    println!("  boot layouts    {layouts:?}");
    println!("  size field agrees with the file: {size_field_agrees} of {parsed}");
    for (kind, n) in &rejected {
        println!("  rejected: {n} × {kind}");
    }

    assert!(total > 0, "the corpus directory held no .atr files");
    // The set carries a handful of files that are not ATRs at all. Everything
    // that opens with the magic must parse.
    let not_atr = rejected.get("not an ATR image").copied().unwrap_or(0);
    assert_eq!(
        parsed + not_atr,
        total,
        "every image carrying the magic should parse: {rejected:?}"
    );
}
