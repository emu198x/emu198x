//! Reference-backed audible output across every Amiga board and restored phase.
#[path = "../examples/paula_live_attachment_board.rs"]
mod probe;

#[test]
fn volume_attachment_retains_output_across_unmute_and_restore() {
    probe::check_all().expect("48 reference-backed restore checkpoints");
}
