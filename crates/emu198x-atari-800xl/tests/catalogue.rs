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
            "atari-800xl-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        let mut os = vec![0x4c; 16384];
        os[..3].copy_from_slice(&[0x4c, 0, 0xc0]);
        os[16380..16382].copy_from_slice(&[0, 0xc0]);
        std::fs::write(dir.join("atarixl.rom"), os).expect("OS");
        std::fs::write(dir.join("ataribas.rom"), vec![0x42; 8192]).expect("BASIC");
        let mut cart = vec![0x5a; 8192];
        cart[..3].copy_from_slice(&[0x4c, 0, 0xa0]);
        std::fs::write(dir.join("game.rom"), cart).expect("cart");
        let mut disk = vec![0; 16 + 128];
        disk[..6].copy_from_slice(&[0x96, 2, 8, 0, 128, 0]);
        std::fs::write(dir.join("disk.atr"), disk).expect("disk");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-atari-800xl"));
        // The file convention must work without HOME or installed ROMs.
        command.env_clear();
        command.env("EMU198X_A800XL_ROM_DIR", &self.0);
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
    let output = command.output().expect("launch atari-800xl");
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
fn legacy_flags_and_catalogue_pins_override_environment() {
    let fixture = Fixture::new();
    for (flag, id, file, env, addr, byte) in [
        (
            "--os",
            "atari-800xl-os",
            "atarixl.rom",
            "EMU198X_A800XL_OS",
            49152,
            0x4c,
        ),
        (
            "--basic",
            "atari-800xl-basic",
            "ataribas.rom",
            "EMU198X_A800XL_BASIC",
            40960,
            0x42,
        ),
    ] {
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
}

#[test]
fn script_switches_regions_and_keeps_cartridge_without_os_or_home() {
    let fixture = Fixture::new();
    for model in runtime_atari_800xl::Model::ALL {
        let report = output(
            fixture
                .script(json!([
                    {"action":"poke_byte","addr":16384,"value":77},
                    {"action":"set_machine","machine":model.variant_id()},
                    {"action":"memory_read","addr":40963,"len":1},
                    {"action":"memory_read","addr":16384,"len":1}
                ]))
                .env_remove("EMU198X_A800XL_ROM_DIR")
                .args([
                    "--cart",
                    fixture.0.join("game.rom").to_str().expect("path"),
                    "--no-basic",
                ]),
        );
        assert_eq!(report["observations"][1]["profile_id"], model.profile_id());
        assert_eq!(report["observations"][2]["bytes"], json!([0x5a]));
        assert_eq!(report["observations"][3]["bytes"], json!([0]));
        assert_eq!(report["basic_enabled"], false);
    }
}

#[test]
fn mcp_honours_boot_configuration_and_disk_before_switching() {
    let fixture = Fixture::new();
    let responses = mcp(
        &fixture,
        &[
            "--model",
            "atari-800xl-pal",
            "--no-basic",
            "--cart",
            fixture.0.join("game.rom").to_str().expect("path"),
            "--disk",
            fixture.0.join("disk.atr").to_str().expect("path"),
        ],
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":40963,"len":1}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query","arguments":{"path":"disk.loaded"}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"atari-800xl-ntsc"}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"query","arguments":{"path":"basic.enabled"}}}),
            json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"query","arguments":{"path":"disk.loaded"}}}),
        ],
    );
    assert!(
        responses[0]["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["name"] == "set_machine")
    );
    assert_eq!(tool_body(&responses[1])["bytes"], json!([0x5a]));
    assert_eq!(tool_body(&responses[2])["result"]["value"], true);
    assert_eq!(tool_body(&responses[3])["profile_id"], "atari-800xl-ntsc");
    assert_eq!(tool_body(&responses[4])["result"]["value"], false);
    assert_eq!(tool_body(&responses[5])["result"]["value"], true);
}

#[test]
fn optional_firmware_allows_blank_mcp_but_explicit_missing_images_fail() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .env_remove("EMU198X_A800XL_ROM_DIR")
        .arg("--mcp")
        .output()
        .expect("blank MCP");
    assert!(output.status.success());
    for args in [
        vec!["--os", "missing.rom"],
        vec!["--rom", "unknown=missing.rom"],
        vec!["--rom", "ambiguous.rom"],
    ] {
        assert!(
            !fixture
                .command()
                .arg("--mcp")
                .args(args)
                .output()
                .expect("MCP")
                .status
                .success()
        );
    }
}

#[test]
fn failed_switch_keeps_live_ram_disk_and_region() {
    use std::io::{BufRead, BufReader};
    let fixture = Fixture::new();
    let os = fixture.0.join("atarixl.rom");
    let mut child = fixture
        .command()
        .env("EMU198X_A800XL_OS", &os)
        .arg("--mcp")
        .arg("--disk")
        .arg(fixture.0.join("disk.atr"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("MCP");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut call = |name: &str, arguments: Value| -> Value {
        writeln!(stdin, "{}", json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}})).expect("request");
        let mut line = String::new();
        stdout.read_line(&mut line).expect("response");
        serde_json::from_str(&line).expect("JSON")
    };
    tool_body(&call("poke_byte", json!({"addr":16384,"value":77})));
    std::fs::remove_file(os).expect("remove named firmware");
    assert_eq!(
        call("set_machine", json!({"machine":"atari-800xl-pal"}))["result"]["isError"],
        true
    );
    assert_eq!(
        call("set_machine", json!({"machine":"unknown"}))["result"]["isError"],
        true
    );
    assert_eq!(
        tool_body(&call("memory_read", json!({"addr":16384,"len":1})))["bytes"],
        json!([77])
    );
    assert_eq!(
        tool_body(&call("query", json!({"path":"disk.loaded"})))["result"]["value"],
        true
    );
    assert_eq!(
        tool_body(&call("query", json!({"path":"session.profile.profile_id"})))["result"]["value"],
        "atari-800xl-ntsc"
    );
    drop(stdin);
    assert!(child.wait().expect("exit").success());
}
