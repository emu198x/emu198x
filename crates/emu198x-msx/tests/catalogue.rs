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
            "msx-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        std::fs::write(dir.join("monitor.rom"), vec![0x3c; 32768]).expect("synthetic ROM");
        std::fs::write(dir.join("game.rom"), vec![0x5a; 32768]).expect("cartridge");
        let mut bios = vec![0x76; 32768];
        // Set PPI mode, select cartridge 1 on page 1, cartridge 2 on page 2
        // and RAM on page 3. Copy one byte from each cartridge to RAM.
        let program = [
            0x3e, 0x82, 0xd3, 0xab, 0x3e, 0xe4, 0xd3, 0xa8, 0x3a, 0x00, 0x40, 0x32, 0x00, 0xc0,
            0x3a, 0x00, 0x80, 0x32, 0x01, 0xc0, 0x76,
        ];
        bios[..program.len()].copy_from_slice(&program);
        std::fs::write(dir.join("msx.rom"), bios).expect("boot BIOS");
        std::fs::write(dir.join("other.rom"), vec![0x42; 65536]).expect("slot 2");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-msx"));
        // The file convention must work without HOME or installed ROMs.
        command.env_clear();
        command.env("EMU198X_MSX_BIOS", self.0.join("monitor.rom"));
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
    let output = command.output().expect("launch msx");
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
fn firmware_pins_and_directory_conventions_work() {
    let fixture = Fixture::new();
    let bios = fixture.0.join("monitor.rom").display().to_string();
    for (flag, spec) in [
        ("--bios", bios.clone()),
        ("--rom", bios.clone()),
        ("--rom", format!("msx1-bios={bios}")),
    ] {
        let report = output(
            fixture
                .script(json!([{"action":"memory_read","addr":0,"len":1}]))
                .env("EMU198X_MSX_BIOS", "missing.rom")
                .args([flag, &spec]),
        );
        assert_eq!(report["observations"][0]["bytes"], json!([0x3c]));
    }
    for flag in [false, true] {
        let mut command = fixture.script(json!([{"action":"memory_read","addr":0,"len":1}]));
        command.env_remove("EMU198X_MSX_BIOS");
        if flag {
            command.arg("--rom-dir").arg(&fixture.0);
        } else {
            command.env("EMU198X_MSX_ROM_DIR", &fixture.0);
        }
        assert_eq!(
            output(&mut command)["observations"][0]["bytes"],
            json!([0x3e])
        );
    }
}

#[test]
fn scripts_switch_both_regions_with_both_cartridges_and_reset() {
    let fixture = Fixture::new();
    for model in runtime_msx::Model::ALL {
        let report = output(
            fixture
                .script(json!([
                    {"action":"set_machine","machine":model.variant_id()},
                    {"action":"run_frames","frames":1},
                    {"action":"memory_read","addr":49152,"len":2},
                    {"action":"reset"},
                    {"action":"run_frames","frames":1},
                    {"action":"memory_read","addr":49152,"len":2}
                ]))
                .env("EMU198X_MSX_BIOS", fixture.0.join("msx.rom"))
                .args([
                    "--model",
                    model.variant_id(),
                    "--mapper",
                    "konami-scc",
                    "--mapper2",
                    "ascii16",
                ])
                .arg("--cart")
                .arg(fixture.0.join("game.rom"))
                .arg("--cart2")
                .arg(fixture.0.join("other.rom")),
        );
        assert_eq!(report["cart1_loaded"], true);
        assert_eq!(report["cart2_loaded"], true);
        assert_eq!(report["observations"][0]["profile_id"], model.profile_id());
        let reads: Vec<_> = report["observations"]
            .as_array()
            .expect("observations")
            .iter()
            .filter(|o| o["kind"] == "memory_read")
            .collect();
        assert_eq!(reads.len(), 2);
        for read in reads {
            assert_eq!(read["bytes"], json!([0x5a, 0x42]));
        }
    }
}

