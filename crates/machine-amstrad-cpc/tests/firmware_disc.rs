//! The real CPC664 and CPC6128 firmware find their drive through AMSDOS.
//!
//! Gated `#[ignore]` because the firmware is copyrighted and not in the tree.
//! Place MAME's `cpc664.rom`, `cpc6128.rom` and AMSDOS (`amsdos.rom`, MAME's
//! `cpcados.rom`, SHA1 `39102c8e…`) in `~/.emu198x/roms/amstrad-cpc/`, or name
//! them with EMU198X_CPC664_ROM, EMU198X_CPC6128_ROM and
//! EMU198X_CPC_AMSDOS_ROM.
//!
//! ```text
//! cargo test --release -p machine-amstrad-cpc --test firmware_disc -- --ignored
//! ```
//!
//! The disc is built here, in AMSDOS's DATA format, so no disc image is
//! needed either. Two things are proved, end to end through the firmware:
//! `CAT` lists the directory AMSDOS reads off track 0, and `RUN"HELLO`
//! loads an ASCII BASIC file and runs it. Both are Read Data commands polled
//! through `&FB7E`/`&FB7F` with the motor started at `&FA7E`, so a passing
//! test is the drive, the controller and the firmware agreeing.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_nec_upd765a::{DiskSector, DiskTrack};
use machine_amstrad_cpc::{AmstradCpc, CpcModel, DiskImage};

fn rom(env_var: &str, file: &str) -> Option<Vec<u8>> {
    let path = env::var(env_var).map(PathBuf::from).ok().or_else(|| {
        let home = env::var("HOME").ok()?;
        Some(
            PathBuf::from(home)
                .join(".emu198x/roms/amstrad-cpc")
                .join(file),
        )
    })?;
    fs::read(path).ok()
}

/// A DATA-format disc (40 tracks, nine 512-byte sectors `&C1`-`&C9`) holding
/// one file, `HELLO.BAS`: an ASCII BASIC program, which AMSDOS loads because
/// it carries no header.
fn hello_disc() -> DiskImage {
    let mut sectors = vec![vec![0xE5u8; 512]; 40 * 9];

    // The directory: blocks 0 and 1, the first four sectors of track 0.
    // One 32-byte entry: user 0, the name, extent 0, the record count, and
    // the 1 KB blocks that hold the data.
    let program = b"10 PRINT \"DISC OK\"\r\n";
    let mut entry = [0u8; 32];
    entry[1..12].copy_from_slice(b"HELLO   BAS");
    entry[15] = 1; // one 128-byte record
    entry[16] = 2; // block 2: track 0, sectors &C5 and &C6
    sectors[0][..32].copy_from_slice(&entry);

    // Block 2 starts at the fifth sector. ASCII files end at &1A.
    let data = &mut sectors[4];
    data[..program.len()].copy_from_slice(program);
    data[program.len()..128].fill(0x1A);

    let tracks = (0..40u8)
        .map(|c| DiskTrack {
            sectors: (0..9u8)
                .map(|s| DiskSector {
                    c,
                    h: 0,
                    id: 0xC1 + s,
                    n: 2,
                    st1: 0,
                    st2: 0,
                    data: sectors[usize::from(c) * 9 + usize::from(s)].clone(),
                })
                .collect(),
        })
        .collect();
    DiskImage {
        sides: 1,
        tracks_per_side: 40,
        tracks: vec![tracks],
    }
}

fn type_text(cpc: &mut AmstradCpc, text: &str) {
    for c in text.chars() {
        assert!(cpc.press_char(c), "no key produces {c:?}");
        for _ in 0..4 {
            cpc.run_frame();
        }
        cpc.release_char(c);
        for _ in 0..4 {
            cpc.run_frame();
        }
    }
}

fn screen_contains(cpc: &AmstradCpc, needle: &str) -> bool {
    cpc.screen_text().iter().any(|line| line.contains(needle))
}

fn run_frames_until(cpc: &mut AmstradCpc, frames: usize, needle: &str) -> bool {
    for _ in 0..frames {
        if screen_contains(cpc, needle) {
            return true;
        }
        cpc.run_frame();
    }
    screen_contains(cpc, needle)
}

fn boot_and_run_from_disc(model: CpcModel, firmware_env: &str, firmware_file: &str) {
    let (Some(firmware), Some(amsdos)) = (
        rom(firmware_env, firmware_file),
        rom("EMU198X_CPC_AMSDOS_ROM", "amsdos.rom"),
    ) else {
        panic!(
            "{firmware_file} and amsdos.rom not found — set {firmware_env} and \
             EMU198X_CPC_AMSDOS_ROM, or place them in ~/.emu198x/roms/amstrad-cpc/"
        );
    };
    let mut cpc = AmstradCpc::with_model(&firmware, model).expect("build machine");
    cpc.fit_upper_rom(7, &amsdos).expect("AMSDOS fits as ROM 7");
    cpc.insert_disk(0, hello_disc(), false);

    assert!(
        run_frames_until(&mut cpc, 300, "Ready"),
        "the firmware never reached Ready:\n{}",
        cpc.screen_text().join("\n")
    );

    type_text(&mut cpc, "cat\r");
    assert!(
        run_frames_until(&mut cpc, 500, "HELLO"),
        "CAT did not list HELLO.BAS:\n{}",
        cpc.screen_text().join("\n")
    );
    assert!(screen_contains(&cpc, "Drive A"), "CAT names the drive");

    type_text(&mut cpc, "run\"hello\r");
    assert!(
        run_frames_until(&mut cpc, 500, "DISC OK"),
        "RUN\"HELLO did not load and run the program:\n{}",
        cpc.screen_text().join("\n")
    );
}

#[test]
#[ignore = "FIXTURE: needs the CPC6128 firmware and AMSDOS — run with --ignored"]
fn the_6128_catalogues_and_runs_a_program_from_disc() {
    boot_and_run_from_disc(CpcModel::Cpc6128, "EMU198X_CPC6128_ROM", "cpc6128.rom");
}

#[test]
#[ignore = "FIXTURE: needs the CPC664 firmware and AMSDOS — run with --ignored"]
fn the_664_catalogues_and_runs_a_program_from_disc() {
    boot_and_run_from_disc(CpcModel::Cpc664, "EMU198X_CPC664_ROM", "cpc664.rom");
}
