//! Resume an unmodified real-software session in the production native UI.
//! Usage: review_session <kickstart> <disk> <snapshot> [video-filter]
#[path = "../src/app.rs"]
mod app;
#[path = "../src/mcp/mod.rs"]
mod mcp;
#[path = "../src/script.rs"]
mod script;
#[path = "../src/ui.rs"]
mod ui;

use emu198x_shell::MachineCore;
use emu198x_ui::launch::UiApp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // A local macOS review bundle supplies these same session paths in its
    // Resources folder, so Launch Services can run the native executable.
    if args.is_empty() {
        let executable = std::env::current_exe()?;
        let directory = executable.parent().ok_or("executable has no directory")?;
        args =
            serde_json::from_slice(&std::fs::read(directory.join("../Resources/session.json"))?)?;
    }
    if !(3..=4).contains(&args.len()) {
        return Err("usage: review_session <kickstart> <disk> <snapshot> [video-filter]".into());
    }
    let app = app::Amiga {
        model: runtime_commodore_amiga::Model::A500OcsPalA501,
        kickstart: Some(args[0].clone().into()),
        disk: Some(args[1].clone().into()),
        ..Default::default()
    };
    let mut runtime = app.build_ui_runtime()?;
    runtime.restore(&std::fs::read(&args[2])?)?;
    let filter = args.get(3).map_or("monitor", String::as_str).parse()?;
    emu198x_ui::run(app.ui_system(), runtime, 1, filter, true)?;
    Ok(())
}
