//! Explicit Amiga Test Kit v1.21 video conformance gate.
//!
//! The registered references were captured by an independent emulator and
//! are immutable test inputs. This gate has no update mode: a mismatch writes
//! diagnostics under `target/accuracy/` and fails.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::{BufReader, Cursor};
use std::path::{Component, Path, PathBuf};

use emu198x_shell::{
    FamilyRuntime, HeadlessSession, InputEvent, MediaImage, MediaKind, MediaSet, read_media_asset,
};
use runtime_commodore_amiga::{
    AmigaRuntimeKind, AmigaSessionQueryProvider, DISPLAY_HEIGHT, DISPLAY_WIDTH, Model,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const TEST_KIT_ENV: &str = "EMU198X_AMIGA_TEST_KIT_V121_ADF";
const TEST_KIT_BYTES: usize = 901_120;
const TEST_KIT_SHA256: &str = "abe7426c93619a7bb61ce10e3e66a4747fcaf22acd1d1876310033faa700ad28";

const A500_KICKSTART_ENV: &str = "EMU198X_AMIGA_KICKSTART_13_ROM";
const A500_KICKSTART_BYTES: usize = 262_144;
const A500_KICKSTART_SHA256: &str =
    "ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53";
const A1200_KICKSTART_ENV: &str = "EMU198X_AMIGA_KICKSTART_31_A1200_ROM";
const A1200_KICKSTART_BYTES: usize = 524_288;
const A1200_KICKSTART_SHA256: &str =
    "6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707";

const BOOT_FIELDS: u32 = 600;
const KEY_HOLD_FIELDS: u32 = 3;
const KEY_RELEASE_SETTLE_FIELDS: u32 = 1;
const INTER_KEY_FIELDS: u32 = 50;

const RUNTIME_WIDTH: u32 = 768;
const RUNTIME_HEIGHT: u32 = 576;
const VERTICAL_DECIMATION: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PixelEncoding {
    Rgb4,
    Rgb8,
}

impl PixelEncoding {
    const fn label(self) -> &'static str {
        match self {
            Self::Rgb4 => "RGB4",
            Self::Rgb8 => "RGB8",
        }
    }

    const fn diagnostic_red(self) -> u8 {
        match self {
            Self::Rgb4 => 0x0F,
            Self::Rgb8 => 0xFF,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct GateProfile {
    id: &'static str,
    model: Model,
    machine_label: &'static str,
    kickstart_env: &'static str,
    kickstart_label: &'static str,
    kickstart_bytes: usize,
    kickstart_sha256: &'static str,
    crop_x: u32,
    crop_y: u32,
    crop_width: u32,
    crop_height: u32,
    canonical_width: u32,
    canonical_height: u32,
    encoding: PixelEncoding,
    reference_channel_step: u8,
    reference_max_channel_error: u8,
    runtime_channel_step: u8,
    runtime_max_channel_error: u8,
}

const A500_PROFILE: GateProfile = GateProfile {
    id: "a500-a501-ocs-pal",
    model: Model::A500OcsPalA501,
    machine_label: "A500+A501 OCS PAL",
    kickstart_env: A500_KICKSTART_ENV,
    kickstart_label: "Kickstart 1.3 r34.005",
    kickstart_bytes: A500_KICKSTART_BYTES,
    kickstart_sha256: A500_KICKSTART_SHA256,
    crop_x: 20,
    crop_y: 2,
    crop_width: 716,
    crop_height: 570,
    canonical_width: 716,
    canonical_height: 285,
    encoding: PixelEncoding::Rgb4,
    reference_channel_step: 16,
    reference_max_channel_error: 1,
    runtime_channel_step: 17,
    runtime_max_channel_error: 0,
};

const A1200_PROFILE: GateProfile = GateProfile {
    id: "a1200-aga-pal",
    model: Model::A1200AgaPal,
    machine_label: "A1200 AGA PAL",
    kickstart_env: A1200_KICKSTART_ENV,
    kickstart_label: "Kickstart 3.1 r40.068",
    kickstart_bytes: A1200_KICKSTART_BYTES,
    kickstart_sha256: A1200_KICKSTART_SHA256,
    crop_x: 10,
    crop_y: 2,
    crop_width: 752,
    crop_height: 572,
    canonical_width: 752,
    canonical_height: 286,
    encoding: PixelEncoding::Rgb8,
    reference_channel_step: 1,
    reference_max_channel_error: 0,
    runtime_channel_step: 1,
    runtime_max_channel_error: 0,
};

type TestSession = HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Behaviour {
    Static,
    Alternating,
}

struct Case {
    id: &'static str,
    navigation: &'static [&'static str],
    settle_fields: u32,
    behaviour: Behaviour,
}

const CASES: &[Case] = &[
    Case {
        id: "gradients",
        navigation: &["F6", "F1"],
        settle_fields: 150,
        behaviour: Behaviour::Static,
    },
    Case {
        id: "static-checkerboard",
        navigation: &["F6", "F2"],
        settle_fields: 100,
        behaviour: Behaviour::Static,
    },
    Case {
        id: "alternating-checkerboard",
        navigation: &["F6", "F3"],
        settle_fields: 100,
        behaviour: Behaviour::Alternating,
    },
    Case {
        id: "ebu-bars",
        navigation: &["F6", "F4", "F6"],
        settle_fields: 100,
        behaviour: Behaviour::Static,
    },
    Case {
        id: "dots",
        navigation: &["F6", "F5"],
        settle_fields: 100,
        behaviour: Behaviour::Static,
    },
    Case {
        id: "crosshatch",
        navigation: &["F6", "F6"],
        settle_fields: 100,
        behaviour: Behaviour::Static,
    },
];

struct Fixtures {
    kickstart: Vec<u8>,
    test_kit_adf: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct A500Manifest {
    schema_version: u32,
    evidence_level: String,
    suite: SuiteManifest,
    machine: A500MachineManifest,
    viewport: A500ViewportManifest,
    comparison: A500ComparisonManifest,
    producer: A500ProducerManifest,
    producer_viewport: ProducerViewportManifest,
    producer_timing: ProducerTimingManifest,
    execution: ExecutionManifest,
    frames: Vec<FrameManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SuiteManifest {
    name: String,
    version: String,
    source_tag: String,
    source_commit: String,
    adf_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct A500MachineManifest {
    model: String,
    cpu: String,
    chipset: String,
    region: String,
    chip_ram_bytes: u32,
    slow_ram_bytes: u32,
    kickstart_revision: String,
    kickstart_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct A500ViewportManifest {
    runtime_width: u32,
    runtime_height: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    vertical_decimation: u32,
    canonical_width: u32,
    canonical_height: u32,
    pixel_format: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct A500ComparisonManifest {
    format: String,
    reference_channel_step: u8,
    runtime_channel_step: u8,
    rounding: String,
    reference_max_error: u8,
    runtime_max_error: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct A500ProducerManifest {
    id: String,
    emulator: String,
    version: String,
    revision: String,
    implementation_family: String,
    configuration: String,
    capture_method: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProducerViewportManifest {
    texture_x_start: u32,
    texture_x_end_exclusive: u32,
    texture_y_start: u32,
    texture_y_end_exclusive: u32,
    beam_hpos_start: u32,
    beam_hpos_end_exclusive: u32,
    beam_vpos_start: u32,
    beam_vpos_end_exclusive: u32,
    width: u32,
    height: u32,
    pixel_format: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProducerTimingManifest {
    unit: String,
    boot_wait: u32,
    keyboard_auto_release_milliseconds: u32,
    inter_key_wait: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionManifest {
    boot_fields: u32,
    key_hold_fields: u32,
    key_release_settle_fields: u32,
    inter_key_fields: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameManifest {
    id: String,
    navigation: Vec<String>,
    execution_settle_fields: u32,
    behaviour: String,
    capture_provenance: Option<CaptureProvenanceManifest>,
    references: Vec<ReferenceImageManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceImageManifest {
    phase: String,
    file: String,
    png_sha256: String,
    rgb_sha256: String,
    producer_final_wait_seconds: Option<u32>,
    source_core_field: Option<u32>,
    source_raw_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct A1200Manifest {
    schema_version: u32,
    evidence_level: String,
    suite: SuiteManifest,
    machine: A1200MachineManifest,
    viewport: A1200ViewportManifest,
    comparison: A1200ComparisonManifest,
    producer: A1200ProducerManifest,
    capture_adapter: CaptureAdapterManifest,
    packaging: PackagingManifest,
    execution: ExecutionManifest,
    frames: Vec<FrameManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct A1200MachineManifest {
    model: String,
    cpu: String,
    chipset: String,
    region: String,
    chip_ram_bytes: u32,
    expansion_ram_bytes: u32,
    kickstart_revision: String,
    kickstart_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct A1200ViewportManifest {
    producer_raw_width: u32,
    producer_raw_height: u32,
    producer_x: u32,
    producer_y: u32,
    runtime_width: u32,
    runtime_height: u32,
    runtime_x: u32,
    runtime_y: u32,
    width: u32,
    height: u32,
    vertical_decimation: u32,
    canonical_width: u32,
    canonical_height: u32,
    pixel_format: String,
    alignment_search: bool,
    horizontal_mapping: HorizontalMappingManifest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HorizontalMappingManifest {
    formula: String,
    basis: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct A1200ComparisonManifest {
    format: String,
    channel_tolerance: u8,
    reference_alpha: String,
    runtime_alpha: String,
    row_pair_policy: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct A1200ProducerManifest {
    id: String,
    emulator: String,
    version: String,
    revision: String,
    uae_base_version: String,
    implementation_family: String,
    configuration: String,
    capture_method: String,
    capture_patch_sha256: String,
    binary_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureAdapterManifest {
    #[serde(rename = "capture.sh")]
    capture_sh: String,
    #[serde(rename = "capture_manifest.py")]
    capture_manifest_py: String,
    #[serde(rename = "config.uae.in")]
    config_uae_in: String,
    #[serde(rename = "Portable.ini")]
    portable_ini: String,
    #[serde(rename = "fs-uae-5.0.7-test-kit-video-capture.patch")]
    capture_patch: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackagingManifest {
    tool: String,
    tool_sha256: String,
    python_version: String,
    zlib_version: String,
    png_encoding: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureProvenanceManifest {
    capture_manifest_sha256: String,
    configuration_sha256: String,
    run_log_sha256: String,
    raw_sha256: Vec<String>,
    inputs_before_sha256: String,
    inputs_after_sha256: String,
    captured_core_fields: Vec<u32>,
    frontend_wait_status: i32,
    captured_at_utc: String,
    host: String,
    operator: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PixelMismatch {
    differing_pixels: usize,
    first_x: u32,
    first_y: u32,
    first_expected: [u8; 3],
    first_actual: [u8; 3],
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssertionsContract {
    schema_version: u32,
    profile_id: String,
    producer_manifest_sha256: String,
    canonical_width: u32,
    canonical_height: u32,
    pixel_encoding: String,
    normalized_actual_channel_encoding: String,
    diff_mask_encoding: String,
    assertions: Vec<FrameAssertionContract>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameAssertionContract {
    case: String,
    phase: String,
    expectation: AssertionExpectation,
    disagreement_id: Option<String>,
    normalized_actual_channel_bytes_sha256: String,
    diff_mask_sha256: String,
    differing_pixels: usize,
    first_difference: Option<AssertionFirstDifference>,
    bounding_box: Option<AssertionBoundingBox>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum AssertionExpectation {
    Exact,
    RegisteredDisagreement,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AssertionFirstDifference {
    x: u32,
    y: u32,
    expected_rgb: [u8; 3],
    actual_rgb: [u8; 3],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AssertionBoundingBox {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ObservedFrameSignature {
    normalized_actual_channel_bytes_sha256: String,
    diff_mask_sha256: String,
    differing_pixels: usize,
    first_difference: Option<AssertionFirstDifference>,
    bounding_box: Option<AssertionBoundingBox>,
}

#[derive(Clone, Copy)]
struct DiagnosticContext<'a> {
    profile: &'a GateProfile,
    case_id: &'a str,
    frame_manifest: &'a FrameManifest,
    producer_id: &'a str,
}

#[derive(Clone, Copy)]
struct CaseRunContext<'a> {
    profile: &'a GateProfile,
    case: &'a Case,
    frame_manifest: &'a FrameManifest,
    producer_id: &'a str,
    assertions: &'a AssertionsContract,
}

#[test]
fn amiga_test_kit_v121_reference_manifest_is_self_consistent() {
    let a500_dir = reference_dir(&A500_PROFILE);
    let a500_manifest = load_a500_manifest(&a500_dir);
    validate_a500_manifest(&a500_manifest);
    for frame in &a500_manifest.frames {
        for reference in &frame.references {
            let _ = load_reference(&A500_PROFILE, &a500_dir, reference);
        }
    }

    let a1200_dir = reference_dir(&A1200_PROFILE);
    let a1200_manifest = load_a1200_manifest(&a1200_dir);
    validate_a1200_manifest(&a1200_manifest);
    for frame in &a1200_manifest.frames {
        for reference in &frame.references {
            let _ = load_reference(&A1200_PROFILE, &a1200_dir, reference);
        }
    }
}

#[test]
fn amiga_test_kit_v121_assertion_contracts_are_self_consistent() {
    let a500_dir = reference_dir(&A500_PROFILE);
    let a500_manifest = load_a500_manifest(&a500_dir);
    let a500_assertions = load_assertions_contract(&a500_dir);
    validate_assertions_contract(
        &A500_PROFILE,
        &a500_dir,
        &a500_manifest.frames,
        &a500_assertions,
    );

    let a1200_dir = reference_dir(&A1200_PROFILE);
    let a1200_manifest = load_a1200_manifest(&a1200_dir);
    let a1200_assertions = load_assertions_contract(&a1200_dir);
    validate_assertions_contract(
        &A1200_PROFILE,
        &a1200_dir,
        &a1200_manifest.frames,
        &a1200_assertions,
    );
}

#[test]
fn assertion_signature_uses_one_mask_byte_per_pixel() {
    let profile = GateProfile {
        canonical_width: 2,
        canonical_height: 1,
        ..A1200_PROFILE
    };
    let expected = [0x00, 0x00, 0x00, 0x11, 0x22, 0x33];
    let actual = [0x00, 0x00, 0x00, 0x11, 0x22, 0x44];
    let mismatch = pixel_mismatch(&profile, &actual, &expected).expect("second pixel differs");

    assert_eq!(diff_mask(&actual, &expected), [0, 1]);
    assert_eq!(
        observed_frame_signature(&profile, &actual, &expected, Some(&mismatch)),
        ObservedFrameSignature {
            normalized_actual_channel_bytes_sha256: sha256_hex(&actual),
            diff_mask_sha256: sha256_hex(&[0, 1]),
            differing_pixels: 1,
            first_difference: Some(AssertionFirstDifference {
                x: 1,
                y: 0,
                expected_rgb: [0x11, 0x22, 0x33],
                actual_rgb: [0x11, 0x22, 0x44],
            }),
            bounding_box: Some(AssertionBoundingBox {
                min_x: 1,
                min_y: 0,
                max_x: 1,
                max_y: 0,
            }),
        }
    );
}

#[test]
fn registered_disagreement_rejects_agreement_and_signature_drift() {
    let assertion = FrameAssertionContract {
        case: "case".to_owned(),
        phase: "static".to_owned(),
        expectation: AssertionExpectation::RegisteredDisagreement,
        disagreement_id: Some("registered-id".to_owned()),
        normalized_actual_channel_bytes_sha256: "11".repeat(32),
        diff_mask_sha256: "22".repeat(32),
        differing_pixels: 1,
        first_difference: Some(AssertionFirstDifference {
            x: 0,
            y: 0,
            expected_rgb: [0, 0, 0],
            actual_rgb: [1, 1, 1],
        }),
        bounding_box: Some(AssertionBoundingBox {
            min_x: 0,
            min_y: 0,
            max_x: 0,
            max_y: 0,
        }),
    };
    let registered = ObservedFrameSignature {
        normalized_actual_channel_bytes_sha256: assertion
            .normalized_actual_channel_bytes_sha256
            .clone(),
        diff_mask_sha256: assertion.diff_mask_sha256.clone(),
        differing_pixels: assertion.differing_pixels,
        first_difference: assertion.first_difference.clone(),
        bounding_box: assertion.bounding_box.clone(),
    };
    assert_eq!(
        validate_observed_frame_signature(&assertion, &registered),
        Ok(Some("registered-id".to_owned()))
    );

    let unexpected_agreement = ObservedFrameSignature {
        differing_pixels: 0,
        first_difference: None,
        bounding_box: None,
        ..registered.clone()
    };
    assert!(
        validate_observed_frame_signature(&assertion, &unexpected_agreement)
            .expect_err("registered disagreement must reject exact agreement")
            .contains("unexpected agreement")
    );

    let changed = ObservedFrameSignature {
        normalized_actual_channel_bytes_sha256: "33".repeat(32),
        ..registered
    };
    assert!(
        validate_observed_frame_signature(&assertion, &changed)
            .expect_err("changed registered signature must fail")
            .contains("assertion signature changed")
    );
}

#[test]
#[ignore = "FIXTURE: explicit Amiga Test Kit v1.21 reference-pattern gate"]
fn amiga_test_kit_v121_a500_a501_ocs_pal_matches_reference() {
    let reference_dir = reference_dir(&A500_PROFILE);
    let manifest = load_a500_manifest(&reference_dir);
    validate_a500_manifest(&manifest);
    let assertions = load_assertions_contract(&reference_dir);
    validate_assertions_contract(&A500_PROFILE, &reference_dir, &manifest.frames, &assertions);
    run_profile_gate(
        &A500_PROFILE,
        &reference_dir,
        &manifest.producer.id,
        &manifest.frames,
        &assertions,
    );
}

#[test]
#[ignore = "FIXTURE: explicit Amiga Test Kit v1.21 A1200 AGA reference-pattern gate"]
fn amiga_test_kit_v121_a1200_aga_pal_matches_reference() {
    let reference_dir = reference_dir(&A1200_PROFILE);
    let manifest = load_a1200_manifest(&reference_dir);
    validate_a1200_manifest(&manifest);
    let assertions = load_assertions_contract(&reference_dir);
    validate_assertions_contract(
        &A1200_PROFILE,
        &reference_dir,
        &manifest.frames,
        &assertions,
    );
    run_profile_gate(
        &A1200_PROFILE,
        &reference_dir,
        &manifest.producer.id,
        &manifest.frames,
        &assertions,
    );
}

fn run_profile_gate(
    profile: &GateProfile,
    reference_dir: &Path,
    producer_id: &str,
    frames: &[FrameManifest],
    assertions: &AssertionsContract,
) {
    prepare_diagnostics_dir(profile);

    // Validate every registered oracle before spending time booting the guest.
    for frame in frames {
        for reference in &frame.references {
            let _ = load_reference(profile, reference_dir, reference);
        }
    }

    let fixtures = load_fixtures(profile);
    let mut boot = build_session(profile, &fixtures);
    boot.run_frames(BOOT_FIELDS)
        .expect("Test Kit v1.21 should boot to its main menu");
    let menu_checkpoint = boot
        .snapshot_bytes()
        .expect("encode settled Test Kit v1.21 main-menu checkpoint");

    let mut failures = Vec::new();
    for case in CASES {
        let frame_manifest = manifest_frame(frames, case.id);
        let expected: Vec<_> = frame_manifest
            .references
            .iter()
            .map(|reference| load_reference(profile, reference_dir, reference))
            .collect();
        let mut session = build_session(profile, &fixtures);
        session
            .restore_snapshot(&menu_checkpoint)
            .unwrap_or_else(|error| panic!("restore Test Kit menu for {}: {error}", case.id));

        match run_case(
            profile,
            &mut session,
            case,
            frame_manifest,
            producer_id,
            &expected,
            assertions,
        ) {
            Ok(disagreements) if disagreements.is_empty() => eprintln!(
                "Amiga Test Kit v1.21 {} video: {} matched exactly",
                profile.machine_label, case.id
            ),
            Ok(disagreements) => eprintln!(
                "Amiga Test Kit v1.21 {} video: {} matched registered disagreement signature(s): {}",
                profile.machine_label,
                case.id,
                disagreements.into_iter().collect::<Vec<_>>().join(", ")
            ),
            Err(error) => failures.push(format!("{}: {error}", case.id)),
        }
    }

    assert!(
        failures.is_empty(),
        "Amiga Test Kit v1.21 {} video conformance failed:\n{}",
        profile.machine_label,
        failures.join("\n")
    );
}

fn run_case(
    profile: &GateProfile,
    session: &mut TestSession,
    case: &Case,
    frame_manifest: &FrameManifest,
    producer_id: &str,
    expected: &[Vec<u8>],
    assertions: &AssertionsContract,
) -> Result<BTreeSet<String>, String> {
    for (index, key) in case.navigation.iter().enumerate() {
        press_registered_key(session, key)?;
        if index + 1 < case.navigation.len() {
            session
                .run_frames(INTER_KEY_FIELDS)
                .map_err(|error| format!("settle after {key}: {error}"))?;
        }
    }
    session
        .run_frames(case.settle_fields)
        .map_err(|error| format!("settle reference pattern: {error}"))?;

    let context = CaseRunContext {
        profile,
        case,
        frame_manifest,
        producer_id,
        assertions,
    };
    match case.behaviour {
        Behaviour::Static => run_static_case(
            context,
            session,
            &expected[0],
            &frame_manifest.references[0],
        ),
        Behaviour::Alternating => run_alternating_case(context, session, expected),
    }
}

fn press_registered_key(session: &mut TestSession, name: &str) -> Result<(), String> {
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: true,
    });
    session
        .run_frames(KEY_HOLD_FIELDS)
        .map_err(|error| format!("hold {name}: {error}"))?;
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: false,
    });
    session
        .run_frames(KEY_RELEASE_SETTLE_FIELDS)
        .map_err(|error| format!("settle after releasing {name}: {error}"))?;
    Ok(())
}

fn run_static_case(
    context: CaseRunContext<'_>,
    session: &mut TestSession,
    expected: &[u8],
    reference: &ReferenceImageManifest,
) -> Result<BTreeSet<String>, String> {
    let CaseRunContext {
        profile,
        case,
        frame_manifest,
        producer_id,
        assertions,
    } = context;
    let first = normalized_frame(profile, session)?;
    session
        .run_frames(1)
        .map_err(|error| format!("capture adjacent stability field: {error}"))?;
    let second = normalized_frame(profile, session)?;
    if first != second {
        write_temporal_diagnostics(
            profile,
            case.id,
            frame_manifest,
            producer_id,
            &first,
            &second,
            "static-frame-changed",
        );
        return Err(format!(
            "static pattern changed across adjacent fields; diagnostics: {}",
            diagnostics_dir(profile).display()
        ));
    }
    let assertion = frame_assertion(assertions, case.id, &reference.phase);
    let disagreement = assert_frame_contract(
        DiagnosticContext {
            profile,
            case_id: case.id,
            frame_manifest,
            producer_id,
        },
        &second,
        expected,
        reference,
        assertion,
    )?;
    Ok(disagreement.into_iter().collect())
}

fn run_alternating_case(
    context: CaseRunContext<'_>,
    session: &mut TestSession,
    expected: &[Vec<u8>],
) -> Result<BTreeSet<String>, String> {
    let CaseRunContext {
        profile,
        case,
        frame_manifest,
        producer_id,
        assertions,
    } = context;
    let phase_a = normalized_frame(profile, session)?;
    session
        .run_frames(1)
        .map_err(|error| format!("capture alternating phase B: {error}"))?;
    let phase_b = normalized_frame(profile, session)?;
    session
        .run_frames(1)
        .map_err(|error| format!("capture alternating phase A2: {error}"))?;
    let phase_a2 = normalized_frame(profile, session)?;
    session
        .run_frames(1)
        .map_err(|error| format!("capture alternating phase B2: {error}"))?;
    let phase_b2 = normalized_frame(profile, session)?;

    if phase_a == phase_b || phase_a != phase_a2 || phase_b != phase_b2 {
        write_alternating_diagnostics(
            DiagnosticContext {
                profile,
                case_id: case.id,
                frame_manifest,
                producer_id,
            },
            &phase_a,
            &phase_b,
            &phase_a2,
            &phase_b2,
        );
        return Err(format!(
            "pattern did not satisfy A != B, A == A2 and B == B2; diagnostics: {}",
            diagnostics_dir(profile).display()
        ));
    }

    assert_eq!(
        expected.len(),
        2,
        "alternating case must have two registered phases"
    );
    let direct_score = differing_pixel_count(&phase_a, &expected[0])
        + differing_pixel_count(&phase_b, &expected[1]);
    let reversed_score = differing_pixel_count(&phase_a, &expected[1])
        + differing_pixel_count(&phase_b, &expected[0]);
    let (expected_a_index, expected_b_index) = if direct_score <= reversed_score {
        (0, 1)
    } else {
        (1, 0)
    };
    let comparisons = [
        (&phase_a, expected_a_index, "runtime phase A"),
        (&phase_b, expected_b_index, "runtime phase B"),
    ];
    let mut disagreements = BTreeSet::new();
    let mut failures = Vec::new();
    for (actual, expected_index, runtime_phase) in comparisons {
        let reference = &frame_manifest.references[expected_index];
        let artifact_id = assertion_artifact_id(case.id, &reference.phase);
        let assertion = frame_assertion(assertions, case.id, &reference.phase);
        match assert_frame_contract(
            DiagnosticContext {
                profile,
                case_id: &artifact_id,
                frame_manifest,
                producer_id,
            },
            actual,
            &expected[expected_index],
            reference,
            assertion,
        ) {
            Ok(Some(disagreement_id)) => {
                disagreements.insert(disagreement_id);
            }
            Ok(None) => {}
            Err(error) => failures.push(format!(
                "{runtime_phase} mapped to reference phase {}: {error}",
                reference.phase
            )),
        }
    }
    if failures.is_empty() {
        Ok(disagreements)
    } else {
        Err(failures.join("; "))
    }
}

fn assert_frame_contract(
    context: DiagnosticContext<'_>,
    actual: &[u8],
    expected: &[u8],
    reference: &ReferenceImageManifest,
    assertion: &FrameAssertionContract,
) -> Result<Option<String>, String> {
    let DiagnosticContext {
        profile,
        case_id,
        frame_manifest,
        producer_id,
    } = context;
    let mismatch = pixel_mismatch(profile, actual, expected);
    if let Some(mismatch) = &mismatch {
        write_mismatch_diagnostics(
            DiagnosticContext {
                profile,
                case_id,
                frame_manifest,
                producer_id,
            },
            reference,
            actual,
            expected,
            mismatch,
        );
    }

    let observed = observed_frame_signature(profile, actual, expected, mismatch.as_ref());
    validate_observed_frame_signature(assertion, &observed).map_err(|error| {
        if assertion.expectation == AssertionExpectation::Exact
            && let Some(mismatch) = &mismatch
        {
            format!("{error}; {}", mismatch_message(profile, mismatch))
        } else {
            error
        }
    })
}

fn validate_observed_frame_signature(
    assertion: &FrameAssertionContract,
    observed: &ObservedFrameSignature,
) -> Result<Option<String>, String> {
    match (assertion.expectation, observed.differing_pixels) {
        (AssertionExpectation::Exact, 1..) => {
            return Err("expected an exact match but pixels differ".to_owned());
        }
        (AssertionExpectation::RegisteredDisagreement, 0) => {
            return Err(format!(
                "unexpected agreement for registered disagreement {}",
                assertion
                    .disagreement_id
                    .as_deref()
                    .expect("validated disagreement must have an ID")
            ));
        }
        _ => {}
    }

    let mut changes = Vec::new();
    if observed.normalized_actual_channel_bytes_sha256
        != assertion.normalized_actual_channel_bytes_sha256
    {
        changes.push(format!(
            "normalized actual SHA-256 {} != {}",
            observed.normalized_actual_channel_bytes_sha256,
            assertion.normalized_actual_channel_bytes_sha256
        ));
    }
    if observed.diff_mask_sha256 != assertion.diff_mask_sha256 {
        changes.push(format!(
            "diff-mask SHA-256 {} != {}",
            observed.diff_mask_sha256, assertion.diff_mask_sha256
        ));
    }
    if observed.differing_pixels != assertion.differing_pixels {
        changes.push(format!(
            "differing-pixel count {} != {}",
            observed.differing_pixels, assertion.differing_pixels
        ));
    }
    if observed.first_difference != assertion.first_difference {
        changes.push(format!(
            "first difference {:?} != {:?}",
            observed.first_difference, assertion.first_difference
        ));
    }
    if observed.bounding_box != assertion.bounding_box {
        changes.push(format!(
            "bounding box {:?} != {:?}",
            observed.bounding_box, assertion.bounding_box
        ));
    }
    if !changes.is_empty() {
        return Err(format!(
            "assertion signature changed: {}",
            changes.join("; ")
        ));
    }

    Ok(assertion.disagreement_id.clone())
}

fn frame_assertion<'a>(
    assertions: &'a AssertionsContract,
    case_id: &str,
    phase: &str,
) -> &'a FrameAssertionContract {
    assertions
        .assertions
        .iter()
        .find(|assertion| assertion.case == case_id && assertion.phase == phase)
        .unwrap_or_else(|| panic!("assertions contract lacks {case_id} phase {phase}"))
}

fn assertion_artifact_id(case_id: &str, phase: &str) -> String {
    if phase == "static" {
        case_id.to_owned()
    } else {
        format!("{case_id}-phase-{phase}")
    }
}

fn observed_frame_signature(
    profile: &GateProfile,
    actual: &[u8],
    expected: &[u8],
    mismatch: Option<&PixelMismatch>,
) -> ObservedFrameSignature {
    let diff_mask = diff_mask(actual, expected);
    let (differing_pixels, first_difference, bounding_box) = mismatch.map_or_else(
        || (0, None, None),
        |mismatch| {
            (
                mismatch.differing_pixels,
                Some(AssertionFirstDifference {
                    x: mismatch.first_x,
                    y: mismatch.first_y,
                    expected_rgb: mismatch.first_expected,
                    actual_rgb: mismatch.first_actual,
                }),
                Some(AssertionBoundingBox {
                    min_x: mismatch.min_x,
                    min_y: mismatch.min_y,
                    max_x: mismatch.max_x,
                    max_y: mismatch.max_y,
                }),
            )
        },
    );
    assert_eq!(
        diff_mask.len(),
        (profile.canonical_width * profile.canonical_height) as usize
    );
    ObservedFrameSignature {
        normalized_actual_channel_bytes_sha256: sha256_hex(actual),
        diff_mask_sha256: sha256_hex(&diff_mask),
        differing_pixels,
        first_difference,
        bounding_box,
    }
}

fn diff_mask(actual: &[u8], expected: &[u8]) -> Vec<u8> {
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual.len() % 3, 0);
    actual
        .chunks_exact(3)
        .zip(expected.chunks_exact(3))
        .map(|(actual_pixel, expected_pixel)| u8::from(actual_pixel != expected_pixel))
        .collect()
}

fn differing_pixel_count(actual: &[u8], expected: &[u8]) -> usize {
    actual
        .chunks_exact(3)
        .zip(expected.chunks_exact(3))
        .filter(|(actual_pixel, expected_pixel)| actual_pixel != expected_pixel)
        .count()
}

fn mismatch_message(profile: &GateProfile, mismatch: &PixelMismatch) -> String {
    let total = u64::from(profile.canonical_width) * u64::from(profile.canonical_height);
    let percentage = mismatch.differing_pixels as f64 * 100.0 / total as f64;
    format!(
        "{} pixels differ ({percentage:.6}%); first at ({}, {}), expected {} {}, actual {} {}; bounding box ({}, {})..({}, {}); diagnostics: {}",
        mismatch.differing_pixels,
        mismatch.first_x,
        mismatch.first_y,
        profile.encoding.label(),
        format_pixel(profile.encoding, mismatch.first_expected),
        profile.encoding.label(),
        format_pixel(profile.encoding, mismatch.first_actual),
        mismatch.min_x,
        mismatch.min_y,
        mismatch.max_x,
        mismatch.max_y,
        diagnostics_dir(profile).display()
    )
}

fn format_pixel(encoding: PixelEncoding, pixel: [u8; 3]) -> String {
    match encoding {
        PixelEncoding::Rgb4 => format!("${:X}{:X}{:X}", pixel[0], pixel[1], pixel[2]),
        PixelEncoding::Rgb8 => format!("#{:02X}{:02X}{:02X}", pixel[0], pixel[1], pixel[2]),
    }
}

fn pixel_mismatch(profile: &GateProfile, actual: &[u8], expected: &[u8]) -> Option<PixelMismatch> {
    assert_eq!(
        actual.len(),
        (profile.canonical_width * profile.canonical_height * 3) as usize
    );
    assert_eq!(actual.len(), expected.len());

    let mut differing_pixels = 0;
    let mut first = None;
    let mut min_x = profile.canonical_width;
    let mut min_y = profile.canonical_height;
    let mut max_x = 0;
    let mut max_y = 0;

    for (index, (actual_pixel, expected_pixel)) in actual
        .chunks_exact(3)
        .zip(expected.chunks_exact(3))
        .enumerate()
    {
        if actual_pixel == expected_pixel {
            continue;
        }
        let x = index as u32 % profile.canonical_width;
        let y = index as u32 / profile.canonical_width;
        differing_pixels += 1;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        first.get_or_insert((
            x,
            y,
            [expected_pixel[0], expected_pixel[1], expected_pixel[2]],
            [actual_pixel[0], actual_pixel[1], actual_pixel[2]],
        ));
    }

    first.map(
        |(first_x, first_y, first_expected, first_actual)| PixelMismatch {
            differing_pixels,
            first_x,
            first_y,
            first_expected,
            first_actual,
            min_x,
            min_y,
            max_x,
            max_y,
        },
    )
}

fn normalized_frame(profile: &GateProfile, session: &TestSession) -> Result<Vec<u8>, String> {
    let frame = session
        .latest_frame()
        .ok_or_else(|| "Test Kit did not emit a framebuffer".to_owned())?;
    if frame.width != DISPLAY_WIDTH
        || frame.height != DISPLAY_HEIGHT
        || frame.width != RUNTIME_WIDTH
        || frame.height != RUNTIME_HEIGHT
    {
        return Err(format!(
            "runtime frame is {}x{}, expected {RUNTIME_WIDTH}x{RUNTIME_HEIGHT}",
            frame.width, frame.height
        ));
    }
    let rgba = frame
        .rgba_pixels()
        .map_err(|error| format!("convert runtime frame to RGBA: {error}"))?;
    if let Some((index, alpha)) = rgba
        .chunks_exact(4)
        .enumerate()
        .find_map(|(index, pixel)| (pixel[3] != 0xFF).then_some((index, pixel[3])))
    {
        return Err(format!(
            "runtime frame pixel {index} has non-opaque alpha {alpha:#04x}"
        ));
    }

    if profile.crop_width != profile.canonical_width
        || profile.crop_height != profile.canonical_height * VERTICAL_DECIMATION
        || profile.crop_x + profile.crop_width > RUNTIME_WIDTH
        || profile.crop_y + profile.crop_height > RUNTIME_HEIGHT
    {
        return Err(format!(
            "invalid registered runtime crop for {}",
            profile.id
        ));
    }

    let mut rgb =
        Vec::with_capacity((profile.canonical_width * profile.canonical_height * 3) as usize);
    for output_y in 0..profile.canonical_height {
        let source_y_a = profile.crop_y + output_y * VERTICAL_DECIMATION;
        let source_y_b = source_y_a + 1;
        let row_start_a = ((source_y_a * RUNTIME_WIDTH + profile.crop_x) * 4) as usize;
        let row_start_b = ((source_y_b * RUNTIME_WIDTH + profile.crop_x) * 4) as usize;
        for source_x in 0..profile.crop_width as usize {
            let offset_a = row_start_a + source_x * 4;
            let offset_b = row_start_b + source_x * 4;
            for channel_index in 0..3 {
                let channel_a = rgba[offset_a + channel_index];
                let channel_b = rgba[offset_b + channel_index];
                let Some(normalized_a) = normalize_runtime_channel(profile, channel_a) else {
                    return Err(format!(
                        "runtime channel {channel_a} at ({source_x}, {source_y_a}) is outside the registered {} encoding",
                        profile.encoding.label()
                    ));
                };
                let Some(normalized_b) = normalize_runtime_channel(profile, channel_b) else {
                    return Err(format!(
                        "runtime channel {channel_b} at ({source_x}, {source_y_b}) is outside the registered {} encoding",
                        profile.encoding.label()
                    ));
                };
                if normalized_a != normalized_b {
                    return Err(format!(
                        "doubled runtime rows differ at canonical ({source_x}, {output_y}), channel {channel_index}: {normalized_a:02X} != {normalized_b:02X}"
                    ));
                }
                rgb.push(normalized_a);
            }
        }
    }
    Ok(rgb)
}

fn normalize_runtime_channel(profile: &GateProfile, channel: u8) -> Option<u8> {
    match profile.encoding {
        PixelEncoding::Rgb4 => quantize_channel(
            channel,
            profile.runtime_channel_step,
            profile.runtime_max_channel_error,
        ),
        PixelEncoding::Rgb8 => Some(channel),
    }
}

fn load_fixtures(profile: &GateProfile) -> Fixtures {
    let test_kit_path = required_path(TEST_KIT_ENV);
    let loaded = read_media_asset(&test_kit_path, MediaKind::Disk)
        .unwrap_or_else(|error| panic!("read {}: {error}", test_kit_path.display()));
    assert_fixture(
        "Amiga Test Kit v1.21 ADF",
        &loaded.bytes,
        TEST_KIT_BYTES,
        TEST_KIT_SHA256,
    );

    let kickstart_path = required_path(profile.kickstart_env);
    let kickstart = fs::read(&kickstart_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", kickstart_path.display()));
    assert_fixture(
        profile.kickstart_label,
        &kickstart,
        profile.kickstart_bytes,
        profile.kickstart_sha256,
    );

    Fixtures {
        kickstart,
        test_kit_adf: loaded.bytes,
    }
}

fn required_path(variable: &str) -> PathBuf {
    let value = std::env::var_os(variable)
        .unwrap_or_else(|| panic!("{variable} must name the registered external fixture"));
    let path = PathBuf::from(value);
    assert!(
        path.is_file(),
        "{variable} does not name a readable file: {}",
        path.display()
    );
    path
}

fn assert_fixture(label: &str, bytes: &[u8], expected_len: usize, expected_sha256: &str) {
    assert_eq!(
        bytes.len(),
        expected_len,
        "{label} has the wrong byte length"
    );
    assert_eq!(
        sha256_hex(bytes),
        expected_sha256,
        "{label} does not match the registered fixture"
    );
}

fn build_session(profile: &GateProfile, fixtures: &Fixtures) -> TestSession {
    let runtime =
        AmigaRuntimeKind::new(profile.model, fixtures.kickstart.clone()).unwrap_or_else(|error| {
            panic!(
                "construct {} Test Kit runtime: {error}",
                profile.machine_label
            )
        });
    let frame_ticks = runtime.native_frame_ticks();
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, frame_ticks, AmigaSessionQueryProvider);
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "floppy-0",
        MediaKind::Disk,
        &fixtures.test_kit_adf,
    ));
    session
        .load_media(&media)
        .expect("insert registered Test Kit v1.21 ADF into DF0");
    session
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("runtime crate should be two levels below repository root")
        .to_path_buf()
}

fn reference_dir(profile: &GateProfile) -> PathBuf {
    repo_root()
        .join("test-data/amiga-test-kit-v1.21")
        .join(profile.id)
}

fn diagnostics_dir(profile: &GateProfile) -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                repo_root().join(path)
            }
        })
        .unwrap_or_else(|| repo_root().join("target"));
    target
        .join("accuracy/amiga-test-kit-v1.21")
        .join(profile.id)
}

fn prepare_diagnostics_dir(profile: &GateProfile) {
    let dir = diagnostics_dir(profile);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .unwrap_or_else(|error| panic!("clear stale diagnostics {}: {error}", dir.display()));
    }
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("create diagnostics {}: {error}", dir.display()));
}

fn load_a500_manifest(reference_dir: &Path) -> A500Manifest {
    load_manifest(reference_dir)
}

fn load_a1200_manifest(reference_dir: &Path) -> A1200Manifest {
    load_manifest(reference_dir)
}

fn load_assertions_contract(reference_dir: &Path) -> AssertionsContract {
    let path = reference_dir.join("assertions.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("read assertions contract {}: {error}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "decode strict assertions contract {}: {error}",
            path.display()
        )
    })
}

fn load_manifest<T>(reference_dir: &Path) -> T
where
    T: for<'de> Deserialize<'de>,
{
    let path = reference_dir.join("manifest.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("read reference manifest {}: {error}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "decode strict reference manifest {}: {error}",
            path.display()
        )
    })
}

fn validate_assertions_contract(
    profile: &GateProfile,
    reference_dir: &Path,
    frames: &[FrameManifest],
    contract: &AssertionsContract,
) {
    assert_eq!(contract.schema_version, 1);
    assert_eq!(contract.profile_id, profile.id);
    assert_eq!(contract.canonical_width, profile.canonical_width);
    assert_eq!(contract.canonical_height, profile.canonical_height);
    assert_eq!(contract.pixel_encoding, profile.encoding.label());
    assert_eq!(
        contract.normalized_actual_channel_encoding,
        "packed-row-major-profile-channel-bytes"
    );
    assert_eq!(
        contract.diff_mask_encoding,
        "one-byte-per-pixel-0-equal-1-different"
    );
    assert_sha256_text(
        &contract.producer_manifest_sha256,
        "producer manifest binding",
    );
    let manifest_path = reference_dir.join("manifest.json");
    let manifest_bytes = fs::read(&manifest_path).unwrap_or_else(|error| {
        panic!(
            "read producer manifest binding {}: {error}",
            manifest_path.display()
        )
    });
    assert_eq!(
        contract.producer_manifest_sha256,
        sha256_hex(&manifest_bytes),
        "assertions contract is bound to different producer manifest bytes"
    );

    let expected_keys: BTreeSet<_> = frames
        .iter()
        .flat_map(|frame| {
            frame
                .references
                .iter()
                .map(move |reference| (frame.id.as_str(), reference.phase.as_str()))
        })
        .collect();
    let actual_keys: BTreeSet<_> = contract
        .assertions
        .iter()
        .map(|assertion| (assertion.case.as_str(), assertion.phase.as_str()))
        .collect();
    assert_eq!(
        actual_keys.len(),
        contract.assertions.len(),
        "assertions contract contains duplicate case/phase keys"
    );
    assert_eq!(
        actual_keys, expected_keys,
        "assertions contract must cover every producer reference phase exactly once"
    );

    let total_pixels = (profile.canonical_width * profile.canonical_height) as usize;
    let zero_diff_mask_sha256 = sha256_hex(&vec![0; total_pixels]);
    for assertion in &contract.assertions {
        let frame = manifest_frame(frames, &assertion.case);
        let reference = frame
            .references
            .iter()
            .find(|reference| reference.phase == assertion.phase)
            .unwrap_or_else(|| {
                panic!(
                    "assertion {} phase {} has no producer reference",
                    assertion.case, assertion.phase
                )
            });
        let expected_disagreement =
            expected_disagreement_id(profile, &assertion.case, &assertion.phase);
        let expected_expectation = if expected_disagreement.is_some() {
            AssertionExpectation::RegisteredDisagreement
        } else {
            AssertionExpectation::Exact
        };
        assert_eq!(
            assertion.expectation, expected_expectation,
            "{} phase {} has the wrong assertion class",
            assertion.case, assertion.phase
        );
        assert_eq!(
            assertion.disagreement_id.as_deref(),
            expected_disagreement,
            "{} phase {} has the wrong disagreement ID",
            assertion.case,
            assertion.phase
        );
        assert_sha256_text(
            &assertion.normalized_actual_channel_bytes_sha256,
            &format!(
                "{} {} normalized actual channels",
                assertion.case, assertion.phase
            ),
        );
        assert_sha256_text(
            &assertion.diff_mask_sha256,
            &format!("{} {} diff mask", assertion.case, assertion.phase),
        );

        let normalized_reference = load_reference(profile, reference_dir, reference);
        let normalized_reference_sha256 = sha256_hex(&normalized_reference);
        match assertion.expectation {
            AssertionExpectation::Exact => {
                assert_eq!(assertion.differing_pixels, 0);
                assert!(assertion.first_difference.is_none());
                assert!(assertion.bounding_box.is_none());
                assert_eq!(
                    assertion.normalized_actual_channel_bytes_sha256, normalized_reference_sha256,
                    "exact assertion actual hash must equal its normalized reference"
                );
                assert_eq!(
                    assertion.diff_mask_sha256, zero_diff_mask_sha256,
                    "exact assertion diff mask must be entirely zero"
                );
            }
            AssertionExpectation::RegisteredDisagreement => {
                assert!(assertion.differing_pixels > 0);
                assert!(assertion.differing_pixels <= total_pixels);
                assert_ne!(
                    assertion.normalized_actual_channel_bytes_sha256, normalized_reference_sha256,
                    "registered disagreement must not pin an exact reference hash"
                );
                assert_ne!(
                    assertion.diff_mask_sha256, zero_diff_mask_sha256,
                    "registered disagreement must not pin an all-zero mask"
                );
                let first = assertion.first_difference.as_ref().unwrap_or_else(|| {
                    panic!(
                        "{} phase {} disagreement lacks first difference",
                        assertion.case, assertion.phase
                    )
                });
                let bounding_box = assertion.bounding_box.as_ref().unwrap_or_else(|| {
                    panic!(
                        "{} phase {} disagreement lacks bounding box",
                        assertion.case, assertion.phase
                    )
                });
                assert!(first.x < profile.canonical_width);
                assert!(first.y < profile.canonical_height);
                assert!(bounding_box.min_x <= first.x && first.x <= bounding_box.max_x);
                assert!(bounding_box.min_y <= first.y && first.y <= bounding_box.max_y);
                assert!(bounding_box.max_x < profile.canonical_width);
                assert!(bounding_box.max_y < profile.canonical_height);
                let bounding_area = usize::try_from(
                    (bounding_box.max_x - bounding_box.min_x + 1)
                        * (bounding_box.max_y - bounding_box.min_y + 1),
                )
                .expect("assertion bounding-box area fits in usize");
                assert!(assertion.differing_pixels <= bounding_area);
                let first_offset = ((first.y * profile.canonical_width + first.x) * 3) as usize;
                assert_eq!(
                    first.expected_rgb,
                    normalized_reference[first_offset..first_offset + 3],
                    "registered first expected colour must come from the bound reference"
                );
                assert_ne!(first.actual_rgb, first.expected_rgb);
                if profile.encoding == PixelEncoding::Rgb4 {
                    assert!(first.actual_rgb.iter().all(|channel| *channel <= 0x0F));
                }
            }
        }
    }
}

fn expected_disagreement_id(
    profile: &GateProfile,
    case_id: &str,
    phase: &str,
) -> Option<&'static str> {
    match (profile.id, case_id, phase) {
        ("a500-a501-ocs-pal", "gradients" | "ebu-bars", "static") => {
            Some("denise-ocs-color-output-phase")
        }
        ("a1200-aga-pal", "gradients" | "static-checkerboard", "static")
        | ("a1200-aga-pal", "alternating-checkerboard", "a" | "b") => {
            Some("aga-sprite-horizontal-output-phase")
        }
        ("a500-a501-ocs-pal" | "a1200-aga-pal", _, _) => None,
        _ => panic!("no assertion policy registered for profile {}", profile.id),
    }
}

fn validate_a500_manifest(manifest: &A500Manifest) {
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.evidence_level, "single-independent-implementation");
    assert_eq!(manifest.suite.name, "Amiga Test Kit");
    assert_eq!(manifest.suite.version, "1.21");
    assert_eq!(manifest.suite.source_tag, "testkit-v1.21");
    assert_eq!(
        manifest.suite.source_commit,
        "9477599d1611da2326f43532dbe563c2848e308b"
    );
    assert_eq!(manifest.suite.adf_sha256, TEST_KIT_SHA256);

    assert_eq!(manifest.machine.model, "commodore-amiga-a500-ocs-pal-a501");
    assert_eq!(manifest.machine.cpu, "MC68000");
    assert_eq!(manifest.machine.chipset, "OCS");
    assert_eq!(manifest.machine.region, "PAL");
    assert_eq!(manifest.machine.chip_ram_bytes, 512 * 1024);
    assert_eq!(manifest.machine.slow_ram_bytes, 512 * 1024);
    assert_eq!(manifest.machine.kickstart_revision, "1.3 r34.005");
    assert_eq!(manifest.machine.kickstart_sha256, A500_KICKSTART_SHA256);

    assert_eq!(manifest.viewport.runtime_width, RUNTIME_WIDTH);
    assert_eq!(manifest.viewport.runtime_height, RUNTIME_HEIGHT);
    assert_eq!(manifest.viewport.x, A500_PROFILE.crop_x);
    assert_eq!(manifest.viewport.y, A500_PROFILE.crop_y);
    assert_eq!(manifest.viewport.width, A500_PROFILE.crop_width);
    assert_eq!(manifest.viewport.height, A500_PROFILE.crop_height);
    assert_eq!(manifest.viewport.vertical_decimation, VERTICAL_DECIMATION);
    assert_eq!(
        manifest.viewport.canonical_width,
        A500_PROFILE.canonical_width
    );
    assert_eq!(
        manifest.viewport.canonical_height,
        A500_PROFILE.canonical_height
    );
    assert_eq!(manifest.viewport.pixel_format, "rgb8");
    assert_eq!(manifest.comparison.format, "rgb4");
    assert_eq!(
        manifest.comparison.reference_channel_step,
        A500_PROFILE.reference_channel_step
    );
    assert_eq!(
        manifest.comparison.runtime_channel_step,
        A500_PROFILE.runtime_channel_step
    );
    assert_eq!(manifest.comparison.rounding, "nearest");
    assert_eq!(
        manifest.comparison.reference_max_error,
        A500_PROFILE.reference_max_channel_error
    );
    assert_eq!(
        manifest.comparison.runtime_max_error,
        A500_PROFILE.runtime_max_channel_error
    );

    assert!(!manifest.producer.id.is_empty());
    assert_eq!(manifest.producer.emulator, "vAmiga");
    assert_eq!(manifest.producer.version, "4.4b12");
    assert_eq!(
        manifest.producer.revision,
        "60fd1e6b69dcd77c9f44d1291bd37ec715362ab0"
    );
    assert_eq!(manifest.producer.implementation_family, "vAmiga");
    assert_eq!(manifest.producer.configuration, "A500_OCS_1MB");
    assert_eq!(
        manifest.producer.capture_method,
        "VAHeadless RegressionTester raw RGB"
    );
    assert!(
        !manifest.producer.emulator.eq_ignore_ascii_case("Emu198x"),
        "Emu198x output cannot be its own reference producer"
    );
    assert_eq!(manifest.producer_viewport.texture_x_start, 4 * 0x31);
    assert_eq!(manifest.producer_viewport.texture_x_end_exclusive, 912);
    assert_eq!(manifest.producer_viewport.texture_y_start, 26);
    assert_eq!(manifest.producer_viewport.texture_y_end_exclusive, 311);
    assert_eq!(manifest.producer_viewport.beam_hpos_start, 0x31);
    assert_eq!(manifest.producer_viewport.beam_hpos_end_exclusive, 0xE4);
    assert_eq!(manifest.producer_viewport.beam_vpos_start, 26);
    assert_eq!(manifest.producer_viewport.beam_vpos_end_exclusive, 311);
    assert_eq!(
        manifest.producer_viewport.width,
        A500_PROFILE.canonical_width
    );
    assert_eq!(
        manifest.producer_viewport.height,
        A500_PROFILE.canonical_height
    );
    assert_eq!(
        manifest.producer_viewport.pixel_format,
        "packed-row-major-rgb8"
    );
    assert_eq!(manifest.producer_timing.unit, "simulated-seconds");
    assert_eq!(manifest.producer_timing.boot_wait, 12);
    assert_eq!(
        manifest.producer_timing.keyboard_auto_release_milliseconds,
        500
    );
    assert_eq!(manifest.producer_timing.inter_key_wait, 1);
    assert_eq!(manifest.execution.boot_fields, BOOT_FIELDS);
    assert_eq!(manifest.execution.key_hold_fields, KEY_HOLD_FIELDS);
    assert_eq!(
        manifest.execution.key_release_settle_fields,
        KEY_RELEASE_SETTLE_FIELDS
    );
    assert_eq!(manifest.execution.inter_key_fields, INTER_KEY_FIELDS);

    let expected_ids: BTreeSet<_> = CASES.iter().map(|case| case.id).collect();
    let actual_ids: BTreeSet<_> = manifest
        .frames
        .iter()
        .map(|frame| frame.id.as_str())
        .collect();
    assert_eq!(
        actual_ids.len(),
        manifest.frames.len(),
        "reference manifest contains duplicate frame IDs"
    );
    assert_eq!(
        actual_ids, expected_ids,
        "manifest cases and executable case table differ"
    );

    for case in CASES {
        let frame = manifest_frame(&manifest.frames, case.id);
        assert!(
            frame.capture_provenance.is_none(),
            "{} A500 reference must not contain A1200 capture provenance",
            case.id
        );
        assert_eq!(
            frame.navigation, case.navigation,
            "{} navigation differs from executable procedure",
            case.id
        );
        assert_eq!(
            frame.execution_settle_fields, case.settle_fields,
            "{} settle time differs from executable procedure",
            case.id
        );
        let expected_behaviour = match case.behaviour {
            Behaviour::Static => "static",
            Behaviour::Alternating => "alternating",
        };
        assert_eq!(frame.behaviour, expected_behaviour);

        match case.behaviour {
            Behaviour::Static => {
                assert_eq!(
                    frame.references.len(),
                    1,
                    "{} must have one static reference",
                    case.id
                );
                let reference = &frame.references[0];
                assert_eq!(reference.phase, "static");
                assert_eq!(reference.file, format!("{}.png", case.id));
                let expected_wait = if case.id == "gradients" { 3 } else { 2 };
                assert_eq!(reference.producer_final_wait_seconds, Some(expected_wait));
            }
            Behaviour::Alternating => {
                assert_eq!(
                    frame.references.len(),
                    2,
                    "{} must have two reference phases",
                    case.id
                );
                assert_eq!(frame.references[0].phase, "a");
                assert_eq!(
                    frame.references[0].file,
                    "alternating-checkerboard-phase-a.png"
                );
                assert_eq!(frame.references[0].producer_final_wait_seconds, Some(2));
                assert_eq!(frame.references[1].phase, "b");
                assert_eq!(
                    frame.references[1].file,
                    "alternating-checkerboard-phase-b.png"
                );
                assert_eq!(frame.references[1].producer_final_wait_seconds, Some(3));
            }
        }

        let phases: BTreeSet<_> = frame
            .references
            .iter()
            .map(|reference| reference.phase.as_str())
            .collect();
        assert_eq!(
            phases.len(),
            frame.references.len(),
            "{} contains duplicate reference phases",
            case.id
        );
        for reference in &frame.references {
            assert!(reference.source_core_field.is_none());
            assert!(reference.source_raw_sha256.is_none());
            assert_safe_relative_file(&reference.file);
            assert_sha256_text(
                &reference.png_sha256,
                &format!("{} {} PNG", case.id, reference.phase),
            );
            assert_sha256_text(
                &reference.rgb_sha256,
                &format!("{} {} RGB", case.id, reference.phase),
            );
        }
    }
}

fn validate_a1200_manifest(manifest: &A1200Manifest) {
    assert_eq!(manifest.schema_version, 2);
    assert_eq!(manifest.evidence_level, "single-independent-implementation");
    assert_eq!(manifest.suite.name, "Amiga Test Kit");
    assert_eq!(manifest.suite.version, "1.21");
    assert_eq!(manifest.suite.source_tag, "testkit-v1.21");
    assert_eq!(
        manifest.suite.source_commit,
        "9477599d1611da2326f43532dbe563c2848e308b"
    );
    assert_eq!(manifest.suite.adf_sha256, TEST_KIT_SHA256);

    assert_eq!(manifest.machine.model, "commodore-amiga-a1200-aga-pal");
    assert_eq!(manifest.machine.cpu, "68EC020");
    assert_eq!(manifest.machine.chipset, "AGA");
    assert_eq!(manifest.machine.region, "PAL");
    assert_eq!(manifest.machine.chip_ram_bytes, 2 * 1024 * 1024);
    assert_eq!(manifest.machine.expansion_ram_bytes, 0);
    assert_eq!(manifest.machine.kickstart_revision, "3.1 r40.068");
    assert_eq!(manifest.machine.kickstart_sha256, A1200_KICKSTART_SHA256);

    assert_eq!(manifest.viewport.producer_raw_width, 756);
    assert_eq!(manifest.viewport.producer_raw_height, 576);
    assert_eq!(manifest.viewport.producer_x, 2);
    assert_eq!(manifest.viewport.producer_y, 0);
    assert_eq!(manifest.viewport.runtime_width, RUNTIME_WIDTH);
    assert_eq!(manifest.viewport.runtime_height, RUNTIME_HEIGHT);
    assert_eq!(manifest.viewport.runtime_x, A1200_PROFILE.crop_x);
    assert_eq!(manifest.viewport.runtime_y, A1200_PROFILE.crop_y);
    assert_eq!(manifest.viewport.width, A1200_PROFILE.crop_width);
    assert_eq!(manifest.viewport.height, A1200_PROFILE.crop_height);
    assert_eq!(manifest.viewport.vertical_decimation, VERTICAL_DECIMATION);
    assert_eq!(
        manifest.viewport.canonical_width,
        A1200_PROFILE.canonical_width
    );
    assert_eq!(
        manifest.viewport.canonical_height,
        A1200_PROFILE.canonical_height
    );
    assert_eq!(manifest.viewport.pixel_format, "rgb8");
    assert!(!manifest.viewport.alignment_search);
    assert_eq!(
        manifest.viewport.horizontal_mapping.formula,
        "runtime_x = producer_raw_x + 8"
    );
    assert_eq!(
        manifest.viewport.horizontal_mapping.basis,
        "beam-absolute PAL host-HIRES mapping: producer raw x=0 is HB coarse coordinate 46; Emu198x x=0 is CCK 44"
    );

    assert_eq!(manifest.comparison.format, "rgb8-exact");
    assert_eq!(manifest.comparison.channel_tolerance, 0);
    assert_eq!(
        manifest.comparison.reference_alpha,
        "discard-after-opaque-validation"
    );
    assert_eq!(manifest.comparison.runtime_alpha, "must-be-opaque");
    assert_eq!(
        manifest.comparison.row_pair_policy,
        "require-identical-before-decimation"
    );

    assert_eq!(manifest.producer.id, "fs-uae-5.0.7-f362278c-a1200-aga-pal");
    assert_eq!(manifest.producer.emulator, "FS-UAE");
    assert_eq!(manifest.producer.version, "5.0.7");
    assert_eq!(
        manifest.producer.revision,
        "f362278ccd4c60991caac3b4d240d4a3f751bea2"
    );
    assert_eq!(manifest.producer.uae_base_version, "WinUAE 6.0.1");
    assert_eq!(manifest.producer.implementation_family, "UAE");
    assert_eq!(
        manifest.producer.configuration,
        "A1200 AGA PAL cycle-exact 68EC020"
    );
    assert_eq!(
        manifest.producer.capture_method,
        "environment-gated raw chipset framebuffer hook"
    );
    assert_eq!(
        manifest.producer.capture_patch_sha256,
        "6116765eab7036cf756cb3212968675c9d1ca3ef327b8da3e4d194f05ffbb767"
    );
    assert_eq!(
        manifest.producer.binary_sha256,
        "5c3d9e35d100445a5603c5f86a19cc431a7363828053d4ede7d260c2c5d6899f"
    );
    assert!(
        !manifest.producer.emulator.eq_ignore_ascii_case("Emu198x"),
        "Emu198x output cannot be its own reference producer"
    );

    assert_eq!(
        manifest.capture_adapter.capture_sh,
        "511cfed52f2b5d8a03a3d335bc144fbee59d2624f94c734e828c135b566eca28"
    );
    assert_eq!(
        manifest.capture_adapter.capture_manifest_py,
        "896d310b9eecdcb09d67d29436e3ee1a389bde7385d595fab6048a13ee3a076e"
    );
    assert_eq!(
        manifest.capture_adapter.config_uae_in,
        "cf8f5bdb01142cfe08c271158a0e1253ddef700f03b52a9f6f67698fc2648745"
    );
    assert_eq!(
        manifest.capture_adapter.portable_ini,
        "f6ea7ad62b30f5b1d3092081990d41206c60aea7dcc29379a2977e89c4d994f0"
    );
    assert_eq!(
        manifest.capture_adapter.capture_patch,
        manifest.producer.capture_patch_sha256
    );

    assert_eq!(manifest.packaging.tool, "package.py");
    assert_eq!(
        manifest.packaging.tool_sha256,
        "e238b3baea92c07b22e7183c5c2ece080982a4bd76e6b546fe098f5cf1feed82"
    );
    assert_eq!(manifest.packaging.python_version, "3.14.3");
    assert_eq!(manifest.packaging.zlib_version, "1.2.12");
    assert_eq!(
        manifest.packaging.png_encoding,
        "RGB8 filter-none zlib-level-9"
    );

    assert_eq!(manifest.execution.boot_fields, BOOT_FIELDS);
    assert_eq!(manifest.execution.key_hold_fields, KEY_HOLD_FIELDS);
    assert_eq!(
        manifest.execution.key_release_settle_fields,
        KEY_RELEASE_SETTLE_FIELDS
    );
    assert_eq!(manifest.execution.inter_key_fields, INTER_KEY_FIELDS);

    let expected_ids: BTreeSet<_> = CASES.iter().map(|case| case.id).collect();
    let actual_ids: BTreeSet<_> = manifest
        .frames
        .iter()
        .map(|frame| frame.id.as_str())
        .collect();
    assert_eq!(
        actual_ids.len(),
        manifest.frames.len(),
        "A1200 reference manifest contains duplicate frame IDs"
    );
    assert_eq!(
        actual_ids, expected_ids,
        "A1200 manifest cases and executable case table differ"
    );

    for case in CASES {
        let frame = manifest_frame(&manifest.frames, case.id);
        assert_eq!(frame.navigation, case.navigation);
        assert_eq!(frame.execution_settle_fields, case.settle_fields);
        let expected_behaviour = match case.behaviour {
            Behaviour::Static => "static",
            Behaviour::Alternating => "alternating",
        };
        assert_eq!(frame.behaviour, expected_behaviour);

        let expected_first_field = BOOT_FIELDS
            + u32::try_from(case.navigation.len()).expect("navigation length fits in u32")
                * (KEY_HOLD_FIELDS + KEY_RELEASE_SETTLE_FIELDS)
            + u32::try_from(case.navigation.len().saturating_sub(1))
                .expect("navigation wait count fits in u32")
                * INTER_KEY_FIELDS
            + case.settle_fields;
        let capture = frame
            .capture_provenance
            .as_ref()
            .unwrap_or_else(|| panic!("{} lacks capture provenance", case.id));
        assert_eq!(
            capture.captured_core_fields,
            [
                expected_first_field,
                expected_first_field + 1,
                expected_first_field + 2
            ]
        );
        assert_eq!(capture.frontend_wait_status, 0);
        assert_eq!(capture.inputs_before_sha256, capture.inputs_after_sha256);
        assert!(capture.captured_at_utc.ends_with("+00:00"));
        assert!(!capture.host.is_empty());
        assert!(!capture.operator.is_empty());
        assert_sha256_text(&capture.capture_manifest_sha256, "capture manifest");
        assert_sha256_text(&capture.configuration_sha256, "capture configuration");
        assert_sha256_text(&capture.run_log_sha256, "capture run log");
        assert_sha256_text(&capture.inputs_before_sha256, "capture inputs");
        assert_eq!(capture.raw_sha256.len(), 3);
        for hash in &capture.raw_sha256 {
            assert_sha256_text(hash, "raw capture");
        }
        match case.behaviour {
            Behaviour::Static => {
                assert_eq!(capture.raw_sha256[0], capture.raw_sha256[1]);
                assert_eq!(capture.raw_sha256[1], capture.raw_sha256[2]);
            }
            Behaviour::Alternating => {
                assert_ne!(capture.raw_sha256[0], capture.raw_sha256[1]);
                assert_eq!(capture.raw_sha256[0], capture.raw_sha256[2]);
            }
        }

        let expected_reference_count = match case.behaviour {
            Behaviour::Static => 1,
            Behaviour::Alternating => 2,
        };
        assert_eq!(frame.references.len(), expected_reference_count);
        for (index, reference) in frame.references.iter().enumerate() {
            let expected_phase = match case.behaviour {
                Behaviour::Static => "static",
                Behaviour::Alternating if index == 0 => "a",
                Behaviour::Alternating => "b",
            };
            let expected_file = match case.behaviour {
                Behaviour::Static => format!("{}.png", case.id),
                Behaviour::Alternating if index == 0 => {
                    "alternating-checkerboard-phase-a.png".to_owned()
                }
                Behaviour::Alternating => "alternating-checkerboard-phase-b.png".to_owned(),
            };
            assert_eq!(reference.phase, expected_phase);
            assert_eq!(reference.file, expected_file);
            assert!(reference.producer_final_wait_seconds.is_none());
            assert_safe_relative_file(&reference.file);
            assert_sha256_text(&reference.png_sha256, "reference PNG");
            assert_sha256_text(&reference.rgb_sha256, "reference RGB");

            let source_field = reference
                .source_core_field
                .unwrap_or_else(|| panic!("{} {} lacks source field", case.id, expected_phase));
            let source_index = usize::try_from(source_field - expected_first_field)
                .expect("source field offset fits in usize");
            assert!(source_index < capture.raw_sha256.len());
            assert_eq!(
                reference.source_raw_sha256.as_deref(),
                Some(capture.raw_sha256[source_index].as_str())
            );
            assert_eq!(source_index, index);
        }
    }
}

fn manifest_frame<'a>(frames: &'a [FrameManifest], id: &str) -> &'a FrameManifest {
    frames
        .iter()
        .find(|frame| frame.id == id)
        .unwrap_or_else(|| panic!("reference manifest is missing case {id}"))
}

fn assert_safe_relative_file(file: &str) {
    let path = Path::new(file);
    assert!(
        !path.is_absolute(),
        "reference path must be relative: {file}"
    );
    assert!(
        path.components()
            .all(|component| matches!(component, Component::Normal(_))),
        "reference path must contain only normal components: {file}"
    );
}

fn assert_sha256_text(value: &str, label: &str) {
    assert_eq!(value.len(), 64, "{label} SHA-256 has the wrong length");
    assert!(
        value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{label} SHA-256 is not hexadecimal"
    );
    assert_eq!(
        value,
        value.to_ascii_lowercase(),
        "{label} SHA-256 must use lowercase hexadecimal"
    );
}

fn load_reference(
    profile: &GateProfile,
    reference_dir: &Path,
    reference: &ReferenceImageManifest,
) -> Vec<u8> {
    let path = reference_dir.join(&reference.file);
    let png_bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("read registered reference {}: {error}", path.display()));
    assert_eq!(
        sha256_hex(&png_bytes),
        reference.png_sha256,
        "{} PNG bytes do not match the manifest",
        reference.file
    );

    let decoder = png::Decoder::new(BufReader::new(Cursor::new(&png_bytes)));
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|error| panic!("read strict RGB8 PNG {}: {error}", path.display()));
    assert_eq!(
        reader.info().color_type,
        png::ColorType::Rgb,
        "{} must be an RGB PNG",
        path.display()
    );
    assert_eq!(
        reader.info().bit_depth,
        png::BitDepth::Eight,
        "{} must use eight-bit channels",
        path.display()
    );
    let buffer_size = reader
        .output_buffer_size()
        .expect("registered reference PNG buffer size must fit in usize");
    let mut rgb = vec![0; buffer_size];
    let info = reader
        .next_frame(&mut rgb)
        .unwrap_or_else(|error| panic!("decode registered reference {}: {error}", path.display()));
    rgb.truncate(info.buffer_size());
    assert_eq!(info.width, profile.canonical_width);
    assert_eq!(info.height, profile.canonical_height);
    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    assert_eq!(
        rgb.len(),
        (profile.canonical_width * profile.canonical_height * 3) as usize
    );
    assert_eq!(
        sha256_hex(&rgb),
        reference.rgb_sha256,
        "{} decoded RGB does not match the manifest",
        reference.file
    );
    match profile.encoding {
        PixelEncoding::Rgb4 => rgb
            .into_iter()
            .map(|channel| {
                quantize_channel(
                    channel,
                    profile.reference_channel_step,
                    profile.reference_max_channel_error,
                )
                .unwrap_or_else(|| {
                    panic!(
                        "{} contains channel {channel} outside the registered RGB4 encoding",
                        path.display()
                    )
                })
            })
            .collect(),
        PixelEncoding::Rgb8 => rgb,
    }
}

fn quantize_channel(channel: u8, step: u8, max_error: u8) -> Option<u8> {
    let quantized = (u16::from(channel) + u16::from(step / 2)) / u16::from(step);
    let rgb4 = u8::try_from(quantized.min(15)).expect("quantized RGB4 channel must fit in u8");
    let encoded = rgb4 * step;
    (channel.abs_diff(encoded) <= max_error).then_some(rgb4)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("format SHA-256 byte");
    }
    output
}

fn write_mismatch_diagnostics(
    context: DiagnosticContext<'_>,
    reference: &ReferenceImageManifest,
    actual: &[u8],
    expected: &[u8],
    mismatch: &PixelMismatch,
) {
    let DiagnosticContext {
        profile,
        case_id,
        frame_manifest,
        producer_id,
    } = context;
    let dir = diagnostics_dir(profile);
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("create diagnostics {}: {error}", dir.display()));
    write_rgb_png(profile, &dir.join(format!("{case_id}.actual.png")), actual);

    let diff = diff_pixels(profile, actual, expected);
    write_rgb_png(profile, &dir.join(format!("{case_id}.diff.png")), &diff);

    let result = serde_json::json!({
        "schema_version": 1,
        "case": frame_manifest.id,
        "artifact": case_id,
        "status": "pixel-mismatch",
        "producer_id": producer_id,
        "reference": reference_identity(reference),
        "canonical_width": profile.canonical_width,
        "canonical_height": profile.canonical_height,
        "pixel_encoding": profile.encoding.label(),
        "differing_pixels": mismatch.differing_pixels,
        "total_pixels": u64::from(profile.canonical_width) * u64::from(profile.canonical_height),
        "first": {
            "x": mismatch.first_x,
            "y": mismatch.first_y,
            "expected_rgb": mismatch.first_expected,
            "actual_rgb": mismatch.first_actual
        },
        "bounding_box": {
            "min_x": mismatch.min_x,
            "min_y": mismatch.min_y,
            "max_x": mismatch.max_x,
            "max_y": mismatch.max_y
        }
    });
    write_result_json(profile, case_id, &result);
}

