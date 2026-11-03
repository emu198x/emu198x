//! Preserve eight unchanged v10 boards and Sony's corrected v11 continuation.
#[path = "common/board_baseline.rs"]
mod baseline;
#[test]
fn eight_v10_boards_and_corrected_sony_v11_continue_exactly() {
    baseline::all(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/msx/board-split-v10"),
        false,
    )
    .expect("original states and complete native output");
}
