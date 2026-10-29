//! Capture the existing thirteen profiles before deciding their concrete ownership.
#[path = "../tests/common/board_baseline.rs"]
mod baseline;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::path::PathBuf::from(std::env::args_os().nth(1).ok_or("output directory")?);
    std::fs::create_dir(&output)?;
    baseline::all(&output, true)
}
