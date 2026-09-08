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
            "acorn-bbc-micro-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        std::fs::write(dir.join("monitor.rom"), vec![0x4c; 16384]).expect("synthetic ROM");
        std::fs::write(dir.join("game.rom"), vec![0x5a; 16384]).expect("cartridge");
        std::fs::copy(dir.join("monitor.rom"), dir.join("os.rom")).expect("MOS name");
        std::fs::write(dir.join("basic.rom"), vec![0x42; 16384]).expect("BASIC");
        std::fs::write(dir.join("saa5050.rom"), vec![0x3c; 960]).expect("font");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-acorn-bbc-micro"));
        // The file convention must work without HOME or installed ROMs.
        command.env_clear();
        command.env("EMU198X_BBC_MOS", self.0.join("monitor.rom"));
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
    let output = command.output().expect("launch acorn-bbc-micro");
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
fn conventional_basic_is_a_window_default_but_an_explicit_pin_selects_it_in_scripts() {
    let fixture = Fixture::new();
    let steps = json!([{"action":"poke_byte","addr":65072,"value":15},{"action":"memory_read","addr":32768,"len":1}]);
    let report = output(
        fixture
            .script(steps.clone())
            .env("EMU198X_BBC_ROM_DIR", &fixture.0),
    );
    assert_eq!(report["observations"][1]["bytes"], json!([0xff]));
    let pin = format!("acorn-bbc-basic={}", fixture.0.join("basic.rom").display());
    let report = output(fixture.script(steps).args(["--rom", &pin]));
    assert_eq!(report["observations"][1]["bytes"], json!([0x42]));
    assert_eq!(report["sideways_count"], 1);
}

#[test]
fn explicit_sideways_banks_override_basic_and_survive_reset_in_scripts_and_mcp() {
    let fixture = Fixture::new();
    let pin = format!("acorn-bbc-basic={}", fixture.0.join("basic.rom").display());
    let bank = format!("15={}", fixture.0.join("game.rom").display());
    let flags = ["--rom", pin.as_str(), "--sideways", bank.as_str()];
    let report=output(fixture.script(json!([
        {"action":"reset"},{"action":"poke_byte","addr":65072,"value":15},{"action":"memory_read","addr":32768,"len":1}
    ])).args(flags));
    assert_eq!(
        report["observations"]
            .as_array()
            .expect("observations")
            .last()
            .expect("read")["bytes"],
        json!([0x5a])
    );
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"reset","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"poke_byte","arguments":{"addr":65072,"value":15}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":32768,"len":1}}}),
    ];
    for args in [flags.to_vec(), vec!["--sideways", bank.as_str()]] {
        let replies = mcp(&fixture, &args, &requests);
        for r in &replies {
            tool_body(r);
        }
        assert_eq!(tool_body(&replies[2])["bytes"], json!([0x5a]));
    }
}

#[test]
fn mos_pins_override_environment_and_directory_options_work() {
    let fixture = Fixture::new();
    for (flag, value) in [
        ("--mos", fixture.0.join("monitor.rom").display().to_string()),
        (
            "--rom",
            format!("acorn-bbc-mos={}", fixture.0.join("monitor.rom").display()),
        ),
    ] {
        let report = output(
            fixture
                .script(json!([{"action":"memory_read","addr":49152,"len":1}]))
                .env("EMU198X_BBC_MOS", "missing.rom")
                .args([flag, &value]),
        );
        assert_eq!(report["observations"][0]["bytes"], json!([0x4c]));
    }
    for flag in [false, true] {
        let mut command = fixture.script(json!([]));
        command.env_remove("EMU198X_BBC_MOS");
        if flag {
            command.arg("--rom-dir").arg(&fixture.0);
        } else {
            command.env("EMU198X_BBC_ROM_DIR", &fixture.0);
        }
        assert_eq!(output(&mut command)["mos_loaded"], true);
    }
}

#[test]
fn missing_explicit_images_invalid_mos_and_missing_sideways_roms_fail_in_both_modes() {
    let fixture = Fixture::new();
    let bad = fixture.0.join("bad.rom");
    std::fs::write(&bad, [0; 3]).expect("bad MOS");
    for mode in ["--headless", "--mcp"] {
        for flags in [
            ["--mos", "missing.rom"],
            ["--rom", "acorn-bbc-saa5050=missing.rom"],
            ["--rom", "unknown=missing.rom"],
            ["--rom", "ambiguous.rom"],
            ["--sideways", "15=missing.rom"],
        ] {
            assert!(
                !fixture
                    .command()
                    .arg(mode)
                    .args(flags)
                    .output()
                    .expect("launch")
                    .status
                    .success()
            );
        }
        assert!(
            !fixture
                .command()
                .arg(mode)
                .arg("--mos")
                .arg(&bad)
                .output()
                .expect("launch")
                .status
                .success()
        );
        assert!(
            !fixture
                .command()
                .arg(mode)
                .env("EMU198X_BBC_SAA5050", "missing.rom")
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
            .arg("--mcp")
            .env("EMU198X_BBC_BASIC", fixture.0.join("basic.rom"))
            .output()
            .expect("partial firmware")
            .status
            .success()
    );
}
