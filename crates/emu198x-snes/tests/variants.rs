//! Exercise CLI, script and MCP catalogue selection with a synthetic
//! cartridge. The cartridge's NMI handler copies the auto-read pad into
//! work RAM every frame, so memory reads show the program ran and saw the
//! pad.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use runtime_nintendo_snes::Model;
use serde_json::{Value, json};

/// Work RAM the program writes: loop passes, frames, then the pad word.
const LOOP_COUNT: u32 = 0x10;
const JOY1: u32 = 0x12;

/// A 32 KB LoROM. The program turns on NMI and auto-read, then counts
/// loop passes; the NMI handler copies JOY1 into `JOY1`. With a
/// `destination`, it carries a valid header naming that region.
fn cartridge(destination: Option<u8>) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    let program: &[u8] = &[
        0xA9, 0x81, // LDA #$81
        0x8D, 0x00, 0x42, // STA NMITIMEN
        0xEE, 0x10, 0x00, // loop: INC $0010
        0x80, 0xFB, // BRA loop
    ];
    let handler: &[u8] = &[
        0xAD, 0x10, 0x42, // LDA RDNMI
        0xEE, 0x11, 0x00, // INC $0011
        0xAD, 0x12, 0x42, // wait: LDA HVBJOY
        0x29, 0x01, // AND #1
        0xD0, 0xF9, // BNE wait
        0xAD, 0x18, 0x42, // LDA JOY1L
        0x8D, 0x12, 0x00, // STA $0012
        0xAD, 0x19, 0x42, // LDA JOY1H
        0x8D, 0x13, 0x00, // STA $0013
        0x40, // RTI
    ];
    rom[..program.len()].copy_from_slice(program);
    rom[0x100..0x100 + handler.len()].copy_from_slice(handler);
    rom[0x7FFC..0x7FFE].copy_from_slice(&0x8000u16.to_le_bytes());
    rom[0x7FFA..0x7FFC].copy_from_slice(&0x8100u16.to_le_bytes());
    if let Some(destination) = destination {
        let base = 0x7FC0;
        rom[base..base + 21].copy_from_slice(b"EMU198X VARIANT TEST ");
        rom[base + 0x15] = 0x20;
        rom[base + 0x17] = 0x05;
        rom[base + 0x19] = destination;
        // The checksum pair sums to $FFFF whatever the checksum is, so
        // write the pair first and compute over the image that holds it.
        rom[base + 0x1C..base + 0x20].copy_from_slice(&[0xFF, 0xFF, 0, 0]);
        let checksum = rom.iter().fold(0u16, |sum, &b| sum.wrapping_add(b.into()));
        rom[base + 0x1C..base + 0x1E].copy_from_slice(&(!checksum).to_le_bytes());
        rom[base + 0x1E..base + 0x20].copy_from_slice(&checksum.to_le_bytes());
    }
    rom
}

/// The same image behind a 512-byte copier header.
fn with_copier_header(rom: &[u8]) -> Vec<u8> {
    let mut smc = vec![0; 512];
    smc[0] = (rom.len() / 0x2000) as u8;
    smc.extend_from_slice(rom);
    smc
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "nintendo-snes-variants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        std::fs::write(dir.join("game.sfc"), cartridge(None)).expect("cartridge");
        std::fs::write(dir.join("europe.sfc"), cartridge(Some(2))).expect("cartridge");
        std::fs::write(dir.join("japan.sfc"), cartridge(Some(0))).expect("cartridge");
        std::fs::write(dir.join("game.smc"), with_copier_header(&cartridge(None)))
            .expect("cartridge");
        Self(dir)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emu198x-snes"));
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
    let output = command.output().expect("launch emu198x-snes");
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

/// The program runs for a few frames and reads the pad: Start pressed sets
/// bit 12 of the auto-read word.
#[test]
fn a_synthetic_cartridge_runs_and_reads_the_pad_through_a_script() {
    let fixture = Fixture::new();
    for image in ["game.sfc", "game.smc"] {
        let report = output(
            fixture
                .script(json!([
                    {"action":"run_frames", "frames":3},
                    {"action":"memory_read", "addr":JOY1, "len":2},
                    {"action":"input", "events":[{"Button":{"port":1, "name":"start", "pressed":true}}]},
                    {"action":"run_frames", "frames":2},
                    {"action":"memory_read", "addr":JOY1, "len":2},
                    {"action":"memory_read", "addr":LOOP_COUNT, "len":1},
                    {"action":"query", "path":"machine.mclk"}
                ]))
                .arg(fixture.0.join(image)),
        );
        assert_eq!(report["cart_loaded"], true, "{image}");
        assert!(report["frames_run"].as_u64().expect("frames") >= 5);
        let mclk = report["observations"]
            .as_array()
            .expect("observations")
            .iter()
            .find(|o| o["kind"] == "query")
            .expect("query")["result"]["value"]
            .clone();
        assert_eq!(report["time"], mclk, "{image}: time counts master clocks");
        assert!(report["time"].as_u64().expect("time") >= 5 * Model::SnesNtsc.frame_ticks());
        let reads = memory_reads(&report);
        assert_eq!(reads[0], json!([0, 0]), "{image}: nothing pressed");
        assert_eq!(reads[1], json!([0, 0x10]), "{image}: Start pressed");
        assert_ne!(reads[2], json!([0]), "{image}: the loop ran");
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
    assert_eq!(variant(&[], "europe.sfc"), json!("snes-pal"));
    assert_eq!(variant(&[], "japan.sfc"), json!("super-famicom"));
    assert_eq!(variant(&[], "game.sfc"), json!("snes-ntsc"));
    assert_eq!(
        variant(&["--model", "super-famicom"], "europe.sfc"),
        json!("super-famicom")
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
                    {"action":"memory_read", "addr":0x8000, "len":2},
                    {"action":"reset"},
                    {"action":"memory_read", "addr":0x8000, "len":2}
                ]))
                .args(["--model", model.variant_id()])
                .arg(fixture.0.join("game.smc")),
        );
        assert_eq!(report["cart_loaded"], true);
        assert_eq!(report["observations"][0]["profile_id"], model.profile_id());
        let reads = memory_reads(&report);
        assert_eq!(reads.len(), 2);
        for read in reads {
            assert_eq!(read, json!([0xA9, 0x81]));
        }
    }
}

#[test]
fn mcp_loads_flagged_and_positional_cartridges_and_preserves_failed_switches() {
    let fixture = Fixture::new();
    let path = fixture.0.join("game.sfc").display().to_string();
    for args in [vec!["--cart", path.as_str()], vec![path.as_str()]] {
        let responses = mcp(
            &fixture,
            &args,
            &[
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":0x8000,"len":1}}}),
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"nintendo-snes-pal"}}}),
                json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"set_machine","arguments":{"machine":"unknown"}}}),
                json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"memory_read","arguments":{"addr":0x8000,"len":1}}}),
            ],
        );
        assert!(
            responses[0]["result"]["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .any(|t| t["name"] == "set_machine")
        );
        assert_eq!(tool_body(&responses[1])["bytes"], json!([0xA9]));
        assert_eq!(
            tool_body(&responses[2])["profile_id"],
            Model::SnesPal.profile_id()
        );
        assert_eq!(responses[3]["result"]["isError"], true);
        assert_eq!(tool_body(&responses[4])["bytes"], json!([0xA9]));
    }
}

#[test]
fn explicit_missing_cartridges_and_unknown_models_fail_in_both_modes() {
    let fixture = Fixture::new();
    for mode in ["--headless", "--mcp"] {
        for flags in [
            ["--cart", "missing.sfc"],
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