fn write_temporal_diagnostics(
    profile: &GateProfile,
    case_id: &str,
    frame_manifest: &FrameManifest,
    producer_id: &str,
    first: &[u8],
    second: &[u8],
    status: &str,
) {
    let dir = diagnostics_dir(profile);
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("create diagnostics {}: {error}", dir.display()));
    write_rgb_png(profile, &dir.join(format!("{case_id}.field-a.png")), first);
    write_rgb_png(profile, &dir.join(format!("{case_id}.field-b.png")), second);
    write_rgb_png(
        profile,
        &dir.join(format!("{case_id}.field-diff.png")),
        &diff_pixels(profile, first, second),
    );
    let mismatch = pixel_mismatch(profile, first, second);
    let mut result = serde_json::json!({
        "schema_version": 1,
        "case": frame_manifest.id,
        "artifact": case_id,
        "status": status,
        "producer_id": producer_id,
        "references": reference_identities(frame_manifest),
        "canonical_width": profile.canonical_width,
        "canonical_height": profile.canonical_height,
        "pixel_encoding": profile.encoding.label(),
        "field_a_equals_field_b": mismatch.is_none()
    });
    if let Some(mismatch) = mismatch {
        result["differing_pixels"] = serde_json::json!(mismatch.differing_pixels);
        result["total_pixels"] = serde_json::json!(
            u64::from(profile.canonical_width) * u64::from(profile.canonical_height)
        );
        result["first"] = serde_json::json!({
            "x": mismatch.first_x,
            "y": mismatch.first_y,
            "field_b_rgb": mismatch.first_expected,
            "field_a_rgb": mismatch.first_actual
        });
        result["bounding_box"] = serde_json::json!({
            "min_x": mismatch.min_x,
            "min_y": mismatch.min_y,
            "max_x": mismatch.max_x,
            "max_y": mismatch.max_y
        });
    }
    write_result_json(profile, case_id, &result);
}

