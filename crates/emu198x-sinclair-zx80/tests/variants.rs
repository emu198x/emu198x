//! Exercise the runtime catalogue through the real CLI and MCP server.
//! A synthetic monitor is sufficient: these tests configure machines but
//! execute no CPU instructions, so they need no external firmware.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "zx80-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        std::fs::write(dir.join("monitor.rom"), vec![0; 4096]).expect("synthetic ROM");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-sinclair-zx80"));
        // The file convention must work without HOME or installed ROMs.
        command.env_clear();
        command.env("EMU198X_ZX80_ROM", self.0.join("monitor.rom"));
        command
    }

    fn script(&self, steps: Value) -> Command {
        let path = self.0.join("script.json");
        std::fs::write(&path, steps.to_string()).expect("script");
        let mut command = self.command();
        command.arg("--script").arg(path).args(["--frames", "0"]);
        command
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn output(command: &mut Command) -> Value {
    let output = command.output().expect("launch ZX80");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON report")
}

#[test]
fn model_defaults_and_ram_overrides_reach_the_launched_machine() {
    let fixture = Fixture::new();
    for (model, ram) in [
        ("sinclair-zx80", 1024),
        ("sinclair-zx80-16k", 16384),
        ("sinclair-zx80-usa", 1024),
    ] {
        let report = output(fixture.script(json!([])).args(["--model", model]));
        assert_eq!(report["ram_bytes"], ram, "{report}");
    }
    let report = output(fixture.script(json!([])).args([
        "--model",
        "sinclair-zx80-usa",
        "--ram-bytes",
        "8192",
    ]));
    assert_eq!(report["ram_bytes"], 8192);
    let bad = fixture
        .script(json!([]))
        .args(["--model", "unknown"])
        .output()
        .expect("launch");
    assert!(!bad.status.success());
}

#[test]
fn an_explicit_rom_wins_over_the_file_environment_convention() {
    let fixture = Fixture::new();
    let report = output(
        fixture
            .script(json!([]))
            .env("EMU198X_ZX80_ROM", fixture.0.join("missing.rom"))
            .arg("--rom")
            .arg(fixture.0.join("monitor.rom")),
    );
    assert_eq!(report["rom_loaded"], true);
}

#[test]
fn a_missing_environment_rom_is_reported() {
    let fixture = Fixture::new();
    let result = fixture
        .script(json!([]))
        .env("EMU198X_ZX80_ROM", fixture.0.join("missing.rom"))
        .output()
        .expect("launch");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("missing.rom"));
}

#[test]
fn script_switches_all_models_and_reports_the_live_ram() {
    let fixture = Fixture::new();
    let report = output(&mut fixture.script(json!([
        {"action":"set_machine", "machine":"sinclair-zx80-16k"},
        {"action":"set_machine", "machine":"sinclair-zx80"},
        {"action":"set_machine", "machine":"sinclair-zx80-usa"}
    ])));
    let observations = report["observations"].as_array().expect("observations");
    assert_eq!(observations.len(), 3);
    for (observation, id) in
        observations
            .iter()
            .zip(["sinclair-zx80-16k", "sinclair-zx80", "sinclair-zx80-usa"])
    {
        assert_eq!(observation["profile_id"], id);
    }
    assert_eq!(
        report["ram_bytes"], 1024,
        "report must follow the live model"
    );
}

