//! Cassette SAVE flush for the Acorn Atom (#375).
//!
//! The end-to-end SAVE -> LOAD data round-trip (capture -> demodulate -> UEF ->
//! parse -> demodulate) is proven in `format-acorn-uef`; here we just check the
//! runtime's flush wiring.

use runtime_acorn_atom::{AtomRuntime, Model};

fn runtime() -> AtomRuntime {
    AtomRuntime::new(Model::AtomFull, vec![0u8; 24 * 1024])
        .expect("synthetic ROM builds the machine")
}

#[test]
fn flush_tape_image_is_none_when_nothing_was_saved() {
    let mut rt = runtime();
    assert!(
        rt.flush_tape_image().is_none(),
        "no SAVE captured yet -> no tape image to write"
    );
}
