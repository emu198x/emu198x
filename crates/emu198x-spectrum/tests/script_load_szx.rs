//! SZX import must restore and execute a program without proprietary firmware.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::{Value, json};

struct FixtureDir(PathBuf);

impl FixtureDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "emu198x-spectrum-szx-{name}-{}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create fixture directory");
        Self(path)
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn chunk(snapshot: &mut Vec<u8>, id: &[u8; 4], body: &[u8]) {
    snapshot.extend_from_slice(id);
    snapshot.extend_from_slice(&u32::try_from(body.len()).expect("chunk size").to_le_bytes());
    snapshot.extend_from_slice(body);
}

fn snapshot_48k() -> Vec<u8> {
    let mut snapshot = b"ZXST\x01\x05\x01\x00".to_vec();
    let mut registers = [0; 37];
    registers[20..22].copy_from_slice(&0xbff0u16.to_le_bytes());
    registers[22..24].copy_from_slice(&0x8000u16.to_le_bytes());
    chunk(&mut snapshot, b"Z80R", &registers);
    chunk(&mut snapshot, b"SPCR", &[0; 8]);
    for bank in [5, 2, 0] {
        let mut page = vec![0; 3 + 16_384];
        page[2] = bank;
        if bank == 2 {
            // At $8000: LD A,$5A; LD ($9000),A; JR $. The destination
            // starts at zero: observing $5A there proves resumed execution.
            page[3..10].copy_from_slice(&[0x3e, 0x5a, 0x32, 0x00, 0x90, 0x18, 0xfe]);
        }
        chunk(&mut snapshot, b"RAMP", &page);
    }
    snapshot
}

#[test]
fn a_script_restores_szx_and_executes_the_restored_program() {
    let dir = FixtureDir::new("resume");
    let snapshot_path = dir.0.join("resume.SZX");
    let snapshot = snapshot_48k();
    fs::write(&snapshot_path, &snapshot).expect("write SZX");
    let rom = dir.0.join("zero.rom");
    fs::write(&rom, [0; 16_384]).expect("write synthetic firmware");
    let script = dir.0.join("resume.json");
    fs::write(
        &script,
        json!([
            // The first load exercises boot pre-scan; the second uses the
            // live-session helper shared with MCP.
            {"action": "load_snapshot", "path": snapshot_path},
            {"action": "memory_read", "addr": 36864, "len": 1},
            {"action": "run_frames", "frames": 1},
            {"action": "memory_read", "addr": 36864, "len": 1},
            {"action": "load_snapshot", "path": snapshot_path},
            {"action": "memory_read", "addr": 36864, "len": 1},
            {"action": "run_frames", "frames": 1},
            {"action": "memory_read", "addr": 36864, "len": 1}
        ])
        .to_string(),
    )
    .expect("write script");
    let output = Command::new(env!("CARGO_BIN_EXE_emu198x-spectrum"))
        .args(["--rom", rom.to_str().expect("ROM path")])
        .args(["--script", script.to_str().expect("script path")])
        .output()
        .expect("run Spectrum");
    assert!(
        output.status.success(),
        "script failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("JSON report");
    let reads: Vec<_> = report["observations"]
        .as_array()
        .expect("observations")
        .iter()
        .filter(|observation| observation["kind"] == "memory_read")
        .map(|observation| observation["bytes"].clone())
        .collect();
    assert_eq!(
        reads,
        [json!([0]), json!([0x5a]), json!([0]), json!([0x5a])]
    );
    assert_eq!(
        fs::read(&snapshot_path).expect("original snapshot"),
        snapshot
    );
}

#[test]
fn malformed_szx_reports_the_parser_error() {
    let dir = FixtureDir::new("malformed");
    let snapshot = dir.0.join("broken.szx");
    // A complete header followed by a truncated chunk header.
    fs::write(&snapshot, b"ZXST\x01\x05\x01\x00Z80R").expect("write malformed SZX");
    let script = dir.0.join("broken.json");
    fs::write(
        &script,
        json!([{"action": "load_snapshot", "path": snapshot}]).to_string(),
    )
    .expect("write script");
    let output = Command::new(env!("CARGO_BIN_EXE_emu198x-spectrum"))
        .arg("--script")
        .arg(&script)
        .output()
        .expect("run Spectrum");
    assert!(!output.status.success(), "malformed snapshot was accepted");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("SZX truncated"), "wrong error: {stderr}");
}
