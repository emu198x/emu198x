//! MCP boot probe tests for comparing Amiga model bring-up checkpoints.
//!
//! These tests are ignored by default because they require local Kickstart ROMs
//! and write JSON reports under `test_output/amiga/probes/`.

mod common;

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use base64::Engine;
use emu_core::mcp::{McpEmulator, ToolResult};
use machine_amiga::mcp::AmigaMcp;
use serde::Serialize;
use serde_json::{Value as JsonValue, json};

use common::load_rom;

const DEFAULT_CHECKPOINT_FRAMES: &[u64] = &[0, 1, 50, 200, 1000];
const COMMON_SUPPORT_PATHS: &[(&str, &str, &[&str])] = &[
    (
        "paula",
        "paula.",
        &["paula.intena", "paula.intreq", "paula.adkcon"],
    ),
    (
        "cia_a",
        "cia_a.",
        &[
            "cia_a.timer_a",
            "cia_a.timer_b",
            "cia_a.icr_status",
            "cia_a.icr_mask",
            "cia_a.cra",
            "cia_a.crb",
        ],
    ),
    (
        "cia_b",
        "cia_b.",
        &[
            "cia_b.timer_a",
            "cia_b.timer_b",
            "cia_b.icr_status",
            "cia_b.icr_mask",
            "cia_b.cra",
            "cia_b.crb",
        ],
    ),
];

struct ProbeSpec {
    model: &'static str,
    rom_path: &'static str,
    report_name: &'static str,
    slow_ram_kib: u64,
    required_support_prefixes: &'static [&'static str],
}

#[derive(Serialize)]
struct ProbeReport {
    model: &'static str,
    rom_path: &'static str,
    cpu_paths: Vec<String>,
    agnus_paths: Vec<String>,
    denise_paths: Vec<String>,
    support_paths: BTreeMap<String, Vec<String>>,
    checkpoints: Vec<ProbeCheckpoint>,
}

#[derive(Serialize)]
struct ProbeCheckpoint {
    frame: u64,
    master_clock: JsonValue,
    cpu: BTreeMap<String, JsonValue>,
    agnus: BTreeMap<String, JsonValue>,
    denise: BTreeMap<String, JsonValue>,
    support: BTreeMap<String, BTreeMap<String, JsonValue>>,
}

fn dispatch_success(mcp: &mut AmigaMcp, method: &str, params: JsonValue) -> JsonValue {
    match mcp.dispatch_tool(method, &params) {
        ToolResult::Success(value) => value,
        ToolResult::Error { code, message } => {
            panic!("{method} failed with {code}: {message}");
        }
    }
}

fn query_paths(mcp: &mut AmigaMcp, prefix: &str) -> Vec<String> {
    let result = dispatch_success(mcp, "query_paths", json!({ "prefix": prefix }));
    let mut paths: Vec<String> = result
        .get("paths")
        .and_then(JsonValue::as_array)
        .expect("paths array")
        .iter()
        .filter_map(|value| value.as_str())
        .filter(|path| !path.contains('<'))
        .map(ToOwned::to_owned)
        .collect();
    paths.sort();
    paths
}

fn query_value(mcp: &mut AmigaMcp, path: &str) -> JsonValue {
    let result = dispatch_success(mcp, "query", json!({ "path": path }));
    result
        .get("value")
        .cloned()
        .unwrap_or_else(|| panic!("query result for {path} missing value"))
}

fn collect_values(mcp: &mut AmigaMcp, paths: &[String]) -> BTreeMap<String, JsonValue> {
    let mut values = BTreeMap::new();
    for path in paths {
        values.insert(path.clone(), query_value(mcp, path));
    }
    values
}

fn collect_known_paths(mcp: &mut AmigaMcp, prefix: &str, wanted_paths: &[&str]) -> Vec<String> {
    let available_paths = query_paths(mcp, prefix);
    let mut paths = Vec::with_capacity(wanted_paths.len());
    for wanted_path in wanted_paths {
        assert!(
            available_paths.iter().any(|path| path == wanted_path),
            "expected probe path {wanted_path}"
        );
        paths.push((*wanted_path).to_owned());
    }
    paths
}

fn support_key(prefix: &str) -> String {
    prefix.trim_end_matches('.').to_string()
}

fn output_path(report_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_output/amiga/probes")
        .join(format!("{report_name}.json"))
}

fn checkpoint_frames() -> Vec<u64> {
    let Some(raw) = env::var_os("AMIGA_PROBE_FRAMES") else {
        return DEFAULT_CHECKPOINT_FRAMES.to_vec();
    };

    let mut frames = Vec::new();
    for token in raw.to_string_lossy().split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let frame = trimmed
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("invalid AMIGA_PROBE_FRAMES entry: {trimmed}"));
        if frames.last().copied() != Some(frame) {
            frames.push(frame);
        }
    }

    assert!(
        !frames.is_empty(),
        "AMIGA_PROBE_FRAMES must contain at least one frame"
    );
    for window in frames.windows(2) {
        assert!(
            window[0] <= window[1],
            "AMIGA_PROBE_FRAMES must be sorted ascending"
        );
    }
    frames
}

