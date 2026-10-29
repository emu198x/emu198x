//! Compare all old profiles' live states and restored native output.
#[path = "common/board_baseline.rs"]
mod baseline;
#[test]
fn original_version_10_profiles_continue_identically() {
    baseline::all(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/msx/board-split-v10"),
        false,
    )
    .expect("original states and complete native output");
}
