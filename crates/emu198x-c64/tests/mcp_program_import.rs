//! The actual MCP startup must honour --load before serving requests (#1858).
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

fn tool(id: u32, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}})
}
fn body(response: &Value) -> Value {
    assert_ne!(response["result"]["isError"], true, "{response}");
    serde_json::from_str(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("text"),
    )
    .expect("tool JSON")
}
#[test]
#[ignore = "FIXTURE: needs the C64 ROMs — run with --ignored"]
fn mcp_loads_a_prg_and_basic_run_executes_it() {
    let dir = std::env::temp_dir().join(format!("emu198x-c64-mcp-import-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp directory");
    let path = dir.join("probe.prg");
    // 10 POKE 49152,42; a linked BASIC V2 line, followed by the end marker.
    let payload = [
        0x0f, 0x08, 10, 0, 0x97, b'4', b'9', b'1', b'5', b'2', b',', b'4', b'2', 0, 0, 0,
    ];
    let mut prg = vec![1, 8];
    prg.extend(payload);
    fs::write(&path, prg).expect("PRG");
    let requests = [
        tool(1, "memory_read", json!({"addr":2049,"len":16})),
        tool(2, "type_string", json!({"text":"RUN\n"})),
        tool(3, "run_frames", json!({"frames":120})),
        tool(4, "memory_read", json!({"addr":49152,"len":1})),
    ];
    let mut child = Command::new(env!("CARGO_BIN_EXE_emu198x-c64"))
        .args(["--mcp", "--model", "pal", "--load"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP");
    let mut input = child.stdin.take().expect("stdin");
    for r in requests {
        writeln!(input, "{r}").expect("request");
    }
    drop(input);
    let output = child.wait_with_output().expect("MCP output");
    fs::remove_dir_all(dir).expect("remove temp fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies: Vec<Value> = String::from_utf8(output.stdout)
        .expect("UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("MCP stdout must contain only JSON"))
        .collect();
    assert_eq!(replies.len(), 4);
    assert_eq!(
        body(&replies[0])["bytes"],
        json!(payload),
        "startup import is missing"
    );
    assert_eq!(
        body(&replies[3])["bytes"],
        json!([42]),
        "BASIC RUN did not execute the imported program"
    );
}
