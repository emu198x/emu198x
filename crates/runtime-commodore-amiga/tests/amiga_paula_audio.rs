//! Explicit consumer for the emulator-neutral Paula-audio corpus.
//!
//! The neutral corpus carries no emulator-authored expected waveforms. This
//! consumer validates artifact identity and probe execution, checks internal
//! invariants that follow from the three controlled cases, and compares
//! routing, cadence, and the paired half-volume relationship with the
//! registered vAmiga reference package. Absolute output level is deliberately
//! not compared across the two modelled analogue pipelines.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use emu198x_shell::{FamilyRuntime, HeadlessSession, MediaImage, MediaKind, MediaSet};
use runtime_commodore_amiga::{
    A500_PAL_CCK_HZ, AmigaLiveAccess, AmigaRuntimeKind, AmigaSessionQueryProvider, Model,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const DIST_ENV: &str = "EMU198X_AMIGA_PAULA_AUDIO_DIST";
const KICKSTART_ENV: &str = "EMU198X_AMIGA_KICKSTART_13_ROM";
const REFERENCE_PACKAGE: &str =
    "../../test-data/commodore/amiga/paula-audio/references/vamiga-4.4b12-60fd1e6b";
const REFERENCE_PACKAGE_SHA256: &str =
    "9f8694c1a802efa47a210ef1b3c723dc83167821e2b6e0acd632fccdee13d494";
const REFERENCE_PRODUCER_REVISION: &str = "60fd1e6b69dcd77c9f44d1291bd37ec715362ab0";

const SUITE_ID: &str = "org.198x.amiga.paula-audio";
const SUITE_VERSION: &str = "1.0.0";
const SOURCE_REVISION: &str = "source-v1";
const MANIFEST_SCHEMA_VERSION: &str = "1.0.0";
const CASE_FILE_SHA256: &str = "62e539a30bd29d91dfcfefcc9acf1e4c3cd11157048c6fe9eb576006e6328fd7";
const KICKSTART_SHA256: &str = "ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53";
const ADF_BYTES: usize = 901_120;
const KICKSTART_BYTES: usize = 256 * 1024;

const READY_MAGIC: u32 = 0x5041_5544;
const READY_BASE: u32 = 0x0002_FF00;
const READY_CASE_NUMBER: u32 = READY_BASE + 0x04;
const READY_SCHEMA_VERSION: u32 = READY_BASE + 0x06;
const READY_FIELD_COUNTER: u32 = READY_BASE + 0x08;
const READY_CHANNEL: u32 = READY_BASE + 0x0C;
const READY_PERIOD_CCK: u32 = READY_BASE + 0x0E;
const READY_VOLUME: u32 = READY_BASE + 0x10;
const READY_SAMPLE_WORD: u32 = READY_BASE + 0x12;
const READY_SAMPLE_WORDS: u32 = READY_BASE + 0x14;
const READY_DMACON_WORD: u32 = READY_BASE + 0x16;
const READY_SAMPLE_ADDRESS: u32 = READY_BASE + 0x18;
const READY_IDENTITY: u32 = READY_BASE + 0x20;
const READY_SCHEMA_V1: u16 = 1;

const EXPECTED_ADF_SHA256: &[(&str, &str)] = &[
    (
        "channel-0-full",
        "f47320f1a08d6c8985df716e62e0e8af22d067546cbe0ff0c3cc6a852b4ad0d2",
    ),
    (
        "channel-1-full",
        "19be17045bc856522f1e159b84db9d7070050dbd976d6871560edde1497f1cab",
    ),
    (
        "channel-0-half",
        "240f56d63234c64d603661b5c9b8e17eaa8886f610b0383e3c960f0a94b867b3",
    ),
];

type TestSession = HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>;

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
    applicability: serde_json::Value,
    sample: Sample,
    period_cck: u16,
    filter_control: serde_json::Value,
    capture: Capture,
    channel: u16,
    volume: u16,
    visual_identity: serde_json::Value,
    serial_identity: String,
    comparison: Option<Comparison>,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sample {
    word: String,
    encoding: String,
    words: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    ready_record_address: String,
    ready_magic: String,
    byte_order: String,
    ready_timeout_fields: u32,
    settle_fields: u32,
    capture_fields: u32,
    automatic_gain_control: bool,
    channel_remapping: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Comparison {
    case_id: String,
    differing_fields: Vec<String>,
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

#[derive(Clone, Copy, Debug)]
struct Measurement {
    sample_rate_hz: u32,
    left_rms: f64,
    right_rms: f64,
    left_fundamental_hz: Option<f64>,
    right_fundamental_hz: Option<f64>,
}

#[derive(Debug)]
struct ReferenceObservation {
    measurement: Measurement,
    dominant_output: String,
    amplitude_ratio: Option<f64>,
}

#[test]
#[ignore = "FIXTURE: explicit project-authored Paula waveform corpus gate"]
fn paula_audio_corpus_matches_registered_routing_cadence_and_volume_relationship() {
    let dist = required_directory(DIST_ENV);
    let manifest = load_and_validate_suite(&dist);
    let references = load_and_validate_reference_package(&manifest);
    let firmware = load_verified_kickstart();
    let mut measurements = BTreeMap::new();

    for case in &manifest.cases {
        let artifact = artifact_for_case(&manifest, &case.id);
        let adf = load_verified_artifact(&dist, artifact);
        let mut session = build_session(&firmware, &adf);
        wait_for_ready_record(&mut session, case)
            .unwrap_or_else(|error| panic!("{} probe did not become ready: {error}", case.id));

        session.clear_audio_capture();
        session
            .run_frames(case.capture.capture_fields)
            .unwrap_or_else(|error| panic!("capture {} audio: {error}", case.id));
        let wav = session
            .audio_wav_bytes()
            .unwrap_or_else(|error| panic!("encode {} audio: {error}", case.id));
        let measurement = measure_pcm16_stereo(&wav)
            .unwrap_or_else(|error| panic!("measure {} audio: {error}", case.id));
        validate_case_measurement(case, measurement);
        println!(
            "Paula observation: case={} sample_rate={} left_rms={:.9} right_rms={:.9} left_hz={:?} right_hz={:?}",
            case.id,
            measurement.sample_rate_hz,
            measurement.left_rms,
            measurement.right_rms,
            measurement.left_fundamental_hz,
            measurement.right_fundamental_hz,
        );
        measurements.insert(case.id.as_str(), measurement);
    }

    let channel_0_full = measurements["channel-0-full"];
    let channel_1_full = measurements["channel-1-full"];
    let channel_0_half = measurements["channel-0-half"];

    let full_channel_balance =
        (channel_0_full.right_rms - channel_1_full.left_rms).abs() / channel_0_full.right_rms;
    assert!(
        full_channel_balance < 0.001,
        "equivalent full-volume channels differ by {:.4}%",
        full_channel_balance * 100.0
    );

    let half_ratio = channel_0_half.right_rms / channel_0_full.right_rms;
    assert!(
        (half_ratio - 0.5).abs() < 0.01,
        "volume 32 / volume 64 RMS ratio is {half_ratio}, expected 0.5"
    );

    for case in &manifest.cases {
        let emu198x = measurements[case.id.as_str()];
        let reference = &references[&case.id];
        let (expected_output, _, _, emu198x_hz) = dominant_observation(case.channel, emu198x);
        assert_eq!(
            reference.dominant_output, expected_output,
            "{} reference routes to the wrong output",
            case.id
        );
        let (_, _, _, reference_hz) = dominant_observation(case.channel, reference.measurement);
        let emu198x_hz = emu198x_hz.expect("Emu198x dominant output must have a fundamental");
        let reference_hz = reference_hz.expect("reference dominant output must have a fundamental");
        let relative_difference = (emu198x_hz - reference_hz).abs() / reference_hz;
        assert!(
            relative_difference < 0.001,
            "{} cadence differs from the registered reference by {:.4}%: Emu198x={emu198x_hz} Hz, reference={reference_hz} Hz",
            case.id,
            relative_difference * 100.0
        );
    }

    let reference_half_ratio = references["channel-0-half"]
        .amplitude_ratio
        .expect("half-volume reference must record its paired amplitude ratio");
    assert!(
        (half_ratio - reference_half_ratio).abs() < 0.001,
        "Emu198x half/full ratio {half_ratio} differs from registered reference {reference_half_ratio}"
    );
}

fn load_and_validate_reference_package(
    manifest: &Manifest,
) -> BTreeMap<String, ReferenceObservation> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(REFERENCE_PACKAGE);
    assert!(
        root.is_dir(),
        "registered reference package is missing: {}",
        root.display()
    );

    let package_path = root.join("package-v1.json");
    let package_bytes = fs::read(&package_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", package_path.display()));
    assert_eq!(sha256_hex(&package_bytes), REFERENCE_PACKAGE_SHA256);
    let package: serde_json::Value = serde_json::from_slice(&package_bytes)
        .unwrap_or_else(|error| panic!("decode {}: {error}", package_path.display()));

    assert_eq!(json_str(&package, "/schema_version"), "1.0.0");
    assert_eq!(json_str(&package, "/suite/id"), SUITE_ID);
    assert_eq!(json_str(&package, "/suite/version"), SUITE_VERSION);
    assert_eq!(
        json_str(&package, "/producer/implementation_family"),
        "vAmiga"
    );
    assert_eq!(
        json_str(&package, "/producer/revision"),
        REFERENCE_PRODUCER_REVISION
    );
    assert_eq!(json_str(&package, "/matrix/machine"), "A500 OCS PAL");
    assert_eq!(
        json_str(&package, "/matrix/sample_format"),
        "IEEE-754 binary32 little-endian WAVE"
    );
    assert_eq!(json_u64(&package, "/matrix/sample_rate_hz"), 48_000);
    assert_eq!(
        json_value(&package, "/matrix/captured_guest_fields"),
        &serde_json::json!([9, 10, 11])
    );
    load_verified_reference_file(
        &root,
        &package,
        "producer_build/log_file",
        "producer_build/log_sha256",
        "log",
    );
    load_verified_reference_file(
        &root,
        &package,
        "producer_build/record_file",
        "producer_build/record_sha256",
        "json",
    );

    let runs = json_value(&package, "/runs")
        .as_array()
        .expect("reference package runs must be an array");
    assert_eq!(runs.len(), manifest.cases.len());
    let mut references = BTreeMap::new();

    for run in runs {
        let case_id = json_str(run, "/case_id");
        let case = manifest
            .cases
            .iter()
            .find(|case| case.id == case_id)
            .unwrap_or_else(|| panic!("reference package has unknown case {case_id}"));
        let artifact = artifact_for_case(manifest, case_id);

        for (file_field, hash_field, extension) in [
            ("configuration_file", "configuration_sha256", "retrosh"),
            ("manifest_file", "manifest_sha256", "json"),
            ("producer_log_file", "producer_log_sha256", "log"),
        ] {
            load_verified_reference_file(&root, run, file_field, hash_field, extension);
        }

        let (_, capture_bytes) =
            load_verified_reference_file(&root, run, "capture_file", "capture_sha256", "wav");
        let (_, record_bytes) =
            load_verified_reference_file(&root, run, "record_file", "record_sha256", "json");
        let record: serde_json::Value = serde_json::from_slice(&record_bytes)
            .unwrap_or_else(|error| panic!("decode reference record for {case_id}: {error}"));

        assert_eq!(json_str(&record, "/schema_version"), "1.0.0");
        assert_eq!(json_str(&record, "/suite_id"), SUITE_ID);
        assert_eq!(json_str(&record, "/suite_version"), SUITE_VERSION);
        assert_eq!(json_str(&record, "/case_id"), case_id);
        assert_eq!(
            json_str(&record, "/producer/implementation_family"),
            "vAmiga"
        );
        assert_eq!(
            json_str(&record, "/producer/revision"),
            REFERENCE_PRODUCER_REVISION
        );
        assert_eq!(
            json_value(&record, "/execution/captured_fields"),
            &serde_json::json!([9, 10, 11])
        );
        assert!(json_bool(&record, "/execution/cold_boot"));
        assert_eq!(json_u64(&record, "/execution/ready_observed_field"), 8);
        assert_eq!(json_u64(&record, "/execution/settle_fields"), 8);

        assert_eq!(json_str(&record, "/artifact/adf_file"), artifact.adf_file);
        assert_eq!(
            json_str(&record, "/artifact/adf_sha256"),
            artifact.sha256.adf
        );
        assert_eq!(
            json_str(&record, "/artifact/payload_file"),
            artifact.payload_file
        );
        assert_eq!(
            json_str(&record, "/artifact/payload_sha256"),
            artifact.sha256.payload
        );

        assert_eq!(
            json_str(&record, "/source_capture/file_name"),
            format!("../captures/{case_id}.wav")
        );
        assert_eq!(
            json_str(&record, "/source_capture/file_sha256"),
            json_str(run, "/capture_sha256")
        );
        assert_eq!(
            json_str(&record, "/source_capture/domain"),
            "modelled-analogue-output"
        );
        assert_eq!(
            json_str(&record, "/source_capture/sample_format"),
            "IEEE-754 binary32 little-endian WAVE"
        );
        assert_eq!(json_u64(&record, "/source_capture/sample_rate_hz"), 48_000);
        assert!(!json_bool(
            &record,
            "/source_capture/automatic_gain_control"
        ));
        assert!(!json_bool(&record, "/source_capture/channel_remapping"));
        assert_eq!(json_str(&record, "/observations/status"), "observed");

        let measurement = measure_float32_stereo(&capture_bytes)
            .unwrap_or_else(|error| panic!("measure registered {case_id} capture: {error}"));
        assert_eq!(measurement.sample_rate_hz, 48_000);
        assert_close(
            measurement.left_rms,
            json_f64(&record, "/observations/rms/left"),
            1.0e-12,
            &format!("{case_id} left RMS"),
        );
        assert_close(
            measurement.right_rms,
            json_f64(&record, "/observations/rms/right"),
            1.0e-12,
            &format!("{case_id} right RMS"),
        );
        assert_optional_close(
            measurement.left_fundamental_hz,
            json_value(&record, "/observations/fundamental_hz/left"),
            1.0e-9,
            &format!("{case_id} left fundamental"),
        );
        assert_optional_close(
            measurement.right_fundamental_hz,
            json_value(&record, "/observations/fundamental_hz/right"),
            1.0e-9,
            &format!("{case_id} right fundamental"),
        );

        let dominant_output = json_str(&record, "/observations/dominant_channel").to_owned();
        let (expected_output, dominant_rms, silent_rms, _) =
            dominant_observation(case.channel, measurement);
        assert_eq!(dominant_output, expected_output);
        assert!(dominant_rms > 0.0);
        assert_eq!(silent_rms, 0.0);

        let amplitude_ratio = match json_value(&record, "/observations/amplitude_ratio") {
            serde_json::Value::Null => None,
            ratio => {
                assert_eq!(json_str(ratio, "/metric"), "dominant-channel AC RMS");
                assert_eq!(json_str(ratio, "/reference_case_id"), "channel-0-full");
                Some(json_f64(ratio, "/value"))
            }
        };

        assert!(
            references
                .insert(
                    case_id.to_owned(),
                    ReferenceObservation {
                        measurement,
                        dominant_output,
                        amplitude_ratio,
                    },
                )
                .is_none(),
            "duplicate registered reference case {case_id}"
        );
    }

    let expected_ids: BTreeSet<&str> = manifest.cases.iter().map(|case| case.id.as_str()).collect();
    assert_eq!(
        references
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        expected_ids
    );

    let full = references["channel-0-full"].measurement.right_rms;
    let half = references["channel-0-half"].measurement.right_rms;
    let recorded_ratio = references["channel-0-half"]
        .amplitude_ratio
        .expect("half-volume reference must record its paired amplitude ratio");
    assert_close(
        half / full,
        recorded_ratio,
        1.0e-12,
        "registered half/full amplitude ratio",
    );

    references
}

fn load_verified_reference_file(
    root: &Path,
    run: &serde_json::Value,
    file_field: &str,
    hash_field: &str,
    extension: &str,
) -> (PathBuf, Vec<u8>) {
    let file = json_str(run, &format!("/{file_field}"));
    assert_safe_relative_file(file, extension);
    let path = root.join(file);
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    assert_eq!(
        sha256_hex(&bytes),
        json_str(run, &format!("/{hash_field}")),
        "registered file hash changed: {}",
        path.display()
    );
    (path, bytes)
}

fn json_value<'a>(value: &'a serde_json::Value, pointer: &str) -> &'a serde_json::Value {
    value
        .pointer(pointer)
        .unwrap_or_else(|| panic!("reference JSON is missing {pointer}"))
}

fn json_str<'a>(value: &'a serde_json::Value, pointer: &str) -> &'a str {
    json_value(value, pointer)
        .as_str()
        .unwrap_or_else(|| panic!("reference JSON {pointer} is not a string"))
}

