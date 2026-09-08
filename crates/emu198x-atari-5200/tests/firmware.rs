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
            "atari-5200-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        std::fs::write(dir.join("monitor.rom"), vec![0x3c; 2048]).expect("synthetic ROM");
        std::fs::write(dir.join("game.rom"), vec![0x5a; 32768]).expect("cartridge");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-atari-5200"));
        // The file convention must work without HOME or installed ROMs.
        command.env_clear();
        command.env("EMU198X_A5200_BIOS", self.0.join("monitor.rom"));
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
    let output = command.output().expect("launch atari-5200");
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
fn scripts_load_bios_and_cartridge_and_retain_both_across_reset() {
    let fixture = Fixture::new();
    for positional in [false, true] {
        let mut command = fixture.script(json!([
            {"action":"memory_read","addr":63488,"len":1},
            {"action":"memory_read","addr":16384,"len":1},
            {"action":"reset"},
            {"action":"memory_read","addr":63488,"len":1},
            {"action":"memory_read","addr":16384,"len":1}
        ]));
        if !positional {
            command.arg("--cart");
        }
        let report = output(command.arg(fixture.0.join("game.rom")));
        assert_eq!(report["cart_loaded"], true);
        assert_eq!(report["bios_loaded"], true);
        let reads: Vec<_> = report["observations"]
            .as_array()
            .expect("observations")
            .iter()
            .filter(|o| o["kind"] == "memory_read")
            .map(|o| o["bytes"].clone())
            .collect();
        assert_eq!(
            reads,
            vec![json!([0x3c]), json!([0x5a]), json!([0x3c]), json!([0x5a])]
        );
    }
}

#[test]
fn optional_bios_needs_no_home_or_directory() {
    let fixture = Fixture::new();
    let report = output(
        fixture
            .script(json!([]))
            .env_clear()
            .arg(fixture.0.join("game.rom")),
    );
    assert_eq!(report["cart_loaded"], true);
    assert_eq!(report["bios_loaded"], false);
    assert!(
        fixture
            .command()
            .env_clear()
            .arg("--mcp")
            .output()
            .expect("blank MCP")
            .status
            .success()
    );
    assert!(
        !fixture
            .command()
            .env_clear()
            .arg("--headless")
            .output()
            .expect("cart required")
            .status
            .success()
    );
}

#[test]
fn bios_pins_override_environment_and_both_conventional_names_work() {
    let fixture = Fixture::new();
    let bios = fixture.0.join("monitor.rom").display().to_string();
    for (flag, spec) in [
        ("--bios", bios.clone()),
        ("--rom", bios.clone()),
        ("--rom", format!("atari-5200-bios={bios}")),
    ] {
        let report = output(
            fixture
                .script(json!([{"action":"memory_read","addr":63488,"len":1}]))
                .env("EMU198X_A5200_BIOS", "missing.rom")
                .args([flag, &spec])
                .arg(fixture.0.join("game.rom")),
        );
        assert_eq!(report["observations"][0]["bytes"], json!([0x3c]));
    }
    for filename in ["bios.rom", "5200.rom"] {
        std::fs::copy(fixture.0.join("monitor.rom"), fixture.0.join(filename))
            .expect("conventional name");
        for flag in [false, true] {
            let mut command =
                fixture.script(json!([{"action":"memory_read","addr":63488,"len":1}]));
            command.env_remove("EMU198X_A5200_BIOS");
            if flag {
                command.arg("--rom-dir").arg(&fixture.0);
            } else {
                command.env("EMU198X_A5200_ROM_DIR", &fixture.0);
            }
            let report = output(command.arg(fixture.0.join("game.rom")));
            assert_eq!(report["observations"][0]["bytes"], json!([0x3c]));
        }
        std::fs::remove_file(fixture.0.join(filename)).expect("remove name");
    }
}

#[test]
fn explicit_missing_bios_or_cartridge_fails_in_both_modes() {
    let fixture = Fixture::new();
    for mode in ["--headless", "--mcp"] {
        for flags in [
            ["--bios", "missing.rom"],
            ["--rom", "unknown=missing.rom"],
            ["--cart", "missing.rom"],
            ["--region", "pal"],
        ] {
            let result = fixture
                .command()
                .arg(mode)
                .arg(fixture.0.join("game.rom"))
                .args(flags)
                .output()
                .expect("launch");
            assert!(!result.status.success(), "{mode} {flags:?}");
        }
        let result = fixture
            .command()
            .arg(mode)
            .arg(fixture.0.join("game.rom"))
            .env("EMU198X_A5200_BIOS", "absent.rom")
            .output()
            .expect("launch");
        assert!(!result.status.success());
    }
}

#[test]
fn mcp_loads_bios_and_parsed_cartridge_without_adding_a_switch_tool() {
    let fixture = Fixture::new();
    let cart = fixture.0.join("game.rom").display().to_string();
    let bios = format!(
        "atari-5200-bios={}",
        fixture.0.join("monitor.rom").display()
    );
    for args in [
        vec![cart.as_str()],
        vec!["--cart", cart.as_str(), "--rom", bios.as_str()],
    ] {
        let responses = mcp(
            &fixture,
            &args,
            &[
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":63488,"len":1}}}),
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":16384,"len":1}}}),
            ],
        );
        assert!(
            !responses[0]["result"]["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .any(|t| t["name"] == "set_machine")
        );
        assert_eq!(tool_body(&responses[1])["bytes"], json!([0x3c]));
        assert_eq!(tool_body(&responses[2])["bytes"], json!([0x5a]));
    }
}