#[test]
fn mcp_loads_both_slots_and_rejects_unknown_switch_without_losing_them() {
    let fixture = Fixture::new();
    let responses = mcp(
        &fixture,
        &[
            "--bios",
            fixture.0.join("msx.rom").to_str().expect("path"),
            "--cart",
            fixture.0.join("game.rom").to_str().expect("path"),
            "--cart2",
            fixture.0.join("other.rom").to_str().expect("path"),
            "--mapper",
            "ascii8",
            "--mapper2",
            "ascii16",
        ],
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_frames","arguments":{"frames":1}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"unknown"}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":49152,"len":2}}}),
        ],
    );
    assert!(
        responses[0]["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|t| t["name"] == "set_machine")
    );
    assert_eq!(responses[2]["result"]["isError"], true);
    assert_eq!(tool_body(&responses[3])["bytes"], json!([0x5a, 0x42]));
}

#[test]
fn explicit_firmware_and_media_errors_fail_in_all_modes_but_mcp_can_start_blank() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("bad.rom"), [1, 2, 3]).expect("invalid BIOS");
    for mode in ["--headless", "--mcp"] {
        for flags in [
            ["--bios", "missing.rom"],
            ["--rom", "unknown=missing.rom"],
            ["--cart", "missing.rom"],
            ["--cart2", "missing.rom"],
            ["--model", "unknown"],
            ["--bios", fixture.0.join("bad.rom").to_str().expect("path")],
        ] {
            let result = fixture
                .command()
                .arg(mode)
                .args(flags)
                .output()
                .expect("launch");
            assert!(!result.status.success(), "{mode} {flags:?}");
        }
        let result = fixture
            .command()
            .arg(mode)
            .env("EMU198X_MSX_BIOS", "absent.rom")
            .output()
            .expect("launch");
        assert!(!result.status.success());
    }
    let blank = fixture
        .command()
        .env_remove("EMU198X_MSX_BIOS")
        .arg("--mcp")
        .output()
        .expect("blank MCP");
    assert!(
        blank.status.success(),
        "{}",
        String::from_utf8_lossy(&blank.stderr)
    );
}

#[test]
fn mcp_switch_retains_in_memory_media_and_failed_firmware_keeps_live_state() {
    use std::io::{BufRead, BufReader};
    let fixture = Fixture::new();
    let bios = fixture.0.join("msx.rom");
    let cart1 = fixture.0.join("game.rom");
    let cart2 = fixture.0.join("other.rom");
    let mut child = fixture
        .command()
        .env("EMU198X_MSX_BIOS", &bios)
        .args(["--mcp", "--mapper", "konami-scc", "--mapper2", "ascii16"])
        .arg("--cart")
        .arg(&cart1)
        .arg("--cart2")
        .arg(&cart2)
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
    let run = call("run_frames", json!({"frames":1}));
    assert_ne!(run["result"]["isError"], true, "{run}");
    let before = call("memory_read", json!({"addr":49152,"len":2}));
    assert_eq!(tool_body(&before)["bytes"], json!([0x5a, 0x42]));
    std::fs::remove_file(cart1).expect("remove slot 1 source");
    std::fs::remove_file(cart2).expect("remove slot 2 source");
    let switched = call("set_machine", json!({"machine":"microsoft-msx1-pal"}));
    assert_eq!(tool_body(&switched)["profile_id"], "microsoft-msx1-pal");
    call("run_frames", json!({"frames":1}));
    let after = call("memory_read", json!({"addr":49152,"len":2}));
    assert_eq!(tool_body(&after)["bytes"], json!([0x5a, 0x42]));
    std::fs::remove_file(bios).expect("remove firmware");
    let failed = call("set_machine", json!({"machine":"microsoft-msx1-ntsc"}));
    assert_eq!(failed["result"]["isError"], true);
    let retained = call("memory_read", json!({"addr":49152,"len":2}));
    assert_eq!(tool_body(&retained)["bytes"], json!([0x5a, 0x42]));
    drop(stdin);
    assert!(child.wait().expect("exit").success());
}
