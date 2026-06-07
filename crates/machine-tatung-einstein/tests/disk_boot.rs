//! Tatung Einstein: a real CPCEMU `.DSK` image (the `einstein_flop` / TOSEC
//! "Tatung Einstein TC-01" set) parses and inserts through `insert_cpc_dsk`.
//!
//! Gated on a disk image: set `EMU198X_EINSTEIN_DISK` to a `.dsk`. Run with
//! `--ignored`. The parser's read-back correctness is covered by an in-module
//! unit test against a synthetic DSK; this test confirms the real images in the
//! wild are accepted.
//!
//! NOTE — full OS boot via Ctrl-BREAK is *not* covered here. The MOS reaches
//! its prompt, but the Ctrl-BREAK load path stalls before issuing any FDC
//! command (the keyboard interrupt services once and the FDC is never
//! accessed). That is an Einstein keyboard-interrupt / Z80-daisy-chain
//! integration gap, not a controller gap — see `docs/systems/tatung/einstein.md`.

use std::env;
use std::fs;

use machine_tatung_einstein::{Einstein, EinsteinRegion};

#[test]
#[ignore = "needs EMU198X_EINSTEIN_DISK (a CPCEMU .dsk) — run with --ignored"]
fn real_dsk_parses_and_inserts() {
    let Ok(path) = env::var("EMU198X_EINSTEIN_DISK") else {
        panic!("set EMU198X_EINSTEIN_DISK to a CPCEMU .dsk (e.g. einstein_flop basic.dsk)");
    };
    let dsk = fs::read(&path).expect("read .dsk");

    let mut sys = Einstein::new(vec![0u8; 0x2000], EinsteinRegion::Pal);
    sys.insert_cpc_dsk(0, &dsk)
        .unwrap_or_else(|e| panic!("parse {path}: {e}"));
}
