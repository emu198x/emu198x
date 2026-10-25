//! The binary in script mode on the stand-in BIOS and a synthetic
//! cartridge, given as a directory of ROM files and as a zip of them.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use runtime_snk_neo_geo::test_support::{bios, p_rom};
use serde_json::{Value, json};

/// The program copies controller 1 to `$100000` and counts passes at
/// `$100002`.
fn program() -> Vec<u8> {
    let p = p_rom(&[
        0x13F9, 0x0030, 0x0000, 0x0010, 0x0000, // MOVE.B $300000,$100000
        0x5279, 0x0010, 0x0002, // ADDQ.W #1,$100002
        0x60EE, // BRA.S back
    ]);
    // As MAME stores a P-ROM: words byte-swapped.
    p.chunks(2).flat_map(|w| [w[1], w[0]]).collect()
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "snk-neo-geo-launch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("game")).expect("fixture directory");
        std::fs::write(dir.join("neo-epo.bin"), bios()).expect("BIOS");
        std::fs::write(dir.join("game").join("t-p1.p1"), program()).expect("P-ROM");
        Self(dir)
    }

    fn script(&self, steps: &Value, cart: &str) -> Value {
        let path = self.0.join("script.json");
        std::fs::write(&path, steps.to_string()).expect("script");
        let output = Command::new(env!("CARGO_BIN_EXE_emu198x-snk-neo-geo"))
            .env_clear()
            .arg("--bios")
            .arg(self.0.join("neo-epo.bin"))
            .arg("--script")
            .arg(path)
            .args(["--frames", "0"])
            .arg(self.0.join(cart))
            .output()
            .expect("launch");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("JSON report")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_directory_cartridge_runs_and_reads_the_controller() {
    let fixture = Fixture::new();
    let report = fixture.script(
        &json!([
            {"action":"run_frames", "frames":2},
            {"action":"memory_read", "addr":0x10_0000, "len":1},
            {"action":"input", "events":[{"Button":{"port":1, "name":"a", "pressed":true}}]},
            {"action":"run_frames", "frames":2},
            {"action":"memory_read", "addr":0x10_0000, "len":3},
            {"action":"query", "path":"machine.variant"}
        ]),
        "game",
    );
    assert_eq!(report["bios_loaded"], true, "{report}");
    assert_eq!(report["cart_loaded"], true);
    assert!(report["frames_run"].as_u64().expect("frames") >= 4);
    let observations = report["observations"].as_array().expect("observations");
    let reads: Vec<&Value> = observations
        .iter()
        .filter(|o| o["kind"] == "memory_read")
        .map(|o| &o["bytes"])
        .collect();
    assert_eq!(reads[0], &json!([0xFF]), "nothing pressed");
    assert_eq!(reads[1][0], json!(0xEF), "A pressed");
    assert_ne!(reads[1][2], json!(0), "the loop ran");
    let variant = observations
        .iter()
        .find(|o| o["kind"] == "query")
        .expect("query");
    assert_eq!(variant["result"]["value"], "aes");
}