fn json_u64(value: &serde_json::Value, pointer: &str) -> u64 {
    json_value(value, pointer)
        .as_u64()
        .unwrap_or_else(|| panic!("reference JSON {pointer} is not an unsigned integer"))
}

fn json_f64(value: &serde_json::Value, pointer: &str) -> f64 {
    json_value(value, pointer)
        .as_f64()
        .unwrap_or_else(|| panic!("reference JSON {pointer} is not numeric"))
}

fn json_bool(value: &serde_json::Value, pointer: &str) -> bool {
    json_value(value, pointer)
        .as_bool()
        .unwrap_or_else(|| panic!("reference JSON {pointer} is not Boolean"))
}

fn load_and_validate_suite(dist: &Path) -> Manifest {
    let path = dist.join("suite-v1.json");
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("decode strict manifest {}: {error}", path.display()));

    assert_eq!(manifest.schema_version, MANIFEST_SCHEMA_VERSION);
    assert_eq!(manifest.suite.id, SUITE_ID);
    assert_eq!(manifest.suite.version, SUITE_VERSION);
    assert_eq!(manifest.suite.license, "CC0-1.0");
    assert_eq!(manifest.suite.source_revision, SOURCE_REVISION);
    assert_eq!(manifest.build.case_file_sha256, CASE_FILE_SHA256);
    assert!(manifest.build.toolchain.is_object());
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
            "manifest is missing source identity {source}"
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
        assert!(case.question.ends_with('?'));
        assert!(case.applicability.is_object());
        assert!(case.filter_control.is_object());
        assert!(case.visual_identity.is_object());
        assert_eq!(
            case.sample.encoding,
            "two signed 8-bit PCM samples, high byte first"
        );
        assert_eq!(case.sample.word, "0x7f81");
        assert_eq!(case.sample.words, 256);
        assert_eq!(case.period_cck, 512);
        assert!((0..=3).contains(&case.channel));
        assert!((0..=64).contains(&case.volume));
        assert!(case.serial_identity.is_ascii());
        assert!(case.serial_identity.len() < 64);
        assert_eq!(case.expected.status, "unresolved");
        assert!(case.expected.observations.is_empty());
        validate_capture_contract(&case.capture);
        if let Some(comparison) = &case.comparison {
            assert_eq!(comparison.case_id, "channel-0-full");
            assert!(
                comparison
                    .differing_fields
                    .iter()
                    .any(|field| field == "volume")
            );
        }
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
            expected_hashes[artifact.case_id.as_str()]
        );
        assert_eq!(artifact.sha256.payload.len(), 64);
        assert_safe_relative_file(&artifact.adf_file, "adf");
        assert_safe_relative_file(&artifact.payload_file, "bin");
    }
    assert_eq!(artifact_ids, case_ids);
    manifest
}

