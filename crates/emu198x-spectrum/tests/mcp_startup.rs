//! MCP must resolve the requested machine's firmware before serving tools.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("spectrum-mcp-{name}-{}", std::process::id()));
        fs::create_dir(&path).expect("fixture directory");
        Self(path)
    }
    fn rom(&self, index: u8) -> String {
        let path = self.0.join(format!("{index}.rom"));
        fs::write(&path, vec![index + 0x40; 16384]).expect("synthetic ROM");
        path.to_str().expect("path").to_owned()
    }
    fn run(&self, flags: &[String], requests: &[Value]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_emu198x-spectrum"))
            .arg("--mcp")
            .args(flags)
            .env("HOME", &self.0)
            .env_remove("EMU198X_SPECTRUM_ROM_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start MCP");
        let mut stdin = child.stdin.take().expect("stdin");
        for request in requests {
            writeln!(stdin, "{request}").expect("request");
        }
        drop(stdin);
        let deadline = Instant::now() + Duration::from_secs(15);
        while child.try_wait().expect("poll MCP").is_none() {
            if Instant::now() >= deadline {
                child.kill().expect("kill timed out MCP");
                let _ = child.wait();
                panic!("MCP exceeded host timeout");
            }
            thread::sleep(Duration::from_millis(10));
        }
        child.wait_with_output().expect("MCP output")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn call(id: usize, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}})
}
fn body(response: &Value) -> Value {
    assert!(response.get("error").is_none(), "{response}");
    assert_ne!(response["result"]["isError"], true, "{response}");
    serde_json::from_str(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("text"),
    )
    .expect("tool JSON")
}
#[test]
fn explicit_plus3_bundle_selects_machine_and_all_four_roms() {
    let fixture = Fixture::new("plus3");
    let mut flags = vec!["--machine".into(), "spectrum_plus3".into()];
    for index in 0..4 {
        flags.extend([
            "--rom".into(),
            format!(
                "sinclair-zx-spectrum-plus3-rom-{index}={}",
                fixture.rom(index)
            ),
        ]);
    }
    let mut requests = vec![call(
        0,
        "query",
        json!({"path":"session.profile.profile_id"}),
    )];
    for index in 0..4 {
        requests.push(call(
            requests.len(),
            "port_write",
            json!({"port":0x1ffd,"value":(index & 2) << 1}),
        ));
        requests.push(call(
            requests.len(),
            "port_write",
            json!({"port":0x7ffd,"value":(index & 1) << 4}),
        ));
        requests.push(call(
            requests.len(),
            "memory_read",
            json!({"addr":0,"len":1}),
        ));
    }
    requests.push(json!({"jsonrpc":"2.0","id":13,"method":"tools/list"}));
    let output = fixture.run(&flags, &requests);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .expect("UTF8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("response"))
        .collect();
    assert_eq!(responses.len(), 14);
    assert!(
        responses[13]["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["name"] == "export_media")
    );
    assert_eq!(
        body(&responses[0])["result"]["value"],
        "sinclair-zx-spectrum-plus3-pal"
    );
    for index in 0..4 {
        assert_eq!(
            body(&responses[3 + index * 3])["bytes"],
            json!([0x40 + index])
        );
    }
}
#[test]
fn omitted_machine_keeps_explicit_48k_firmware() {
    let fixture = Fixture::new("default");
    let output = fixture.run(
        &["--rom".into(), fixture.rom(0)],
        &[
            call(0, "query", json!({"path":"session.profile.profile_id"})),
            call(1, "memory_read", json!({"addr":0,"len":1})),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .expect("UTF8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("response"))
        .collect();
    assert_eq!(
        body(&responses[0])["result"]["value"],
        "sinclair-zx-spectrum-48k-pal"
    );
    assert_eq!(body(&responses[1])["bytes"], json!([0x40]));
}
#[test]
fn invalid_machine_rom_identity_and_missing_rom_fail_before_serving() {
    let fixture = Fixture::new("errors");
    for (flags, message) in [
        (
            vec!["--machine".into(), "spectrum_invalid".into()],
            "unknown machine id",
        ),
        (
            vec![
                "--machine".into(),
                "spectrum_plus3".into(),
                "--rom".into(),
                format!("wrong-id={}", fixture.rom(0)),
            ],
            "no ROM `wrong-id` on spectrum_plus3",
        ),
        (
            vec![
                "--machine".into(),
                "spectrum_plus3".into(),
                "--rom".into(),
                format!(
                    "sinclair-zx-spectrum-plus3-rom-0={}/absent.rom",
                    fixture.0.display()
                ),
            ],
            "no spectrum_plus3 ROM directory found",
        ),
    ] {
        let output = fixture.run(&flags, &[]);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
