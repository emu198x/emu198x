//! Headless Amiga runner — `--script` / `--headless` mode.
//!
//! Boots the chosen Amiga model from Kickstart firmware, runs native
//! frames, executes shared JSON session steps, inserts a DF0 ADF, and
//! captures screenshots / audio / boot-state queries. The
//! non-interactive half of the `emu198x-amiga` binary; the shared
//! launcher routes here through `MachineApp::run_script` when a headless
//! flag is present. The launcher parses the flags; this module runs them.
//! The rich chip-level debugging surface lives in `--mcp` mode.
//!
//! The shared script loop is not used because the report carries boot
//! detection and printed queries it has no shape for, prints a plain
//! summary rather than JSON when no `--script` was given. Every step,
//! `set_machine` included, runs in the shell.

use emu198x_shell::launch::{CommonCli, LaunchError, MachineApp};
use emu198x_shell::{
    BootArtifacts, FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessScript, HeadlessSession,
    MediaImage, MediaKind, MediaSet, ScriptObservation, boot_machine, read_firmware,
    read_media_asset,
};
use runtime_commodore_amiga::{AmigaRuntimeKind, AmigaSessionQueryProvider};
use serde::Serialize;
use serde_json::Value;

// Model selection + Kickstart resolution are the runtime's catalogue,
// resolved by the shell for every mode; script mode used to carry a
// parallel copy whose ROM-candidate lists had drifted (A500+ tried KS1.3
// before KS2.04).
use crate::app::{Amiga, DEFAULT_FLOPPY_SLOT};

#[derive(Debug, Serialize)]
struct RunnerReport {
    observations: Vec<ScriptObservation>,
    time: u64,
    boot_detected: bool,
    boot_reason: String,
    query_values: Vec<ReportedQuery>,
}

#[derive(Debug, Serialize)]
struct ReportedQuery {
    path: String,
    value: Value,
}

/// Headless entry point. Runs the session and prints the JSON (script
/// mode) or summary report.
///
/// # Errors
///
/// Returns the failure of the boot, the media load, the script, a
/// capture, or a query.
pub fn run(app: &Amiga, common: &CommonCli) -> Result<(), LaunchError> {
    let script_mode = common.script.is_some();
    let report = run_cli(app, common)?;
    if script_mode {
        let json = serde_json::to_string(&report)
            .map_err(|err| format!("failed to serialize runner report: {err}"))?;
        println!("{json}");
    } else {
        println!(
            "Amiga runtime: time={} boot_detected={} boot_reason={}",
            report.time, report.boot_detected, report.boot_reason
        );
        for query in &report.query_values {
            println!("{}={}", query.path, query.value);
        }
    }
    Ok(())
}

