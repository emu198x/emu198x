//! Original Acid800 GTIA executables, with hash-pinned inputs and exact verdicts.
//! See tests/data/acid800-gtia.md. Known failures are not hardware passes.
use std::path::{Path, PathBuf};

use machine_atari_800xl::{Atari800xl, Atari800xlRegion};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    version: u8,
    os_sha256: String,
    basic_sha256: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    xex_sha256: String,
    symbols_sha256: String,
    ntsc: Expectation,
    pal: Expectation,
}

#[derive(Deserialize)]
struct Expectation {
    verdict: String,
    failure_signature: Option<String>,
}

fn verified_file(path: &Path, expected: &str) -> Vec<u8> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        expected,
        "fixture changed: {}",
        path.display()
    );
    bytes
}

fn symbol(labels: &str, name: &str) -> u16 {
    let matches: Vec<_> = labels
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.len() == 3 && fields[2] == name)
                .then(|| u16::from_str_radix(fields[1], 16).expect("symbol address"))
        })
        .collect();
    assert_eq!(matches.len(), 1, "unique symbol {name}");
    matches[0]
}

fn screen(machine: &Atari800xl) -> String {
    let address = u16::from(machine.peek(0x58)) | (u16::from(machine.peek(0x59)) << 8);
    let mut text = String::new();
    for offset in 0..960 {
        let code = machine.peek(address.wrapping_add(offset)) & 0x7f;
        text.push(match code {
            0..=0x3f => char::from(code + 0x20),
            0x60..=0x7f => char::from(code),
            _ => ' ',
        });
        if offset % 40 == 39 {
            text.push('\n');
        }
    }
    text
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn guest_verdict(ended: bool, exit_y: u8) -> &'static str {
    if !ended {
        return "timeout";
    }
    match exit_y {
        0 => "pass",
        0x80 => "fail",
        0x40 => "skip",
        _ => "unknown",
    }
}

#[test]
fn only_a_completed_guest_with_zero_status_passes() {
    for status in 0..=u8::MAX {
        assert_eq!(guest_verdict(false, status), "timeout");
        assert_eq!(guest_verdict(true, status) == "pass", status == 0);
    }
    assert_eq!(guest_verdict(true, 0x80), "fail");
    assert_eq!(guest_verdict(true, 0x40), "skip");
    assert_eq!(guest_verdict(true, 1), "unknown");
}

#[test]
#[ignore = "FIXTURE: hash-pinned Acid800 standalone XEX/symbols and Atari XL OS/BASIC ROMs"]
fn original_gtia_probes_ntsc() {
    survey(Atari800xlRegion::Ntsc, "ntsc");
}

#[test]
#[ignore = "FIXTURE: hash-pinned Acid800 standalone XEX/symbols and Atari XL OS/BASIC ROMs"]
fn original_gtia_probes_pal() {
    survey(Atari800xlRegion::Pal, "pal");
}

