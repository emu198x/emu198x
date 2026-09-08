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
            "sega-game-gear-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        std::fs::write(dir.join("monitor.rom"), vec![0; 8192]).expect("synthetic ROM");
        std::fs::write(dir.join("game.rom"), vec![0x5a; 8192]).expect("cartridge");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-sega-game-gear"));
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
    let output = command.output().expect("launch sega-game-gear");
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
fn mcp_accepts_parsed_cartridges_without_exposing_a_console_switch() {
    let fixture = Fixture::new();
    let path = fixture.0.join("game.rom").display().to_string();
    for args in [vec![path.as_str()], vec!["--cart", path.as_str()]] {
        let replies = mcp(
            &fixture,
            &args,
            &[
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":0,"len":1}}}),
            ],
        );
        assert!(
            !replies[0]["result"]["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .any(|t| t["name"] == "set_machine")
        );
        assert_eq!(tool_body(&replies[1])["bytes"], json!([0x5a]));
    }
    assert!(
        !fixture
            .command()
            .arg("--mcp")
            .args(["--model", "sms-ntsc"])
            .output()
            .expect("launch")
            .status
            .success()
    );
}
#[test]
fn script_loads_the_same_cartridge_and_reset_retains_it() {
    let fixture = Fixture::new();
    let report = output(
        fixture
            .script(json!([{"action":"reset"},{"action":"memory_read","addr":0,"len":1}]))
            .arg(fixture.0.join("game.rom"))
            .args(["--model", "sega-game-gear"]),
    );
    assert_eq!(report["cart_loaded"], true);
    assert_eq!(
        report["observations"]
            .as_array()
            .expect("observations")
            .last()
            .expect("read")["bytes"],
        json!([0x5a])
    );
}
