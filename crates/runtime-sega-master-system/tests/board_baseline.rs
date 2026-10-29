//! Replay original states and all emitted pixels/audio bits at native boundaries.
#[path = "common/board_baseline.rs"]
mod baseline;
#[test]
fn original_version_10_boards_continue_identically() {
    baseline::all(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/sega/board-split-v10"),
        false,
    )
    .expect("original state and full output");
}