fn survey(region: Atari800xlRegion, region_name: &str) {
    let Some(fixtures) = std::env::var_os("EMU198X_ACID800_ROOT").map(PathBuf::from) else {
        emu198x_test_skip::skip!("set EMU198X_ACID800_ROOT to Acid800's standalone directory");
    };
    let Some(roms) = std::env::var_os("EMU198X_ROMS_ROOT").map(PathBuf::from) else {
        emu198x_test_skip::skip!("set EMU198X_ROMS_ROOT to the firmware root");
    };
    let roms = roms.join("atari-800xl");
    let manifest: Manifest =
        serde_json::from_str(include_str!("data/acid800-gtia.json")).expect("manifest");
    assert_eq!(manifest.version, 1);
    assert_eq!(manifest.cases.len(), 11);
    // Preflight all artifacts before starting an expensive machine run.
    let inputs: Vec<_> = manifest
        .cases
        .iter()
        .map(|case| {
            let bytes = verified_file(
                &fixtures.join(format!("{}.xex", case.name)),
                &case.xex_sha256,
            );
            let labels = String::from_utf8(verified_file(
                &fixtures.join(format!("{}.lab", case.name)),
                &case.symbols_sha256,
            ))
            .expect("symbols UTF-8");
            (case, bytes, labels)
        })
        .collect();
    let mut boot = Atari800xl::new(
        Some(verified_file(
            &roms.join("atarixl.rom"),
            &manifest.os_sha256,
        )),
        Some(verified_file(
            &roms.join("ataribas.rom"),
            &manifest.basic_sha256,
        )),
        None,
        region,
        true,
    )
    .expect("machine");
    for _ in 0..700 {
        boot.run_frame();
    }
    assert!(!boot.cpu().halted, "firmware halted");
    assert!(
        screen(&boot).contains("READY"),
        "boot not ready: {}",
        screen(&boot)
    );
    let state = postcard::to_allocvec(&boot).expect("boot snapshot");
    let mut results = Vec::new();
    let mut unexpected = Vec::new();
    let strict = std::env::var("EMU198X_ACID800_STRICT").is_ok_and(|value| value == "1");
    let mut hardware_failures = Vec::new();
    for (case, bytes, labels) in inputs {
        let expected = if region_name == "ntsc" {
            &case.ntsc
        } else {
            &case.pal
        };
        let mut machine: Atari800xl = postcard::from_bytes(&state).expect("restore boot");
        let xex = format198x_atari_8bit_xex::parse(&bytes).expect("parse XEX");
        let mut runad_loaded = false;
        for segment in &xex.segments {
            let end = u32::from(segment.start) + segment.data.len() as u32;
            assert!(
                !(segment.start <= 0x2e3 && end > 0x2e2),
                "INITAD needs full loader support"
            );
            runad_loaded |= segment.start <= 0x2e0 && end >= 0x2e2;
            for (offset, &byte) in segment.data.iter().enumerate() {
                machine.load_program_byte(segment.start.wrapping_add(offset as u16), byte);
            }
        }
        assert!(runad_loaded, "explicit RUNAD required");
        let entry = u16::from(machine.peek(0x2e0)) | (u16::from(machine.peek(0x2e1)) << 8);
        assert_eq!(entry, symbol(&labels, "main"));
        assert!(machine.launch_loaded_program(entry, 100_000));
        // _testEnd is the common return point: Y=$00 pass, $80 fail, $40 skip.
        // Stop before it waits for a key or resets; do not patch the guest.
        let (clocks, ended) = machine.run_until_pc(symbol(&labels, "_testEnd"), 30_000_000);
        let verdict = guest_verdict(ended, machine.cpu().regs.y);
        if verdict != "pass" {
            hardware_failures.push(case.name.clone());
        }
        let text = screen(&machine);
        let signature_matches = match expected.failure_signature.as_deref() {
            Some(signature) => compact(&text).contains(&compact(signature)),
            None => compact(&text).contains("Pass"),
        };
        let matches_baseline = verdict == expected.verdict && signature_matches;
        eprintln!(
            "{} {region_name}: {verdict}, baseline_match={matches_baseline}",
            case.name
        );
        if !matches_baseline {
            unexpected.push(case.name.clone());
        }
        results.push(serde_json::json!({"name":case.name, "verdict":verdict,
            "matches_baseline":matches_baseline, "colour_clocks":clocks,
            "pc":machine.cpu().regs.pc, "exit_y":machine.cpu().regs.y, "screen":text}));
    }
    let report = serde_json::json!({"region":region_name, "results":results,
        "hardware_failures":hardware_failures, "strict":strict,
        "note":"Known failures remain failures; the test checks a pinned baseline, not full conformance."});
    if let Some(directory) = std::env::var_os("EMU198X_ACID800_REPORT_DIR") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).expect("report directory");
        std::fs::write(
            directory.join(format!("gtia-{region_name}.json")),
            serde_json::to_vec_pretty(&report).expect("report JSON"),
        )
        .expect("write report");
    }
    assert!(
        !strict || hardware_failures.is_empty(),
        "Acid800 conformance failures: {hardware_failures:?}\n{report:#}"
    );
    assert!(
        unexpected.is_empty(),
        "Acid800 baseline changed: {unexpected:?}\n{report:#}"
    );
}
