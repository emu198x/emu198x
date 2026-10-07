//! Explicit live-attachment differential diagnostic. Supply the generated WinUAE CSV.
#[path = "../tests/support/live_attachment.rs"]
mod probe;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("expected generated WinUAE CSV path")?;
    probe::check(&std::fs::read_to_string(path)?)
}
