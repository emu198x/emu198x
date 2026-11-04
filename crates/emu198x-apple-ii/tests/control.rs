//! Exercise the shipped process, including JSON-RPC framing, not just trait calls.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    flags: Vec<String>,
}
impl Fixture {
    fn authored() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "apple-ii-control-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).expect("unique fixture directory");
        let mut rom = vec![0xea; 8192];
        rom[..3].copy_from_slice(&[0x4c, 0, 0xe0]);
        rom[0x1ffc..0x1ffe].copy_from_slice(&[0, 0xe0]);
        fs::write(dir.join("rom"), rom).expect("authored ROM");
        fs::write(dir.join("glyphs"), [0; 512]).expect("glyphs");
        let flags = vec![
            "--rom".into(),
            dir.join("rom").to_string_lossy().into(),
            "--characters".into(),
            dir.join("glyphs").to_string_lossy().into(),
        ];
        Self { dir, flags }
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_emu198x-apple-ii"));
        c.args(&self.flags);
        c
    }
    fn script(&self, steps: Value) -> std::process::Output {
        let script = self.dir.join("script.json");
        fs::write(&script, steps.to_string()).expect("script");
        self.command()
            .args(["--frames", "0", "--script"])
            .arg(script)
            .output()
            .expect("runner")
    }
    fn mcp(&self, requests: &[Value]) -> Vec<Value> {
        let mut child = self
            .command()
            .arg("--mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("MCP process");
        {
            let mut stdin = child.stdin.take().expect("stdin");
            for request in requests {
                writeln!(stdin, "{request}").expect("request");
            }
        }
        let out = child.wait_with_output().expect("server exit");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .expect("UTF-8")
            .lines()
            .map(|s| serde_json::from_str(s).expect("JSON-RPC"))
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).expect("clean fixture");
    }
}
fn call(id: u32, name: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})
}
fn observation(response: &Value) -> Value {
    assert!(response.get("error").is_none(), "{response}");
    assert_ne!(response["result"]["isError"], true, "{response}");
    serde_json::from_str(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool text"),
    )
    .expect("observation")
}
#[test]
fn scripts_and_stdio_mcp_execute_the_same_shared_operations() {
    let f = Fixture::authored();
    let steps = json!([
        {"action":"run_ticks","ticks":123},
        {"action":"query","path":"cpu.master_clock"},
        {"action":"poke_byte","addr":768,"value":42},
        {"action":"memory_read","addr":768,"len":1},
        {"action":"disasm","addr":57344,"instructions":1},
        {"action":"step","instructions":2},
        {"action":"query_cpu"}
    ]);
    let out = f.script(steps.clone());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).expect("report");
    let requests: Vec<_> = steps
        .as_array()
        .expect("steps")
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut args = s.as_object().expect("step").clone();
            let action = args.remove("action").expect("action");
            call(i as u32, action.as_str().expect("name"), args.into())
        })
        .collect();
    let responses = f.mcp(&requests);
    assert_eq!(responses.len(), requests.len());
    for (i, response) in responses.iter().enumerate() {
        assert_eq!(observation(response), report["observations"][i]);
    }
    let list = f.mcp(&[json!({"jsonrpc":"2.0","id":0,"method":"tools/list"})]);
    let names: Vec<_> = list[0]["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    for name in [
        "type_string",
        "press_key",
        "step",
        "run_ticks",
        "query",
        "export_media",
        "save_screenshot",
    ] {
        assert!(names.contains(&name), "{name}");
    }
    assert!(!names.contains(&"watch_memory_start"));
}
#[test]
fn waits_and_missing_firmware_fail_and_capture_is_a_real_png() {
    let f = Fixture::authored();
    let out=f.script(json!([{ "action":"wait_for_query_contains","path":"screen.text","needle":"IMPOSSIBLE","max_frames":1}]));
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("IMPOSSIBLE"));
    let responses = f.mcp(&[call(
        1,
        "wait_for_query_contains",
        json!({"path":"screen.text","needle":"IMPOSSIBLE","max_frames":1}),
    )]);
    assert_eq!(responses[0]["result"]["isError"], true);
    let png = f.dir.join("screen.png");
    let out = f.script(
        json!([{ "action":"run_frames","frames":1},{"action":"save_screenshot","path":png}]),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = fs::read(png).expect("PNG");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(
        u32::from_be_bytes(bytes[16..20].try_into().expect("width")),
        560
    );
    assert_eq!(
        u32::from_be_bytes(bytes[20..24].try_into().expect("height")),
        192
    );
    for mode in ["--headless", "--mcp"] {
        let out = Command::new(env!("CARGO_BIN_EXE_emu198x-apple-ii"))
            .arg(mode)
            .output()
            .expect("missing firmware");
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("apple-ii-rom"));
    }
}
#[test]
#[ignore = "external original firmware; set EMU198X_APPLE_II_ROM and EMU198X_APPLE_II_CHARS"]
fn original_basic_runs_through_scripts_and_live_mcp() {
    let mut f = Fixture::authored();
    f.flags = vec![
        "--rom".into(),
        std::env::var("EMU198X_APPLE_II_ROM").expect("ROM path"),
        "--characters".into(),
        std::env::var("EMU198X_APPLE_II_CHARS").expect("glyph path"),
    ];
    let steps = json!([
        {"action":"wait_for_boot","max_frames":120},
        {"action":"type_string","text":"\u{2}\n10 PRINT 6*7\nRUN\n"},
        {"action":"wait_for_query_contains","path":"screen.text","needle":"42","max_frames":120},
        {"action":"query","path":"screen.text"}
    ]);
    let out = f.script(steps.clone());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).expect("report");
    let requests: Vec<_> = steps
        .as_array()
        .expect("steps")
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut args = s.as_object().expect("step").clone();
            let action = args.remove("action").expect("action");
            call(i as u32, action.as_str().expect("name"), args.into())
        })
        .collect();
    let responses = f.mcp(&requests);
    for (i, response) in responses.iter().enumerate() {
        assert_eq!(observation(response), report["observations"][i]);
    }
    let screen = observation(responses.last().expect("screen"));
    assert!(
        screen["result"]["value"]
            .as_str()
            .expect("text")
            .lines()
            .any(|s| s.trim() == "42")
    );
}

