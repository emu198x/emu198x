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
            "oric-atmos-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        std::fs::write(dir.join("monitor.rom"), vec![0; 16384]).expect("synthetic ROM");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-oric-atmos"));
        // The file convention must work without HOME or installed ROMs.
        command.env_clear();
        command.env("EMU198X_ORIC_ROM", self.0.join("monitor.rom"));
        command.env("EMU198X_ORIC1_ROM", self.0.join("monitor.rom"));
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
    let output = command.output().expect("launch oric-atmos");
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
    for model in runtime_oric_atmos::Model::ALL {
        let report = output(
            fixture
                .script(json!([
                    {"action":"set_machine", "machine":model.variant_id()}
                ]))
                .args([
                    "--model",
                    if model == runtime_oric_atmos::Model::Atmos {
                        "oric-1"
                    } else {
                        "atmos"
                    },
                ]),
        );
        assert_eq!(report["rom_loaded"], true);
        assert_eq!(report["model"], model.variant_id());
        assert_eq!(report["observations"][0]["profile_id"], model.profile_id());
    }
}

#[test]
fn firmware_options_and_errors_use_the_shared_resolver() {
    let fixture = Fixture::new();
    std::fs::copy(fixture.0.join("monitor.rom"), fixture.0.join("oric.rom"))
        .expect("conventional name");
    for directory_flag in [false, true] {
        let mut command = fixture.script(json!([]));
        command
            .env_remove("EMU198X_ORIC_ROM")
            .env_remove("EMU198X_ORIC1_ROM");
        if directory_flag {
            command.arg("--rom-dir").arg(&fixture.0);
        } else {
            command.env("EMU198X_ORIC_ROM_DIR", &fixture.0);
        }
        assert_eq!(output(&mut command)["rom_loaded"], true);
    }
    for spec in [
        fixture.0.join("monitor.rom").display().to_string(),
        format!(
            "{}={}",
            runtime_oric_atmos::BIOS_FIRMWARE_ID,
            fixture.0.join("monitor.rom").display()
        ),
    ] {
        assert_eq!(
            output(
                fixture
                    .script(json!([]))
                    .env("EMU198X_ORIC_ROM", "missing.rom")
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
                .env_remove("EMU198X_ORIC_ROM")
                .env_remove("EMU198X_ORIC1_ROM")
                .arg(mode)
                .args(args)
                .output()
                .expect("launch");
            assert!(!result.status.success(), "{mode} {args:?} must fail");
        }
    }
    let blank = fixture
        .command()
        .env_remove("EMU198X_ORIC_ROM")
        .env_remove("EMU198X_ORIC1_ROM")
        .arg("--mcp")
        .output()
        .expect("blank MCP");
    assert!(blank.status.success());
    std::fs::write(fixture.0.join("bad.rom"), [0; 8]).expect("invalid image");
    let bad = fixture
        .command()
        .env("EMU198X_ORIC_ROM", fixture.0.join("bad.rom"))
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
        json!({"jsonrpc":"2.0", "id":2, "method":"tools/call", "params":{"name":"set_machine", "arguments":{"machine":"atmos"}}}),
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
        runtime_oric_atmos::Model::Atmos.profile_id(),
        "{switched}"
    );
}