fn write_alternating_diagnostics(
    context: DiagnosticContext<'_>,
    phase_a: &[u8],
    phase_b: &[u8],
    phase_a2: &[u8],
    phase_b2: &[u8],
) {
    let DiagnosticContext {
        profile,
        case_id,
        frame_manifest,
        producer_id,
    } = context;
    let dir = diagnostics_dir(profile);
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("create diagnostics {}: {error}", dir.display()));
    write_rgb_png(
        profile,
        &dir.join(format!("{case_id}.phase-a.png")),
        phase_a,
    );
    write_rgb_png(
        profile,
        &dir.join(format!("{case_id}.phase-b.png")),
        phase_b,
    );
    write_rgb_png(
        profile,
        &dir.join(format!("{case_id}.phase-a2.png")),
        phase_a2,
    );
    write_rgb_png(
        profile,
        &dir.join(format!("{case_id}.phase-b2.png")),
        phase_b2,
    );
    write_rgb_png(
        profile,
        &dir.join(format!("{case_id}.phase-a-vs-b.diff.png")),
        &diff_pixels(profile, phase_a, phase_b),
    );
    write_rgb_png(
        profile,
        &dir.join(format!("{case_id}.phase-a-vs-a2.diff.png")),
        &diff_pixels(profile, phase_a, phase_a2),
    );
    write_rgb_png(
        profile,
        &dir.join(format!("{case_id}.phase-b-vs-b2.diff.png")),
        &diff_pixels(profile, phase_b, phase_b2),
    );

    let result = serde_json::json!({
        "schema_version": 1,
        "case": frame_manifest.id,
        "artifact": case_id,
        "status": "alternation-invariant-failed",
        "producer_id": producer_id,
        "references": reference_identities(frame_manifest),
        "canonical_width": profile.canonical_width,
        "canonical_height": profile.canonical_height,
        "pixel_encoding": profile.encoding.label(),
        "invariants": {
            "phase_a_differs_from_phase_b": phase_a != phase_b,
            "phase_a_repeats_after_two_fields": phase_a == phase_a2,
            "phase_b_repeats_after_two_fields": phase_b == phase_b2
        },
        "comparisons": {
            "phase_a_vs_phase_b": frame_comparison(profile, phase_a, phase_b),
            "phase_a_vs_phase_a2": frame_comparison(profile, phase_a, phase_a2),
            "phase_b_vs_phase_b2": frame_comparison(profile, phase_b, phase_b2)
        }
    });
    write_result_json(profile, case_id, &result);
}