fn validate_capture_contract(capture: &Capture) {
    assert_eq!(capture.ready_record_address, "0x0002ff00");
    assert_eq!(capture.ready_magic, "PAUD");
    assert_eq!(capture.byte_order, "big-endian");
    assert!(capture.ready_timeout_fields > capture.settle_fields);
    assert_eq!(capture.settle_fields, 8);
    assert_eq!(capture.capture_fields, 3);
    assert!(!capture.automatic_gain_control);
    assert!(!capture.channel_remapping);
}

fn artifact_for_case<'a>(manifest: &'a Manifest, case_id: &str) -> &'a Artifact {
    manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.case_id == case_id)
        .unwrap_or_else(|| panic!("suite is missing artifact for {case_id}"))
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

fn load_verified_kickstart() -> Vec<u8> {
    let path = required_file(KICKSTART_ENV);
    let firmware =
        fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    assert_eq!(firmware.len(), KICKSTART_BYTES);
    assert_eq!(sha256_hex(&firmware), KICKSTART_SHA256);
    firmware
}

fn build_session(firmware: &[u8], adf: &[u8]) -> TestSession {
    let runtime = AmigaRuntimeKind::new(Model::A500OcsPal, firmware.to_vec())
        .unwrap_or_else(|error| panic!("construct A500 runtime: {error}"));
    let frame_ticks = runtime.native_frame_ticks();
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, frame_ticks, AmigaSessionQueryProvider);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, adf));
    session
        .load_media(&media)
        .unwrap_or_else(|error| panic!("insert corpus ADF: {error}"));
    session
}

