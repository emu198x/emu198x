//! Explicit consumer for the emulator-neutral programmable-HBLANK corpus.
//!
//! The neutral corpus deliberately contains no emulator-authored expectations.
//! This consumer validates fixture identity, probe execution, settled-field
//! stability, and measurement integrity, then asserts only the semantic
//! CCK-aligned observations on which the registered UAE and Copperline
//! implementation families agree. Disputed gates remain observations.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};

use emu198x_shell::{FamilyRuntime, HeadlessSession, MediaImage, MediaKind, MediaSet, PixelFormat};
use runtime_commodore_amiga::{
    AmigaLiveAccess, AmigaRuntimeKind, AmigaSessionQueryProvider, DISPLAY_HEIGHT, DISPLAY_WIDTH,
    Model,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const DIST_ENV: &str = "EMU198X_AMIGA_PROGRAMMABLE_HBLANK_DIST";
const KICKSTART_204_ENV: &str = "EMU198X_AMIGA_KICKSTART_204_ROM";
const KICKSTART_31_A1200_ENV: &str = "EMU198X_AMIGA_KICKSTART_31_A1200_ROM";

const SUITE_ID: &str = "org.198x.amiga.programmable-hblank";
const MANIFEST_SCHEMA_VERSION: &str = "1.0.0";
const SUITE_VERSION: &str = "1.0.1";
const SOURCE_REVISION: &str = "source-v2";
const CASE_FILE_SHA256: &str = "cdf0f8ea0e46250f5d2d310034d61b8b0fc13050ce7ba0659550d01153c39d3d";
const ADF_BYTES: usize = 901_120;

const READY_MAGIC: u32 = 0x4842_4C4B;
const READY_BASE: u32 = 0x0002_FF00;
const READY_CASE_NUMBER: u32 = READY_BASE + 0x04;
const READY_SCHEMA_VERSION: u32 = READY_BASE + 0x06;
const READY_FIELD_COUNTER: u32 = READY_BASE + 0x08;
const READY_BPLCON0: u32 = READY_BASE + 0x0C;
const READY_BPLCON3: u32 = READY_BASE + 0x0E;
const READY_BEAMCON0: u32 = READY_BASE + 0x10;
const READY_HBSTRT: u32 = READY_BASE + 0x12;
const READY_HBSTOP: u32 = READY_BASE + 0x14;
const READY_COLOR00: u32 = READY_BASE + 0x16;
const READY_IDENTITY: u32 = READY_BASE + 0x20;
const READY_SCHEMA_V1: u16 = 1;

const PAL_VIEWPORT_H_START_CCK: u32 = 0x2C;
const PAL_VIEWPORT_V_START_LINE: u32 = 0x19;
const OUTPUT_PIXELS_PER_CCK: u32 = 4;
const OUTPUT_ROWS_PER_BEAM_LINE: u32 = 2;
const PIXEL_DUPLICATION: usize = 2;

const RELEVANT_CUSTOM_OFFSETS: &[u16] = &[0x100, 0x106, 0x1C4, 0x1C6, 0x1DC];

type TestSession = HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>;

const EXPECTED_ADF_SHA256: &[(&str, &str)] = &[
    (
        "fixed-control",
        "fcaffa26d67316904ea9765266f158549c2a7175b1c48ae062210be451bb4e81",
    ),
    (
        "ecsena-gate",
        "342383da85bd7522976e35a6901b2e83baf6bd525edfdc846224e2fa1c345385",
    ),
    (
        "extblken-gate",
        "a24473b6ea38c0ba8461001dc7afa684ed05e8a2d4d0ff9bdc40832a10f72023",
    ),
    (
        "blanken-path",
        "6d9114e9d07d4f0e551e0f2e955552e73db65bca3a0da63cc333d20a399584f0",
    ),
    (
        "programmed-central",
        "dbce868b3ac2c932883c70b820cb1b3e0773ffc4f46f7ca21242ab362357976a",
    ),
    (
        "programmed-wrap",
        "30d01c9542c28da5f31ef56335e85f33f88aad142527ccdab27d4c54764f37ca",
    ),
    (
        "programmed-equal",
        "136f73e79668040f49f5a7e497e2de2e480f2bd1cab4a30512003ef87d172ac7",
    ),
    (
        "aga-fine-lores",
        "4ef2e3f4fdb0fdb996156f7c07facad56b7b589a9536d21627d888c02ebaaf1b",
    ),
    (
        "aga-fine-hires",
        "5983638dbea2d33123fdea377cac241498bccfae2d584f480eba7f3cf4b642e2",
    ),
    (
        "aga-fine-shres",
        "d543a6eb5f551b4cd1f0226ec20d9795b943e7c8757db7c441269997e3c940f4",
    ),
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: String,
    suite: Suite,
    build: Build,
    cases: Vec<Case>,
    artifacts: Vec<Artifact>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Suite {
    id: String,
    version: String,
    license: String,
    source_revision: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Build {
    toolchain: serde_json::Value,
    case_file_sha256: String,
    source_sha256: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    numeric_id: u16,
    question: String,
    applicability: Applicability,
    line_geometry: LineGeometry,
    programming_schedule: serde_json::Value,
    registers: Registers,
    resolution: String,
    identity: Identity,
    settle_capture: SettleCapture,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Applicability {
    agnus: Vec<String>,
    display_chip: Vec<String>,
    min_chip_ram_bytes: u32,
    regions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LineGeometry {
    coordinate_source: String,
    register_minimum: u16,
    register_maximum: u16,
    register_modulus: u16,
    sample_beam_line: u32,
    horizontal_mapping: String,
    guard_register: String,
    guard_color_word: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registers {
    bplcon0: Register,
    bplcon3: Register,
    beamcon0: Register,
    hbstrt: Register,
    hbstop: Register,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Register {
    word: String,
    symbols: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    visual: VisualIdentity,
    serial: SerialIdentity,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VisualIdentity {
    method: String,
    color00: String,
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SerialIdentity {
    encoding: String,
    terminator: String,
    value: String,
    address: String,
    maximum_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettleCapture {
    ready_record_address: String,
    ready_magic: String,
    ready_magic_width_bytes: u8,
    case_number_address: String,
    case_number_width_bytes: u8,
    schema_version_address: String,
    schema_version_width_bytes: u8,
    field_counter_address: String,
    field_counter_width_bytes: u8,
    byte_order: String,
    ready_timeout_fields: u32,
    settle_fields: u32,
    capture_fields: u32,
    adjacent_field_stability_required: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    status: String,
    observations: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    case_id: String,
    adf_file: String,
    payload_file: String,
    adf_bytes: usize,
    payload_bytes: usize,
    sha256: ArtifactHashes,
    load_address: u32,
    sectors: u32,
    bootblock_checksum: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactHashes {
    adf: String,
    payload: String,
}

#[derive(Clone, Copy)]
struct Profile {
    id: &'static str,
    display_chip: &'static str,
    model: Model,
    firmware_env: &'static str,
    firmware_label: &'static str,
    firmware_sha256: &'static str,
}

const PROFILES: &[Profile] = &[
    Profile {
        id: "a500-plus-ecs-pal",
        display_chip: "ECS Denise",
        model: Model::A500PlusEcsPal,
        firmware_env: KICKSTART_204_ENV,
        firmware_label: "Kickstart 2.04 r37.175",
        firmware_sha256: "d0b70e8a1772614b897f92c33cb299bed3fc8e3de488fc12f67f97fc2486eb79",
    },
    Profile {
        id: "a1200-aga-pal",
        display_chip: "AGA Lisa",
        model: Model::A1200AgaPal,
        firmware_env: KICKSTART_31_A1200_ENV,
        firmware_label: "A1200 Kickstart 3.1 r40.068",
        firmware_sha256: "6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707",
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct BlackRun {
    start: usize,
    end_exclusive: usize,
}

#[derive(Clone)]
struct CapturedField {
    field_counter: u32,
    rgba: Vec<u8>,
    sha256: String,
}

#[test]
#[ignore = "explicit programmable-HBLANK corpus consensus and measurement gate"]
fn programmable_hblank_corpus_matches_consensus_and_records_disagreements() {
    let dist = required_directory(DIST_ENV);
    let manifest = load_and_validate_suite(&dist);

    let aligned_cases: Vec<&Case> = manifest
        .cases
        .iter()
        .filter(|case| case_is_cck_aligned(case))
        .collect();
    assert_eq!(
        aligned_cases.len(),
        7,
        "suite v1 must retain exactly seven CCK-aligned cases"
    );
    assert_eq!(
        manifest.cases.len() - aligned_cases.len(),
        3,
        "suite v1 must retain exactly three AGA fine-position cases"
    );

    let firmware: BTreeMap<&str, Vec<u8>> = PROFILES
        .iter()
        .map(|profile| (profile.id, load_verified_firmware(*profile)))
        .collect();

    for case in aligned_cases {
        let artifact = artifact_for_case(&manifest, &case.id);
        let adf = load_verified_artifact(&dist, artifact);
        for profile in PROFILES {
            assert!(
                case.applicability
                    .display_chip
                    .iter()
                    .any(|chip| chip == profile.display_chip),
                "{} must declare applicability to {}",
                case.id,
                profile.display_chip
            );
            let mut session = build_session(
                *profile,
                firmware
                    .get(profile.id)
                    .expect("profile firmware must have been loaded"),
                &adf,
            );
            let mut captured = Vec::new();
            let expected = consensus_expected_runs(&case.id, profile.id);
            let result = execute_case(&mut session, case, &mut captured).and_then(|observation| {
                if let Some(expected) = expected.as_ref()
                    && observation.as_slice() != expected.as_slice()
                {
                    return Err(format!(
                        "measured black runs {observation:?}; registered cross-family consensus is {expected:?}"
                    ));
                }
                Ok(observation)
            });
            let observation = match result {
                Ok(observation) => observation,
                Err(error) => {
                    write_failure_diagnostics(
                        &session, &manifest, case, artifact, *profile, &captured, &error,
                    );
                    panic!(
                        "{} / {} programmable-HBLANK measurement failed: {error}",
                        case.id, profile.id
                    );
                }
            };
            println!(
                "HBLANK {} observation: case={} profile={} frame_sha256={} black_runs={:?} coordinate=lores-samples origin_hpos={:#x} samples_per_cck={}",
                if expected.is_some() {
                    "cross-family-consensus"
                } else {
                    "unresolved"
                },
                case.id,
                profile.id,
                captured
                    .iter()
                    .map(|field| field.sha256.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                observation,
                PAL_VIEWPORT_H_START_CCK,
                OUTPUT_PIXELS_PER_CCK / PIXEL_DUPLICATION as u32,
            );
        }
    }
}

fn consensus_expected_runs(case_id: &str, profile_id: &str) -> Option<Vec<BlackRun>> {
    let sample = |hpos| {
        ((hpos - PAL_VIEWPORT_H_START_CCK) * (OUTPUT_PIXELS_PER_CCK / PIXEL_DUPLICATION as u32))
            as usize
    };
    match case_id {
        "fixed-control" | "programmed-equal" => Some(Vec::new()),
        "programmed-central" => Some(vec![BlackRun {
            start: sample(0x80),
            end_exclusive: sample(0xA0),
        }]),
        "programmed-wrap" => Some(vec![
            BlackRun {
                start: 0,
                end_exclusive: sample(0x40),
            },
            BlackRun {
                start: sample(0xD0),
                end_exclusive: DISPLAY_WIDTH as usize / PIXEL_DUPLICATION,
            },
        ]),
        // Both registered families suppress the programmed interval on ECS
        // when BLANKEN is clear. Their AGA interpretations disagree.
        "blanken-path" if profile_id == "a500-plus-ecs-pal" => Some(Vec::new()),
        // ECSENA, EXTBLKEN, and AGA BLANKEN remain software-family
        // disagreements and therefore measurement-only cases.
        _ => None,
    }
}

fn load_and_validate_suite(dist: &Path) -> Manifest {
    let manifest_path = dist.join("suite-v1.json");
    let bytes = fs::read(&manifest_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", manifest_path.display()));
    let manifest: Manifest = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "decode strict suite manifest {}: {error}",
            manifest_path.display()
        )
    });

    assert_eq!(manifest.schema_version, MANIFEST_SCHEMA_VERSION);
    assert_eq!(manifest.suite.id, SUITE_ID);
    assert_eq!(manifest.suite.version, SUITE_VERSION);
    assert_eq!(manifest.suite.license, "CC0-1.0");
    assert_eq!(manifest.suite.source_revision, SOURCE_REVISION);
    assert_eq!(manifest.build.case_file_sha256, CASE_FILE_SHA256);
    assert!(
        manifest.build.toolchain.is_object(),
        "suite toolchain record must be an object"
    );
    for source in [
        "src/bootblock.S",
        "src/probe.S",
        "src/custom-registers.inc",
        "tools/build.py",
        "tools/make_adf.py",
        "schema/suite-v1.schema.json",
        "schema/capture-v1.schema.json",
    ] {
        assert!(
            manifest.build.source_sha256.contains_key(source),
            "suite manifest is missing source identity {source}"
        );
    }

    let expected_hashes: BTreeMap<&str, &str> = EXPECTED_ADF_SHA256.iter().copied().collect();
    assert_eq!(manifest.cases.len(), expected_hashes.len());
    assert_eq!(manifest.artifacts.len(), expected_hashes.len());

    let mut case_ids = BTreeSet::new();
    for case in &manifest.cases {
        assert!(
            case_ids.insert(case.id.as_str()),
            "duplicate case {}",
            case.id
        );
        assert_eq!(case.expected.status, "unresolved");
        assert!(
            case.expected.observations.is_empty(),
            "{} must not contain emulator-authored expectations",
            case.id
        );
        validate_case_contract(case);
    }
    assert_eq!(
        case_ids,
        expected_hashes.keys().copied().collect(),
        "suite case identities changed"
    );

    let mut artifact_ids = BTreeSet::new();
    for artifact in &manifest.artifacts {
        assert!(
            artifact_ids.insert(artifact.case_id.as_str()),
            "duplicate artifact {}",
            artifact.case_id
        );
        assert_eq!(artifact.adf_bytes, ADF_BYTES);
        assert!(artifact.payload_bytes > 0);
        assert_eq!(artifact.load_address, 0x0003_0000);
        assert!(artifact.sectors >= 1);
        assert!(artifact.bootblock_checksum.starts_with("0x"));
        assert_eq!(
            artifact.sha256.adf,
            expected_hashes[artifact.case_id.as_str()],
            "{} ADF identity changed",
            artifact.case_id
        );
        assert_eq!(artifact.sha256.payload.len(), 64);
        assert_safe_relative_file(&artifact.adf_file, "adf");
        assert_safe_relative_file(&artifact.payload_file, "bin");
    }
    assert_eq!(artifact_ids, case_ids);
    manifest
}

fn validate_case_contract(case: &Case) {
    assert!(case.numeric_id > 0);
    assert!(case.question.ends_with('?'));
    assert_eq!(case.applicability.regions, ["PAL"]);
    assert!(case.applicability.min_chip_ram_bytes >= 512 * 1024);
    assert!(!case.applicability.agnus.is_empty());
    assert!(case.programming_schedule.is_object());
    assert!(matches!(
        case.resolution.as_str(),
        "lores" | "hires" | "super-hires"
    ));
    assert_eq!(
        case.line_geometry.coordinate_source,
        "raw 11-bit HBSTRT/HBSTOP register words"
    );
    assert_eq!(case.line_geometry.register_minimum, 0);
    assert_eq!(case.line_geometry.register_maximum, 2047);
    assert_eq!(case.line_geometry.register_modulus, 2048);
    assert_eq!(case.line_geometry.sample_beam_line, 128);
    assert_eq!(case.line_geometry.horizontal_mapping, "producer-recorded");
    assert_eq!(case.line_geometry.guard_register, "COLOR00");
    assert_eq!(
        case.line_geometry.guard_color_word,
        case.identity.visual.color00
    );
    assert_eq!(case.identity.visual.method, "solid COLOR00");
    assert!(!case.identity.visual.description.is_empty());
    assert_eq!(case.identity.serial.encoding, "US-ASCII");
    assert_eq!(case.identity.serial.terminator, "NUL");
    assert_eq!(case.identity.serial.address, "0x0002ff20");
    assert_eq!(case.identity.serial.maximum_bytes, 64);
    assert!(case.identity.serial.value.is_ascii());
    assert!(case.identity.serial.value.len() < 64);
    validate_settle_contract(&case.settle_capture);

    for register in [
        &case.registers.bplcon0,
        &case.registers.bplcon3,
        &case.registers.beamcon0,
        &case.registers.hbstrt,
        &case.registers.hbstop,
    ] {
        assert!(register.symbols.iter().all(|symbol| !symbol.is_empty()));
        assert!(parse_hex_word(&register.word).is_ok());
    }
}

fn validate_settle_contract(contract: &SettleCapture) {
    assert_eq!(contract.ready_record_address, "0x0002ff00");
    assert_eq!(contract.ready_magic, "HBLK");
    assert_eq!(contract.ready_magic_width_bytes, 4);
    assert_eq!(contract.case_number_address, "0x0002ff04");
    assert_eq!(contract.case_number_width_bytes, 2);
    assert_eq!(contract.schema_version_address, "0x0002ff06");
    assert_eq!(contract.schema_version_width_bytes, 2);
    assert_eq!(contract.field_counter_address, "0x0002ff08");
    assert_eq!(contract.field_counter_width_bytes, 4);
    assert_eq!(contract.byte_order, "big-endian");
    assert!(contract.ready_timeout_fields > contract.settle_fields);
    assert_eq!(contract.capture_fields, 3);
    assert!(contract.adjacent_field_stability_required);
}

fn artifact_for_case<'a>(manifest: &'a Manifest, case_id: &str) -> &'a Artifact {
    manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.case_id == case_id)
        .unwrap_or_else(|| panic!("suite is missing artifact for {case_id}"))
}

fn case_is_cck_aligned(case: &Case) -> bool {
    let start = parse_hex_word(&case.registers.hbstrt.word)
        .unwrap_or_else(|error| panic!("{} HBSTRT: {error}", case.id));
    let stop = parse_hex_word(&case.registers.hbstop.word)
        .unwrap_or_else(|error| panic!("{} HBSTOP: {error}", case.id));
    start & 0x0700 == 0 && stop & 0x0700 == 0
}

fn load_verified_artifact(dist: &Path, artifact: &Artifact) -> Vec<u8> {
    let adf_path = dist.join(&artifact.adf_file);
    let adf = fs::read(&adf_path)
        .unwrap_or_else(|error| panic!("read corpus ADF {}: {error}", adf_path.display()));
    assert_eq!(adf.len(), artifact.adf_bytes);
    assert_eq!(sha256_hex(&adf), artifact.sha256.adf);

    let payload_path = dist.join(&artifact.payload_file);
    let payload = fs::read(&payload_path)
        .unwrap_or_else(|error| panic!("read corpus payload {}: {error}", payload_path.display()));
    assert_eq!(payload.len(), artifact.payload_bytes);
    assert_eq!(sha256_hex(&payload), artifact.sha256.payload);
    adf
}

fn load_verified_firmware(profile: Profile) -> Vec<u8> {
    let path = required_file(profile.firmware_env);
    let firmware =
        fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    assert_eq!(
        firmware.len(),
        512 * 1024,
        "{} has the wrong byte length",
        profile.firmware_label
    );
    assert_eq!(
        sha256_hex(&firmware),
        profile.firmware_sha256,
        "{} does not match the registered firmware",
        profile.firmware_label
    );
    firmware
}

fn build_session(profile: Profile, firmware: &[u8], adf: &[u8]) -> TestSession {
    let runtime = AmigaRuntimeKind::new(profile.model, firmware.to_vec())
        .unwrap_or_else(|error| panic!("construct {} runtime: {error}", profile.id));
    let frame_ticks = runtime.native_frame_ticks();
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, frame_ticks, AmigaSessionQueryProvider);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, adf));
    session
        .load_media(&media)
        .unwrap_or_else(|error| panic!("insert corpus ADF into {} DF0: {error}", profile.id));
    session
        .machine_mut()
        .cpu_trace_arm(Some((0x0003_0000, 0x0003_00FF)), 2_048);
    session
}

fn execute_case(
    session: &mut TestSession,
    case: &Case,
    captured: &mut Vec<CapturedField>,
) -> Result<Vec<BlackRun>, String> {
    wait_for_ready_record(session, case)?;

    for _ in 0..case.settle_capture.capture_fields {
        session
            .run_frames(1)
            .map_err(|error| format!("run settled capture field: {error}"))?;
        let field_counter = session.machine().read_long(READY_FIELD_COUNTER);
        let frame = session
            .latest_frame()
            .ok_or_else(|| "probe did not emit a framebuffer".to_owned())?;
        if frame.format != PixelFormat::Rgba8888 {
            return Err(format!(
                "frame format is {:?}, expected RGBA8888",
                frame.format
            ));
        }
        if frame.width != DISPLAY_WIDTH || frame.height != DISPLAY_HEIGHT {
            return Err(format!(
                "frame is {}x{}, expected {}x{}",
                frame.width, frame.height, DISPLAY_WIDTH, DISPLAY_HEIGHT
            ));
        }
        let rgba = frame
            .rgba_pixels()
            .map_err(|error| format!("convert emitted frame to RGBA: {error}"))?;
        if let Some((index, alpha)) = rgba
            .chunks_exact(4)
            .enumerate()
            .find_map(|(index, pixel)| (pixel[3] != 0xFF).then_some((index, pixel[3])))
        {
            return Err(format!(
                "frame pixel {index} has non-opaque alpha {alpha:#04x}"
            ));
        }
        captured.push(CapturedField {
            field_counter,
            sha256: sha256_hex(&rgba),
            rgba,
        });
    }

    let first = captured
        .first()
        .ok_or_else(|| "capture contract produced no fields".to_owned())?;
    if let Some(different) = captured
        .iter()
        .skip(1)
        .find(|field| field.rgba != first.rgba)
    {
        return Err(format!(
            "static fields differ: counter {} hash {} versus counter {} hash {}",
            first.field_counter, first.sha256, different.field_counter, different.sha256
        ));
    }

    let row = canonical_probe_row(&first.rgba, case)?;
    measure_black_runs(&row, guard_rgb(case)?)
}

fn wait_for_ready_record(session: &mut TestSession, case: &Case) -> Result<u32, String> {
    for observed_field in 0..case.settle_capture.ready_timeout_fields {
        session
            .run_frames(1)
            .map_err(|error| format!("run probe boot field {observed_field}: {error}"))?;
        if session.machine().read_long(READY_BASE) != READY_MAGIC {
            continue;
        }

        let actual_case = session.machine().read_word(READY_CASE_NUMBER);
        let actual_schema = session.machine().read_word(READY_SCHEMA_VERSION);
        let counter = session.machine().read_long(READY_FIELD_COUNTER);
        if actual_case != case.numeric_id {
            return Err(format!(
                "ready record names case {actual_case}, expected {}",
                case.numeric_id
            ));
        }
        if actual_schema != READY_SCHEMA_V1 {
            return Err(format!(
                "ready record schema is {actual_schema}, expected {READY_SCHEMA_V1}"
            ));
        }
        validate_ready_registers(session, case)?;
        let identity =
            read_guest_ascii(session, READY_IDENTITY, case.identity.serial.maximum_bytes)?;
        if identity != case.identity.serial.value {
            return Err(format!(
                "ready identity is {identity:?}, expected {:?}",
                case.identity.serial.value
            ));
        }
        if counter >= case.settle_capture.settle_fields {
            return Ok(observed_field);
        }
    }
    Err(format!(
        "ready record did not reach field counter {} within {} emulated fields",
        case.settle_capture.settle_fields, case.settle_capture.ready_timeout_fields
    ))
}

fn validate_ready_registers(session: &TestSession, case: &Case) -> Result<(), String> {
    for (name, address, register) in [
        ("BPLCON0", READY_BPLCON0, &case.registers.bplcon0),
        ("BPLCON3", READY_BPLCON3, &case.registers.bplcon3),
        ("BEAMCON0", READY_BEAMCON0, &case.registers.beamcon0),
        ("HBSTRT", READY_HBSTRT, &case.registers.hbstrt),
        ("HBSTOP", READY_HBSTOP, &case.registers.hbstop),
    ] {
        let expected = parse_hex_word(&register.word)?;
        let actual = session.machine().read_word(address);
        if actual != expected {
            return Err(format!(
                "ready {name} is {actual:#06x}, expected {expected:#06x}"
            ));
        }
    }
    let expected_color = parse_hex_word(&case.identity.visual.color00)?;
    let actual_color = session.machine().read_word(READY_COLOR00);
    if actual_color != expected_color {
        return Err(format!(
            "ready COLOR00 is {actual_color:#05x}, expected {expected_color:#05x}"
        ));
    }
    Ok(())
}

fn read_guest_ascii(session: &TestSession, address: u32, maximum: usize) -> Result<String, String> {
    let mut bytes = Vec::with_capacity(maximum);
    for offset in 0..maximum {
        let byte_address = address + offset as u32;
        let word = session.machine().read_word(byte_address & !1);
        let byte = if byte_address & 1 == 0 {
            (word >> 8) as u8
        } else {
            word as u8
        };
        if byte == 0 {
            return String::from_utf8(bytes)
                .map_err(|error| format!("ready identity is not US-ASCII/UTF-8: {error}"));
        }
        if !byte.is_ascii() {
            return Err(format!(
                "ready identity contains non-ASCII byte {byte:#04x}"
            ));
        }
        bytes.push(byte);
    }
    Err(format!(
        "ready identity has no NUL terminator within {maximum} bytes"
    ))
}

fn canonical_probe_row(rgba: &[u8], case: &Case) -> Result<Vec<[u8; 3]>, String> {
    let beam_line = case.line_geometry.sample_beam_line;
    if beam_line < PAL_VIEWPORT_V_START_LINE {
        return Err(format!(
            "sample beam line {beam_line} precedes the PAL viewport"
        ));
    }
    let row = (beam_line - PAL_VIEWPORT_V_START_LINE) * OUTPUT_ROWS_PER_BEAM_LINE;
    if row + 1 >= DISPLAY_HEIGHT {
        return Err(format!("sample beam line {beam_line} is outside the frame"));
    }

    let row_bytes = DISPLAY_WIDTH as usize * 4;
    let first_start = row as usize * row_bytes;
    let second_start = first_start + row_bytes;
    let first = &rgba[first_start..first_start + row_bytes];
    let second = &rgba[second_start..second_start + row_bytes];
    if first != second {
        return Err(format!(
            "line-doubled rows {row} and {} differ for beam line {beam_line}",
            row + 1
        ));
    }

    let mut samples = Vec::with_capacity(DISPLAY_WIDTH as usize / PIXEL_DUPLICATION);
    for (pair_index, pair) in first.chunks_exact(8).enumerate() {
        if pair[..4] != pair[4..] {
            return Err(format!(
                "declared horizontal duplicate pair {pair_index} differs"
            ));
        }
        samples.push([pair[0], pair[1], pair[2]]);
    }
    if samples.len() * PIXEL_DUPLICATION != DISPLAY_WIDTH as usize {
        return Err("frame width is not divisible by the declared pixel duplication".to_owned());
    }
    Ok(samples)
}

fn guard_rgb(case: &Case) -> Result<[u8; 3], String> {
    let color = parse_hex_word(&case.line_geometry.guard_color_word)?;
    if color > 0x0FFF {
        return Err(format!("guard colour {color:#06x} exceeds RGB4"));
    }
    Ok([
        (((color >> 8) & 0xF) as u8) * 0x11,
        (((color >> 4) & 0xF) as u8) * 0x11,
        ((color & 0xF) as u8) * 0x11,
    ])
}

fn measure_black_runs(samples: &[[u8; 3]], guard: [u8; 3]) -> Result<Vec<BlackRun>, String> {
    let mut runs = Vec::new();
    let mut run_start = None;
    for (index, sample) in samples.iter().enumerate() {
        if *sample != [0, 0, 0] && *sample != guard {
            return Err(format!(
                "sample {index} is {sample:?}, expected black or guard {guard:?}"
            ));
        }
        if *sample == [0, 0, 0] {
            run_start.get_or_insert(index);
        } else if let Some(start) = run_start.take() {
            runs.push(BlackRun {
                start,
                end_exclusive: index,
            });
        }
    }
    if let Some(start) = run_start {
        runs.push(BlackRun {
            start,
            end_exclusive: samples.len(),
        });
    }
    Ok(runs)
}

fn write_failure_diagnostics(
    session: &TestSession,
    manifest: &Manifest,
    case: &Case,
    artifact: &Artifact,
    profile: Profile,
    captured: &[CapturedField],
    error: &str,
) {
    let dir = diagnostics_root()
        .join(&manifest.suite.version)
        .join(&case.id)
        .join(profile.id);
    if let Err(io_error) = fs::create_dir_all(&dir) {
        eprintln!(
            "failed to create HBLANK diagnostics {}: {io_error}",
            dir.display()
        );
        return;
    }

    for (index, field) in captured.iter().enumerate() {
        let path = dir.join(format!("field-{index}.rgba"));
        if let Err(io_error) = fs::write(&path, &field.rgba) {
            eprintln!("failed to write {}: {io_error}", path.display());
        }
    }

    let mut report = String::new();
    let _ = writeln!(report, "error={error}");
    let _ = writeln!(report, "suite_id={}", manifest.suite.id);
    let _ = writeln!(report, "suite_version={}", manifest.suite.version);
    let _ = writeln!(report, "case_id={}", case.id);
    let _ = writeln!(report, "case_number={}", case.numeric_id);
    let _ = writeln!(report, "profile={}", profile.id);
    let _ = writeln!(report, "artifact_file={}", artifact.adf_file);
    let _ = writeln!(report, "artifact_sha256={}", artifact.sha256.adf);
    let _ = writeln!(report, "firmware_sha256={}", profile.firmware_sha256);
    let _ = writeln!(report, "cpu_pc={:#010x}", session.machine().cpu_pc());
    let _ = writeln!(report, "machine_tick={}", session.machine().tick_count());
    let drive = session.machine().drive();
    let _ = writeln!(report, "drive_has_disk={}", drive.has_disk());
    let _ = writeln!(report, "drive_motor_on={}", drive.motor_on());
    let _ = writeln!(report, "drive_motor_spinning={}", drive.motor_spinning());
    let _ = writeln!(report, "drive_cylinder={}", drive.cylinder());
    let _ = writeln!(report, "drive_head={}", drive.head());
    let _ = writeln!(
        report,
        "ready_magic={:#010x}",
        session.machine().read_long(READY_BASE)
    );
    let _ = writeln!(
        report,
        "ready_case={}",
        session.machine().read_word(READY_CASE_NUMBER)
    );
    let _ = writeln!(
        report,
        "ready_schema={}",
        session.machine().read_word(READY_SCHEMA_VERSION)
    );
    let _ = writeln!(
        report,
        "ready_field_counter={}",
        session.machine().read_long(READY_FIELD_COUNTER)
    );
    for (index, field) in captured.iter().enumerate() {
        let _ = writeln!(
            report,
            "field_{index}_counter={} sha256={}",
            field.field_counter, field.sha256
        );
    }
    let _ = writeln!(report, "probe_cpu_trace:");
    for &(tick, pc, sr, opcode) in session.machine().cpu_trace_entries() {
        let _ = writeln!(
            report,
            "  tick={tick} pc={pc:#010x} sr={sr:#06x} opcode={opcode:#06x}"
        );
    }
    let _ = writeln!(report, "disk_register_writes:");
    for &(tick, pc, register, value) in session.machine().dsk_write_log() {
        let _ = writeln!(
            report,
            "  tick={tick} pc={pc:#010x} register={register:#05x} value={value:#06x}"
        );
    }
    let _ = writeln!(report, "chip_ram_dos_magic:");
    for address in (0_u32..0x0001_0000)
        .step_by(2)
        .filter(|&address| session.machine().read_long(address) == 0x444F_5300)
        .take(16)
    {
        let _ = writeln!(
            report,
            "  address={address:#010x} checksum={:#010x} root={:#010x} code0={:#010x}",
            session.machine().read_long(address + 4),
            session.machine().read_long(address + 8),
            session.machine().read_long(address + 12),
        );
    }
    let _ = writeln!(report, "relevant_custom_writes:");
    for &(tick, pc, address, offset, value, is_word) in session
        .machine()
        .custom_write_log()
        .iter()
        .filter(|entry| RELEVANT_CUSTOM_OFFSETS.contains(&entry.3))
    {
        let _ = writeln!(
            report,
            "  tick={tick} pc={pc:#010x} address={address:#010x} offset={offset:#05x} value={value:#06x} is_word={is_word}"
        );
    }
    let path = dir.join("failure.txt");
    if let Err(io_error) = fs::write(&path, report) {
        eprintln!("failed to write {}: {io_error}", path.display());
    } else {
        eprintln!("HBLANK failure diagnostics: {}", dir.display());
    }
}

fn required_directory(variable: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(variable)
            .unwrap_or_else(|| panic!("{variable} must name the built corpus directory")),
    );
    assert!(
        path.is_dir(),
        "{variable} does not name a directory: {}",
        path.display()
    );
    path
}

fn required_file(variable: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(variable)
            .unwrap_or_else(|| panic!("{variable} must name the registered firmware")),
    );
    assert!(
        path.is_file(),
        "{variable} does not name a readable file: {}",
        path.display()
    );
    path
}

fn assert_safe_relative_file(file: &str, extension: &str) {
    let path = Path::new(file);
    assert!(
        !path.is_absolute()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
            && path.extension().and_then(|value| value.to_str()) == Some(extension),
        "unsafe corpus artifact path: {file:?}"
    );
}

fn parse_hex_word(value: &str) -> Result<u16, String> {
    let digits = value
        .strip_prefix("0x")
        .ok_or_else(|| format!("{value:?} is not a hexadecimal word"))?;
    if digits.len() != 3 && digits.len() != 4 {
        return Err(format!("{value:?} is not a three- or four-digit word"));
    }
    u16::from_str_radix(digits, 16).map_err(|error| format!("parse {value:?}: {error}"))
}

fn diagnostics_root() -> PathBuf {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("runtime crate should be two levels below repository root");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                repo_root.join(path)
            }
        })
        .unwrap_or_else(|| repo_root.join("target"));
    target.join("accuracy/amiga-programmable-hblank")
}

fn sha256_hex(bytes: &[u8]) -> String {
    // Formatted by hand rather than with `{:x}`. RustCrypto's digest output
    // stopped implementing `LowerHex` when it moved from `GenericArray` to
    // `hybrid-array`, so the format string fails to compile on sha2 0.11.
    // Iterating the bytes works on both, which keeps this independent of
    // which version is pinned.
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}
