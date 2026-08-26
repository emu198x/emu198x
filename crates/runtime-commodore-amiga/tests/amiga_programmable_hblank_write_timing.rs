//! Strict consumer for the programmable-HBLANK write-timing corpus.
//!
//! The corpus itself contains questions and deterministic artifacts, not
//! emulator-authored expectations. This consumer binds the registered
//! FS-UAE package, validates its ten software observations, then compares
//! Emu198x at fixed beam and output coordinates. Agreement is evidence of
//! UAE-family compatibility; it is not a physical-hardware conformance claim.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use emu198x_shell::{FamilyRuntime, HeadlessSession, MediaImage, MediaKind, MediaSet, PixelFormat};
use runtime_commodore_amiga::{
    AmigaLiveAccess, AmigaRuntimeKind, AmigaSessionQueryProvider, DISPLAY_HEIGHT, DISPLAY_WIDTH,
    Model,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DIST_ENV: &str = "EMU198X_AMIGA_PROGRAMMABLE_HBLANK_WRITE_TIMING_DIST";
const KICKSTART_204_ENV: &str = "EMU198X_AMIGA_KICKSTART_204_ROM";
const KICKSTART_31_A1200_ENV: &str = "EMU198X_AMIGA_KICKSTART_31_A1200_ROM";
const GIT_REVISION_ENV: &str = "EMU198X_ACCURACY_GIT_REVISION";
const GIT_DIRTY_ENV: &str = "EMU198X_ACCURACY_GIT_DIRTY";

const SUITE_ID: &str = "org.198x.amiga.programmable-hblank-write-timing";
const SCHEMA_VERSION: &str = "1.0.0";
const SUITE_VERSION: &str = "1.0.0";
const SOURCE_REVISION: &str = "source-v1";
const CASE_FILE_SHA256: &str = "455836110ce67c1678f21f3c795359613a9cbda940efee6a9c0781cbe8c70433";
const REGISTERED_SUITE_MANIFEST_SHA256: &str =
    "fc4c4568b3ae5a7d06e07eddb18f9048937717291d022f02cfc32e68d5f6faf3";
const PACKAGE_SHA256: &str = "b6a04ae162aaf4b137a21e57c2f0ab0e5cd14bd91a4713d2b153ba1e0c95e0f3";
const PRODUCER_BUILD_SHA256: &str =
    "f3c9a7bc52a91eda942d9befd97861d5f46603d6becb1a833a53c6943d7f4ac0";
const PACKAGER_SCRIPT_SHA256: &str =
    "aa8b7cb9b92f4bf78f6acb483fef04cfd830935109243bf0ab098a171aeb6fb1";
const FS_UAE_BINARY_SHA256: &str =
    "81fdcc09bf36b6a275a9d39b27407e3484815b5713b411e16dbfe6024cf2899b";
const FS_UAE_CAPTURE_PATCH_SHA256: &str =
    "73e423453152097723b22e4ba0db7cb626b4756e5697d49154bfd98055ddd0ed";

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

const PAL_VIEWPORT_V_START_LINE: u32 = 0x19;
const OUTPUT_ROWS_PER_BEAM_LINE: u32 = 2;
const FS_RAW_WIDTH: usize = 756;
const FS_STORAGE_EXCLUSION_END: usize = 2;
const FS_TO_EMU_OUTPUT_X: usize = 8;
const EMU_COMPARISON_START: usize = FS_STORAGE_EXCLUSION_END + FS_TO_EMU_OUTPUT_X;
const EMU_COMPARISON_END: usize = FS_RAW_WIDTH + FS_TO_EMU_OUTPUT_X;
const COLOR00_OFFSET: u16 = 0x0180;
const MARKER_WORD: u16 = 0x0F0F;
const MARKER_MOVE_HPOS_CCK: u16 = 138;
const TESTED_MOVE_HPOS_CCK: u16 = 142;

const CASE_IDS: &[&str] = &[
    "midline-hbstrt-past",
    "midline-hbstop-future",
    "midline-ecsena-enable",
    "midline-extblken-enable",
    "midline-blanken-enable",
];

const SOURCE_HASHES: &[(&str, &str)] = &[
    (
        "src/bootblock.S",
        "52098f04db6a18626b4836a239d96d0f6440f5fe9acf8293b90fc7b61bc7e2bf",
    ),
    (
        "src/probe.S",
        "b9b63fc99241d52e9b66d009b1554a79af96236b4a2fe8ea008196ba811cd8bc",
    ),
    (
        "src/custom-registers.inc",
        "6cb8b48da21e745ec680c8507e919ffd825a218016f61dcdbf20b164ec20f1f5",
    ),
    (
        "tools/build.py",
        "1f59bc730ca5fddcb68ae77cb9da97edf5713f9ac680e7892271eb27982949e0",
    ),
    (
        "tools/make_adf.py",
        "abc9531dbbd38b90d13fcf8be663e339388cb321404b274ca9dd25cd3373d48d",
    ),
    (
        "schema/suite-v1.schema.json",
        "9ae0c74575bc15a89be74076ea7de0057ec2ca3fcacdbeba14686b42b440a022",
    ),
    (
        "schema/capture-v1.schema.json",
        "78b8b713fa7f2da8e24ec7f2e467149aa4b825672de3e4158b27348793bc7e74",
    ),
];

#[derive(Clone, Copy)]
struct ExpectedArtifact {
    case_id: &'static str,
    numeric_id: u16,
    adf_sha256: &'static str,
    payload_sha256: &'static str,
}

const EXPECTED_ARTIFACTS: &[ExpectedArtifact] = &[
    ExpectedArtifact {
        case_id: "midline-hbstrt-past",
        numeric_id: 1,
        adf_sha256: "4919a273e66b0c1573d97407502d0861e894d4085852844900ae2d1de3ff5ee7",
        payload_sha256: "beabe2f3328a9b34b7a7bb0ca4dee32b91b55db149fd2b724f6f2aa0aa33598e",
    },
    ExpectedArtifact {
        case_id: "midline-hbstop-future",
        numeric_id: 2,
        adf_sha256: "be45c90e522bc379015517b47f78b663070a14764ad703166c36fdaac4d486b4",
        payload_sha256: "f068b0573371e45848f3501ecf760c135dcb015edd63675fe10210f72faf3b25",
    },
    ExpectedArtifact {
        case_id: "midline-ecsena-enable",
        numeric_id: 3,
        adf_sha256: "6fbacdba30b045432a7eb281712e6542eed3447aaff6b10e8f209c01f49fc5f3",
        payload_sha256: "4fe30c111a624ae2034fcd4ee95049cbf1096ac84dd9fce8340d7f909880c928",
    },
    ExpectedArtifact {
        case_id: "midline-extblken-enable",
        numeric_id: 4,
        adf_sha256: "2af980fe37ece8018d727b9a661dc0f58a8c7a228a668ef0fb891a07c23b930a",
        payload_sha256: "f033d15ab15e409eddb18de084c800dc5e94bb320173c3928a56a1036dd2dedc",
    },
    ExpectedArtifact {
        case_id: "midline-blanken-enable",
        numeric_id: 5,
        adf_sha256: "45384fa006b082aadaf503b1467b7f63941ce38d01aa554feaa8a115c5263e17",
        payload_sha256: "fc5adfed1654c1f247d3ec763843d2ad5376ea22312489742828ed0a50f240a4",
    },
];

type TestSession = HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>;

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
        id: "ecs",
        display_chip: "ECS Denise",
        model: Model::A500PlusEcsPal,
        firmware_env: KICKSTART_204_ENV,
        firmware_label: "Kickstart 2.04 r37.175",
        firmware_sha256: "d0b70e8a1772614b897f92c33cb299bed3fc8e3de488fc12f67f97fc2486eb79",
    },
    Profile {
        id: "aga",
        display_chip: "AGA Lisa",
        model: Model::A1200AgaPal,
        firmware_env: KICKSTART_31_A1200_ENV,
        firmware_label: "A1200 Kickstart 3.1 r40.068",
        firmware_sha256: "6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707",
    },
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
    programming_schedule: ProgrammingSchedule,
    registers: Registers,
    timed_write: TimedWrite,
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
struct ProgrammingSchedule {
    phase: String,
    write_order: Vec<String>,
    steady_state: String,
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
struct TimedWrite {
    reset_beam_line: u32,
    reset_hpos_cck: u16,
    beam_line: u32,
    wait_hpos_cck: u16,
    register: String,
    word: String,
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
    marker_color00: String,
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferencePackage {
    capture_tools: BTreeMap<String, String>,
    configurations: BTreeMap<String, String>,
    matrix: PackageMatrix,
    packager: PackagePackager,
    producer: PackageProducer,
    runs: Vec<PackageRun>,
    schema_version: String,
    suite: PackageSuite,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageMatrix {
    cases: Vec<String>,
    packaged_pixel_format: String,
    profiles: Vec<String>,
    raw_height: u32,
    raw_pixel_format: String,
    raw_width: u32,
    run_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackagePackager {
    pillow_version: String,
    python_implementation: String,
    python_version: String,
    script_sha256: String,
    zlib_build_version: String,
    zlib_runtime_version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageProducer {
    binary_sha256: String,
    build_manifest_file: String,
    build_manifest_sha256: String,
    capture_patch_sha256: String,
    product: String,
    revision: String,
    source_url: String,
    uae_base_version: String,
    version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageRun {
    capture_file: String,
    capture_manifest_file: String,
    capture_manifest_sha256: String,
    capture_sha256: String,
    case_id: String,
    configuration_file: String,
    configuration_sha256: String,
    decoded_pixel_sha256: String,
    mutation_output_rows: Vec<u32>,
    profile: String,
    record_file: String,
    record_sha256: String,
    run_log_file: String,
    run_log_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageSuite {
    id: String,
    manifest_sha256: String,
    source_revision: String,
    version: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ReferenceRecord {
    artifact: RecordArtifact,
    case_id: String,
    execution: RecordExecution,
    machine: RecordMachine,
    normalization: RecordNormalization,
    observations: RecordObservations,
    producer: RecordProducer,
    schema_version: String,
    source_capture: RecordSourceCapture,
    stimulus: RecordStimulus,
    suite_id: String,
    suite_version: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordArtifact {
    adf_file: String,
    adf_sha256: String,
    payload_file: String,
    payload_sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordExecution {
    adjacent_field_stability: String,
    captured_fields: Vec<u32>,
    cold_boot: bool,
    configuration_sha256: String,
    ready_rule: RecordReadyRule,
    settle_fields: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordReadyRule {
    byte_order: String,
    case_number: u16,
    field_counter_minimum: u32,
    magic: String,
    record_address: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordMachine {
    chipset: String,
    firmware: RecordFirmware,
    region: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordFirmware {
    sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordNormalization {
    alignment_search: bool,
    beam_coordinate: RecordBeamCoordinate,
    crop: RecordCrop,
    field_handling: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordBeamCoordinate {
    horizontal_origin_sample: i32,
    horizontal_samples_per_register_increment_denominator: i32,
    horizontal_samples_per_register_increment_numerator: i32,
    phase_denominator: i32,
    phase_numerator: i32,
    sample_beam_line: u32,
    sample_row: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordCrop {
    height: u32,
    width: u32,
    x: u32,
    y: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordObservations {
    following_line_carry: String,
    guard_color_word: String,
    interval_convention: String,
    lines: Vec<RecordLine>,
    marker_color_word: String,
    storage_exclusion: [usize; 2],
    uncertainty_samples: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordLine {
    black_runs: Vec<[usize; 2]>,
    guard_runs: Vec<[usize; 2]>,
    marker_runs: Vec<[usize; 2]>,
    raw_rows: [u32; 2],
    role: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordProducer {
    implementation_family: String,
    kind: String,
    product: String,
    revision: String,
    source_url: String,
    version: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordSourceCapture {
    blanking_retained: bool,
    decoded_pixel_sha256: String,
    file_sha256: String,
    filter: String,
    height: u32,
    overscan_retained: bool,
    pixel_format: String,
    scaling: String,
    shader: String,
    stride_bytes: u32,
    width: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordStimulus {
    baseline_word: String,
    mutation_beam_line: u32,
    mutation_wait_hpos_cck: u16,
    mutation_word: String,
    reset_beam_line: u32,
    reset_wait_hpos_cck: u16,
    tested_register: String,
    write_position_evidence: RecordWritePositionEvidence,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordWritePositionEvidence {
    marker_start_sample: usize,
    method: String,
    tested_write_sample: Option<usize>,
}

#[derive(Clone)]
struct ReferenceObservation {
    run: PackageRun,
    record: ReferenceRecord,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct SemanticRun {
    start: usize,
    end_exclusive: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct SemanticLine {
    role: String,
    black_runs: Vec<SemanticRun>,
    guard_runs: Vec<SemanticRun>,
    marker_runs: Vec<SemanticRun>,
}

#[derive(Clone, Debug, Serialize)]
struct RevisionIdentity {
    full: String,
    dirty: bool,
}

#[derive(Clone, Debug, Serialize)]
struct ReadyEvidence {
    observed_after_fields: u32,
    field_counter: u32,
    copper_log_start_index: usize,
}

#[derive(Clone)]
struct CapturedField {
    field_counter: u32,
    rgba: Vec<u8>,
    sha256: String,
}

#[derive(Clone, Debug, Serialize)]
struct CapturedFieldEvidence {
    field_counter: u32,
    rgba_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
struct CopperMoveEvidence {
    cck: u64,
    vpos: u16,
    hpos: u16,
    custom_offset: u16,
    value: u16,
}

#[derive(Default)]
struct ExecutionState {
    ready: Option<ReadyEvidence>,
    captured: Vec<CapturedField>,
    copper_fields: Vec<Vec<CopperMoveEvidence>>,
    actual_lines: Option<Vec<SemanticLine>>,
}

#[derive(Debug, Serialize)]
struct CustomWriteEvidence {
    tick: u64,
    pc: u32,
    address: u32,
    custom_offset: u16,
    value: u16,
    is_word: bool,
}

#[derive(Debug, Serialize)]
struct MachineEvidence {
    cpu_pc: u32,
    machine_tick: u64,
    ready_magic: u32,
    ready_case: u16,
    ready_schema: u16,
    ready_field_counter: u32,
    relevant_cpu_custom_writes: Vec<CustomWriteEvidence>,
    recent_relevant_copper_moves: Vec<CopperMoveEvidence>,
}

#[derive(Debug, Serialize)]
struct CoordinateMapping {
    alignment_search: bool,
    reference_domain_start: usize,
    reference_domain_end_exclusive: usize,
    emu_domain_start: usize,
    emu_domain_end_exclusive: usize,
    horizontal_formula: &'static str,
    baseline_beam_line: u32,
    baseline_output_rows: [u32; 2],
    mutation_beam_line: u32,
    mutation_output_rows: [u32; 2],
    following_beam_line: u32,
    following_output_rows: [u32; 2],
}

#[derive(Debug, Serialize)]
struct CaseReport {
    schema_version: &'static str,
    status: &'static str,
    evidence_scope: &'static str,
    revision: RevisionIdentity,
    suite_id: &'static str,
    suite_version: &'static str,
    built_suite_manifest_sha256: String,
    registered_suite_manifest_sha256: &'static str,
    registered_package_sha256: &'static str,
    registered_record_file: String,
    registered_record_sha256: String,
    case_id: String,
    case_number: u16,
    profile: String,
    artifact_sha256: String,
    firmware_sha256: String,
    coordinate_mapping: CoordinateMapping,
    expected_lines: Vec<SemanticLine>,
    actual_lines: Option<Vec<SemanticLine>>,
    ready: Option<ReadyEvidence>,
    captured_fields: Vec<CapturedFieldEvidence>,
    copper_fields: Vec<Vec<CopperMoveEvidence>>,
    machine: Option<MachineEvidence>,
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct CaseSummary {
    case_id: String,
    profile: String,
    status: &'static str,
    result_file: String,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct LaneSummary {
    schema_version: &'static str,
    status: &'static str,
    evidence_scope: &'static str,
    revision: RevisionIdentity,
    suite_id: &'static str,
    suite_version: &'static str,
    built_suite_manifest_sha256: Option<String>,
    registered_suite_manifest_sha256: &'static str,
    registered_package_sha256: &'static str,
    expected_runs: usize,
    passed_runs: usize,
    failed_runs: usize,
    cases: Vec<CaseSummary>,
    fatal_error: Option<String>,
}

struct ValidatedSuite {
    manifest: Manifest,
    manifest_sha256: String,
}

struct CaseReportContext<'a> {
    revision: &'a RevisionIdentity,
    suite: &'a ValidatedSuite,
    case: &'a Case,
    artifact: &'a Artifact,
    profile: Profile,
    reference: &'a ReferenceObservation,
    expected_lines: Vec<SemanticLine>,
}

#[test]
#[ignore = "FIXTURE: explicit ten-run programmable-HBLANK write-timing UAE-family compatibility gate"]
fn programmable_hblank_write_timing_matches_registered_uae_observations() {
    let revision = revision_from_environment()
        .unwrap_or_else(|error| panic!("invalid accuracy revision identity: {error}"));
    let report_root = diagnostics_root().join(SUITE_VERSION).join(&revision.full);
    if let Err(error) = fs::create_dir_all(&report_root) {
        panic!(
            "create write-timing report root {}: {error}",
            report_root.display()
        );
    }

    if let Err(error) = run_lane(&revision, &report_root) {
        panic!("programmable-HBLANK write-timing gate failed: {error}");
    }
}

fn run_lane(revision: &RevisionIdentity, report_root: &Path) -> Result<(), String> {
    let prepared = (|| {
        let dist = required_directory(DIST_ENV)?;
        let suite = load_and_validate_suite(&dist)?;
        let references = load_and_validate_reference_package(&suite.manifest)?;
        let adfs = suite
            .manifest
            .artifacts
            .iter()
            .map(|artifact| {
                load_verified_artifact(&dist, artifact)
                    .map(|bytes| (artifact.case_id.clone(), bytes))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let firmware = PROFILES
            .iter()
            .map(|profile| load_verified_firmware(*profile).map(|bytes| (profile.id, bytes)))
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok::<_, String>((suite, references, adfs, firmware))
    })();

    let (suite, references, adfs, firmware) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let summary = LaneSummary {
                schema_version: SCHEMA_VERSION,
                status: "failed",
                evidence_scope: "registered UAE-family software observations; not hardware conformance",
                revision: revision.clone(),
                suite_id: SUITE_ID,
                suite_version: SUITE_VERSION,
                built_suite_manifest_sha256: None,
                registered_suite_manifest_sha256: REGISTERED_SUITE_MANIFEST_SHA256,
                registered_package_sha256: PACKAGE_SHA256,
                expected_runs: CASE_IDS.len() * PROFILES.len(),
                passed_runs: 0,
                failed_runs: CASE_IDS.len() * PROFILES.len(),
                cases: Vec::new(),
                fatal_error: Some(error.clone()),
            };
            write_json(&report_root.join("summary.json"), &summary)?;
            return Err(error);
        }
    };

    let mut summaries = Vec::new();
    let mut passed = 0_usize;
    let mut failed = 0_usize;

    for case in &suite.manifest.cases {
        let artifact = artifact_for_case(&suite.manifest, &case.id)?;
        let adf = adfs
            .get(&case.id)
            .ok_or_else(|| format!("missing loaded artifact for {}", case.id))?;

        for profile in PROFILES {
            if !case
                .applicability
                .display_chip
                .iter()
                .any(|chip| chip == profile.display_chip)
            {
                return Err(format!(
                    "{} does not declare applicability to {}",
                    case.id, profile.display_chip
                ));
            }
            let reference = references
                .get(&(profile.id.to_owned(), case.id.clone()))
                .ok_or_else(|| {
                    format!("registered package is missing {} / {}", profile.id, case.id)
                })?;
            let expected_lines = mapped_reference_lines(&reference.record)?;
            let case_dir = report_root.join(&case.id).join(profile.id);
            fs::create_dir_all(&case_dir)
                .map_err(|error| format!("create {}: {error}", case_dir.display()))?;

            let mut state = ExecutionState::default();
            let mut session = match build_session(
                *profile,
                firmware
                    .get(profile.id)
                    .ok_or_else(|| format!("missing loaded firmware for {}", profile.id))?,
                adf,
            ) {
                Ok(session) => session,
                Err(error) => {
                    let report = build_case_report(
                        CaseReportContext {
                            revision,
                            suite: &suite,
                            case,
                            artifact,
                            profile: *profile,
                            reference,
                            expected_lines,
                        },
                        &state,
                        None,
                        Some(error.clone()),
                    )?;
                    write_json(&case_dir.join("result.json"), &report)?;
                    failed += 1;
                    summaries.push(CaseSummary {
                        case_id: case.id.clone(),
                        profile: profile.id.to_owned(),
                        status: "failed",
                        result_file: relative_result_file(case, *profile),
                        error: Some(error),
                    });
                    continue;
                }
            };

            let result = execute_case(&mut session, case, &expected_lines, &mut state);
            let machine = Some(capture_machine_evidence(&session, case));
            let error = result.err();
            if error.is_some() {
                write_failure_frames(&case_dir, &state.captured)?;
            }
            let report = build_case_report(
                CaseReportContext {
                    revision,
                    suite: &suite,
                    case,
                    artifact,
                    profile: *profile,
                    reference,
                    expected_lines,
                },
                &state,
                machine,
                error.clone(),
            )?;
            write_json(&case_dir.join("result.json"), &report)?;

            let status = if error.is_some() {
                failed += 1;
                "failed"
            } else {
                passed += 1;
                "passed"
            };
            summaries.push(CaseSummary {
                case_id: case.id.clone(),
                profile: profile.id.to_owned(),
                status,
                result_file: relative_result_file(case, *profile),
                error,
            });
        }
    }

    let expected_runs = CASE_IDS.len() * PROFILES.len();
    if passed + failed != expected_runs {
        return Err(format!(
            "executed {} runs, expected {expected_runs}",
            passed + failed
        ));
    }
    let summary = LaneSummary {
        schema_version: SCHEMA_VERSION,
        status: if failed == 0 { "passed" } else { "failed" },
        evidence_scope: "registered UAE-family software observations; not hardware conformance",
        revision: revision.clone(),
        suite_id: SUITE_ID,
        suite_version: SUITE_VERSION,
        built_suite_manifest_sha256: Some(suite.manifest_sha256.clone()),
        registered_suite_manifest_sha256: REGISTERED_SUITE_MANIFEST_SHA256,
        registered_package_sha256: PACKAGE_SHA256,
        expected_runs,
        passed_runs: passed,
        failed_runs: failed,
        cases: summaries,
        fatal_error: None,
    };
    write_json(&report_root.join("summary.json"), &summary)?;

    if failed == 0 {
        println!(
            "programmable-HBLANK write timing: all {passed} Emu198x runs match the registered UAE-family observations; reports={}",
            report_root.display()
        );
        Ok(())
    } else {
        Err(format!(
            "{failed} of {expected_runs} runs disagree; reports are below {}",
            report_root.display()
        ))
    }
}

fn load_and_validate_suite(dist: &Path) -> Result<ValidatedSuite, String> {
    let manifest_path = dist.join("suite-v1.json");
    let bytes = read_file(&manifest_path)?;
    let manifest_sha256 = sha256_hex(&bytes);
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("decode strict {}: {error}", manifest_path.display()))?;

    require_eq(&manifest.schema_version, SCHEMA_VERSION, "suite schema")?;
    require_eq(&manifest.suite.id, SUITE_ID, "suite id")?;
    require_eq(&manifest.suite.version, SUITE_VERSION, "suite version")?;
    require_eq(&manifest.suite.license, "CC0-1.0", "suite license")?;
    require_eq(
        &manifest.suite.source_revision,
        SOURCE_REVISION,
        "suite source revision",
    )?;
    require_eq(
        &manifest.build.case_file_sha256,
        CASE_FILE_SHA256,
        "case-file hash",
    )?;
    if !manifest.build.toolchain.is_object() {
        return Err("suite toolchain record is not an object".to_owned());
    }
    let expected_sources: BTreeMap<String, String> = SOURCE_HASHES
        .iter()
        .map(|(path, hash)| ((*path).to_owned(), (*hash).to_owned()))
        .collect();
    if manifest.build.source_sha256 != expected_sources {
        return Err(format!(
            "suite source hash map changed: {:?}",
            manifest.build.source_sha256
        ));
    }

    if manifest.cases.len() != EXPECTED_ARTIFACTS.len()
        || manifest.artifacts.len() != EXPECTED_ARTIFACTS.len()
    {
        return Err(format!(
            "suite contains {} cases and {} artifacts; expected five of each",
            manifest.cases.len(),
            manifest.artifacts.len()
        ));
    }

    for (case, expected) in manifest.cases.iter().zip(EXPECTED_ARTIFACTS) {
        validate_case_contract(case, *expected)?;
    }
    for expected in EXPECTED_ARTIFACTS {
        let artifact = artifact_for_case(&manifest, expected.case_id)?;
        validate_artifact_contract(artifact, *expected)?;
    }

    Ok(ValidatedSuite {
        manifest,
        manifest_sha256,
    })
}

fn validate_case_contract(case: &Case, expected: ExpectedArtifact) -> Result<(), String> {
    require_eq(&case.id, expected.case_id, "ordered case id")?;
    if case.numeric_id != expected.numeric_id {
        return Err(format!(
            "{} numeric id is {}, expected {}",
            case.id, case.numeric_id, expected.numeric_id
        ));
    }
    if !case.question.ends_with('?') {
        return Err(format!("{} does not ask one explicit question", case.id));
    }
    if case.applicability.agnus != ["ECS Agnus".to_owned(), "AGA Alice".to_owned()]
        || case.applicability.display_chip != ["ECS Denise".to_owned(), "AGA Lisa".to_owned()]
        || case.applicability.min_chip_ram_bytes != 524_288
        || case.applicability.regions != ["PAL".to_owned()]
    {
        return Err(format!("{} applicability contract changed", case.id));
    }
    if case.line_geometry.coordinate_source != "raw 11-bit HBSTRT/HBSTOP register words"
        || case.line_geometry.register_minimum != 0
        || case.line_geometry.register_maximum != 2047
        || case.line_geometry.register_modulus != 2048
        || case.line_geometry.sample_beam_line != 128
        || case.line_geometry.horizontal_mapping != "producer-recorded"
        || case.line_geometry.guard_register != "COLOR00"
        || case.line_geometry.guard_color_word != case.identity.visual.color00
    {
        return Err(format!("{} line-geometry contract changed", case.id));
    }
    let expected_order = [
        "static register setup",
        "COP1LC",
        "DMACON Copper enable",
        "COPJMP1",
        "line 127 baseline reset",
        "line 128 COLOR00 marker",
        "line 128 tested register write",
    ];
    if case.programming_schedule.phase
        != "once per field through a Copper list after the ready record is published"
        || case.programming_schedule.write_order != expected_order.map(str::to_owned).to_vec()
        || case.programming_schedule.steady_state
            != "The Copper restores the initial register and guard colour on beam line 127, then applies one marked test write on beam line 128 in every field."
    {
        return Err(format!("{} programming schedule changed", case.id));
    }
    if case.resolution != "lores" {
        return Err(format!("{} is not a lores case", case.id));
    }
    validate_registers(case)?;
    validate_timed_write(case)?;
    validate_identity(case)?;
    validate_settle_contract(&case.settle_capture)?;
    if case.expected.status != "unresolved" || !case.expected.observations.is_empty() {
        return Err(format!(
            "{} corpus expectations must remain unresolved and empty",
            case.id
        ));
    }
    Ok(())
}

fn validate_registers(case: &Case) -> Result<(), String> {
    let registers = [
        ("BPLCON0", &case.registers.bplcon0),
        ("BPLCON3", &case.registers.bplcon3),
        ("BEAMCON0", &case.registers.beamcon0),
        ("HBSTRT", &case.registers.hbstrt),
        ("HBSTOP", &case.registers.hbstop),
    ];
    for (name, register) in registers {
        parse_hex_word(&register.word).map_err(|error| format!("{} {name}: {error}", case.id))?;
        if register.symbols.iter().any(String::is_empty) {
            return Err(format!("{} {name} contains an empty symbol", case.id));
        }
        let unique: BTreeSet<_> = register.symbols.iter().collect();
        if unique.len() != register.symbols.len() {
            return Err(format!("{} {name} repeats a symbol", case.id));
        }
    }
    if parse_hex_word(&case.registers.hbstrt.word)? > 0x07FF
        || parse_hex_word(&case.registers.hbstop.word)? > 0x07FF
    {
        return Err(format!("{} comparator word exceeds 11 bits", case.id));
    }
    Ok(())
}

fn validate_timed_write(case: &Case) -> Result<(), String> {
    let timed = &case.timed_write;
    if timed.reset_beam_line != 127
        || timed.reset_hpos_cck != 32
        || timed.beam_line != 128
        || timed.wait_hpos_cck != 136
    {
        return Err(format!("{} timed-write geometry changed", case.id));
    }
    let _ = timed_register_offset(&timed.register)?;
    let timed_word = parse_hex_word(&timed.word)?;
    if matches!(timed.register.as_str(), "HBSTRT" | "HBSTOP") && timed_word > 0x07FF {
        return Err(format!("{} timed comparator exceeds 11 bits", case.id));
    }
    Ok(())
}

fn validate_identity(case: &Case) -> Result<(), String> {
    if case.identity.visual.method != "scheduled COLOR00 marker"
        || parse_hex_word(&case.identity.visual.marker_color00)? != MARKER_WORD
        || case.identity.visual.color00 != case.line_geometry.guard_color_word
        || case.identity.visual.description.is_empty()
    {
        return Err(format!("{} visual identity changed", case.id));
    }
    let serial = &case.identity.serial;
    if serial.encoding != "US-ASCII"
        || serial.terminator != "NUL"
        || serial.address != "0x0002ff20"
        || serial.maximum_bytes != 64
        || !serial.value.is_ascii()
        || serial.value.len() + 1 > serial.maximum_bytes
    {
        return Err(format!("{} serial identity changed", case.id));
    }
    Ok(())
}

fn validate_settle_contract(contract: &SettleCapture) -> Result<(), String> {
    if contract.ready_record_address != "0x0002ff00"
        || contract.ready_magic != "HBLK"
        || contract.ready_magic_width_bytes != 4
        || contract.case_number_address != "0x0002ff04"
        || contract.case_number_width_bytes != 2
        || contract.schema_version_address != "0x0002ff06"
        || contract.schema_version_width_bytes != 2
        || contract.field_counter_address != "0x0002ff08"
        || contract.field_counter_width_bytes != 4
        || contract.byte_order != "big-endian"
        || contract.ready_timeout_fields != 600
        || contract.settle_fields != 8
        || contract.capture_fields != 3
        || !contract.adjacent_field_stability_required
    {
        return Err("settle/capture contract changed".to_owned());
    }
    Ok(())
}

fn validate_artifact_contract(
    artifact: &Artifact,
    expected: ExpectedArtifact,
) -> Result<(), String> {
    require_eq(&artifact.case_id, expected.case_id, "artifact case id")?;
    if artifact.adf_file != format!("{}.adf", expected.case_id)
        || artifact.payload_file != format!("{}.bin", expected.case_id)
        || artifact.adf_bytes != ADF_BYTES
        || artifact.payload_bytes == 0
        || artifact.sha256.adf != expected.adf_sha256
        || artifact.sha256.payload != expected.payload_sha256
        || artifact.load_address != 0x0003_0000
        || artifact.sectors != 1
        || artifact.bootblock_checksum != "0xb25c02fb"
    {
        return Err(format!("{} artifact contract changed", expected.case_id));
    }
    assert_safe_relative_file(&artifact.adf_file, "adf")?;
    assert_safe_relative_file(&artifact.payload_file, "bin")?;
    Ok(())
}

fn load_and_validate_reference_package(
    manifest: &Manifest,
) -> Result<BTreeMap<(String, String), ReferenceObservation>, String> {
    let package_root = repository_root().join(
        "test-data/commodore/amiga/programmable-hblank-write-timing/references/fs-uae-5.0.7-f362278c",
    );
    let package_path = package_root.join("package-v1.json");
    let package_bytes = read_file(&package_path)?;
    require_eq(
        &sha256_hex(&package_bytes),
        PACKAGE_SHA256,
        "registered package hash",
    )?;
    let package: ReferencePackage = serde_json::from_slice(&package_bytes)
        .map_err(|error| format!("decode strict {}: {error}", package_path.display()))?;

    validate_package_identity(&package, &package_root)?;

    let cases: BTreeMap<&str, &Case> = manifest
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect();
    let artifacts: BTreeMap<&str, &Artifact> = manifest
        .artifacts
        .iter()
        .map(|artifact| (artifact.case_id.as_str(), artifact))
        .collect();
    let mut observations = BTreeMap::new();

    for run in &package.runs {
        if !CASE_IDS.contains(&run.case_id.as_str()) {
            return Err(format!("package contains unknown case {}", run.case_id));
        }
        let profile = PROFILES
            .iter()
            .find(|profile| profile.id == run.profile)
            .copied()
            .ok_or_else(|| format!("package contains unknown profile {}", run.profile))?;
        let expected_stem = format!("{}--{}", run.profile, run.case_id);
        validate_run_paths_and_hashes(&package_root, run, &expected_stem)?;

        let configuration_key = expected_stem.clone();
        let registered_configuration = package
            .configurations
            .get(&configuration_key)
            .ok_or_else(|| format!("configuration map is missing {configuration_key}"))?;
        require_eq(
            registered_configuration,
            &run.configuration_sha256,
            "configuration-map hash",
        )?;

        let record_path = package_root.join(&run.record_file);
        let record_bytes = read_file(&record_path)?;
        let record: ReferenceRecord = serde_json::from_slice(&record_bytes)
            .map_err(|error| format!("decode {}: {error}", record_path.display()))?;
        let case = cases
            .get(run.case_id.as_str())
            .copied()
            .ok_or_else(|| format!("suite is missing case {}", run.case_id))?;
        let artifact = artifacts
            .get(run.case_id.as_str())
            .copied()
            .ok_or_else(|| format!("suite is missing artifact {}", run.case_id))?;
        validate_reference_record(&record, run, case, artifact, profile)?;

        let key = (run.profile.clone(), run.case_id.clone());
        if observations
            .insert(
                key.clone(),
                ReferenceObservation {
                    run: run.clone(),
                    record,
                },
            )
            .is_some()
        {
            return Err(format!("package repeats {} / {}", key.0, key.1));
        }
    }

    let expected_keys: BTreeSet<(String, String)> = PROFILES
        .iter()
        .flat_map(|profile| {
            CASE_IDS
                .iter()
                .map(move |case| (profile.id.to_owned(), (*case).to_owned()))
        })
        .collect();
    if observations.keys().cloned().collect::<BTreeSet<_>>() != expected_keys {
        return Err("registered package does not contain the exact ten-run matrix".to_owned());
    }
    Ok(observations)
}

fn validate_package_identity(
    package: &ReferencePackage,
    package_root: &Path,
) -> Result<(), String> {
    require_eq(&package.schema_version, SCHEMA_VERSION, "package schema")?;
    require_eq(&package.suite.id, SUITE_ID, "package suite id")?;
    require_eq(
        &package.suite.version,
        SUITE_VERSION,
        "package suite version",
    )?;
    require_eq(
        &package.suite.source_revision,
        SOURCE_REVISION,
        "package source revision",
    )?;
    require_eq(
        &package.suite.manifest_sha256,
        REGISTERED_SUITE_MANIFEST_SHA256,
        "registered suite-manifest hash",
    )?;

    if package.matrix.cases
        != CASE_IDS
            .iter()
            .map(|case| (*case).to_owned())
            .collect::<Vec<_>>()
        || package.matrix.profiles != ["ecs".to_owned(), "aga".to_owned()]
        || package.matrix.run_count != 10
        || package.matrix.raw_width != FS_RAW_WIDTH as u32
        || package.matrix.raw_height != 576
        || package.matrix.raw_pixel_format != "BGRA8888"
        || package.matrix.packaged_pixel_format != "RGBA8888"
    {
        return Err("registered package matrix identity changed".to_owned());
    }
    if package.runs.len() != package.matrix.run_count
        || package.configurations.len() != package.matrix.run_count
    {
        return Err("registered package run/configuration count changed".to_owned());
    }

    let producer = &package.producer;
    if producer.product != "FS-UAE"
        || producer.version != "5.0.7"
        || producer.revision != "f362278ccd4c60991caac3b4d240d4a3f751bea2"
        || producer.source_url != "https://github.com/FrodeSolheim/fs-uae"
        || producer.uae_base_version != "WinUAE 6.0.1"
        || producer.binary_sha256 != FS_UAE_BINARY_SHA256
        || producer.capture_patch_sha256 != FS_UAE_CAPTURE_PATCH_SHA256
        || producer.build_manifest_file != "producer-build-v1.json"
        || producer.build_manifest_sha256 != PRODUCER_BUILD_SHA256
    {
        return Err("registered FS-UAE producer identity changed".to_owned());
    }
    verify_hashed_file(
        package_root,
        &producer.build_manifest_file,
        &producer.build_manifest_sha256,
    )?;

    let packager = &package.packager;
    if packager.pillow_version != "12.2.0"
        || packager.python_implementation != "cpython"
        || packager.python_version != "3.14.3"
        || packager.script_sha256 != PACKAGER_SCRIPT_SHA256
        || packager.zlib_build_version != "1.2.12"
        || packager.zlib_runtime_version != "1.2.12"
    {
        return Err("registered packager identity changed".to_owned());
    }
    verify_hashed_file(package_root, "package.py", &packager.script_sha256)?;

    let expected_capture_tools = BTreeMap::from([
        (
            "capture.sh".to_owned(),
            "397c5454b34d6344ce2cd64c5addb94dc6d25f32e9f116cc12e27a7f9c3ff9d3".to_owned(),
        ),
        (
            "capture_manifest.py".to_owned(),
            "61d52dc05e197b3bab95d4e5ce0ccba70841020a943bd83355950187859cc37f".to_owned(),
        ),
        (
            "config.uae.in".to_owned(),
            "49de327d26d7f2b632bc7a62c987dd625c18acee1cdd00af5558626e3d071678".to_owned(),
        ),
    ]);
    if package.capture_tools != expected_capture_tools {
        return Err("registered capture-tool identity changed".to_owned());
    }
    let capture_tool_root = repository_root().join("tools/fs-uae-hblank-write-timing-capture");
    for (file, hash) in &package.capture_tools {
        verify_hashed_file(&capture_tool_root, file, hash)?;
    }
    Ok(())
}

fn validate_run_paths_and_hashes(
    package_root: &Path,
    run: &PackageRun,
    expected_stem: &str,
) -> Result<(), String> {
    let expected = [
        (
            run.capture_file.as_str(),
            format!("captures/{expected_stem}.apng"),
            run.capture_sha256.as_str(),
        ),
        (
            run.capture_manifest_file.as_str(),
            format!("manifests/{expected_stem}.json"),
            run.capture_manifest_sha256.as_str(),
        ),
        (
            run.configuration_file.as_str(),
            format!("configs/{expected_stem}.uae"),
            run.configuration_sha256.as_str(),
        ),
        (
            run.record_file.as_str(),
            format!("records/{expected_stem}.json"),
            run.record_sha256.as_str(),
        ),
        (
            run.run_log_file.as_str(),
            format!("logs/{expected_stem}.log"),
            run.run_log_sha256.as_str(),
        ),
    ];
    for (actual_file, expected_file, hash) in expected {
        require_eq(actual_file, &expected_file, "registered run file")?;
        verify_hashed_file(package_root, actual_file, hash)?;
    }
    if !is_sha256(&run.decoded_pixel_sha256) || run.mutation_output_rows != [204, 205] {
        return Err(format!("{expected_stem} decoded-capture contract changed"));
    }
    Ok(())
}

fn validate_reference_record(
    record: &ReferenceRecord,
    run: &PackageRun,
    case: &Case,
    artifact: &Artifact,
    profile: Profile,
) -> Result<(), String> {
    require_eq(&record.schema_version, SCHEMA_VERSION, "record schema")?;
    require_eq(&record.suite_id, SUITE_ID, "record suite id")?;
    require_eq(&record.suite_version, SUITE_VERSION, "record suite version")?;
    require_eq(&record.case_id, &case.id, "record case id")?;

    if record.artifact.adf_file != artifact.adf_file
        || record.artifact.adf_sha256 != artifact.sha256.adf
        || record.artifact.payload_file != artifact.payload_file
        || record.artifact.payload_sha256 != artifact.sha256.payload
    {
        return Err(format!(
            "{} / {} artifact identity changed",
            profile.id, case.id
        ));
    }
    let expected_chipset = if profile.id == "ecs" { "ECS" } else { "AGA" };
    if record.machine.chipset != expected_chipset
        || record.machine.region != "PAL"
        || record.machine.firmware.sha256 != profile.firmware_sha256
    {
        return Err(format!(
            "{} / {} machine identity changed",
            profile.id, case.id
        ));
    }
    if record.producer.implementation_family != "UAE"
        || record.producer.kind != "software-emulator"
        || record.producer.product != "FS-UAE"
        || record.producer.version != "5.0.7"
        || record.producer.revision != "f362278ccd4c60991caac3b4d240d4a3f751bea2"
        || record.producer.source_url != "https://github.com/FrodeSolheim/fs-uae"
    {
        return Err(format!(
            "{} / {} producer identity changed",
            profile.id, case.id
        ));
    }

    let execution = &record.execution;
    if !execution.cold_boot
        || execution.adjacent_field_stability != "confirmed"
        || execution.settle_fields != case.settle_capture.settle_fields
        || execution.configuration_sha256 != run.configuration_sha256
        || execution.captured_fields.len() != 3
        || !execution
            .captured_fields
            .windows(2)
            .all(|pair| pair[1] == pair[0] + 1)
        || execution.ready_rule.byte_order != "big-endian"
        || execution.ready_rule.case_number != case.numeric_id
        || execution.ready_rule.field_counter_minimum != case.settle_capture.settle_fields
        || execution.ready_rule.magic != "HBLK"
        || execution.ready_rule.record_address != "0x0002ff00"
    {
        return Err(format!(
            "{} / {} execution contract changed",
            profile.id, case.id
        ));
    }

    let normalization = &record.normalization;
    if normalization.alignment_search
        || normalization.beam_coordinate.horizontal_origin_sample != -184
        || normalization
            .beam_coordinate
            .horizontal_samples_per_register_increment_numerator
            != 4
        || normalization
            .beam_coordinate
            .horizontal_samples_per_register_increment_denominator
            != 1
        || normalization.beam_coordinate.phase_numerator != 0
        || normalization.beam_coordinate.phase_denominator != 1
        || normalization.beam_coordinate.sample_beam_line != 128
        || normalization.beam_coordinate.sample_row != 204
        || normalization.crop.x != 0
        || normalization.crop.y != 0
        || normalization.crop.width != FS_RAW_WIDTH as u32
        || normalization.crop.height != 576
        || normalization.field_handling != "bob"
    {
        return Err(format!(
            "{} / {} normalization contract changed",
            profile.id, case.id
        ));
    }

    let source = &record.source_capture;
    if !source.blanking_retained
        || !source.overscan_retained
        || source.file_sha256 != run.capture_sha256
        || source.decoded_pixel_sha256 != run.decoded_pixel_sha256
        || source.filter != "none"
        || source.scaling != "none"
        || source.shader != "none"
        || source.width != FS_RAW_WIDTH as u32
        || source.height != 576
        || source.stride_bytes != (FS_RAW_WIDTH * 4) as u32
        || source.pixel_format
            != "RGBA8888, tightly packed, row-major; alpha retained from source BGRA8888"
    {
        return Err(format!(
            "{} / {} source capture changed",
            profile.id, case.id
        ));
    }

    let stimulus = &record.stimulus;
    if stimulus.baseline_word != baseline_register(case, &case.timed_write.register)?.word
        || stimulus.mutation_word != case.timed_write.word
        || stimulus.tested_register != case.timed_write.register
        || stimulus.reset_beam_line != case.timed_write.reset_beam_line
        || stimulus.reset_wait_hpos_cck != case.timed_write.reset_hpos_cck
        || stimulus.mutation_beam_line != case.timed_write.beam_line
        || stimulus.mutation_wait_hpos_cck != case.timed_write.wait_hpos_cck
        || stimulus
            .write_position_evidence
            .tested_write_sample
            .is_some()
        || stimulus.write_position_evidence.method
            != "Copper schedule with immediately preceding visible COLOR00 marker"
        || !(FS_STORAGE_EXCLUSION_END..FS_RAW_WIDTH)
            .contains(&stimulus.write_position_evidence.marker_start_sample)
    {
        return Err(format!("{} / {} stimulus changed", profile.id, case.id));
    }

    let observations = &record.observations;
    if observations.following_line_carry != "mutation confirmed"
        || observations.guard_color_word != case.identity.visual.color00
        || observations.marker_color_word != case.identity.visual.marker_color00
        || observations.interval_convention != "start-inclusive-stop-exclusive"
        || observations.storage_exclusion != [0, FS_STORAGE_EXCLUSION_END]
        || observations.uncertainty_samples != 0
        || observations.lines.len() != 3
    {
        return Err(format!(
            "{} / {} observation contract changed",
            profile.id, case.id
        ));
    }
    let expected_roles = [
        ("pre-mutation baseline", [202, 203]),
        ("mutation output", [204, 205]),
        ("post-mutation control", [206, 207]),
    ];
    for (line, (role, rows)) in observations.lines.iter().zip(expected_roles) {
        if line.role != role || line.raw_rows != rows {
            return Err(format!("{} / {} line roles changed", profile.id, case.id));
        }
        validate_semantic_partition(line)?;
    }
    Ok(())
}

fn validate_semantic_partition(line: &RecordLine) -> Result<(), String> {
    let mut ownership = vec![None; FS_RAW_WIDTH - FS_STORAGE_EXCLUSION_END];
    for (kind, runs) in [
        ("black", &line.black_runs),
        ("guard", &line.guard_runs),
        ("marker", &line.marker_runs),
    ] {
        let mut previous_end = FS_STORAGE_EXCLUSION_END;
        for &[start, end_exclusive] in runs {
            if start < FS_STORAGE_EXCLUSION_END
                || end_exclusive > FS_RAW_WIDTH
                || start >= end_exclusive
                || start < previous_end
            {
                return Err(format!(
                    "{} {kind} run [{start},{end_exclusive}) is invalid",
                    line.role
                ));
            }
            previous_end = end_exclusive;
            for sample in start..end_exclusive {
                let slot = &mut ownership[sample - FS_STORAGE_EXCLUSION_END];
                if let Some(existing) = slot {
                    return Err(format!(
                        "{} sample {sample} belongs to both {existing} and {kind}",
                        line.role
                    ));
                }
                *slot = Some(kind);
            }
        }
    }
    if let Some(index) = ownership.iter().position(Option::is_none) {
        return Err(format!(
            "{} leaves reference sample {} unclassified",
            line.role,
            index + FS_STORAGE_EXCLUSION_END
        ));
    }
    Ok(())
}

fn execute_case(
    session: &mut TestSession,
    case: &Case,
    expected_lines: &[SemanticLine],
    state: &mut ExecutionState,
) -> Result<(), String> {
    state.ready = Some(wait_for_ready_record(session, case)?);

    for capture_index in 0..case.settle_capture.capture_fields {
        let copper_start = session.machine().copper_move_log().len();
        session
            .run_frames(1)
            .map_err(|error| format!("run capture field {capture_index}: {error}"))?;
        let copper_end = session.machine().copper_move_log().len();
        let field_counter = session.machine().read_long(READY_FIELD_COUNTER);
        let frame = session
            .latest_frame()
            .ok_or_else(|| format!("capture field {capture_index} emitted no framebuffer"))?;
        if frame.format != PixelFormat::Rgba8888 {
            return Err(format!(
                "capture field {capture_index} format is {:?}, expected RGBA8888",
                frame.format
            ));
        }
        if frame.width != DISPLAY_WIDTH || frame.height != DISPLAY_HEIGHT {
            return Err(format!(
                "capture field {capture_index} is {}x{}, expected {}x{}",
                frame.width, frame.height, DISPLAY_WIDTH, DISPLAY_HEIGHT
            ));
        }
        let rgba = frame
            .rgba_pixels()
            .map_err(|error| format!("convert capture field {capture_index} to RGBA: {error}"))?;
        if let Some((pixel, alpha)) = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .find_map(|(index, pixel)| (pixel[3] != 0xFF).then_some((index, pixel[3])))
        {
            return Err(format!(
                "capture field {capture_index} pixel {pixel} has alpha {alpha:#04x}"
            ));
        }
        state.captured.push(CapturedField {
            field_counter,
            sha256: sha256_hex(&rgba),
            rgba,
        });

        let field_moves = validate_copper_field(
            &session.machine().copper_move_log()[copper_start..copper_end],
            case,
            capture_index,
        )?;
        state.copper_fields.push(field_moves);
    }

    if state.captured.len() != case.settle_capture.capture_fields as usize {
        return Err(format!(
            "captured {} fields, expected {}",
            state.captured.len(),
            case.settle_capture.capture_fields
        ));
    }
    if let Some(pair) = state
        .captured
        .windows(2)
        .find(|pair| pair[1].field_counter != pair[0].field_counter + 1)
    {
        return Err(format!(
            "guest field counters are not adjacent: {} then {}",
            pair[0].field_counter, pair[1].field_counter
        ));
    }
    let first = state
        .captured
        .first()
        .ok_or_else(|| "capture contract produced no fields".to_owned())?;
    if let Some(different) = state
        .captured
        .iter()
        .skip(1)
        .find(|field| field.rgba != first.rgba)
    {
        return Err(format!(
            "adjacent fields differ: counter {} hash {} versus counter {} hash {}",
            first.field_counter, first.sha256, different.field_counter, different.sha256
        ));
    }

    let guard = rgb4_word_to_rgb8(parse_hex_word(&case.identity.visual.color00)?)?;
    let marker = rgb4_word_to_rgb8(parse_hex_word(&case.identity.visual.marker_color00)?)?;
    let role_lines = [
        ("pre-mutation baseline", case.timed_write.reset_beam_line),
        ("mutation output", case.timed_write.beam_line),
        ("post-mutation control", case.timed_write.beam_line + 1),
    ];
    let actual_lines = role_lines
        .into_iter()
        .map(|(role, beam_line)| measure_semantic_line(&first.rgba, beam_line, role, guard, marker))
        .collect::<Result<Vec<_>, _>>()?;
    state.actual_lines = Some(actual_lines.clone());

    if actual_lines != expected_lines {
        let mut disagreements = Vec::new();
        for (actual, expected) in actual_lines.iter().zip(expected_lines) {
            if actual != expected {
                disagreements.push(format!(
                    "{} expected black={:?} guard={:?} marker={:?}; actual black={:?} guard={:?} marker={:?}",
                    expected.role,
                    expected.black_runs,
                    expected.guard_runs,
                    expected.marker_runs,
                    actual.black_runs,
                    actual.guard_runs,
                    actual.marker_runs,
                ));
            }
        }
        return Err(format!(
            "fixed-coordinate semantic disagreement: {}",
            disagreements.join(" | ")
        ));
    }
    Ok(())
}

fn wait_for_ready_record(session: &mut TestSession, case: &Case) -> Result<ReadyEvidence, String> {
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
            return Ok(ReadyEvidence {
                observed_after_fields: observed_field + 1,
                field_counter: counter,
                copper_log_start_index: session.machine().copper_move_log().len(),
            });
        }
    }
    Err(format!(
        "ready record did not reach counter {} within {} fields",
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
            "ready COLOR00 is {actual_color:#06x}, expected {expected_color:#06x}"
        ));
    }
    Ok(())
}

fn validate_copper_field(
    entries: &[(u64, u16, u16, u16, u16)],
    case: &Case,
    capture_index: u32,
) -> Result<Vec<CopperMoveEvidence>, String> {
    let tested_offset = timed_register_offset(&case.timed_write.register)?;
    let baseline = parse_hex_word(&baseline_register(case, &case.timed_write.register)?.word)?;
    let mutation = parse_hex_word(&case.timed_write.word)?;
    let guard = parse_hex_word(&case.identity.visual.color00)?;
    let relevant: Vec<CopperMoveEvidence> = entries
        .iter()
        .filter(|entry| {
            matches!(
                (entry.1 as u32, entry.3),
                (line, offset)
                    if (line == case.timed_write.reset_beam_line
                        || line == case.timed_write.beam_line)
                        && (offset == tested_offset || offset == COLOR00_OFFSET)
            )
        })
        .map(
            |&(cck, vpos, hpos, custom_offset, value)| CopperMoveEvidence {
                cck,
                vpos,
                hpos,
                custom_offset,
                value,
            },
        )
        .collect();

    let expected = [
        (
            case.timed_write.reset_beam_line as u16,
            tested_offset,
            baseline,
        ),
        (
            case.timed_write.reset_beam_line as u16,
            COLOR00_OFFSET,
            guard,
        ),
        (
            case.timed_write.beam_line as u16,
            COLOR00_OFFSET,
            MARKER_WORD,
        ),
        (case.timed_write.beam_line as u16, tested_offset, mutation),
    ];
    if relevant.len() != expected.len() {
        return Err(format!(
            "capture field {capture_index} has {} relevant Copper MOVEs, expected four: {relevant:?}",
            relevant.len()
        ));
    }
    for (index, (actual, (vpos, offset, value))) in relevant.iter().zip(expected).enumerate() {
        if (actual.vpos, actual.custom_offset, actual.value) != (vpos, offset, value) {
            return Err(format!(
                "capture field {capture_index} Copper MOVE {index} is {actual:?}; expected vpos={vpos} offset={offset:#05x} value={value:#06x}"
            ));
        }
    }
    if relevant[0].hpos < case.timed_write.reset_hpos_cck
        || relevant[1].hpos < case.timed_write.reset_hpos_cck
    {
        return Err(format!(
            "capture field {capture_index} reset MOVE precedes its programmed WAIT: {relevant:?}"
        ));
    }
    if relevant[2].hpos != MARKER_MOVE_HPOS_CCK
        || relevant[3].hpos != TESTED_MOVE_HPOS_CCK
        || relevant[3].hpos - relevant[2].hpos != 4
    {
        return Err(format!(
            "capture field {capture_index} mutation-line Copper timing changed: marker hpos={} tested hpos={}; expected {} then {} (four hpos units apart)",
            relevant[2].hpos, relevant[3].hpos, MARKER_MOVE_HPOS_CCK, TESTED_MOVE_HPOS_CCK,
        ));
    }

    let mutation_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            (entry.1 as u32 == case.timed_write.beam_line
                && entry.3 == tested_offset
                && entry.4 == mutation)
                .then_some(index)
        })
        .collect();
    if mutation_indices.len() != 1 {
        return Err(format!(
            "capture field {capture_index} has {} tested mutation MOVEs, expected one",
            mutation_indices.len()
        ));
    }
    let mutation_index = mutation_indices[0];
    let preceding = mutation_index
        .checked_sub(1)
        .and_then(|index| entries.get(index))
        .ok_or_else(|| format!("capture field {capture_index} mutation has no preceding MOVE"))?;
    if preceding.1 as u32 != case.timed_write.beam_line
        || preceding.3 != COLOR00_OFFSET
        || preceding.4 != MARKER_WORD
    {
        return Err(format!(
            "capture field {capture_index} tested mutation is not immediately preceded by the marker: {preceding:?}"
        ));
    }
    Ok(relevant)
}

fn measure_semantic_line(
    rgba: &[u8],
    beam_line: u32,
    role: &str,
    guard: [u8; 3],
    marker: [u8; 3],
) -> Result<SemanticLine, String> {
    let row = output_row_for_beam_line(beam_line)?;
    let row_bytes = DISPLAY_WIDTH as usize * 4;
    let first_start = row as usize * row_bytes;
    let second_start = first_start + row_bytes;
    let first = rgba
        .get(first_start..first_start + row_bytes)
        .ok_or_else(|| format!("{role} row {row} is outside the framebuffer"))?;
    let second = rgba
        .get(second_start..second_start + row_bytes)
        .ok_or_else(|| format!("{role} doubled row {} is outside the framebuffer", row + 1))?;
    if first != second {
        return Err(format!(
            "{role} line-doubled output rows {row} and {} differ",
            row + 1
        ));
    }

    let mut classes = Vec::with_capacity(EMU_COMPARISON_END - EMU_COMPARISON_START);
    for x in EMU_COMPARISON_START..EMU_COMPARISON_END {
        let pixel = &first[x * 4..x * 4 + 4];
        let rgb = [pixel[0], pixel[1], pixel[2]];
        let class = if rgb == [0, 0, 0] {
            PixelClass::Black
        } else if rgb == guard {
            PixelClass::Guard
        } else if rgb == marker {
            PixelClass::Marker
        } else {
            return Err(format!(
                "{role} output pixel {x} is {rgb:?}; expected black, guard {guard:?}, or marker {marker:?}"
            ));
        };
        classes.push(class);
    }
    Ok(SemanticLine {
        role: role.to_owned(),
        black_runs: runs_for_class(&classes, PixelClass::Black),
        guard_runs: runs_for_class(&classes, PixelClass::Guard),
        marker_runs: runs_for_class(&classes, PixelClass::Marker),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PixelClass {
    Black,
    Guard,
    Marker,
}

fn runs_for_class(classes: &[PixelClass], selected: PixelClass) -> Vec<SemanticRun> {
    let mut runs = Vec::new();
    let mut start = None;
    for (relative, class) in classes.iter().enumerate() {
        let x = EMU_COMPARISON_START + relative;
        if *class == selected {
            start.get_or_insert(x);
        } else if let Some(run_start) = start.take() {
            runs.push(SemanticRun {
                start: run_start,
                end_exclusive: x,
            });
        }
    }
    if let Some(run_start) = start {
        runs.push(SemanticRun {
            start: run_start,
            end_exclusive: EMU_COMPARISON_END,
        });
    }
    runs
}

fn mapped_reference_lines(record: &ReferenceRecord) -> Result<Vec<SemanticLine>, String> {
    record
        .observations
        .lines
        .iter()
        .map(|line| {
            validate_semantic_partition(line)?;
            Ok(SemanticLine {
                role: line.role.clone(),
                black_runs: map_reference_runs(&line.black_runs),
                guard_runs: map_reference_runs(&line.guard_runs),
                marker_runs: map_reference_runs(&line.marker_runs),
            })
        })
        .collect()
}

fn map_reference_runs(runs: &[[usize; 2]]) -> Vec<SemanticRun> {
    runs.iter()
        .map(|&[start, end_exclusive]| SemanticRun {
            start: start + FS_TO_EMU_OUTPUT_X,
            end_exclusive: end_exclusive + FS_TO_EMU_OUTPUT_X,
        })
        .collect()
}

fn build_case_report(
    context: CaseReportContext<'_>,
    state: &ExecutionState,
    machine: Option<MachineEvidence>,
    error: Option<String>,
) -> Result<CaseReport, String> {
    let CaseReportContext {
        revision,
        suite,
        case,
        artifact,
        profile,
        reference,
        expected_lines,
    } = context;
    Ok(CaseReport {
        schema_version: SCHEMA_VERSION,
        status: if error.is_some() { "failed" } else { "passed" },
        evidence_scope: "registered UAE-family software observation; not hardware conformance",
        revision: revision.clone(),
        suite_id: SUITE_ID,
        suite_version: SUITE_VERSION,
        built_suite_manifest_sha256: suite.manifest_sha256.clone(),
        registered_suite_manifest_sha256: REGISTERED_SUITE_MANIFEST_SHA256,
        registered_package_sha256: PACKAGE_SHA256,
        registered_record_file: reference.run.record_file.clone(),
        registered_record_sha256: reference.run.record_sha256.clone(),
        case_id: case.id.clone(),
        case_number: case.numeric_id,
        profile: profile.id.to_owned(),
        artifact_sha256: artifact.sha256.adf.clone(),
        firmware_sha256: profile.firmware_sha256.to_owned(),
        coordinate_mapping: coordinate_mapping(case)?,
        expected_lines,
        actual_lines: state.actual_lines.clone(),
        ready: state.ready.clone(),
        captured_fields: state
            .captured
            .iter()
            .map(|field| CapturedFieldEvidence {
                field_counter: field.field_counter,
                rgba_sha256: field.sha256.clone(),
            })
            .collect(),
        copper_fields: state.copper_fields.clone(),
        machine,
        error,
    })
}

fn coordinate_mapping(case: &Case) -> Result<CoordinateMapping, String> {
    let baseline_row = output_row_for_beam_line(case.timed_write.reset_beam_line)?;
    let mutation_row = output_row_for_beam_line(case.timed_write.beam_line)?;
    let following_row = output_row_for_beam_line(case.timed_write.beam_line + 1)?;
    Ok(CoordinateMapping {
        alignment_search: false,
        reference_domain_start: FS_STORAGE_EXCLUSION_END,
        reference_domain_end_exclusive: FS_RAW_WIDTH,
        emu_domain_start: EMU_COMPARISON_START,
        emu_domain_end_exclusive: EMU_COMPARISON_END,
        horizontal_formula: "emu_output_pixel = fs_uae_raw_sample + 8",
        baseline_beam_line: case.timed_write.reset_beam_line,
        baseline_output_rows: [baseline_row, baseline_row + 1],
        mutation_beam_line: case.timed_write.beam_line,
        mutation_output_rows: [mutation_row, mutation_row + 1],
        following_beam_line: case.timed_write.beam_line + 1,
        following_output_rows: [following_row, following_row + 1],
    })
}

fn capture_machine_evidence(session: &TestSession, case: &Case) -> MachineEvidence {
    let tested_offset = timed_register_offset(&case.timed_write.register).unwrap_or(u16::MAX);
    let relevant_offsets = [
        0x0100,
        0x0106,
        COLOR00_OFFSET,
        0x01C4,
        0x01C6,
        0x01DC,
        tested_offset,
    ];
    let mut custom_writes: Vec<CustomWriteEvidence> = session
        .machine()
        .custom_write_log()
        .iter()
        .rev()
        .filter(|entry| relevant_offsets.contains(&entry.3))
        .take(128)
        .map(
            |&(tick, pc, address, custom_offset, value, is_word)| CustomWriteEvidence {
                tick,
                pc,
                address,
                custom_offset,
                value,
                is_word,
            },
        )
        .collect();
    custom_writes.reverse();

    let mut copper_moves: Vec<CopperMoveEvidence> = session
        .machine()
        .copper_move_log()
        .iter()
        .rev()
        .filter(|entry| {
            (entry.1 as u32 == case.timed_write.reset_beam_line
                || entry.1 as u32 == case.timed_write.beam_line)
                && (entry.3 == tested_offset || entry.3 == COLOR00_OFFSET)
        })
        .take(32)
        .map(
            |&(cck, vpos, hpos, custom_offset, value)| CopperMoveEvidence {
                cck,
                vpos,
                hpos,
                custom_offset,
                value,
            },
        )
        .collect();
    copper_moves.reverse();

    MachineEvidence {
        cpu_pc: session.machine().cpu_pc(),
        machine_tick: session.machine().tick_count(),
        ready_magic: session.machine().read_long(READY_BASE),
        ready_case: session.machine().read_word(READY_CASE_NUMBER),
        ready_schema: session.machine().read_word(READY_SCHEMA_VERSION),
        ready_field_counter: session.machine().read_long(READY_FIELD_COUNTER),
        relevant_cpu_custom_writes: custom_writes,
        recent_relevant_copper_moves: copper_moves,
    }
}

fn write_failure_frames(case_dir: &Path, captured: &[CapturedField]) -> Result<(), String> {
    for (index, field) in captured.iter().enumerate() {
        let path = case_dir.join(format!("field-{index}.rgba"));
        fs::write(&path, &field.rgba)
            .map_err(|error| format!("write failure frame {}: {error}", path.display()))?;
    }
    Ok(())
}

fn relative_result_file(case: &Case, profile: Profile) -> String {
    format!("{}/{}/result.json", case.id, profile.id)
}

fn output_row_for_beam_line(beam_line: u32) -> Result<u32, String> {
    let relative = beam_line
        .checked_sub(PAL_VIEWPORT_V_START_LINE)
        .ok_or_else(|| format!("beam line {beam_line} precedes the PAL viewport"))?;
    let row = relative * OUTPUT_ROWS_PER_BEAM_LINE;
    if row + 1 >= DISPLAY_HEIGHT {
        return Err(format!(
            "beam line {beam_line} maps outside the framebuffer"
        ));
    }
    Ok(row)
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
                .map_err(|error| format!("ready identity is not ASCII/UTF-8: {error}"));
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

fn artifact_for_case<'a>(manifest: &'a Manifest, case_id: &str) -> Result<&'a Artifact, String> {
    manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.case_id == case_id)
        .ok_or_else(|| format!("suite is missing artifact for {case_id}"))
}

fn load_verified_artifact(dist: &Path, artifact: &Artifact) -> Result<Vec<u8>, String> {
    let adf_path = dist.join(&artifact.adf_file);
    let adf = read_file(&adf_path)?;
    if adf.len() != artifact.adf_bytes {
        return Err(format!(
            "{} has {} bytes, expected {}",
            adf_path.display(),
            adf.len(),
            artifact.adf_bytes
        ));
    }
    require_eq(&sha256_hex(&adf), &artifact.sha256.adf, "ADF hash")?;

    let payload_path = dist.join(&artifact.payload_file);
    let payload = read_file(&payload_path)?;
    if payload.len() != artifact.payload_bytes {
        return Err(format!(
            "{} has {} bytes, expected {}",
            payload_path.display(),
            payload.len(),
            artifact.payload_bytes
        ));
    }
    require_eq(
        &sha256_hex(&payload),
        &artifact.sha256.payload,
        "payload hash",
    )?;
    Ok(adf)
}

fn load_verified_firmware(profile: Profile) -> Result<Vec<u8>, String> {
    let path = required_file(profile.firmware_env)?;
    let firmware = read_file(&path)?;
    if firmware.len() != 512 * 1024 {
        return Err(format!(
            "{} has {} bytes, expected 524288",
            profile.firmware_label,
            firmware.len()
        ));
    }
    require_eq(
        &sha256_hex(&firmware),
        profile.firmware_sha256,
        profile.firmware_label,
    )?;
    Ok(firmware)
}

fn build_session(profile: Profile, firmware: &[u8], adf: &[u8]) -> Result<TestSession, String> {
    let runtime = AmigaRuntimeKind::new(profile.model, firmware.to_vec())
        .map_err(|error| format!("construct {} runtime: {error}", profile.id))?;
    let frame_ticks = runtime.native_frame_ticks();
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, frame_ticks, AmigaSessionQueryProvider);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, adf));
    session
        .load_media(&media)
        .map_err(|error| format!("insert corpus ADF into {} DF0: {error}", profile.id))?;
    session
        .machine_mut()
        .cpu_trace_arm(Some((0x0003_0000, 0x0003_00FF)), 2_048);
    Ok(session)
}

fn baseline_register<'a>(case: &'a Case, name: &str) -> Result<&'a Register, String> {
    match name {
        "BPLCON0" => Ok(&case.registers.bplcon0),
        "BPLCON3" => Ok(&case.registers.bplcon3),
        "BEAMCON0" => Ok(&case.registers.beamcon0),
        "HBSTRT" => Ok(&case.registers.hbstrt),
        "HBSTOP" => Ok(&case.registers.hbstop),
        _ => Err(format!("unsupported timed register {name:?}")),
    }
}

fn timed_register_offset(name: &str) -> Result<u16, String> {
    match name {
        "BPLCON0" => Ok(0x0100),
        "BPLCON3" => Ok(0x0106),
        "BEAMCON0" => Ok(0x01DC),
        "HBSTRT" => Ok(0x01C4),
        "HBSTOP" => Ok(0x01C6),
        _ => Err(format!("unsupported timed register {name:?}")),
    }
}

fn rgb4_word_to_rgb8(word: u16) -> Result<[u8; 3], String> {
    if word > 0x0FFF {
        return Err(format!("RGB4 word {word:#06x} exceeds 12 bits"));
    }
    Ok([
        (((word >> 8) & 0x0F) as u8) * 0x11,
        (((word >> 4) & 0x0F) as u8) * 0x11,
        ((word & 0x0F) as u8) * 0x11,
    ])
}

fn required_directory(variable: &str) -> Result<PathBuf, String> {
    let value = std::env::var_os(variable)
        .ok_or_else(|| format!("{variable} must name the built corpus directory"))?;
    let path = PathBuf::from(value);
    if !path.is_dir() {
        return Err(format!(
            "{variable} does not name a directory: {}",
            path.display()
        ));
    }
    Ok(path)
}

fn required_file(variable: &str) -> Result<PathBuf, String> {
    let value = std::env::var_os(variable)
        .ok_or_else(|| format!("{variable} must name the registered firmware"))?;
    let path = PathBuf::from(value);
    if !path.is_file() {
        return Err(format!(
            "{variable} does not name a file: {}",
            path.display()
        ));
    }
    Ok(path)
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("runtime crate should be two levels below the repository root")
        .to_path_buf()
}

fn diagnostics_root() -> PathBuf {
    let repo_root = repository_root();
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
    target.join("accuracy/amiga-programmable-hblank-write-timing")
}

fn revision_from_environment() -> Result<RevisionIdentity, String> {
    let full =
        std::env::var(GIT_REVISION_ENV).map_err(|_| format!("{GIT_REVISION_ENV} is not set"))?;
    if full.len() != 40
        || !full
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "{GIT_REVISION_ENV} must be a full lowercase 40-digit hexadecimal revision"
        ));
    }
    let dirty = match std::env::var(GIT_DIRTY_ENV).as_deref() {
        Ok("true") => true,
        Ok("false") => false,
        Ok(other) => {
            return Err(format!(
                "{GIT_DIRTY_ENV} is {other:?}, expected true or false"
            ));
        }
        Err(_) => return Err(format!("{GIT_DIRTY_ENV} is not set")),
    };
    Ok(RevisionIdentity { full, dirty })
}

fn verify_hashed_file(root: &Path, relative: &str, expected_sha256: &str) -> Result<(), String> {
    assert_safe_relative_path(relative)?;
    if !is_sha256(expected_sha256) {
        return Err(format!(
            "invalid SHA-256 for {relative}: {expected_sha256:?}"
        ));
    }
    let path = root.join(relative);
    let bytes = read_file(&path)?;
    require_eq(
        &sha256_hex(&bytes),
        expected_sha256,
        &format!("{} hash", path.display()),
    )
}

fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("encode {}: {error}", path.display()))?;
    fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

fn assert_safe_relative_file(file: &str, extension: &str) -> Result<(), String> {
    assert_safe_relative_path(file)?;
    if Path::new(file).extension().and_then(|value| value.to_str()) != Some(extension) {
        return Err(format!("corpus path {file:?} is not a .{extension} file"));
    }
    Ok(())
}

fn assert_safe_relative_path(file: &str) -> Result<(), String> {
    let path = Path::new(file);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("unsafe relative path: {file:?}"));
    }
    Ok(())
}

fn parse_hex_word(value: &str) -> Result<u16, String> {
    let digits = value
        .strip_prefix("0x")
        .ok_or_else(|| format!("{value:?} is not a hexadecimal word"))?;
    if digits.len() != 3 && digits.len() != 4 {
        return Err(format!("{value:?} is not a three- or four-digit word"));
    }
    if !digits
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{value:?} is not lowercase hexadecimal"));
    }
    u16::from_str_radix(digits, 16).map_err(|error| format!("parse {value:?}: {error}"))
}

fn require_eq(actual: &str, expected: &str, label: &str) -> Result<(), String> {
    if actual != expected {
        return Err(format!("{label} is {actual:?}, expected {expected:?}"));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_horizontal_mapping_preserves_aga_half_lores_marker_edges() {
        assert_eq!(FS_STORAGE_EXCLUSION_END + FS_TO_EMU_OUTPUT_X, 10);
        assert_eq!(371 + FS_TO_EMU_OUTPUT_X, 379);
        assert_eq!(FS_RAW_WIDTH + FS_TO_EMU_OUTPUT_X, 764);
    }

    #[test]
    fn fixed_beam_rows_are_not_discovered_by_alignment_search() {
        assert_eq!(output_row_for_beam_line(127), Ok(204));
        assert_eq!(output_row_for_beam_line(128), Ok(206));
        assert_eq!(output_row_for_beam_line(129), Ok(208));
    }

    #[test]
    fn revision_identity_requires_full_lowercase_hash() {
        let valid = "bdcc63b68948643dde55cd048cd0063a882bb595";
        assert!(
            valid.len() == 40
                && valid
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        assert!(
            !"BDCC63B68948643DDE55CD048CD0063A882BB595"
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
    }
}
