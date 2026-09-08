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
            "dragon-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        for (name, value) in [
            ("dragon32.rom", 0x32),
            ("dragon64-compat.rom", 0x64),
            ("dragon64.rom", 0x6d),
        ] {
            let mut rom = vec![value; 16384];
            rom[..2].copy_from_slice(&[0x20, 0xfe]);
            rom[16382..].copy_from_slice(&[0x80, 0]);
            std::fs::write(dir.join(name), rom).expect("ROM");
        }
        std::fs::write(dir.join("cart.rom"), vec![0x5a; 16384]).expect("cartridge");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-dragon"));
        // The file convention must work without HOME or installed ROMs.
        command.env_clear();
        command.env("EMU198X_DRAGON_ROM_DIR", &self.0);
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
    let output = command.output().expect("launch dragon");
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
fn scripts_resolve_models_and_clear_media_on_switch() {
    let fixture = Fixture::new();
    let report = output(
        fixture
            .script(json!([
                {"action":"memory_read","addr":49152,"len":1},
                {"action":"set_machine","machine":"dragon-64-pal"},
                {"action":"memory_read","addr":32770,"len":1},
                {"action":"memory_read","addr":49152,"len":1}
            ]))
            .arg("--cart")
            .arg(fixture.0.join("cart.rom")),
    );
    assert_eq!(report["observations"][0]["bytes"], json!([0x5a]));
    assert_eq!(report["observations"][1]["profile_id"], "dragon-64-pal");
    assert_eq!(report["observations"][2]["bytes"], json!([0x64]));
    assert_ne!(report["observations"][3]["bytes"], json!([0x5a]));
}

#[test]
fn mcp_honours_model_and_catalogue_pin_without_home() {
    let fixture = Fixture::new();
    let pin = format!(
        "dragon64-compatible-rom={}",
        fixture.0.join("dragon64-compat.rom").display()
    );
    let responses = mcp(
        &fixture,
        &["--model", "dragon64", "--rom", &pin],
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":32770,"len":1}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"dragon32"}}}),
        ],
    );
    assert!(
        responses[0]["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|t| t["name"] == "set_machine")
    );
    assert_eq!(tool_body(&responses[1])["bytes"], json!([0x64]));
    assert_eq!(tool_body(&responses[2])["profile_id"], "dragon-32-pal");
}

#[test]
fn missing_mode_rom_and_bad_pins_are_errors_in_every_launch_mode() {
    let fixture = Fixture::new();
    std::fs::remove_file(fixture.0.join("dragon64.rom")).expect("remove mode ROM");
    for mode in ["--mcp", "--headless"] {
        let result = fixture
            .command()
            .args([mode, "--model", "dragon64"])
            .output()
            .expect("launch");
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("dragon64-basic-rom"));
        assert!(
            !fixture
                .command()
                .args([mode, "--rom", "unknown=missing.rom"])
                .output()
                .expect("launch")
                .status
                .success()
        );
    }
}