fn run_cli(cli: &Amiga, common: &CommonCli) -> Result<RunnerReport, LaunchError> {
    if (common.screenshot.is_some() || common.audio_capture.is_some())
        && common.frames == 0
        && common.script.is_none()
        && cli.wait_for_boot.is_none()
    {
        return Err(LaunchError::Usage(
            "capture requests require --frames, --script, or --wait-for-boot so the machine emits output".to_owned(),
        ));
    }

    let model = cli.model;
    let images = read_firmware::<AmigaRuntimeKind>(model, &cli.firmware_overrides())
        .map_err(|err| err.to_string())?;
    let mut firmware = FirmwareSet::new();
    for (id, bytes) in &images {
        firmware.push(FirmwareImage::new(*id, bytes));
    }
    let artifacts = BootArtifacts {
        firmware,
        snapshot: None,
    };

    let machine = boot_machine(
        &artifacts,
        |images| AmigaRuntimeKind::from_firmware(model, images),
        || AmigaRuntimeKind::blank(model),
    )
    .map_err(|err| format!("machine construction failed: {err}"))?;

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        cli.frame_ticks(),
        AmigaSessionQueryProvider,
    );

    let mut media_storage = Vec::new();
    let mut media = MediaSet::new();
    if let Some(path) = &cli.disk {
        let loaded = read_media_asset(path, MediaKind::Disk)
            .map_err(|err| format!("failed to read disk {}: {err}", path.display()))?;
        media_storage.push(loaded);
        let bytes = &media_storage
            .last()
            .expect("media_storage just received one disk image")
            .bytes;
        media.push(MediaImage::new(DEFAULT_FLOPPY_SLOT, MediaKind::Disk, bytes));
    }

    session
        .prepare(&media, &[])
        .map_err(|err| format!("machine preparation failed: {err}"))?;

    let mut observations = Vec::new();
    if let Some(max_frames) = cli.wait_for_boot {
        session
            .wait_for_boot(max_frames)
            .map_err(|err| format!("boot wait failed: {err}"))?;
    }

    if let Some(path) = &common.script {
        let script = HeadlessScript::from_path(path)
            .map_err(|err| format!("failed to load script {}: {err}", path.display()))?;
        // Every step runs in the shared executor: `set_machine` through the
        // family runtime's `MachineCore::set_machine` hook, the keyboard
        // verbs through its `KeyboardTarget`.
        observations.extend(
            script
                .execute_collect(&mut session)
                .map_err(|err| format!("script execution failed: {err}"))?,
        );
    }

    if common.frames > 0 {
        session
            .run_frames(common.frames)
            .map_err(|err| format!("run failed: {err}"))?;
    }

    if let Some(path) = &common.screenshot {
        session
            .save_screenshot(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    if let Some(path) = &common.audio_capture {
        session
            .save_audio_capture(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    let query_values = cli
        .print_queries
        .iter()
        .map(|path| {
            session
                .query(path)
                .map(|query| ReportedQuery {
                    path: path.clone(),
                    value: query.value,
                })
                .map_err(|err| format!("failed to resolve query {path}: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let boot_detected = query_bool(&session, "boot.detected")?;
    let boot_reason = query_string(&session, "boot.reason")?;

    Ok(RunnerReport {
        observations,
        time: session.time().get(),
        boot_detected,
        boot_reason,
        query_values,
    })
}

fn query_bool(
    session: &HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>,
    path: &str,
) -> Result<bool, String> {
    session
        .query(path)
        .map_err(|err| format!("failed to query {path}: {err}"))?
        .value
        .as_bool()
        .ok_or_else(|| format!("query {path} did not resolve to a boolean"))
}

fn query_string(
    session: &HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>,
    path: &str,
) -> Result<String, String> {
    session
        .query(path)
        .map_err(|err| format!("failed to query {path}: {err}"))?
        .value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("query {path} did not resolve to a string"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime_commodore_amiga::Model;
    use std::fs;
    use std::path::PathBuf;

    const ADF_SIZE_DD: usize = 80 * 2 * 11 * 512;

    fn dummy_kickstart() -> Vec<u8> {
        let mut kickstart = vec![0u8; 256 * 1024];
        kickstart[0] = 0x00;
        kickstart[1] = 0x08;
        kickstart[2] = 0x00;
        kickstart[3] = 0x00;
        kickstart[4] = 0x00;
        kickstart[5] = 0xF8;
        kickstart[6] = 0x00;
        kickstart[7] = 0x08;
        kickstart[8] = 0x60;
        kickstart[9] = 0xFE;
        kickstart
    }

    /// A `SetMachine` step in a `--script` run swaps the live variant
    /// through the shell's `set_machine`. Launches a dummy-ROM A500,
    /// runs a one-step script swapping to the AGA A1200, and asserts the
    /// SetMachine observation lands. The swap resolves the A1200
    /// Kickstart by convention, so it skips when that ROM is absent.
    #[test]
    fn script_set_machine_step_swaps_variant() {
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return,
        };
        let a1200_rom = PathBuf::from(&home).join(".emu198x/roms/commodore-amiga/kick31a1200.rom");
        if !a1200_rom.exists() {
            emu198x_test_skip::skip!("A1200 Kickstart not staged: {}", a1200_rom.display());
        }

        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let kickstart_path = temp_dir.join(format!("emu198x-amiga-{pid}-setmachine-kick.rom"));
        let script_path = temp_dir.join(format!("emu198x-amiga-{pid}-setmachine.json"));
        fs::write(&kickstart_path, dummy_kickstart()).expect("write dummy Kickstart");
        fs::write(
            &script_path,
            r#"[{"action":"set_machine","machine":"a1200"}]"#,
        )
        .expect("write script");

        let result = run_cli(
            &Amiga {
                model: Model::A500OcsPal,
                kickstart: Some(kickstart_path.clone()),
                ..Amiga::default()
            },
            &CommonCli {
                script: Some(script_path.clone()),
                ..CommonCli::default()
            },
        )
        .expect("script with SetMachine should run, not error as unsupported");

        let swapped = result.observations.iter().find_map(|obs| match obs {
            ScriptObservation::SetMachine {
                machine,
                profile_id,
                ..
            } => Some((machine.clone(), profile_id.clone())),
            _ => None,
        });
        let (machine, profile_id) = swapped.expect("a SetMachine observation must be emitted");
        assert_eq!(machine, "a1200");
        assert!(
            profile_id.contains("a1200"),
            "swapped profile should be an A1200 variant, got {profile_id}"
        );

        let _ = fs::remove_file(kickstart_path);
        let _ = fs::remove_file(script_path);
    }

    #[test]
    fn run_can_capture_png_and_wav() {
        let temp_dir = std::env::temp_dir();
        let kickstart_path =
            temp_dir.join(format!("emu198x-amiga-{}-kick13.rom", std::process::id()));
        let screenshot_path =
            temp_dir.join(format!("emu198x-amiga-{}-frame.png", std::process::id()));
        let audio_path = temp_dir.join(format!("emu198x-amiga-{}-audio.wav", std::process::id()));
        let disk_path = temp_dir.join(format!("emu198x-amiga-{}-disk.adf", std::process::id()));

        fs::write(&kickstart_path, dummy_kickstart())
            .expect("temporary Kickstart write should succeed");
        fs::write(&disk_path, vec![0u8; ADF_SIZE_DD]).expect("temporary ADF write should succeed");

        let result = run_cli(
            &Amiga {
                model: Model::A500OcsPal,
                kickstart: Some(kickstart_path.clone()),
                disk: Some(disk_path.clone()),
                print_queries: vec!["disk.inserted".to_owned()],
                ..Amiga::default()
            },
            &CommonCli {
                screenshot: Some(screenshot_path.clone()),
                audio_capture: Some(audio_path.clone()),
                frames: 2,
                ..CommonCli::default()
            },
        )
        .expect("runner should capture png and wav");

        assert_eq!(result.query_values.len(), 1);
        assert_eq!(result.query_values[0].path, "disk.inserted");
        assert_eq!(result.query_values[0].value, Value::Bool(true));
        assert!(screenshot_path.is_file());
        assert!(audio_path.is_file());
        let wav = fs::read(&audio_path).expect("wav should be readable");
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert!(
            wav.len() > 44,
            "runtime audio capture should contain sample data, not only a WAV header"
        );

        let _ = fs::remove_file(kickstart_path);
        let _ = fs::remove_file(disk_path);
        let _ = fs::remove_file(screenshot_path);
        let _ = fs::remove_file(audio_path);
    }
}
