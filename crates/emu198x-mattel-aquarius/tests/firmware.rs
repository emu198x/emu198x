//! Dual-ROM launch policy through the actual headless and MCP entry points.
use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aquarius-firmware-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("directory");
        for name in ["aquarius.rom", "aquarius-char.rom"] {
            std::fs::write(
                dir.join(name),
                vec![0; if name == "aquarius.rom" { 8192 } else { 2048 }],
            )
            .expect("synthetic ROM");
        }
        Self(dir)
    }
    fn command(&self, mode: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-mattel-aquarius"));
        command.env_clear().arg(mode);
        if mode == "--headless" {
            command.args(["--frames", "0"]);
        }
        command
    }
    fn conventional(&self, mode: &str) -> Command {
        let mut command = self.command(mode);
        command.env("EMU198X_AQUARIUS_BIOS", self.0.join("aquarius.rom"));
        command.env("EMU198X_AQUARIUS_CHAR", self.0.join("aquarius-char.rom"));
        command
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn succeeds(command: &mut Command) {
    let result = command.output().expect("launch");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
fn fails(command: &mut Command) {
    let result = command.output().expect("launch");
    assert!(
        !result.status.success(),
        "explicit firmware failure must not start blank"
    );
}

#[test]
fn legacy_and_shared_firmware_options_load_both_roms_in_both_modes() {
    let fixture = Fixture::new();
    for mode in ["--headless", "--mcp"] {
        succeeds(&mut fixture.conventional(mode));
        succeeds(
            fixture
                .command(mode)
                .arg("--bios")
                .arg(fixture.0.join("aquarius.rom"))
                .arg("--char")
                .arg(fixture.0.join("aquarius-char.rom")),
        );
        succeeds(fixture.command(mode).arg("--rom-dir").arg(&fixture.0));
        succeeds(
            fixture
                .command(mode)
                .env("EMU198X_AQUARIUS_ROM_DIR", &fixture.0),
        );
        succeeds(
            fixture
                .command(mode)
                .args([
                    "--rom",
                    &format!(
                        "mattel-aquarius-rom={}",
                        fixture.0.join("aquarius.rom").display()
                    ),
                    "--rom",
                    &format!(
                        "mattel-aquarius-char-rom={}",
                        fixture.0.join("aquarius-char.rom").display()
                    ),
                ])
                .env("EMU198X_AQUARIUS_BIOS", "missing.rom")
                .env("EMU198X_AQUARIUS_CHAR", "missing.rom"),
        );
    }
}

#[test]
fn explicit_partial_sets_and_invalid_images_never_fall_back_to_blank() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("bad.rom"), [0; 8]).expect("invalid ROM");
    for mode in ["--headless", "--mcp"] {
        for (flag, variable, name) in [
            ("--bios", "EMU198X_AQUARIUS_BIOS", "aquarius.rom"),
            ("--char", "EMU198X_AQUARIUS_CHAR", "aquarius-char.rom"),
        ] {
            fails(fixture.command(mode).arg(flag).arg(fixture.0.join(name)));
            fails(fixture.command(mode).env(variable, fixture.0.join(name)));
            fails(
                fixture
                    .conventional(mode)
                    .env(variable, fixture.0.join("bad.rom")),
            );
            fails(fixture.conventional(mode).env(variable, "missing.rom"));
        }
        for args in [
            ["--rom", "unknown=missing.rom"],
            ["--rom", "ambiguous.rom"],
            ["--rom-dir", "missing-directory"],
        ] {
            fails(fixture.command(mode).args(args));
        }
    }
    succeeds(&mut fixture.command("--mcp"));
    fails(&mut fixture.command("--headless"));
    // One staged conventional ROM without explicit choices may still start blank.
    let home = fixture.0.join("home");
    let dir = home.join(".emu198x/roms/mattel-aquarius");
    std::fs::create_dir_all(&dir).expect("conventional directory");
    std::fs::copy(fixture.0.join("aquarius.rom"), dir.join("aquarius.rom")).expect("staged OS");
    succeeds(fixture.command("--mcp").env("HOME", home));
}

#[test]
fn mcp_loads_both_roms_without_advertising_a_redundant_switch() {
    let fixture = Fixture::new();
    let mut child = fixture
        .conventional("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("server");
    let mut stdin = child.stdin.take().expect("stdin");
    for request in [
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query","arguments":{"path":"bios.loaded"}}}),
    ] {
        writeln!(stdin, "{request}").expect("request");
    }
    drop(stdin);
    let result = child.wait_with_output().expect("output");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let replies: Vec<Value> = String::from_utf8(result.stdout)
        .expect("UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON"))
        .collect();
    assert!(
        !replies[0]["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["name"] == "set_machine")
    );
    let body: Value = serde_json::from_str(
        replies[1]["result"]["content"][0]["text"]
            .as_str()
            .expect("query response"),
    )
    .expect("query");
    assert_eq!(body["result"]["value"], true);
}

fn mcp(fixture: &Fixture, args: &[&str], requests: &[Value]) -> Vec<Value> {
    let mut child = fixture
        .conventional("--mcp")
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
fn cartridge_and_capped_expansion_reach_scripts_and_mcp_and_survive_reset() {
    let fixture = Fixture::new();
    let cart = fixture.0.join("cart.rom");
    std::fs::write(&cart, vec![0x5a; 8192]).expect("cartridge");
    let script = fixture.0.join("script.json");
    std::fs::write(
        &script,
        json!([
            {"action":"poke_byte","addr":16384,"value":90},
            {"action":"memory_read","addr":16384,"len":1},
            {"action":"memory_read","addr":57344,"len":1},
        ])
        .to_string(),
    )
    .expect("script");
    let result = fixture
        .conventional("--headless")
        .arg("--script")
        .arg(script)
        .arg("--cart")
        .arg(&cart)
        .args(["--expansion-kb", "32"])
        .output()
        .expect("script run");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).expect("report");
    assert_eq!(report["expansion_kb"], 16);
    assert_eq!(report["cart_loaded"], true);
    let reads: Vec<_> = report["observations"]
        .as_array()
        .expect("observations")
        .iter()
        .filter(|observation| observation["kind"] == "memory_read")
        .map(|observation| observation["bytes"].clone())
        .collect();
    assert_eq!(reads, vec![json!([0x5a]), json!([0x5a])]);
    let replies = mcp(
        &fixture,
        &[
            "--cart",
            cart.to_str().expect("path"),
            "--expansion-kb",
            "32",
        ],
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"query","arguments":{"path":"expansion.kb"}}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":57344,"len":1}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"reset","arguments":{}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query","arguments":{"path":"cartridge.loaded"}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"query","arguments":{"path":"expansion.kb"}}}),
        ],
    );
    assert_eq!(tool_body(&replies[0])["result"]["value"], 16);
    assert_eq!(tool_body(&replies[1])["bytes"], json!([0x5a]));
    assert_ne!(replies[2]["result"]["isError"], true);
    assert_eq!(tool_body(&replies[3])["result"]["value"], true);
    assert_eq!(tool_body(&replies[4])["result"]["value"], 16);
    fails(
        fixture
            .conventional("--mcp")
            .args(["--cart", "missing.rom"]),
    );
}