#[test]
fn mcp_publishes_and_executes_the_shared_switch_tool() {
    let fixture = Fixture::new();
    let mut child = fixture
        .command()
        .arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP server");
    let mut stdin = child.stdin.take().expect("stdin");
    for request in [
        json!({"jsonrpc":"2.0", "id":1, "method":"tools/list", "params":{}}),
        json!({"jsonrpc":"2.0", "id":2, "method":"tools/call", "params":{"name":"set_machine", "arguments":{"machine":"sinclair-zx80-usa"}}}),
    ] {
        writeln!(stdin, "{request}").expect("request");
    }
    drop(stdin);
    let result = child.wait_with_output().expect("MCP output");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let responses: Vec<Value> = String::from_utf8(result.stdout)
        .expect("UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON response"))
        .collect();
    let listed = responses
        .iter()
        .find(|r| r["id"] == 1)
        .expect("list response");
    assert!(
        listed["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["name"] == "set_machine")
    );
    let switched = responses
        .iter()
        .find(|r| r["id"] == 2)
        .expect("switch response");
    let body: Value = serde_json::from_str(
        switched["result"]["content"][0]["text"]
            .as_str()
            .expect("tool result"),
    )
    .expect("observation");
    assert_eq!(body["profile_id"], "sinclair-zx80-usa", "{switched}");
}

#[test]
fn directory_options_and_named_pins_share_the_resolver() {
    let fixture = Fixture::new();
    std::fs::copy(fixture.0.join("monitor.rom"), fixture.0.join("zx80.rom"))
        .expect("conventional name");
    for use_flag in [true, false] {
        let mut command = fixture.script(json!([]));
        command.env_remove("EMU198X_ZX80_ROM");
        if use_flag {
            command.arg("--rom-dir").arg(&fixture.0);
        } else {
            command.env("EMU198X_ZX80_ROM_DIR", &fixture.0);
        }
        assert_eq!(output(&mut command)["rom_loaded"], true);
    }
    let mut command = fixture.script(json!([]));
    command.env_remove("EMU198X_ZX80_ROM").args([
        "--rom",
        &format!(
            "sinclair-zx80-rom={}",
            fixture.0.join("monitor.rom").display()
        ),
    ]);
    assert_eq!(output(&mut command)["rom_loaded"], true);
}

#[test]
fn mcp_allows_missing_conventional_firmware_but_rejects_bad_requests() {
    let fixture = Fixture::new();
    let result = fixture
        .command()
        .env_remove("EMU198X_ZX80_ROM")
        .args([
            "--mcp",
            "--model",
            "sinclair-zx80-usa",
            "--ram-bytes",
            "8192",
        ])
        .output()
        .expect("blank MCP");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for args in [
        vec!["--rom", "missing.rom"],
        vec!["--rom", "unknown=missing.rom"],
        vec!["--ram-bytes", "3"],
        vec!["--ram-bytes", "0"],
        vec!["--rom-dir", "missing-directory"],
    ] {
        let result = fixture
            .command()
            .env_remove("EMU198X_ZX80_ROM")
            .arg("--mcp")
            .args(&args)
            .output()
            .expect("invalid MCP launch");
        assert!(!result.status.success(), "{args:?} must fail");
    }
    let bad_rom = fixture.0.join("bad.rom");
    std::fs::write(&bad_rom, [0; 8]).expect("invalid ROM");
    for path in [bad_rom, fixture.0.join("missing.rom")] {
        let result = fixture
            .command()
            .env("EMU198X_ZX80_ROM", path)
            .arg("--mcp")
            .output()
            .expect("invalid environment ROM");
        assert!(!result.status.success());
    }
}

#[test]
fn switching_ejects_the_tape_and_reports_the_live_configuration() {
    let fixture = Fixture::new();
    let tape = fixture.0.join("test.o");
    let mut bytes = [0; 12];
    bytes[10] = 0x0c;
    bytes[11] = 0x40;
    std::fs::write(&tape, bytes).expect("cassette image");
    let loaded = output(fixture.script(json!([])).arg("--tape").arg(&tape));
    assert_eq!(loaded["tape_loaded"], true);
    let switched = output(
        fixture
            .script(json!([
                {"action":"set_machine", "machine":"sinclair-zx80-16k"}
            ]))
            .args(["--ram-bytes", "8192"])
            .arg("--tape")
            .arg(tape),
    );
    assert_eq!(switched["tape_loaded"], false);
    assert_eq!(switched["ram_bytes"], 16384);
}