fn wait_for_ready_record(session: &mut TestSession, case: &Case) -> Result<u32, String> {
    for observed_field in 0..case.capture.ready_timeout_fields {
        session
            .run_frames(1)
            .map_err(|error| format!("run boot field {observed_field}: {error}"))?;
        if session.machine().read_long(READY_BASE) != READY_MAGIC {
            continue;
        }

        let actual_case = session.machine().read_word(READY_CASE_NUMBER);
        let actual_schema = session.machine().read_word(READY_SCHEMA_VERSION);
        let field_counter = session.machine().read_long(READY_FIELD_COUNTER);
        if actual_case != case.numeric_id {
            return Err(format!(
                "ready case is {actual_case}, expected {}",
                case.numeric_id
            ));
        }
        if actual_schema != READY_SCHEMA_V1 {
            return Err(format!(
                "ready schema is {actual_schema}, expected {READY_SCHEMA_V1}"
            ));
        }
        validate_ready_record(session, case)?;
        if field_counter >= case.capture.settle_fields {
            return Ok(observed_field);
        }
    }
    Err(format!(
        "ready record did not settle within {} fields",
        case.capture.ready_timeout_fields
    ))
}

fn validate_ready_record(session: &TestSession, case: &Case) -> Result<(), String> {
    for (name, address, expected) in [
        ("channel", READY_CHANNEL, case.channel),
        ("period", READY_PERIOD_CCK, case.period_cck),
        ("volume", READY_VOLUME, case.volume),
        ("sample words", READY_SAMPLE_WORDS, case.sample.words),
    ] {
        let actual = session.machine().read_word(address);
        if actual != expected {
            return Err(format!("ready {name} is {actual}, expected {expected}"));
        }
    }
    let sample_word = parse_hex_word(&case.sample.word)?;
    if session.machine().read_word(READY_SAMPLE_WORD) != sample_word {
        return Err("ready sample word does not match the case".to_owned());
    }
    let expected_dmacon = 0x8200 | (1 << case.channel);
    if session.machine().read_word(READY_DMACON_WORD) != expected_dmacon {
        return Err("ready DMACON word does not match the selected channel".to_owned());
    }
    let sample_address = session.machine().read_long(READY_SAMPLE_ADDRESS);
    if !(0x0003_0000..0x0004_0000).contains(&sample_address) || sample_address & 1 != 0 {
        return Err(format!(
            "ready sample address is invalid: {sample_address:#010x}"
        ));
    }
    let identity = read_guest_ascii(session, READY_IDENTITY, 64)?;
    if identity != case.serial_identity {
        return Err(format!(
            "ready identity is {identity:?}, expected {:?}",
            case.serial_identity
        ));
    }
    Ok(())
}