#[test]
#[ignore = "external original ROMs, P5/P6 and DOS WOZ; see family docs/verification.md"]
fn dos_work_disk_exports_then_reloads_in_a_fresh_mcp_process() {
    let mut f = Fixture::authored();
    f.flags = vec![
        "--rom".into(),
        std::env::var("EMU198X_APPLE_II_ROM").expect("ROM"),
        "--characters".into(),
        std::env::var("EMU198X_APPLE_II_CHARS").expect("glyphs"),
        "--p5".into(),
        std::env::var("EMU198X_DISK_II_P5").expect("P5"),
        "--p6".into(),
        std::env::var("EMU198X_DISK_II_P6").expect("P6"),
    ];
    let source = std::env::var("EMU198X_APPLE_II_DOS").expect("DOS WOZ");
    let before = fs::read(&source).expect("archive");
    let export = f.dir.join("work.woz");
    let out=f.script(json!([
        {"action":"load_media","slot":"floppy-0","kind":"disk","path":source,"writable":true},
        {"action":"wait_for_boot","max_frames":120},
        {"action":"type_string","text":"C600G\n"},
        {"action":"wait_for_query_contains","path":"screen.text","needle":"DOS VERSION 3.3","max_frames":1200},
        {"action":"wait_for_query_bool","path":"disk.motor_on","value":false,"max_frames":600},
        {"action":"type_string","text":"10 PRINT 6*7\n20 END\nSAVE DISKTEST\n"},
        {"action":"run_frames","frames":300},
        {"action":"wait_for_query_bool","path":"disk.motor_on","value":false,"max_frames":600},
        {"action":"query","path":"disk.bit_writes"},
        {"action":"export_media","slot":"floppy-0","format":"woz","path":export}
    ]));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).expect("report");
    assert!(
        report["observations"]
            .as_array()
            .expect("observations")
            .iter()
            .any(|o| o["result"]["path"] == "disk.bit_writes"
                && o["result"]["value"][0].as_u64().is_some_and(|n| n > 1000))
    );
    assert_ne!(fs::read(&export).expect("new WOZ"), before);
    assert_eq!(fs::read(&source).expect("source"), before);
    let responses = f.mcp(&[
        call(
            0,
            "load_media",
            json!({"slot":"floppy-0","kind":"disk","path":export}),
        ),
        call(1, "wait_for_boot", json!({"max_frames":120})),
        call(2, "type_string", json!({"text":"C600G\n"})),
        call(
            3,
            "wait_for_query_contains",
            json!({"path":"screen.text","needle":"DOS VERSION 3.3","max_frames":1200}),
        ),
        call(
            4,
            "wait_for_query_bool",
            json!({"path":"disk.motor_on","value":false,"max_frames":600}),
        ),
        call(5, "type_string", json!({"text":"LOAD DISKTEST\nRUN\n"})),
        call(
            6,
            "wait_for_query_contains",
            json!({"path":"screen.text","needle":"42","max_frames":600}),
        ),
        call(7, "query", json!({"path":"screen.text"})),
        call(8, "type_string", json!({"text":"SAVE DISKTEST\n"})),
        call(
            9,
            "wait_for_query_contains",
            json!({"path":"screen.text","needle":"WRITE PROTECTED","max_frames":600}),
        ),
        call(10, "query", json!({"path":"disk.bit_writes"})),
    ]);
    for response in &responses {
        observation(response);
    }
    assert!(
        observation(&responses[7])["result"]["value"]
            .as_str()
            .expect("text")
            .lines()
            .any(|s| s.trim() == "42")
    );
    assert_eq!(
        observation(&responses[10])["result"]["value"],
        json!([0, 0])
    );
    assert_eq!(fs::read(&source).expect("source"), before);
    // A second export to an existing destination must fail without truncating it.
    let saved = fs::read(&export).expect("saved");
    let responses = f.mcp(&[
        call(
            0,
            "load_media",
            json!({"slot":"floppy-0","kind":"disk","path":export}),
        ),
        call(
            1,
            "export_media",
            json!({"slot":"floppy-0","format":"woz","path":export}),
        ),
    ]);
    assert_eq!(responses[1]["result"]["isError"], true);
    assert_eq!(fs::read(export).expect("unchanged"), saved);
}
