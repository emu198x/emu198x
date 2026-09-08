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
            "amstrad-cpc-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        std::fs::write(dir.join("monitor.rom"), vec![0; 32768]).expect("synthetic ROM");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-amstrad-cpc"));
        // The file convention must work without HOME or installed ROMs.
        command.env_clear();
        command.env("EMU198X_CPC464_ROM", self.0.join("monitor.rom"));
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
    let output = command.output().expect("launch amstrad-cpc");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON report")
}

#[test]
fn firmware_options_and_errors_use_the_shared_resolver() {
    let fixture = Fixture::new();
    std::fs::copy(fixture.0.join("monitor.rom"), fixture.0.join("cpc464.rom"))
        .expect("conventional name");
    for directory_flag in [false, true] {
        let mut command = fixture.script(json!([]));
        command.env_remove("EMU198X_CPC464_ROM");
        if directory_flag {
            command.arg("--rom-dir").arg(&fixture.0);
        } else {
            command.env("EMU198X_CPC_ROM_DIR", &fixture.0);
        }
        assert_eq!(output(&mut command)["rom_loaded"], true);
    }
    for spec in [
        fixture.0.join("monitor.rom").display().to_string(),
        format!(
            "{}={}",
            runtime_amstrad_cpc::ROM_FIRMWARE_ID,
            fixture.0.join("monitor.rom").display()
        ),
    ] {
        assert_eq!(
            output(
                fixture
                    .script(json!([]))
                    .env("EMU198X_CPC464_ROM", "missing.rom")
                    .args(["--rom", &spec])
            )["rom_loaded"],
            true
        );
    }
    for mode in ["--headless", "--mcp"] {
        for args in [
            ["--rom", "missing.rom"],
            ["--rom", "unknown=missing.rom"],
            ["--rom-dir", "missing-directory"],
        ] {
            let result = fixture
                .command()
                .env_remove("EMU198X_CPC464_ROM")
                .arg(mode)
                .args(args)
                .output()
                .expect("launch");
            assert!(!result.status.success(), "{mode} {args:?} must fail");
        }
    }
    let blank = fixture
        .command()
        .env_remove("EMU198X_CPC464_ROM")
        .arg("--mcp")
        .output()
        .expect("blank MCP");
    assert!(blank.status.success());
    std::fs::write(fixture.0.join("bad.rom"), [0; 8]).expect("invalid image");
    let bad = fixture
        .command()
        .env("EMU198X_CPC464_ROM", fixture.0.join("bad.rom"))
        .arg("--mcp")
        .output()
        .expect("invalid MCP");
    assert!(!bad.status.success());
}

#[test]
fn legacy_firmware_flag_still_works() {
    let fixture = Fixture::new();
    assert_eq!(
        output(
            fixture
                .script(json!([]))
                .env("EMU198X_CPC464_ROM", "missing.rom")
                .arg("--rom")
                .arg(fixture.0.join("monitor.rom"))
        )["rom_loaded"],
        true
    );
}

fn mcp(fixture: &Fixture, args: &[&str], requests: &[Value]) -> Vec<Value> {
    let mut child = fixture
        .command()
        .arg("--mcp")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP");
    let mut stdin = child.stdin.take().expect("stdin");
    for request in requests {
        writeln!(stdin, "{request}").expect("request");
    }
    drop(stdin);
    let output = child.wait_with_output().expect("output");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON"))
        .collect()
}

fn tool_body(response: &Value) -> Value {
    assert_ne!(response["result"]["isError"], true, "{response}");
    serde_json::from_str(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("text"),
    )
    .expect("body")
}

#[test]
fn single_model_mcp_loads_firmware_without_advertising_switching() {
    let fixture = Fixture::new();
    let replies = mcp(
        &fixture,
        &[],
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query","arguments":{"path":"firmware.loaded"}}}),
        ],
    );
    assert!(
        !replies[0]["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["name"] == "set_machine")
    );
    assert_eq!(tool_body(&replies[1])["result"]["value"], true);
}

#[test]
fn startup_tape_is_loaded_in_scripts_and_mcp_and_survives_reset() {
    let fixture = Fixture::new();
    let path = fixture.0.join("test.cdt");
    let mut tape = b"ZXTape!\x1a\x01\x14".to_vec();
    tape.extend_from_slice(&[0x10, 0, 0, 1, 0, 0xff]);
    std::fs::write(&path, tape).expect("cassette");
    let report = output(fixture.script(json!([])).arg("--tape").arg(&path));
    assert_eq!(report["tape_loaded"], true);
    let replies = mcp(
        &fixture,
        &["--tape", path.to_str().expect("path")],
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"query","arguments":{"path":"tape.loaded"}}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"reset","arguments":{}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query","arguments":{"path":"tape.loaded"}}}),
        ],
    );
    assert_eq!(tool_body(&replies[0])["result"]["value"], true);
    assert_ne!(replies[1]["result"]["isError"], true);
    assert_eq!(tool_body(&replies[2])["result"]["value"], true);
    let missing = fixture
        .command()
        .args(["--mcp", "--tape", "missing.cdt"])
        .output()
        .expect("missing tape");
    assert!(!missing.status.success());
}