fn validate_case_measurement(case: &Case, measurement: Measurement) {
    assert_eq!(measurement.sample_rate_hz, 48_000);
    let nominal_hz = A500_PAL_CCK_HZ as f64 / (2.0 * f64::from(case.period_cck));
    let (_, dominant_rms, silent_rms, fundamental) =
        dominant_observation(case.channel, measurement);
    assert!(
        dominant_rms > 0.05,
        "{} dominant channel is unexpectedly quiet: {dominant_rms}",
        case.id
    );
    assert!(
        silent_rms < 1.0 / 32_768.0,
        "{} inactive stereo side is not silent: {silent_rms}",
        case.id
    );
    let measured_hz = fundamental.expect("dominant channel must have a measurable fundamental");
    let relative_error = (measured_hz - nominal_hz).abs() / nominal_hz;
    assert!(
        relative_error < 0.005,
        "{} fundamental is {measured_hz} Hz, nominal {nominal_hz} Hz",
        case.id
    );
}

fn dominant_observation(
    channel: u16,
    measurement: Measurement,
) -> (&'static str, f64, f64, Option<f64>) {
    match channel {
        0 | 3 => (
            "right",
            measurement.right_rms,
            measurement.left_rms,
            measurement.right_fundamental_hz,
        ),
        1 | 2 => (
            "left",
            measurement.left_rms,
            measurement.right_rms,
            measurement.left_fundamental_hz,
        ),
        _ => unreachable!("case validation limits channel to 0..3"),
    }
}