fn frame_comparison(profile: &GateProfile, left: &[u8], right: &[u8]) -> serde_json::Value {
    let Some(mismatch) = pixel_mismatch(profile, left, right) else {
        return serde_json::json!({
            "equal": true,
            "differing_pixels": 0,
            "total_pixels": u64::from(profile.canonical_width) * u64::from(profile.canonical_height)
        });
    };
    serde_json::json!({
        "equal": false,
        "differing_pixels": mismatch.differing_pixels,
        "total_pixels": u64::from(profile.canonical_width) * u64::from(profile.canonical_height),
        "first": {
            "x": mismatch.first_x,
            "y": mismatch.first_y,
            "left_rgb": mismatch.first_actual,
            "right_rgb": mismatch.first_expected
        },
        "bounding_box": {
            "min_x": mismatch.min_x,
            "min_y": mismatch.min_y,
            "max_x": mismatch.max_x,
            "max_y": mismatch.max_y
        }
    })
}

fn diff_pixels(profile: &GateProfile, actual: &[u8], expected: &[u8]) -> Vec<u8> {
    let mut diff = Vec::with_capacity(actual.len());
    for (actual_pixel, expected_pixel) in actual.chunks_exact(3).zip(expected.chunks_exact(3)) {
        if actual_pixel == expected_pixel {
            diff.extend_from_slice(&[0, 0, 0]);
        } else {
            diff.extend_from_slice(&[profile.encoding.diagnostic_red(), 0, 0]);
        }
    }
    diff
}

