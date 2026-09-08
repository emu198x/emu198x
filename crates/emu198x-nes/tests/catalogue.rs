//! Exercise CLI, script and MCP catalogue selection with synthetic images.
//! Memory reads verify cartridge mapping without requiring game firmware.

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
            "nes-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        let mut rom = vec![0; 16 + 16384 + 8192];
        rom[..4].copy_from_slice(b"NES\x1a");
        rom[4] = 1;
        rom[5] = 1;
        rom[6] = 2;
        rom[16..19].copy_from_slice(&[0x4c, 0, 0x80]);
        rom[16 + 0x3ffc..16 + 0x3ffe].copy_from_slice(&[0, 0x80]);
        std::fs::write(dir.join("game.nes"), rom).expect("cartridge");
        std::fs::write(dir.join("game.sav"), vec![0x5a; 8192]).expect("save");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-nes"));
        // The file convention must work without HOME or installed ROMs.
        command.env_clear();
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
    let output = command.output().expect("launch nes");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON report")
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
fn explicit_catalogue_model_starts_the_same_cartridge_and_save() {
    let fixture = Fixture::new();
    for (id, expected) in [
        ("nintendo-nes-ntsc", "nintendo-nes-ntsc"),
        ("ntsc", "nintendo-nes-ntsc"),
        ("nintendo-nes-pal", "nintendo-nes-pal"),
        ("pal", "nintendo-nes-pal"),
    ] {
        let report = output(
            fixture
                .script(json!([
                    {"action":"query","path":"session.profile.profile_id"},
                    {"action":"memory_read","addr":24576,"len":1}
                ]))
                .args(["--model", id])
                .arg("--rom")
                .arg(fixture.0.join("game.nes")),
        );
        assert_eq!(report["observations"][0]["result"]["value"], expected);
        assert_eq!(report["observations"][1]["bytes"], json!([0x5a]));
    }
}

#[test]
fn mcp_mounts_media_and_save_once_without_firmware_or_home() {
    let fixture = Fixture::new();
    let spec = format!(
        "cartridge-1:cartridge={}",
        fixture.0.join("game.nes").display()
    );
    let responses = mcp(
        &fixture,
        &["--model", "nintendo-nes-ntsc", "--media", &spec],
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":24576,"len":1}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":32768,"len":1}}}),
        ],
    );
    assert!(
        responses[0]["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|t| t["name"] == "set_machine")
    );
    assert_eq!(tool_body(&responses[1])["bytes"], json!([0x5a]));
    assert_eq!(tool_body(&responses[2])["bytes"], json!([0x4c]));
}

#[test]
fn blank_mcp_is_allowed_and_unknown_models_are_rejected() {
    let fixture = Fixture::new();
    assert!(
        fixture
            .command()
            .arg("--mcp")
            .output()
            .expect("MCP")
            .status
            .success()
    );
    assert!(
        !fixture
            .command()
            .args(["--mcp", "--model", "dendy"])
            .output()
            .expect("MCP")
            .status
            .success()
    );
    assert!(
        !fixture
            .command()
            .args(["--mcp", "--rom", "missing.nes"])
            .output()
            .expect("MCP")
            .status
            .success()
    );
    assert!(
        !fixture
            .command()
            .args([
                "--mcp",
                "--battery-save",
                "missing.sav",
                "--no-battery-save"
            ])
            .output()
            .expect("MCP")
            .status
            .success()
    );
}
