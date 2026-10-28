//! Exercise CLI, script and MCP catalogue selection with a synthetic
//! cartridge. The cartridge copies pad port 1 into work RAM every pass of
//! its loop, so memory reads show the program ran and saw the pad.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use runtime_sega_mega_drive::Model;
use serde_json::{Value, json};

/// Work RAM, where the program writes.
const RAM: u32 = 0xFF_0000;

/// A cartridge with `region` in its header. The program sets TH as an
/// output, high, then loops: copy port 1's data into `RAM`, count the
/// pass at `RAM + 2`.
fn cartridge(region: &[u8; 3]) -> Vec<u8> {
    let mut rom = vec![0u8; 0x4000];
    rom[0..4].copy_from_slice(&0x00FF_FE00u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x200u32.to_be_bytes());
    rom[0x100..0x104].copy_from_slice(b"SEGA");
    rom[0x1F0..0x1F3].copy_from_slice(region);
    let mut words: Vec<u16> = Vec::new();
    let long = |words: &mut Vec<u16>, v: u32| words.extend([(v >> 16) as u16, v as u16]);
    // MOVE.B #$40,$A10009 (port 1 control); MOVE.B #$40,$A10003 (data).
    words.extend([0x13FC, 0x0040]);
    long(&mut words, 0xA1_0009);
    words.extend([0x13FC, 0x0040]);
    long(&mut words, 0xA1_0003);
    // loop: MOVE.B $A10003,RAM; ADDQ.W #1,RAM+2; BRA.S loop.
    words.push(0x13F9);
    long(&mut words, 0xA1_0003);
    long(&mut words, RAM);
    words.push(0x5279);
    long(&mut words, RAM + 2);
    words.push(0x60EE);
    for (i, w) in words.iter().enumerate() {
        rom[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&w.to_be_bytes());
    }
    rom
}