fn reference_identities(frame_manifest: &FrameManifest) -> Vec<serde_json::Value> {
    frame_manifest
        .references
        .iter()
        .map(reference_identity)
        .collect()
}

fn reference_identity(reference: &ReferenceImageManifest) -> serde_json::Value {
    serde_json::json!({
        "phase": reference.phase,
        "file": reference.file,
        "png_sha256": reference.png_sha256,
        "rgb_sha256": reference.rgb_sha256
    })
}

fn write_result_json(profile: &GateProfile, case_id: &str, result: &serde_json::Value) {
    let path = diagnostics_dir(profile).join(format!("{case_id}.result.json"));
    let bytes = serde_json::to_vec_pretty(result).expect("encode diagnostic result JSON");
    fs::write(&path, bytes)
        .unwrap_or_else(|error| panic!("write diagnostic result {}: {error}", path.display()));
}

fn write_rgb_png(profile: &GateProfile, path: &Path, channels: &[u8]) {
    assert_eq!(
        channels.len(),
        (profile.canonical_width * profile.canonical_height * 3) as usize
    );
    let rgb8 = match profile.encoding {
        PixelEncoding::Rgb4 => {
            assert!(
                channels.iter().all(|&channel| channel <= 0x0F),
                "diagnostic input must contain RGB4 channels"
            );
            channels
                .iter()
                .map(|&channel| channel * profile.runtime_channel_step)
                .collect::<Vec<_>>()
        }
        PixelEncoding::Rgb8 => channels.to_vec(),
    };
    let file = fs::File::create(path)
        .unwrap_or_else(|error| panic!("create diagnostic PNG {}: {error}", path.display()));
    let mut encoder = png::Encoder::new(file, profile.canonical_width, profile.canonical_height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .unwrap_or_else(|error| panic!("write diagnostic PNG header {}: {error}", path.display()));
    writer
        .write_image_data(&rgb8)
        .unwrap_or_else(|error| panic!("write diagnostic PNG data {}: {error}", path.display()));
}