#[test]
fn valid_mcp_firmware_pins_are_not_reparsed_as_media() {
    let fixture = Fixture::new();
    for spec in [
        fixture.0.join("monitor.rom").display().to_string(),
        format!(
            "{}={}",
            runtime_oric_atmos::BIOS_FIRMWARE_ID,
            fixture.0.join("monitor.rom").display()
        ),
    ] {
        let result = fixture
            .command()
            .args(["--mcp", "--rom", &spec])
            .output()
            .expect("MCP");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn model_specific_rom_names_win_over_the_legacy_fallback() {
    let fixture = Fixture::new();
    for (name, marker) in [
        ("oric-1.rom", 0x11),
        ("atmos.rom", 0x22),
        ("oric.rom", 0x33),
    ] {
        std::fs::write(fixture.0.join(name), vec![marker; 16384]).expect("ROM");
    }
    for (model, marker) in [
        ("oric-1", 0x11),
        ("oric1", 0x11),
        ("atmos", 0x22),
        ("oric-atmos", 0x22),
    ] {
        let report = output(
            fixture
                .script(json!([{"action":"memory_read","addr":49152,"len":1}]))
                .env_remove("EMU198X_ORIC_ROM")
                .env_remove("EMU198X_ORIC1_ROM")
                .arg("--rom-dir")
                .arg(&fixture.0)
                .args(["--model", model]),
        );
        assert_eq!(report["observations"][0]["bytes"], json!([marker]));
    }
    for name in ["oric-1.rom", "atmos.rom"] {
        std::fs::remove_file(fixture.0.join(name)).expect("remove model ROM");
    }
    let rejected = fixture
        .script(json!([]))
        .env_remove("EMU198X_ORIC_ROM")
        .env_remove("EMU198X_ORIC1_ROM")
        .arg("--rom-dir")
        .arg(&fixture.0)
        .args(["--model", "oric-1"])
        .output()
        .expect("generic-only Oric-1");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("oric1-rom"));
    let report = output(
        fixture
            .script(json!([{"action":"memory_read","addr":49152,"len":1}]))
            .env_remove("EMU198X_ORIC_ROM")
            .env_remove("EMU198X_ORIC1_ROM")
            .arg("--rom-dir")
            .arg(&fixture.0)
            .args(["--model", "atmos"]),
    );
    assert_eq!(report["observations"][0]["bytes"], json!([0x33]));
}

#[test]
fn explicit_firmware_paths_follow_the_final_model_in_either_argument_order() {
    let fixture = Fixture::new();
    let path = fixture.0.join("monitor.rom").display().to_string();
    for model in runtime_oric_atmos::Model::ALL {
        for rom_first in [false, true] {
            let args = if rom_first {
                vec!["--rom", path.as_str(), "--model", model.variant_id()]
            } else {
                vec!["--model", model.variant_id(), "--rom", path.as_str()]
            };
            let report = output(
                fixture
                    .script(json!([{"action":"memory_read","addr":49152,"len":1}]))
                    .env("EMU198X_ORIC_ROM", "missing-atmos.rom")
                    .env("EMU198X_ORIC1_ROM", "missing-oric1.rom")
                    .args(&args),
            );
            assert_eq!(report["model"], model.variant_id());
            assert_eq!(report["observations"][0]["bytes"], json!([0]));
            let mcp = fixture
                .command()
                .env("EMU198X_ORIC_ROM", "missing-atmos.rom")
                .env("EMU198X_ORIC1_ROM", "missing-oric1.rom")
                .arg("--mcp")
                .args(&args)
                .output()
                .expect("MCP explicit firmware");
            assert!(
                mcp.status.success(),
                "{}",
                String::from_utf8_lossy(&mcp.stderr)
            );
        }
    }
}
#[test]
fn explicit_firmware_ids_do_not_change_identity_when_the_model_changes() {
    let fixture = Fixture::new();
    let path = fixture.0.join("monitor.rom").display().to_string();
    for (model, id, wrong) in [
        ("oric-1", "oric1-rom", "oric-rom"),
        ("atmos", "oric-rom", "oric1-rom"),
    ] {
        for mode in ["--headless", "--mcp"] {
            for rom_first in [false, true] {
                let named = format!("{id}={path}");
                let other = format!("{wrong}={path}");
                for (spec, valid) in [(&named, true), (&other, false)] {
                    let args = if rom_first {
                        vec!["--rom", spec.as_str(), "--model", model]
                    } else {
                        vec!["--model", model, "--rom", spec.as_str()]
                    };
                    let result = fixture
                        .command()
                        .arg(mode)
                        .args(["--frames", "0"])
                        .args(args)
                        .output()
                        .expect("named firmware");
                    assert_eq!(
                        result.status.success(),
                        valid,
                        "{mode}/{model}/{spec}: {}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    if !valid {
                        assert!(String::from_utf8_lossy(&result.stderr).contains(wrong));
                    }
                }
            }
        }
    }
}
#[test]
fn model_specific_environment_and_author_filenames_do_not_borrow_atmos() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("oric.rom"), vec![0x33; 16384]).expect("Atmos fallback");
    std::fs::write(fixture.0.join("first.rom"), vec![0x11; 16384]).expect("Oric-1 custom");
    let script = json!([{"action":"memory_read","addr":49152,"len":1}]);
    let report = output(
        fixture
            .script(script.clone())
            .env("EMU198X_ORIC_ROM", "absent.rom")
            .env("EMU198X_ORIC1_ROM", fixture.0.join("first.rom"))
            .args(["--model", "oric-1"]),
    );
    assert_eq!(report["observations"][0]["bytes"], json!([0x11]));
    let rejected = fixture
        .script(script.clone())
        .env_remove("EMU198X_ORIC1_ROM")
        .arg("--rom-dir")
        .arg(&fixture.0)
        .args(["--model", "oric-1"])
        .output()
        .expect("legacy environment is not Oric-1 firmware");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("oric1-rom"));
    for name in ["basic10.rom", "oric1.rom"] {
        std::fs::write(fixture.0.join(name), vec![0x12; 16384]).expect("model-specific filename");
        let report = output(
            fixture
                .script(script.clone())
                .env_remove("EMU198X_ORIC_ROM")
                .env_remove("EMU198X_ORIC1_ROM")
                .arg("--rom-dir")
                .arg(&fixture.0)
                .args(["--model", "oric-1"]),
        );
        assert_eq!(report["observations"][0]["bytes"], json!([0x12]));
        std::fs::remove_file(fixture.0.join(name)).expect("remove candidate");
    }
    std::fs::remove_file(fixture.0.join("oric.rom")).expect("remove legacy name");
    std::fs::write(fixture.0.join("basic11b.rom"), vec![0x22; 16384])
        .expect("author Atmos filename");
    let report = output(
        fixture
            .script(script)
            .env_remove("EMU198X_ORIC_ROM")
            .env_remove("EMU198X_ORIC1_ROM")
            .arg("--rom-dir")
            .arg(&fixture.0)
            .args(["--model", "atmos"]),
    );
    assert_eq!(report["observations"][0]["bytes"], json!([0x22]));
}
