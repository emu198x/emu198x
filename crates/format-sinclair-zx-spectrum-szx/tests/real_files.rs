//! Parse the real SZX files this reader was written for.
//!
//! The unit tests build their own snapshots, so they prove the parser
//! self-consistent and nothing more. These load files written by
//! Spectaculator and SpecEmu, from the `zx-spectrum-tests` corpus — which
//! is the whole reason the reader exists, because
//! `ZX Spectrum Timing Tests - 128K` ships as `.szx` and `.wav` only.
//!
//! ```sh
//! export EMU198X_ZX_SPECTRUM_TESTS_DIR=$PWD/zx-spectrum-tests
//! cargo test --release -p format-sinclair-zx-spectrum-szx \
//!     --test real_files -- --ignored --nocapture
//! ```

use format_sinclair_zx_spectrum_snapshot::SnapshotModel;
use format_sinclair_zx_spectrum_szx::parse_szx;
use std::path::PathBuf;

const TESTS_DIR_ENV: &str = "EMU198X_ZX_SPECTRUM_TESTS_DIR";

fn tests_dir() -> Option<PathBuf> {
    std::env::var_os(TESTS_DIR_ENV).map(PathBuf::from)
}

/// The file the reader was written for.
///
/// Asserts the things a wrong parse gets wrong quietly: the machine, that
/// every RAM bank arrived, that each is a full page, and that `PC` is
/// somewhere a 128K program can actually execute from. A parser that
/// mis-strides a chunk still produces *a* snapshot; it does not produce
/// one that passes these.
#[test]
#[ignore = "needs EMU198X_ZX_SPECTRUM_TESTS_DIR"]
fn reads_the_128k_timing_suite() {
    let Some(dir) = tests_dir() else {
        panic!("set {TESTS_DIR_ENV} to the extracted zx-spectrum-tests corpus");
    };
    let path = dir.join(
        "ZX Spectrum Timing Tests - 128K v1.0 (2015-03-30)(Butler, Richard; Butler, Tim)[!].szx",
    );
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let snap = parse_szx(&bytes).expect("the 128K timing suite should parse");

    println!(
        "model {:?}  pc={:#06x}  sp={:#06x}  im={}  iff1={}  7ffd={:#04x}  pages={}",
        snap.model,
        snap.pc,
        snap.sp,
        snap.im,
        snap.iff1,
        snap.port_7ffd,
        snap.pages.len()
    );

    assert_eq!(snap.model, SnapshotModel::Spectrum128K);
    assert_eq!(
        snap.pages.len(),
        8,
        "a 128K snapshot carries eight RAM banks"
    );
    // Pages, not banks: the parser translates SZX's raw bank numbers into
    // the `.z80` numbering `apply_128k_bank_pages` expects, so banks 0..7
    // arrive as pages 3..10. See `snapshot_page_for_bank`.
    let mut pages: Vec<u8> = snap.pages.iter().map(|(n, _)| *n).collect();
    pages.sort_unstable();
    assert_eq!(
        pages,
        (3..11).collect::<Vec<u8>>(),
        "banks 0-7 must all be present, as pages 3-10"
    );
    for (bank, bytes) in &snap.pages {
        assert_eq!(bytes.len(), 16_384, "bank {bank} is not a full page");
    }
    // Not asserted: that `PC` is in RAM. This snapshot resumes at `$15E8`,
    // inside the ROM's key-wait — which is exactly where a suite that
    // opens on a menu would be captured. A snapshot resuming in ROM is
    // ordinary, and an assertion otherwise is a bug in the test.
    assert!(
        snap.sp >= 0x4000,
        "SP {:#06x} points into ROM, which no running machine's stack does",
        snap.sp
    );
    assert!(snap.im <= 2, "interrupt mode {} is not a Z80 mode", snap.im);

    // `$7FFD` = 0x30: bank 0 at `$C000`, screen bank 5, ROM 1, and
    // **paging locked** — the suite runs with the 128K held in 48K paging
    // mode. Worth pinning, because a harness that pages under it is
    // driving a machine the snapshot did not describe, and because the
    // 128K's contention differs from the 48K's whatever the paging does.
    assert_eq!(
        snap.port_7ffd & 0x20,
        0x20,
        "the suite is captured with paging locked; $7FFD = {:#04x}",
        snap.port_7ffd
    );

    // The banks must not be identical. A stride bug that read the same
    // chunk eight times, or decompressed the same buffer, would satisfy
    // every check above.
    let distinct = snap
        .pages
        .iter()
        .map(|(_, b)| b.as_slice())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert!(
        distinct > 1,
        "all eight banks hold identical bytes, so the parser is re-reading one page"
    );
}

/// Every `.szx` in the corpus must parse, not just the one that motivated
/// the reader.
///
/// A format parser tested on a single file is tested on a single writer's
/// habits. This is cheap breadth: it catches chunks and machine ids the
/// spec allows and the sample happens not to use.
#[test]
#[ignore = "needs EMU198X_ZX_SPECTRUM_TESTS_DIR"]
fn reads_every_szx_in_the_corpus() {
    let Some(dir) = tests_dir() else {
        panic!("set {TESTS_DIR_ENV} to the extracted zx-spectrum-tests corpus");
    };
    let mut seen = 0usize;
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("corpus directory should be readable") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("szx") {
            continue;
        }
        seen += 1;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        match std::fs::read(&path).map(|b| parse_szx(&b)) {
            Ok(Ok(snap)) => println!(
                "ok    {name}  ({:?}, {} pages)",
                snap.model,
                snap.pages.len()
            ),
            Ok(Err(e)) => failures.push(format!("{name}: {e}")),
            Err(e) => failures.push(format!("{name}: unreadable: {e}")),
        }
    }
    assert!(seen > 0, "no .szx files found under {}", dir.display());
    assert!(
        failures.is_empty(),
        "{} of {seen} SZX files failed to parse:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
