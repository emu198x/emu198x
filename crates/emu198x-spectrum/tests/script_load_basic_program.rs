//! `load_basic_program` works from a script on the 48K Spectrum.
//!
//! ```text
//! cargo test --release -p emu198x-spectrum --test script_load_basic_program -- --ignored
//! ```
//!
//! Gated `#[ignore]` because it needs the 48K ROM at
//! `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
//!
//! Drives the built binary rather than calling into the library, because the
//! bug this covers only appears against the real ROM's screen timing: the
//! loader tapped ENTER to clear the copyright banner and then read row 23
//! immediately, catching it cleared but not yet repainted — 32 spaces, so
//! every run failed with `PromptNotReady` (#1413). The library tests passed
//! throughout, because their query provider reported a `K` prompt from the
//! first read and so never modelled the repaint gap.
//!
//! The C64 has had `tests/script_load_basic_program.rs` since #914; the
//! Spectrum having no equivalent is why this reached a release.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn rom_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    path.exists().then_some(path)
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn a_script_can_load_and_run_a_basic_program() {
    let rom = rom_path().expect("needs ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom");

    let dir = std::env::temp_dir().join("emu198x-spectrum-lbp-test");
    fs::create_dir_all(&dir).expect("create temp dir");
    let bas = dir.join("poke.bas");
    let script = dir.join("script.json");

    // Two POKEs into free 48K RAM above the display file. Reading them back
    // proves the program actually ran, rather than merely being installed.
    fs::write(&bas, "10 POKE 32768,7\n20 POKE 32769,7\n").expect("write basic");
    fs::write(
        &script,
        format!(
            r#"[{{"action":"load_basic_program","path":"{}","run":true}},
               {{"action":"run_frames","frames":120}},
               {{"action":"memory_read","addr":32768,"len":2}}]"#,
            bas.display()
        ),
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_emu198x-spectrum"))
        .arg("--rom")
        .arg(&rom)
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
        !stderr.contains("prompt was not ready"),
        "the loader still samples row 23 before the ROM repaints it: {stderr}"
    );
    assert!(
        stdout.contains(r#""kind":"load_basic_program""#),
        "no load_basic_program observation: {stdout}"
    );
    assert!(
        stdout.contains(r#""ran":true"#),
        "the program was installed but not run: {stdout}"
    );
    // The POKEs landed, so the interpreter really executed both lines.
    assert!(
        stdout.contains("[7,7]"),
        "the program did not write the poked bytes: {stdout}"
    );
}