/// The same image as the Super Magic Drive stores it.
fn to_smd(rom: &[u8]) -> Vec<u8> {
    let mut smd = vec![0; 512];
    smd[0] = (rom.len() / 0x4000) as u8;
    smd[1] = 3;
    smd[8] = 0xAA;
    smd[9] = 0xBB;
    for block in rom.as_chunks::<0x4000>().0 {
        smd.extend(block.iter().skip(1).step_by(2));
        smd.extend(block.iter().step_by(2));
    }
    smd
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "sega-mega-drive-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        std::fs::write(dir.join("game.bin"), cartridge(b"JUE")).expect("cartridge");
        std::fs::write(dir.join("europe.md"), cartridge(b"E  ")).expect("cartridge");
        std::fs::write(dir.join("game.smd"), to_smd(&cartridge(b"JUE"))).expect("cartridge");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-sega-mega-drive"));
        // No firmware: the launch must work without HOME or installed ROMs.
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
    let output = command.output().expect("launch sega-mega-drive");
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

fn memory_reads(report: &Value) -> Vec<Value> {
    report["observations"]
        .as_array()
        .expect("observations")
        .iter()
        .filter(|o| o["kind"] == "memory_read")
        .map(|o| o["bytes"].clone())
        .collect()
}

/// The program runs for a few frames and reads the pad: C pressed clears
/// bit 5 of the TH-high read.
#[test]
fn a_synthetic_cartridge_runs_and_reads_the_pad_through_a_script() {
    let fixture = Fixture::new();
    for image in ["game.bin", "game.smd"] {
        let report = output(
            fixture
                .script(json!([
                    {"action":"run_frames", "frames":3},
                    {"action":"memory_read", "addr":RAM, "len":1},
                    {"action":"input", "events":[{"Button":{"port":1, "name":"c", "pressed":true}}]},
                    {"action":"run_frames", "frames":2},
                    {"action":"memory_read", "addr":RAM, "len":1},
                    {"action":"memory_read", "addr":RAM + 2, "len":2},
                    {"action":"query", "path":"machine.mclk"}
                ]))
                .arg(fixture.0.join(image)),
        );
        assert_eq!(report["cart_loaded"], true, "{image}");
        assert_eq!(report["frames_run"], 5);
        let mclk = report["observations"]
            .as_array()
            .expect("observations")
            .iter()
            .find(|o| o["kind"] == "query")
            .expect("query")["result"]["value"]
            .clone();
        assert_eq!(report["time"], mclk, "{image}: time counts master clocks");
        // Compare the script with native machine boundaries, including the
        // initial partial field; nominal frame durations are not a budget.
        let mut direct =
            runtime_sega_mega_drive::MegaDriveRuntime::new(Model::Genesis, cartridge(b"JUE"));
        let expected_ticks: u64 = (0..5)
            .map(|_| direct.machine_mut().expect("loaded").run_frame())
            .sum();
        assert_eq!(report["time"], expected_ticks);
        let reads = memory_reads(&report);
        assert_eq!(reads[0], json!([0x7F]), "{image}: nothing pressed");
        assert_eq!(reads[1], json!([0x5F]), "{image}: C pressed");
        let passes = reads[2].as_array().expect("bytes");
        assert_ne!(passes, &[json!(0), json!(0)], "{image}: the loop ran");
    }
}

#[test]
fn the_header_chooses_the_model_unless_the_flag_does() {
    let fixture = Fixture::new();
    let variant = |extra: &[&str], image: &str| {
        let report = output(
            fixture
                .script(json!([{"action":"query", "path":"machine.variant"}]))
                .args(extra)
                .arg(fixture.0.join(image)),
        );
        report["observations"][0]["result"]["value"].clone()
    };
    assert_eq!(variant(&[], "europe.md"), json!("mega-drive-europe"));
    assert_eq!(variant(&[], "game.bin"), json!("genesis"));
    assert_eq!(
        variant(&["--model", "mega-drive-japan"], "europe.md"),
        json!("mega-drive-japan")
    );
}

#[test]
fn launch_and_script_select_every_model_without_losing_the_cartridge() {
    let fixture = Fixture::new();
    for model in Model::ALL {
        let report = output(
            fixture
                .script(json!([
                    {"action":"set_machine", "machine":model.variant_id()},
                    {"action":"memory_read", "addr":0x100, "len":4},
                    {"action":"reset"},
                    {"action":"memory_read", "addr":0x100, "len":4}
                ]))
                .args(["--model", model.variant_id()])
                .arg(fixture.0.join("game.smd")),
        );
        assert_eq!(report["cart_loaded"], true);
        assert_eq!(report["observations"][0]["profile_id"], model.profile_id());
        let reads = memory_reads(&report);
        assert_eq!(reads.len(), 2);
        for read in reads {
            assert_eq!(read, json!(b"SEGA".to_vec()));
        }
    }
}

#[test]
fn mcp_loads_flagged_and_positional_cartridges_and_preserves_failed_switches() {
    let fixture = Fixture::new();
    let path = fixture.0.join("game.bin").display().to_string();
    for args in [vec!["--cart", path.as_str()], vec![path.as_str()]] {
        let responses = mcp(
            &fixture,
            &args,
            &[
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":0x100,"len":1}}}),
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"sega-mega-drive-europe"}}}),
                json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"unknown"}}}),
                json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":0x100,"len":1}}}),
            ],
        );
        assert!(
            responses[0]["result"]["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .any(|t| t["name"] == "set_machine")
        );
        assert_eq!(tool_body(&responses[1])["bytes"], json!([b'S']));
        assert_eq!(
            tool_body(&responses[2])["profile_id"],
            Model::MegaDriveEurope.profile_id()
        );
        assert_eq!(responses[3]["result"]["isError"], true);
        assert_eq!(tool_body(&responses[4])["bytes"], json!([b'S']));
    }
}

#[test]
fn explicit_missing_cartridges_and_unknown_models_fail_in_both_modes() {
    let fixture = Fixture::new();
    for mode in ["--headless", "--mcp"] {
        for flags in [
            ["--cart", "missing.bin"],
            ["--model", "unknown"],
            ["--variant", "unknown"],
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
    assert!(
        fixture
            .command()
            .arg("--mcp")
            .output()
            .expect("blank MCP")
            .status
            .success()
    );
}