fn measure_pcm16_stereo(wav: &[u8]) -> Result<Measurement, String> {
    if wav.len() < 44 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Err("capture is not a canonical RIFF/WAVE file".to_owned());
    }
    if &wav[12..16] != b"fmt " || read_u32_le(wav, 16)? != 16 {
        return Err("capture does not have the canonical 16-byte fmt chunk".to_owned());
    }
    if read_u16_le(wav, 20)? != 1 || read_u16_le(wav, 22)? != 2 || read_u16_le(wav, 34)? != 16 {
        return Err("capture must be stereo 16-bit PCM".to_owned());
    }
    if &wav[36..40] != b"data" {
        return Err("capture does not have the canonical data chunk".to_owned());
    }

    let sample_rate_hz = read_u32_le(wav, 24)?;
    let data_bytes = usize::try_from(read_u32_le(wav, 40)?)
        .map_err(|_| "WAV data length does not fit usize".to_owned())?;
    let end = 44usize
        .checked_add(data_bytes)
        .ok_or_else(|| "WAV data length overflow".to_owned())?;
    if end != wav.len() || data_bytes % 4 != 0 {
        return Err("WAV data length is inconsistent with stereo PCM".to_owned());
    }

    let mut left = Vec::with_capacity(data_bytes / 4);
    let mut right = Vec::with_capacity(data_bytes / 4);
    for frame in wav[44..].as_chunks::<4>().0.iter() {
        left.push(f64::from(i16::from_le_bytes([frame[0], frame[1]])) / 32_768.0);
        right.push(f64::from(i16::from_le_bytes([frame[2], frame[3]])) / 32_768.0);
    }
    if left.len() < 100 {
        return Err("capture window is too short".to_owned());
    }

    Ok(Measurement {
        sample_rate_hz,
        left_rms: ac_rms(&left),
        right_rms: ac_rms(&right),
        left_fundamental_hz: fundamental_hz(&left, sample_rate_hz),
        right_fundamental_hz: fundamental_hz(&right, sample_rate_hz),
    })
}

