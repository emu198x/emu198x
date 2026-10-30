//! The actual executable must select the same board it reports to the shell.
use serde_json::Value;
use std::{path::Path, process::Command};
#[test]
fn full_profile_aliases_select_the_matching_guest_region_and_machine_id() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/sega-mega-drive/board-split-v2");
    for (id, version) in [
        ("sega-mega-drive-japan", 0x20),
        ("sega-genesis", 0xa0),
        ("sega-mega-drive-europe", 0xe0),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_emu198x-sega-mega-drive"))
            .env_clear()
            .args(["--headless", "--frames", "0", "--model", id, "--cart"])
            .arg(root.join("native.bin"))
            .arg("--script")
            .arg(root.join("board-probe.json"))
            .output()
            .expect("runner");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report: Value = serde_json::from_slice(&out.stdout).expect("report");
        assert_eq!(report["frames_run"], 3);
        let observations = report["observations"].as_array().expect("observations");
        let query = observations
            .iter()
            .find(|o| o["kind"] == "query")
            .expect("machine query");
        assert_eq!(query["result"]["value"], id);
        let read = observations
            .iter()
            .find(|o| o["kind"] == "memory_read")
            .expect("guest region result");
        assert_eq!(read["bytes"], serde_json::json!([version, 0x7f]));
    }
}
