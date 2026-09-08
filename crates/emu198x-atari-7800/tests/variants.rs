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
            "atari-7800-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        std::fs::write(dir.join("monitor.rom"), vec![0; 8192]).expect("synthetic ROM");
        std::fs::write(dir.join("game.rom"), vec![0x5a; 16384]).expect("cartridge");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-atari-7800"));
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
    let output = command.output().expect("launch atari-7800");
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
fn launch_and_script_select_both_regions_without_losing_the_cartridge() {
    let fixture = Fixture::new();
    for model in runtime_atari_7800::Model::ALL {
        let report = output(
            fixture
                .script(json!([
                    {"action":"set_machine", "machine":model.variant_id()},
                    {"action":"memory_read", "addr":49152, "len":1},
                    {"action":"reset"},
                    {"action":"memory_read", "addr":49152, "len":1}
                ]))
                .args(["--model", model.variant_id()])
                .arg(fixture.0.join("game.rom")),
        );
        assert_eq!(report["cart_loaded"], true);
        assert_eq!(report["observations"][0]["profile_id"], model.profile_id());
        let reads: Vec<_> = report["observations"]
            .as_array()
            .expect("observations")
            .iter()
            .filter(|o| o["kind"] == "memory_read")
            .collect();
        assert_eq!(reads.len(), 2);
        for read in reads {
            assert_eq!(read["bytes"], json!([0x5a]));
        }
    }
}

#[test]
fn mcp_loads_flagged_and_positional_cartridges_and_preserves_failed_switches() {
    let fixture = Fixture::new();
    let path = fixture.0.join("game.rom").display().to_string();
    for args in [vec!["--cart", path.as_str()], vec![path.as_str()]] {
        let responses = mcp(
            &fixture,
            &args,
            &[
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":49152,"len":1}}}),
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"atari-7800-pal"}}}),
                json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"unknown"}}}),
                json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":49152,"len":1}}}),
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
        assert_eq!(
            tool_body(&responses[2])["profile_id"],
            runtime_atari_7800::Model::A7800Pal.profile_id()
        );
        assert_eq!(responses[3]["result"]["isError"], true);
        assert_eq!(tool_body(&responses[4])["bytes"], json!([0x5a]));
    }
}

#[test]
fn mcp_switch_uses_cartridge_bytes_after_the_source_file_is_removed() {
    use std::io::{BufRead, BufReader};
    let fixture = Fixture::new();
    let cart = fixture.0.join("game.rom");
    let mut child = fixture
        .command()
        .arg("--mcp")
        .arg(&cart)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("MCP");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let read = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":49152,"len":1}}});
    writeln!(stdin, "{read}").expect("read request");
    let mut line = String::new();
    stdout.read_line(&mut line).expect("startup complete");
    assert_eq!(
        tool_body(&serde_json::from_str(&line).expect("response"))["bytes"],
        json!([0x5a])
    );
    std::fs::remove_file(cart).expect("remove source");
    let switch = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"atari-7800-pal"}}});
    writeln!(
        stdin,
        "{switch}
{read}"
    )
    .expect("switch and read");
    line.clear();
    stdout.read_line(&mut line).expect("switch response");
    assert_eq!(
        tool_body(&serde_json::from_str(&line).expect("response"))["profile_id"],
        runtime_atari_7800::Model::A7800Pal.profile_id()
    );
    line.clear();
    stdout.read_line(&mut line).expect("read response");
    assert_eq!(
        tool_body(&serde_json::from_str(&line).expect("response"))["bytes"],
        json!([0x5a])
    );
    drop(stdin);
    assert!(child.wait().expect("exit").success());
}

#[test]
fn explicit_missing_cartridges_and_unknown_models_fail_in_both_modes() {
    let fixture = Fixture::new();
    for mode in ["--headless", "--mcp"] {
        for flags in [
            ["--cart", "missing.rom"],
            ["--model", "unknown"],
            ["--region", "secam"],
        ] {
            let result = fixture
                .command()
                .arg(mode)
                .args(flags)
                .output()
                .expect("launch");
            assert!(!result.status.success(), "{mode} {flags:?}");
        }
    }
    let mut blank = fixture.command();
    blank.env_clear();
    assert!(
        blank
            .arg("--mcp")
            .output()
            .expect("blank MCP")
            .status
            .success()
    );
}