fn measure_float32_stereo(wav: &[u8]) -> Result<Measurement, String> {
    if wav.len() < 58 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Err("capture is not a canonical RIFF/WAVE file".to_owned());
    }
    let riff_bytes = usize::try_from(read_u32_le(wav, 4)?)
        .map_err(|_| "RIFF length does not fit usize".to_owned())?
        .checked_add(8)
        .ok_or_else(|| "RIFF length overflow".to_owned())?;
    if riff_bytes != wav.len() {
        return Err("RIFF length is inconsistent with the file".to_owned());
    }
    if &wav[12..16] != b"fmt " || read_u32_le(wav, 16)? != 18 {
        return Err("capture does not have the canonical 18-byte fmt chunk".to_owned());
    }
    if read_u16_le(wav, 20)? != 3
        || read_u16_le(wav, 22)? != 2
        || read_u32_le(wav, 24)? != 48_000
        || read_u32_le(wav, 28)? != 384_000
        || read_u16_le(wav, 32)? != 8
        || read_u16_le(wav, 34)? != 32
        || read_u16_le(wav, 36)? != 0
    {
        return Err("capture must be 48 kHz stereo IEEE-754 binary32".to_owned());
    }
    if &wav[38..42] != b"fact" || read_u32_le(wav, 42)? != 4 {
        return Err("capture does not have the canonical fact chunk".to_owned());
    }
    if &wav[50..54] != b"data" {
        return Err("capture does not have the canonical data chunk".to_owned());
    }

    let sample_frames = usize::try_from(read_u32_le(wav, 46)?)
        .map_err(|_| "WAV frame count does not fit usize".to_owned())?;
    let data_bytes = usize::try_from(read_u32_le(wav, 54)?)
        .map_err(|_| "WAV data length does not fit usize".to_owned())?;
    let end = 58usize
        .checked_add(data_bytes)
        .ok_or_else(|| "WAV data length overflow".to_owned())?;
    if end != wav.len() || data_bytes % 8 != 0 || sample_frames != data_bytes / 8 {
        return Err("WAV data length is inconsistent with stereo binary32".to_owned());
    }
    if sample_frames != 2_885 {
        return Err(format!(
            "registered capture has {sample_frames} frames, expected 2885"
        ));
    }

    let mut left = Vec::with_capacity(sample_frames);
    let mut right = Vec::with_capacity(sample_frames);
    for frame in wav[58..].as_chunks::<8>().0.iter() {
        let left_sample = f32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]);
        let right_sample = f32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]);
        if !left_sample.is_finite() || !right_sample.is_finite() {
            return Err("capture contains a non-finite sample".to_owned());
        }
        left.push(f64::from(left_sample));
        right.push(f64::from(right_sample));
    }

    Ok(Measurement {
        sample_rate_hz: 48_000,
        left_rms: ac_rms(&left),
        right_rms: ac_rms(&right),
        left_fundamental_hz: fundamental_hz(&left, 48_000),
        right_fundamental_hz: fundamental_hz(&right, 48_000),
    })
}