fn write_report(report_name: &str, report: &ProbeReport) {
    let path = output_path(report_name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create probe output directory");
    }
    let data = serde_json::to_vec_pretty(report).expect("serialize probe report");
    fs::write(&path, data).expect("write probe report");
    println!("Probe report saved to {}", path.display());
}

fn run_probe(spec: &ProbeSpec) {
    let Some(rom) = load_rom(spec.rom_path) else {
        return;
    };

    let mut mcp = AmigaMcp::new();
    let boot_result = dispatch_success(
        &mut mcp,
        "boot",
        json!({
            "kickstart": base64::engine::general_purpose::STANDARD.encode(rom),
            "model": spec.model,
            "slow_ram": spec.slow_ram_kib,
        }),
    );
    assert_eq!(boot_result.get("status"), Some(&json!("ok")));

    let cpu_paths = query_paths(&mut mcp, "cpu.");
    let agnus_paths = query_paths(&mut mcp, "agnus.");
    let denise_paths = query_paths(&mut mcp, "denise.");
    let mut support_paths = BTreeMap::new();
    for (key, prefix, wanted_paths) in COMMON_SUPPORT_PATHS {
        support_paths.insert(
            (*key).to_owned(),
            collect_known_paths(&mut mcp, prefix, wanted_paths),
        );
    }
    for prefix in ["gayle.", "dmac.", "ramsey.", "fat_gary."] {
        let paths = query_paths(&mut mcp, prefix);
        if !paths.is_empty() {
            support_paths.insert(support_key(prefix), paths);
        }
    }

    assert!(cpu_paths.iter().any(|path| path == "cpu.pc"));
    assert!(cpu_paths.iter().any(|path| path == "cpu.idle"));
    assert!(agnus_paths.iter().any(|path| path == "agnus.vpos"));
    assert!(agnus_paths.iter().any(|path| path == "agnus.beamcon0"));
    assert!(denise_paths.iter().any(|path| path == "denise.bplcon0"));
    assert!(denise_paths.iter().any(|path| path == "denise.palette.31"));
    for prefix in spec.required_support_prefixes {
        let key = support_key(prefix);
        assert!(
            support_paths.contains_key(&key),
            "expected support-chip surface for {prefix}"
        );
    }

    let checkpoint_frames = checkpoint_frames();
    let mut checkpoints = Vec::with_capacity(checkpoint_frames.len());
    let mut last_frame = 0;

    for frame in checkpoint_frames {
        if frame > last_frame {
            dispatch_success(
                &mut mcp,
                "run_frames",
                json!({ "count": frame - last_frame }),
            );
        }
        last_frame = frame;
        let mut support = BTreeMap::new();
        for (key, paths) in &support_paths {
            support.insert(key.clone(), collect_values(&mut mcp, paths));
        }

        checkpoints.push(ProbeCheckpoint {
            frame,
            master_clock: query_value(&mut mcp, "master_clock"),
            cpu: collect_values(&mut mcp, &cpu_paths),
            agnus: collect_values(&mut mcp, &agnus_paths),
            denise: collect_values(&mut mcp, &denise_paths),
            support,
        });
    }

    let report = ProbeReport {
        model: spec.model,
        rom_path: spec.rom_path,
        cpu_paths,
        agnus_paths,
        denise_paths,
        support_paths,
        checkpoints,
    };

    write_report(spec.report_name, &report);
}

#[test]
#[ignore]
fn probe_boot_state_a500() {
    run_probe(&ProbeSpec {
        model: "a500",
        rom_path: "../../roms/kick13.rom",
        report_name: "probe_kick13_a500",
        slow_ram_kib: 512,
        required_support_prefixes: &[],
    });
}

#[test]
#[ignore]
fn probe_boot_state_a1200() {
    run_probe(&ProbeSpec {
        model: "a1200",
        rom_path: "../../roms/kick31_40_068_a1200.rom",
        report_name: "probe_kick31_a1200",
        slow_ram_kib: 0,
        required_support_prefixes: &["gayle."],
    });
}

#[test]
#[ignore]
fn probe_boot_state_a3000() {
    run_probe(&ProbeSpec {
        model: "a3000",
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        report_name: "probe_kick31_a3000",
        slow_ram_kib: 0,
        required_support_prefixes: &["dmac.", "ramsey.", "fat_gary."],
    });
}

#[test]
#[ignore]
fn probe_boot_state_a4000() {
    run_probe(&ProbeSpec {
        model: "a4000",
        rom_path: "../../roms/kick31_40_068_a4000.rom",
        report_name: "probe_kick31_a4000",
        slow_ram_kib: 0,
        required_support_prefixes: &["ramsey.", "fat_gary."],
    });
}
