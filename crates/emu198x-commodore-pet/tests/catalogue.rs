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
            "commodore-pet-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        for (name, size, value) in [
            ("kernal.rom", 4096, 0x4c),
            ("basic.rom", 8192, 0x42),
            ("editor.rom", 2048, 0x45),
            ("chargen.rom", 4096, 0x3c),
        ] {
            let mut bytes = vec![value; size];
            if name == "kernal.rom" {
                bytes[..3].copy_from_slice(&[0x4c, 0x00, 0xf0]);
                bytes[4092..4094].copy_from_slice(&[0x00, 0xf0]);
            }
            std::fs::write(dir.join(name), bytes).expect("firmware");
        }
        std::fs::write(dir.join("hello.prg"), [0x01, 0x04, 0x5a]).expect("PRG");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-commodore-pet"));
        // The file convention must work without HOME or installed ROMs.
        command.env_clear();
        command.env("EMU198X_PET_ROM_DIR", &self.0);
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
    let output = command.output().expect("launch commodore-pet");
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
fn all_roms_resolve_with_pins_and_legacy_flags() {
    let fixture = Fixture::new();
    let cases = [
        (
            "--kernal",
            "commodore-pet-kernal",
            "kernal.rom",
            "EMU198X_PET_KERNAL",
            61440,
            0x4c,
        ),
        (
            "--basic",
            "commodore-pet-basic",
            "basic.rom",
            "EMU198X_PET_BASIC",
            49152,
            0x42,
        ),
        (
            "--editor",
            "commodore-pet-editor",
            "editor.rom",
            "EMU198X_PET_EDITOR",
            57344,
            0x45,
        ),
        (
            "--char",
            "commodore-pet-char",
            "chargen.rom",
            "EMU198X_PET_CHAR",
            61440,
            0x4c,
        ),
    ];
    for (flag, id, file, env, addr, byte) in cases {
        let path = fixture.0.join(file).display().to_string();
        for (flag, spec) in [(flag, path.clone()), ("--rom", format!("{id}={path}"))] {
            let report = output(
                fixture
                    .script(json!([{"action":"memory_read","addr":addr,"len":1}]))
                    .env(env, "missing.rom")
                    .args([flag, &spec]),
            );
            assert_eq!(report["observations"][0]["bytes"], json!([byte]));
        }
    }
    let report = output(
        fixture
            .script(json!([{"action":"memory_read","addr":49152,"len":1}]))
            .env_remove("EMU198X_PET_ROM_DIR")
            .arg("--rom-dir")
            .arg(&fixture.0),
    );
    assert_eq!(report["observations"][0]["bytes"], json!([0x42]));
}

#[test]
fn scripts_report_live_columns_and_switch_clears_queued_program() {
    let fixture = Fixture::new();
    for model in runtime_commodore_pet::Model::ALL {
        let report = output(
            fixture
                .script(json!([
                    {"action":"set_machine","machine":model.variant_id()},
                    {"action":"run_frames","frames":121},
                    {"action":"memory_read","addr":1025,"len":1},
                    {"action":"memory_read","addr":57344,"len":1}
                ]))
                .arg("--prg")
                .arg(fixture.0.join("hello.prg")),
        );
        assert_eq!(report["columns"], model.screen_chars());
        let reads: Vec<_> = report["observations"]
            .as_array()
            .expect("observations")
            .iter()
            .filter(|v| v["kind"] == "memory_read")
            .collect();
        assert_eq!(reads[0]["bytes"], json!([0]));
        assert_eq!(reads[1]["bytes"], json!([0x45]));
    }
    let loaded = output(
        fixture
            .script(json!([
                {"action":"run_frames","frames":121}, {"action":"memory_read","addr":1025,"len":1}
            ]))
            .arg("--prg")
            .arg(fixture.0.join("hello.prg")),
    );
    assert_eq!(loaded["observations"][1]["bytes"], json!([0x5a]));
}

#[test]
fn mcp_loads_prg_and_switches_profiles_without_changing_existing_tools() {
    let fixture = Fixture::new();
    let responses = mcp(
        &fixture,
        &["--prg", fixture.0.join("hello.prg").to_str().expect("path")],
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_frames","arguments":{"frames":121}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":1025,"len":1}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"commodore-pet-80col"}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"unknown"}}}),
            json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":57344,"len":1}}}),
        ],
    );
    assert!(
        responses[0]["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|v| v["name"] == "set_machine")
    );
    assert_eq!(tool_body(&responses[2])["bytes"], json!([0x5a]));
    assert_eq!(
        tool_body(&responses[3])["profile_id"],
        "commodore-pet-80col"
    );
    assert_eq!(responses[4]["result"]["isError"], true);
    assert_eq!(tool_body(&responses[5])["bytes"], json!([0x45]));
}

#[test]
fn explicit_errors_fail_in_both_modes_and_absent_conventions_allow_blank_mcp() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("bad.rom"), [1, 2, 3]).expect("bad ROM");
    for mode in ["--headless", "--mcp"] {
        for flags in [
            ["--columns", "41"],
            ["--rom", "bare.rom"],
            ["--rom", "unknown=missing.rom"],
            ["--kernal", "missing.rom"],
            ["--basic", "missing.rom"],
            ["--editor", "missing.rom"],
            ["--char", "missing.rom"],
            ["--prg", "missing.prg"],
            [
                "--kernal",
                fixture.0.join("bad.rom").to_str().expect("path"),
            ],
        ] {
            assert!(
                !fixture
                    .command()
                    .arg(mode)
                    .args(flags)
                    .output()
                    .expect("launch")
                    .status
                    .success(),
                "{mode} {flags:?}"
            );
        }
        assert!(
            !fixture
                .command()
                .arg(mode)
                .env("EMU198X_PET_EDITOR", "absent.rom")
                .output()
                .expect("launch")
                .status
                .success()
        );
    }
    assert!(
        fixture
            .command()
            .env_clear()
            .arg("--mcp")
            .output()
            .expect("blank")
            .status
            .success()
    );
    assert!(
        !fixture
            .command()
            .env_clear()
            .env("EMU198X_PET_KERNAL", fixture.0.join("kernal.rom"))
            .arg("--mcp")
            .output()
            .expect("partial pin")
            .status
            .success()
    );
}

#[test]
fn failed_firmware_switch_preserves_the_running_machine_and_queued_prg() {
    use std::io::{BufRead, BufReader};
    let fixture = Fixture::new();
    let mut child = fixture
        .command()
        .arg("--mcp")
        .arg("--prg")
        .arg(fixture.0.join("hello.prg"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("MCP");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut call = |name: &str, arguments: Value| -> Value {
        let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}});
        writeln!(stdin, "{request}").expect("request");
        let mut line = String::new();
        stdout.read_line(&mut line).expect("response");
        serde_json::from_str(&line).expect("JSON")
    };
    assert_eq!(
        tool_body(&call("memory_read", json!({"addr":57344,"len":1})))["bytes"],
        json!([0x45])
    );
    std::fs::remove_file(fixture.0.join("editor.rom")).expect("remove conventional firmware");
    assert_eq!(
        call("set_machine", json!({"machine":"commodore-pet-80col"}))["result"]["isError"],
        true
    );
    call("run_frames", json!({"frames":121}));
    assert_eq!(
        tool_body(&call("memory_read", json!({"addr":1025,"len":1})))["bytes"],
        json!([0x5a])
    );
    drop(stdin);
    assert!(child.wait().expect("exit").success());
}
