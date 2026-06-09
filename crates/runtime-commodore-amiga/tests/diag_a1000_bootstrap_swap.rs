//! Diagnostic: drive the real A1000 bootstrap path through WOM lock,
//! then perform the Kickstart-disk to Workbench-disk swap using the
//! shared headless script layer.
//!
//! Run with:
//!   cargo test -p runtime-commodore-amiga --test diag_a1000_bootstrap_swap \
//!       -- --ignored --nocapture

use std::path::{Path, PathBuf};

use emu198x_shell::{HeadlessScript, HeadlessSession, ScriptStep};
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaOcsRuntime, AmigaSessionQueryProvider, Model,
};

fn bootstrap_rom_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/a1000-bootstrap.rom");
    if !path.exists() {
        eprintln!(
            "skipping: A1000 bootstrap ROM missing at {}",
            path.display()
        );
        return None;
    }
    Some(path)
}

fn workbench_disk_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EMU198X_AMIGA_A1000_WORKBENCH_DISK") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".emu198x/media/commodore-amiga/workbench-1.2.adf");
    if !path.exists() {
        eprintln!("skipping: Workbench 1.2 disk missing at {}", path.display());
        return None;
    }
    Some(path)
}

fn kickstart_disk_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EMU198X_AMIGA_A1000_KICKSTART_DISK") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate dir should have repo-root parents");
    let sibling_archive = repo_root
        .parent()
        .expect("repo root should have a parent")
        .join("Emu198x-docs-archive-2026-04-19/Reference/amiga/Kickstart-Disks/Kickstart-Disk v1.2 r33.180 (1986)(Commodore)(A1000).zip");
    if sibling_archive.exists() {
        return Some(sibling_archive);
    }

    eprintln!("skipping: A1000 Kickstart disk not found; set EMU198X_AMIGA_A1000_KICKSTART_DISK");
    None
}

#[test]
#[ignore = "needs local A1000 bootstrap ROM, Kickstart disk, and Workbench 1.2 disk"]
fn script_swaps_after_a1000_wom_lock() {
    let Some(bootstrap_rom_path) = bootstrap_rom_path() else {
        return;
    };
    let Some(kickstart_disk_path) = kickstart_disk_path() else {
        return;
    };
    let Some(workbench_disk_path) = workbench_disk_path() else {
        return;
    };

    let rom = std::fs::read(&bootstrap_rom_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", bootstrap_rom_path.display()));
    let runtime = AmigaOcsRuntime::new(Model::A1000OcsPal, rom).expect("build A1000 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        A500_PAL_FRAME_TICKS,
        AmigaSessionQueryProvider,
    );
    let screenshot_path = std::env::temp_dir().join("a1000-after-kickstart-swap.png");

    let script = HeadlessScript {
        steps: vec![
            ScriptStep::LoadMedia {
                slot: "floppy-0".to_owned(),
                kind: emu198x_shell::ScriptMediaKind::Disk,
                path: kickstart_disk_path,
                writable: false,
            },
            ScriptStep::WaitForQueryBool {
                path: "amiga.a1000.wom_locked".to_owned(),
                value: true,
                max_frames: 1800,
            },
            ScriptStep::WaitForQueryBool {
                path: "amiga.disk.motor_spinning".to_owned(),
                value: false,
                max_frames: 600,
            },
            ScriptStep::LoadMedia {
                slot: "floppy-0".to_owned(),
                kind: emu198x_shell::ScriptMediaKind::Disk,
                path: workbench_disk_path,
                writable: false,
            },
            ScriptStep::RunFrames { frames: 3000 },
            ScriptStep::SaveScreenshot {
                path: screenshot_path.clone(),
            },
        ],
    };

    let observations = script
        .execute_collect(&mut session)
        .expect("A1000 swap script should execute");

    assert_eq!(
        session
            .query("amiga.a1000.boot_rom_visible")
            .expect("query boot-rom-visible")
            .value,
        serde_json::json!(false)
    );
    assert_eq!(
        session
            .query("amiga.a1000.wom_locked")
            .expect("query wom-locked")
            .value,
        serde_json::json!(true)
    );
    assert!(screenshot_path.is_file());

    println!("observations: {observations:#?}");
    println!("screenshot: {}", screenshot_path.display());
}