fn assert_close(actual: f64, expected: f64, tolerance: f64, context: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{context} is {actual}, expected {expected} within {tolerance}"
    );
}

fn assert_optional_close(
    actual: Option<f64>,
    expected: &serde_json::Value,
    tolerance: f64,
    context: &str,
) {
    match (actual, expected) {
        (None, serde_json::Value::Null) => {}
        (Some(actual), expected) => {
            let expected = expected
                .as_f64()
                .unwrap_or_else(|| panic!("{context} is not numeric"));
            assert_close(actual, expected, tolerance, context);
        }
        (None, expected) => panic!("{context} is absent, expected {expected}"),
    }
}

fn ac_rms(samples: &[f64]) -> f64 {
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let mean_square = samples
        .iter()
        .map(|sample| {
            let centred = sample - mean;
            centred * centred
        })
        .sum::<f64>()
        / samples.len() as f64;
    mean_square.sqrt()
}

fn fundamental_hz(samples: &[f64], sample_rate_hz: u32) -> Option<f64> {
    if ac_rms(samples) < 1.0 / 32_768.0 {
        return None;
    }
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let crossings: Vec<usize> = samples
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| {
            (pair[0] - mean <= 0.0 && pair[1] - mean > 0.0).then_some(index + 1)
        })
        .collect();
    let first = *crossings.first()?;
    let last = *crossings.last()?;
    let intervals = crossings.len().checked_sub(1)?;
    if intervals == 0 || last <= first {
        return None;
    }
    Some(f64::from(sample_rate_hz) * intervals as f64 / (last - first) as f64)
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
                .map_err(|error| format!("ready identity is not US-ASCII: {error}"));
        }
        if !byte.is_ascii_graphic() {
            return Err(format!(
                "ready identity contains non-printable byte {byte:#04x}"
            ));
        }
        bytes.push(byte);
    }
    Err("ready identity is not NUL-terminated".to_owned())
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let pair = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "truncated WAV header".to_owned())?;
    Ok(u16::from_le_bytes([pair[0], pair[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let word = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "truncated WAV header".to_owned())?;
    Ok(u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
}

fn parse_hex_word(value: &str) -> Result<u16, String> {
    let digits = value
        .strip_prefix("0x")
        .ok_or_else(|| format!("{value:?} is not hexadecimal"))?;
    u16::from_str_radix(digits, 16).map_err(|error| format!("parse {value:?}: {error}"))
}

fn required_directory(variable: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(variable)
            .unwrap_or_else(|| panic!("{variable} must name the built corpus directory")),
    );
    assert!(
        path.is_dir(),
        "{variable} is not a directory: {}",
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
        "{variable} is not a file: {}",
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
