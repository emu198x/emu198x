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
            "electron-firmware-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("directory");
        for name in ["os.rom", "basic.rom"] {
            std::fs::write(dir.join(name), vec![0; 16384]).expect("synthetic ROM");
        }
        Self(dir)
    }
    fn command(&self, mode: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-acorn-electron"));
        command.env_clear().arg(mode);
        if mode == "--headless" {
            command.args(["--frames", "0"]);
        }
        command
    }
    fn conventional(&self, mode: &str) -> Command {
        let mut command = self.command(mode);
        command.env("EMU198X_ELECTRON_OS", self.0.join("os.rom"));
        command.env("EMU198X_ELECTRON_BASIC", self.0.join("basic.rom"));
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
                .arg("--os")
                .arg(fixture.0.join("os.rom"))
                .arg("--basic")
                .arg(fixture.0.join("basic.rom")),
        );
        succeeds(fixture.command(mode).arg("--rom-dir").arg(&fixture.0));
        succeeds(
            fixture
                .command(mode)
                .env("EMU198X_ELECTRON_ROM_DIR", &fixture.0),
        );
        succeeds(
            fixture
                .command(mode)
                .args([
                    "--rom",
                    &format!("acorn-electron-os={}", fixture.0.join("os.rom").display()),
                    "--rom",
                    &format!(
                        "acorn-electron-basic={}",
                        fixture.0.join("basic.rom").display()
                    ),
                ])
                .env("EMU198X_ELECTRON_OS", "missing.rom")
                .env("EMU198X_ELECTRON_BASIC", "missing.rom"),
        );
    }
}

#[test]
fn explicit_partial_sets_and_invalid_images_never_fall_back_to_blank() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("bad.rom"), [0; 8]).expect("invalid ROM");
    for mode in ["--headless", "--mcp"] {
        for (flag, variable, name) in [
            ("--os", "EMU198X_ELECTRON_OS", "os.rom"),
            ("--basic", "EMU198X_ELECTRON_BASIC", "basic.rom"),
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
    let dir = home.join(".emu198x/roms/acorn-electron");
    std::fs::create_dir_all(&dir).expect("conventional directory");
    std::fs::copy(fixture.0.join("os.rom"), dir.join("os.rom")).expect("staged OS");
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
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query","arguments":{"path":"firmware.loaded"}}}),
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
