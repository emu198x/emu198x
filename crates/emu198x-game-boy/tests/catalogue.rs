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
            "game-boy-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        let mut rom = vec![0; 0x8000];
        rom[0x100..0x102].copy_from_slice(&[0x18, 0xfe]);
        rom[0x147] = 3;
        rom[0x149] = 2;
        rom[0x14d] = rom[0x134..=0x14c]
            .iter()
            .fold(0u8, |sum, b: &u8| sum.wrapping_sub(*b).wrapping_sub(1));
        std::fs::write(dir.join("game.gb"), rom).expect("cartridge");
        std::fs::write(dir.join("game.sav"), vec![0x5a; 8192]).expect("save");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-game-boy"));
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
    let output = command.output().expect("launch game-boy");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON report")
}

fn mcp(command: &mut Command, args: &[&str], requests: &[Value]) -> Vec<Value> {
    let mut child = command
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
fn script_selects_every_profile_and_keeps_the_loaded_save() {
    let fixture = Fixture::new();
    for model in runtime_nintendo_game_boy::Model::ALL {
        let report = output(
            fixture
                .script(json!([
                    {"action":"set_machine","machine":model.profile_id()},
                    {"action":"poke_byte","addr":0,"value":10},
                    {"action":"memory_read","addr":40960,"len":1}
                ]))
                .arg("--rom")
                .arg(fixture.0.join("game.gb")),
        );
        assert_eq!(report["observations"][0]["profile_id"], model.profile_id());
        assert_eq!(report["observations"][2]["bytes"], json!([0x5a]));
    }
    assert_eq!(
        std::fs::read(fixture.0.join("game.sav")).expect("save"),
        vec![0x5a; 8192]
    );
}

#[test]
fn mcp_loads_the_cartridge_and_save_once_and_exposes_selection() {
    let fixture = Fixture::new();
    let responses = mcp(
        &mut fixture.command(),
        &[
            "--rom",
            fixture.0.join("game.gb").to_str().expect("path"),
            "--model",
            "mgb",
        ],
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"poke_byte","arguments":{"addr":0,"value":10}}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":40960,"len":1}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"sgb2"}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"unknown"}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"query","arguments":{"path":"session.profile.profile_id"}}}),
        ],
    );
    assert_eq!(tool_body(&responses[1])["bytes"], json!([0x5a]));
    assert_eq!(
        tool_body(&responses[2])["profile_id"],
        "nintendo-super-game-boy-2"
    );
    assert_eq!(responses[3]["result"]["isError"], true);
    assert_eq!(
        tool_body(&responses[4])["result"]["value"],
        "nintendo-super-game-boy-2"
    );
}

fn call(id: u32, name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    })
}

#[test]
fn native_dmg_startup_distinguishes_boot_firmware_from_a_cartridge() {
    let fixture = Fixture::new();
    let cartridge = fixture.0.join("game.gb");
    let args = ["--rom", cartridge.to_str().expect("path")];
    let conventional = fixture.0.join(".emu198x/roms/nintendo-game-boy");
    let empty = fixture.0.join("empty");
    std::fs::create_dir_all(&conventional).expect("firmware directory");
    std::fs::create_dir(&empty).expect("empty firmware directory");
    let mut firmware = [0; 256];
    firmware[..3].copy_from_slice(&[0xc3, 0xfc, 0]); // JP $00FC
    firmware[0xfc..].copy_from_slice(&[0x3e, 1, 0xe0, 0x50]); // unmap at $0100
    std::fs::write(conventional.join("dmg_boot.bin"), firmware).expect("synthetic firmware");

    let requests = [
        call(1, "query_cpu", json!({})),
        call(2, "memory_read", json!({"addr": 0xff50, "len": 1})),
    ];
    let implicit_firmware = mcp(fixture.command().env("HOME", &fixture.0), &args, &requests);
    assert_eq!(tool_body(&implicit_firmware[0])["registers"]["pc"], "$0000");
    assert_eq!(tool_body(&implicit_firmware[1])["bytes"], json!([0xfe]));

    let skipped = mcp(
        fixture
            .command()
            .env("HOME", &fixture.0)
            .env("EMU198X_GB_ROM_DIR", &empty),
        &args,
        &requests,
    );
    let regs = &tool_body(&skipped[0])["registers"];
    assert_eq!(regs["pc"], "$0100");
    assert_eq!(regs["sp"], "$FFFE");
    assert_eq!(regs["hl"], "$014D");
    assert_eq!(tool_body(&skipped[1])["bytes"], json!([0xff]));
}

#[test]
fn mcp_waits_for_guest_readiness_across_an_lcd_off_interval() {
    let fixture = Fixture::new();
    let cartridge = fixture.0.join("game.gb");
    let mut rom = std::fs::read(&cartridge).expect("synthetic cartridge");
    rom[0x100..0x103].copy_from_slice(&[0xc3, 0x50, 1]); // JP $0150
    let program = [
        0xf0, 0x44, // LDH A, (LY)
        0xfe, 0x90, // CP 144
        0x38, 0xfa, // JR C back to LY
        0xaf, // XOR A
        0xe0, 0x40, // LDH (LCDC), A: LCD off during VBlank
        0x01, 0x00, 0x40, // LD BC, $4000
        0x0b, // DEC BC
        0x78, // LD A, B
        0xb1, // OR C
        0x20, 0xfb, // JR NZ back to DEC BC (longer than one frame attempt)
        0x3e, 0x91, // LD A, $91
        0xe0, 0x40, // LDH (LCDC), A: LCD on
        0xea, 0x00, 0xc0, // LD ($C000), A: readiness sentinel
        0x18, 0xfe, // JR $
    ];
    rom[0x150..0x150 + program.len()].copy_from_slice(&program);
    std::fs::write(&cartridge, rom).expect("startup program");

    let responses = mcp(
        &mut fixture.command(),
        &["--rom", cartridge.to_str().expect("path")],
        &[
            call(1, "query_cpu", json!({})),
            call(2, "run_frames", json!({"frames": 30})),
            call(3, "query", json!({"path": "run.last.stop_reason"})),
            call(4, "memory_read", json!({"addr": 0xc000, "len": 1})),
            call(
                5,
                "run_until_mem_change",
                json!({"addrs": [0xc000], "max_steps": 100_000}),
            ),
            call(6, "run_frames", json!({"frames": 2})),
        ],
    );
    assert_eq!(tool_body(&responses[0])["registers"]["pc"], "$0100");
    assert_eq!(tool_body(&responses[2])["result"]["value"], "no_frame");
    assert_eq!(tool_body(&responses[3])["bytes"], json!([0]));
    let ready = tool_body(&responses[4]);
    assert_eq!(ready["changed"], true);
    assert_eq!(ready["changed_addr"], 0xc000);
    assert_eq!(ready["new"], 0x91);
    assert_eq!(tool_body(&responses[5])["stop_reason"], "ReachedTarget");
}
