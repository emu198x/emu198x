//! `load_basic_program` works from a script, not only from MCP.
//!
//! ```text
//! cargo test --release -p emu198x-c64 --test script_load_basic_program -- --ignored
//! ```
//!
//! Gated `#[ignore]` because it needs the copyrighted C64 ROMs at
//! `~/.emu198x/roms/commodore-c64/`.
//!
//! Drives the built binary rather than calling into the library, because the
//! bug this covers lived in the binary's script loop: the action parsed, was
//! advertised, and then failed at execution with `requires a system-specific
//! handler` — the C64 had the loader all along, wired only into its MCP tool
//! (#914). A library-level test would have passed throughout.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn roms_present() -> bool {
    let Ok(home) = std::env::var("HOME") else {
        return false;
    };
    PathBuf::from(home)
        .join(".emu198x/roms/commodore-c64/kernal.rom")
        .exists()
}

#[test]
#[ignore = "FIXTURE: needs the C64 ROMs — run with --ignored"]
fn a_script_can_load_and_run_a_basic_program() {
    assert!(
        roms_present(),
        "needs ~/.emu198x/roms/commodore-c64/kernal.rom"
    );

    let dir = std::env::temp_dir().join("emu198x-c64-lbp-test");
    fs::create_dir_all(&dir).expect("create temp dir");
    let bas = dir.join("fill.bas");
    let script = dir.join("script.json");

    // Ten screen-RAM bytes set to 1 (a white `A`), which is both easy to check
    // and proves the program actually ran rather than merely being installed.
    fs::write(&bas, "10 FOR I=1024 TO 1033\n20 POKE I,1\n30 NEXT I\n").expect("write basic");
    fs::write(
        &script,
        format!(
            r#"[{{"action":"wait_for_boot","max_frames":400}},
               {{"action":"load_basic_program","path":"{}","run":true}},
               {{"action":"run_frames","frames":120}},
               {{"action":"memory_read","addr":1024,"len":4}}]"#,
            bas.display()
        ),
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_emu198x-c64"))
        .arg("--script")
        .arg(&script)
        .output()
        .expect("run the binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "script run failed: {stderr}\n{stdout}"
    );

    // The regression itself: this step used to abort the whole script.
    assert!(
        !stderr.contains("system-specific handler"),
        "load_basic_program still has no script handler: {stderr}"
    );
    assert!(
        stdout.contains(r#""kind":"load_basic_program""#),
        "no load_basic_program observation: {stdout}"
    );
    assert!(
        stdout.contains(r#""ran":true"#),
        "the program was installed but not run: {stdout}"
    );
    // The POKEs landed: screen RAM at $0400 holds the character code 1.
    assert!(
        stdout.contains("[1,1,1,1]"),
        "the program did not write screen RAM: {stdout}"
    );
}
