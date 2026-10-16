//! Startup must fit the host's normal main-thread stack (#1792).
//!
//! A synthetic ROM suffices: query_cpu runs at reset without executing it.
//! Run the actual binary so Windows exercises its PE stack reserve rather
//! than the test runner's worker-thread stack.

use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn query_cpu_starts_a_48k_session_without_running_frames() {
    let fixture = Fixture(
        std::env::temp_dir().join(format!("spectrum-script-startup-{}", std::process::id())),
    );
    fs::create_dir(&fixture.0).expect("fresh fixture");
    let rom = fixture.0.join("48.rom");
    let script = fixture.0.join("query.json");
    fs::write(&rom, vec![0; 16384]).expect("synthetic firmware");
    fs::write(&script, r#"[{"action":"query_cpu"}]"#).expect("startup query");

    let mut child = Command::new(env!("CARGO_BIN_EXE_emu198x-spectrum"))
        .args(["--machine", "spectrum_48k", "--rom"])
        .arg(&rom)
        .arg("--script")
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start Spectrum");
    let deadline = Instant::now() + Duration::from_secs(15);
    while child.try_wait().expect("poll Spectrum").is_none() {
        if Instant::now() >= deadline {
            child.kill().expect("kill timed out startup");
            let _ = child.wait();
            panic!("startup exceeded 15 seconds");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().expect("startup output");
    assert!(
        output.status.success(),
        "startup exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("startup JSON");
    assert_eq!(report["time"], 0);
    let observations = report["observations"].as_array().expect("observations");
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0]["kind"], "query_cpu");
    assert_eq!(observations[0]["registers"]["pc"], "$0000");
    assert_eq!(observations[0]["registers"]["sp"], "$FFFF");
}
