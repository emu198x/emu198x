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
            "jupiter-ace-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        std::fs::write(dir.join("monitor.rom"), vec![0; 8192]).expect("synthetic ROM");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-jupiter-ace"));
        // The file convention must work without HOME or installed ROMs.
        command.env_clear();
        command.env("EMU198X_JUPITER_ACE_ROM", self.0.join("monitor.rom"));
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
    let output = command.output().expect("launch jupiter-ace");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON report")
}

#[test]
fn launch_and_script_select_every_existing_preset() {
    let fixture = Fixture::new();
    for model in runtime_jupiter_ace::Model::ALL {
        let report = output(
            fixture
                .script(json!([
                    {"action":"set_machine", "machine":model.variant_id()}
                ]))
                .args(["--model", model.variant_id()]),
        );
        assert_eq!(report["rom_loaded"], true);
        assert_eq!(report["observations"][0]["profile_id"], model.profile_id());
        assert_eq!(report["ram_kb"], model.ram_kb());
    }
}

#[test]
fn firmware_options_and_errors_use_the_shared_resolver() {
    let fixture = Fixture::new();
    std::fs::copy(fixture.0.join("monitor.rom"), fixture.0.join("ace.rom"))
        .expect("conventional name");
    for directory_flag in [false, true] {
        let mut command = fixture.script(json!([]));
        command.env_remove("EMU198X_JUPITER_ACE_ROM");
        if directory_flag {
            command.arg("--rom-dir").arg(&fixture.0);
        } else {
            command.env("EMU198X_JUPITER_ACE_ROM_DIR", &fixture.0);
        }
        assert_eq!(output(&mut command)["rom_loaded"], true);
    }
    for spec in [
        fixture.0.join("monitor.rom").display().to_string(),
        format!(
            "{}={}",
            runtime_jupiter_ace::BIOS_FIRMWARE_ID,
            fixture.0.join("monitor.rom").display()
        ),
    ] {
        assert_eq!(
            output(
                fixture
                    .script(json!([]))
                    .env("EMU198X_JUPITER_ACE_ROM", "missing.rom")
                    .args(["--rom", &spec])
            )["rom_loaded"],
            true
        );
    }
    for mode in ["--headless", "--mcp"] {
        for args in [
            ["--rom", "missing.rom"],
            ["--rom", "unknown=missing.rom"],
            ["--model", "unknown"],
            ["--rom-dir", "missing-directory"],
        ] {
            let result = fixture
                .command()
                .env_remove("EMU198X_JUPITER_ACE_ROM")
                .arg(mode)
                .args(args)
                .output()
                .expect("launch");
            assert!(!result.status.success(), "{mode} {args:?} must fail");
        }
    }
    let blank = fixture
        .command()
        .env_remove("EMU198X_JUPITER_ACE_ROM")
        .arg("--mcp")
        .output()
        .expect("blank MCP");
    assert!(blank.status.success());
    std::fs::write(fixture.0.join("bad.rom"), [0; 8]).expect("invalid image");
    let bad = fixture
        .command()
        .env("EMU198X_JUPITER_ACE_ROM", fixture.0.join("bad.rom"))
        .arg("--mcp")
        .output()
        .expect("invalid MCP");
    assert!(!bad.status.success());
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
        json!({"jsonrpc":"2.0", "id":2, "method":"tools/call", "params":{"name":"set_machine", "arguments":{"machine":"jupiter-ace-48k"}}}),
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
    assert_eq!(
        body["profile_id"],
        runtime_jupiter_ace::Model::Ace48k.profile_id(),
        "{switched}"
    );
}

#[test]
fn legacy_ram_thresholds_and_last_selector_wins() {
    let fixture = Fixture::new();
    for (value, expected) in [
        ("0", 3),
        ("15", 3),
        ("16", 16),
        ("47", 16),
        ("48", 48),
        ("64", 48),
    ] {
        assert_eq!(
            output(fixture.script(json!([])).args(["--ram-kb", value]))["ram_kb"],
            expected
        );
    }
    assert_eq!(
        output(
            fixture
                .script(json!([]))
                .args(["--ram-kb", "48", "--model", "jupiter-ace-16k"])
        )["ram_kb"],
        16
    );
    assert_eq!(
        output(
            fixture
                .script(json!([]))
                .args(["--model", "jupiter-ace-16k", "--ram-kb", "48"])
        )["ram_kb"],
        48
    );
    let report = output(
        fixture
            .script(json!([{"action":"set_machine","machine":"jupiter-ace-48k"}]))
            .args(["--ram-kb", "3"]),
    );
    assert_eq!(report["ram_kb"], 48, "report follows the live preset");
}
