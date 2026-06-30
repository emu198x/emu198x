//! Centronics-printer flush for the Acorn Atom (#699).
//!
//! The end-to-end VIA -> /STROBE -> captured-byte path is proven in
//! `machine-acorn-atom`'s `printer.rs`; here we just check the runtime's flush
//! wiring (`--save-print`).

use runtime_acorn_atom::{AtomRuntime, Model};

#[test]
fn flush_printer_output_is_none_when_nothing_was_printed() {
    let mut rt = AtomRuntime::new(Model::AtomFull, vec![0u8; 24 * 1024])
        .expect("synthetic ROM builds the machine");
    assert!(
        rt.flush_printer_output().is_none(),
        "nothing strobed to the printer yet -> no output to write"
    );
}
