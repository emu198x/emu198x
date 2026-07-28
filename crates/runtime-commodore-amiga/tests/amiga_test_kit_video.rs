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
    HeadlessSession, InputEvent, MediaImage, MediaKind, MediaSet, read_media_asset,
};
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaRuntimeKind, AmigaSessionQueryProvider, DISPLAY_HEIGHT,
    DISPLAY_WIDTH, Model,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const TEST_KIT_ENV: &str = "EMU198X_AMIGA_TEST_KIT_V121_ADF";
const KICKSTART_ENV: &str = "EMU198X_AMIGA_KICKSTART_13_ROM";
const TEST_KIT_BYTES: usize = 901_120;
const TEST_KIT_SHA256: &str = "abe7426c93619a7bb61ce10e3e66a4747fcaf22acd1d1876310033faa700ad28";
const KICKSTART_BYTES: usize = 262_144;
const KICKSTART_SHA256: &str = "ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53";

const BOOT_FIELDS: u32 = 600;
const KEY_HOLD_FIELDS: u32 = 3;
const KEY_RELEASE_SETTLE_FIELDS: u32 = 1;
const INTER_KEY_FIELDS: u32 = 50;

const RUNTIME_WIDTH: u32 = 768;
const RUNTIME_HEIGHT: u32 = 576;
const CROP_X: u32 = 20;
const CROP_Y: u32 = 2;
const CROP_WIDTH: u32 = 716;
const CROP_HEIGHT: u32 = 570;
const VERTICAL_DECIMATION: u32 = 2;
const REFERENCE_WIDTH: u32 = 716;
const REFERENCE_HEIGHT: u32 = 285;
const REFERENCE_CHANNEL_STEP: u8 = 16;
const RUNTIME_CHANNEL_STEP: u8 = 17;
const REFERENCE_MAX_CHANNEL_ERROR: u8 = 1;
const RUNTIME_MAX_CHANNEL_ERROR: u8 = 0;

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
struct Manifest {
    schema_version: u32,
    evidence_level: String,
    suite: SuiteManifest,
    machine: MachineManifest,
    viewport: ViewportManifest,
    comparison: ComparisonManifest,
    producer: ProducerManifest,
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
struct MachineManifest {
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
struct ViewportManifest {
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
struct ComparisonManifest {
    format: String,
    reference_channel_step: u8,
    runtime_channel_step: u8,
    rounding: String,
    reference_max_error: u8,
    runtime_max_error: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProducerManifest {
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
    references: Vec<ReferenceImageManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceImageManifest {
    phase: String,
    file: String,
    png_sha256: String,
    rgb_sha256: String,
    producer_final_wait_seconds: u32,
}

#[derive(Debug)]
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

#[test]
fn amiga_test_kit_v121_reference_manifest_is_self_consistent() {
    let reference_dir = reference_dir();
    let manifest = load_manifest(&reference_dir);
    validate_manifest(&manifest);
    for frame in &manifest.frames {
        for reference in &frame.references {
            let _ = load_reference(&reference_dir, reference);
        }
    }
}

#[test]
#[ignore = "explicit Amiga Test Kit v1.21 reference-pattern gate"]
fn amiga_test_kit_v121_a500_a501_ocs_pal_matches_reference() {
    let reference_dir = reference_dir();
    prepare_diagnostics_dir();
    let manifest = load_manifest(&reference_dir);
    validate_manifest(&manifest);

    // Validate every registered oracle before spending time booting the guest.
    for frame in &manifest.frames {
        for reference in &frame.references {
            let _ = load_reference(&reference_dir, reference);
        }
    }

    let fixtures = load_fixtures();
    let mut boot = build_session(&fixtures);
    boot.run_frames(BOOT_FIELDS)
        .expect("Test Kit v1.21 should boot to its main menu");
    let menu_checkpoint = boot
        .snapshot_bytes()
        .expect("encode settled Test Kit v1.21 main-menu checkpoint");

    let mut failures = Vec::new();
    for case in CASES {
        let frame_manifest = manifest_frame(&manifest, case.id);
        let expected: Vec<_> = frame_manifest
            .references
            .iter()
            .map(|reference| load_reference(&reference_dir, reference))
            .collect();
        let mut session = build_session(&fixtures);
        session
            .restore_snapshot(&menu_checkpoint)
            .unwrap_or_else(|error| panic!("restore Test Kit menu for {}: {error}", case.id));

        match run_case(
            &mut session,
            case,
            frame_manifest,
            &manifest.producer.id,
            &expected,
        ) {
            Ok(()) => eprintln!("Amiga Test Kit v1.21 video: {} matched", case.id),
            Err(error) => failures.push(format!("{}: {error}", case.id)),
        }
    }

    assert!(
        failures.is_empty(),
        "Amiga Test Kit v1.21 video conformance failed:\n{}",
        failures.join("\n")
    );
}

fn run_case(
    session: &mut TestSession,
    case: &Case,
    frame_manifest: &FrameManifest,
    producer_id: &str,
    expected: &[Vec<u8>],
) -> Result<(), String> {
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

    match case.behaviour {
        Behaviour::Static => run_static_case(
            session,
            case,
            frame_manifest,
            producer_id,
            &expected[0],
            &frame_manifest.references[0],
        ),
        Behaviour::Alternating => {
            run_alternating_case(session, case, frame_manifest, producer_id, expected)
        }
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
    session: &mut TestSession,
    case: &Case,
    frame_manifest: &FrameManifest,
    producer_id: &str,
    expected: &[u8],
    reference: &ReferenceImageManifest,
) -> Result<(), String> {
    let first = normalized_frame(session)?;
    session
        .run_frames(1)
        .map_err(|error| format!("capture adjacent stability field: {error}"))?;
    let second = normalized_frame(session)?;
    if first != second {
        write_temporal_diagnostics(
            case.id,
            frame_manifest,
            producer_id,
            &first,
            &second,
            "static-frame-changed",
        );
        return Err(format!(
            "static pattern changed across adjacent fields; diagnostics: {}",
            diagnostics_dir().display()
        ));
    }
    compare_or_diagnose(
        case.id,
        frame_manifest,
        producer_id,
        &second,
        expected,
        reference,
    )
}

fn run_alternating_case(
    session: &mut TestSession,
    case: &Case,
    frame_manifest: &FrameManifest,
    producer_id: &str,
    expected: &[Vec<u8>],
) -> Result<(), String> {
    let phase_a = normalized_frame(session)?;
    session
        .run_frames(1)
        .map_err(|error| format!("capture alternating phase B: {error}"))?;
    let phase_b = normalized_frame(session)?;
    session
        .run_frames(1)
        .map_err(|error| format!("capture alternating phase A2: {error}"))?;
    let phase_a2 = normalized_frame(session)?;
    session
        .run_frames(1)
        .map_err(|error| format!("capture alternating phase B2: {error}"))?;
    let phase_b2 = normalized_frame(session)?;

    if phase_a == phase_b || phase_a != phase_a2 || phase_b != phase_b2 {
        write_alternating_diagnostics(
            case.id,
            frame_manifest,
            producer_id,
            &phase_a,
            &phase_b,
            &phase_a2,
            &phase_b2,
        );
        return Err(format!(
            "pattern did not satisfy A != B, A == A2 and B == B2; diagnostics: {}",
            diagnostics_dir().display()
        ));
    }

    assert_eq!(
        expected.len(),
        2,
        "alternating case must have two registered phases"
    );
    if (phase_a == expected[0] && phase_b == expected[1])
        || (phase_a == expected[1] && phase_b == expected[0])
    {
        return Ok(());
    }

    let direct_score = differing_pixel_count(&phase_a, &expected[0])
        + differing_pixel_count(&phase_b, &expected[1]);
    let reversed_score = differing_pixel_count(&phase_a, &expected[1])
        + differing_pixel_count(&phase_b, &expected[0]);
    let (expected_a_index, expected_b_index) = if direct_score <= reversed_score {
        (0, 1)
    } else {
        (1, 0)
    };
    let expected_a = &expected[expected_a_index];
    let expected_b = &expected[expected_b_index];
    let mismatch_a = pixel_mismatch(&phase_a, expected_a);
    let mismatch_b = pixel_mismatch(&phase_b, expected_b);
    if let Some(mismatch) = &mismatch_a {
        write_mismatch_diagnostics(
            &format!("{}-phase-a", case.id),
            frame_manifest,
            producer_id,
            &frame_manifest.references[expected_a_index],
            &phase_a,
            expected_a,
            mismatch,
        );
    }
    if let Some(mismatch) = &mismatch_b {
        write_mismatch_diagnostics(
            &format!("{}-phase-b", case.id),
            frame_manifest,
            producer_id,
            &frame_manifest.references[expected_b_index],
            &phase_b,
            expected_b,
            mismatch,
        );
    }
    let phase_a_result = mismatch_a
        .as_ref()
        .map_or_else(|| "matched".to_owned(), mismatch_message);
    let phase_b_result = mismatch_b
        .as_ref()
        .map_or_else(|| "matched".to_owned(), mismatch_message);
    Err(format!(
        "phase A {}; phase B {}",
        phase_a_result, phase_b_result
    ))
}

fn compare_or_diagnose(
    case_id: &str,
    frame_manifest: &FrameManifest,
    producer_id: &str,
    actual: &[u8],
    expected: &[u8],
    reference: &ReferenceImageManifest,
) -> Result<(), String> {
    let Some(mismatch) = pixel_mismatch(actual, expected) else {
        return Ok(());
    };
    write_mismatch_diagnostics(
        case_id,
        frame_manifest,
        producer_id,
        reference,
        actual,
        expected,
        &mismatch,
    );
    Err(mismatch_message(&mismatch))
}

fn differing_pixel_count(actual: &[u8], expected: &[u8]) -> usize {
    actual
        .chunks_exact(3)
        .zip(expected.chunks_exact(3))
        .filter(|(actual_pixel, expected_pixel)| actual_pixel != expected_pixel)
        .count()
}

fn mismatch_message(mismatch: &PixelMismatch) -> String {
    let total = u64::from(REFERENCE_WIDTH) * u64::from(REFERENCE_HEIGHT);
    let percentage = mismatch.differing_pixels as f64 * 100.0 / total as f64;
    format!(
        "{} pixels differ ({percentage:.6}%); first at ({}, {}), expected RGB4 ${:X}{:X}{:X}, actual RGB4 ${:X}{:X}{:X}; bounding box ({}, {})..({}, {}); diagnostics: {}",
        mismatch.differing_pixels,
        mismatch.first_x,
        mismatch.first_y,
        mismatch.first_expected[0],
        mismatch.first_expected[1],
        mismatch.first_expected[2],
        mismatch.first_actual[0],
        mismatch.first_actual[1],
        mismatch.first_actual[2],
        mismatch.min_x,
        mismatch.min_y,
        mismatch.max_x,
        mismatch.max_y,
        diagnostics_dir().display()
    )
}

fn pixel_mismatch(actual: &[u8], expected: &[u8]) -> Option<PixelMismatch> {
    assert_eq!(
        actual.len(),
        (REFERENCE_WIDTH * REFERENCE_HEIGHT * 3) as usize
    );
    assert_eq!(actual.len(), expected.len());

    let mut differing_pixels = 0;
    let mut first = None;
    let mut min_x = REFERENCE_WIDTH;
    let mut min_y = REFERENCE_HEIGHT;
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
        let x = index as u32 % REFERENCE_WIDTH;
        let y = index as u32 / REFERENCE_WIDTH;
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

fn normalized_frame(session: &TestSession) -> Result<Vec<u8>, String> {
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

    let mut rgb = Vec::with_capacity((REFERENCE_WIDTH * REFERENCE_HEIGHT * 3) as usize);
    for output_y in 0..REFERENCE_HEIGHT {
        let source_y_a = CROP_Y + output_y * VERTICAL_DECIMATION;
        let source_y_b = source_y_a + 1;
        let row_start_a = ((source_y_a * RUNTIME_WIDTH + CROP_X) * 4) as usize;
        let row_start_b = ((source_y_b * RUNTIME_WIDTH + CROP_X) * 4) as usize;
        for source_x in 0..CROP_WIDTH as usize {
            let offset_a = row_start_a + source_x * 4;
            let offset_b = row_start_b + source_x * 4;
            for channel_index in 0..3 {
                let channel_a = rgba[offset_a + channel_index];
                let channel_b = rgba[offset_b + channel_index];
                let Some(rgb4_a) =
                    quantize_channel(channel_a, RUNTIME_CHANNEL_STEP, RUNTIME_MAX_CHANNEL_ERROR)
                else {
                    return Err(format!(
                        "runtime channel {channel_a} at ({source_x}, {source_y_a}) is not an exact RGB4-expanded value"
                    ));
                };
                let Some(rgb4_b) =
                    quantize_channel(channel_b, RUNTIME_CHANNEL_STEP, RUNTIME_MAX_CHANNEL_ERROR)
                else {
                    return Err(format!(
                        "runtime channel {channel_b} at ({source_x}, {source_y_b}) is not an exact RGB4-expanded value"
                    ));
                };
                if rgb4_a != rgb4_b {
                    return Err(format!(
                        "doubled runtime rows differ at canonical ({source_x}, {output_y}), channel {channel_index}: {rgb4_a:X} != {rgb4_b:X}"
                    ));
                }
                rgb.push(rgb4_a);
            }
        }
    }
    Ok(rgb)
}

fn load_fixtures() -> Fixtures {
    let test_kit_path = required_path(TEST_KIT_ENV);
    let loaded = read_media_asset(&test_kit_path, MediaKind::Disk)
        .unwrap_or_else(|error| panic!("read {}: {error}", test_kit_path.display()));
    assert_fixture(
        "Amiga Test Kit v1.21 ADF",
        &loaded.bytes,
        TEST_KIT_BYTES,
        TEST_KIT_SHA256,
    );

    let kickstart_path = required_path(KICKSTART_ENV);
    let kickstart = fs::read(&kickstart_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", kickstart_path.display()));
    assert_fixture(
        "Kickstart 1.3 r34.005",
        &kickstart,
        KICKSTART_BYTES,
        KICKSTART_SHA256,
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

fn build_session(fixtures: &Fixtures) -> TestSession {
    let runtime = AmigaRuntimeKind::new(Model::A500OcsPalA501, fixtures.kickstart.clone())
        .unwrap_or_else(|error| panic!("construct A500+A501 Test Kit runtime: {error}"));
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        A500_PAL_FRAME_TICKS,
        AmigaSessionQueryProvider,
    );
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

fn reference_dir() -> PathBuf {
    repo_root().join("test-data/amiga-test-kit-v1.21/a500-a501-ocs-pal")
}

fn diagnostics_dir() -> PathBuf {
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
    target.join("accuracy/amiga-test-kit-v1.21/a500-a501-ocs-pal")
}

fn prepare_diagnostics_dir() {
    let dir = diagnostics_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .unwrap_or_else(|error| panic!("clear stale diagnostics {}: {error}", dir.display()));
    }
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("create diagnostics {}: {error}", dir.display()));
}

fn load_manifest(reference_dir: &Path) -> Manifest {
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

fn validate_manifest(manifest: &Manifest) {
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
    assert_eq!(manifest.machine.kickstart_sha256, KICKSTART_SHA256);

    assert_eq!(manifest.viewport.runtime_width, RUNTIME_WIDTH);
    assert_eq!(manifest.viewport.runtime_height, RUNTIME_HEIGHT);
    assert_eq!(manifest.viewport.x, CROP_X);
    assert_eq!(manifest.viewport.y, CROP_Y);
    assert_eq!(manifest.viewport.width, CROP_WIDTH);
    assert_eq!(manifest.viewport.height, CROP_HEIGHT);
    assert_eq!(manifest.viewport.vertical_decimation, VERTICAL_DECIMATION);
    assert_eq!(manifest.viewport.canonical_width, REFERENCE_WIDTH);
    assert_eq!(manifest.viewport.canonical_height, REFERENCE_HEIGHT);
    assert_eq!(manifest.viewport.pixel_format, "rgb8");
    assert_eq!(manifest.comparison.format, "rgb4");
    assert_eq!(
        manifest.comparison.reference_channel_step,
        REFERENCE_CHANNEL_STEP
    );
    assert_eq!(
        manifest.comparison.runtime_channel_step,
        RUNTIME_CHANNEL_STEP
    );
    assert_eq!(manifest.comparison.rounding, "nearest");
    assert_eq!(
        manifest.comparison.reference_max_error,
        REFERENCE_MAX_CHANNEL_ERROR
    );
    assert_eq!(
        manifest.comparison.runtime_max_error,
        RUNTIME_MAX_CHANNEL_ERROR
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
    assert_eq!(manifest.producer_viewport.width, REFERENCE_WIDTH);
    assert_eq!(manifest.producer_viewport.height, REFERENCE_HEIGHT);
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
        let frame = manifest_frame(manifest, case.id);
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
                assert_eq!(reference.producer_final_wait_seconds, expected_wait);
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
                assert_eq!(frame.references[0].producer_final_wait_seconds, 2);
                assert_eq!(frame.references[1].phase, "b");
                assert_eq!(
                    frame.references[1].file,
                    "alternating-checkerboard-phase-b.png"
                );
                assert_eq!(frame.references[1].producer_final_wait_seconds, 3);
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

fn manifest_frame<'a>(manifest: &'a Manifest, id: &str) -> &'a FrameManifest {
    manifest
        .frames
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

fn load_reference(reference_dir: &Path, reference: &ReferenceImageManifest) -> Vec<u8> {
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
    assert_eq!(info.width, REFERENCE_WIDTH);
    assert_eq!(info.height, REFERENCE_HEIGHT);
    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    assert_eq!(rgb.len(), (REFERENCE_WIDTH * REFERENCE_HEIGHT * 3) as usize);
    assert_eq!(
        sha256_hex(&rgb),
        reference.rgb_sha256,
        "{} decoded RGB does not match the manifest",
        reference.file
    );
    rgb.into_iter()
        .map(|channel| {
            quantize_channel(channel, REFERENCE_CHANNEL_STEP, REFERENCE_MAX_CHANNEL_ERROR)
                .unwrap_or_else(|| {
                    panic!(
                        "{} contains channel {channel} outside the registered RGB4 encoding",
                        path.display()
                    )
                })
        })
        .collect()
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
    case_id: &str,
    frame_manifest: &FrameManifest,
    producer_id: &str,
    reference: &ReferenceImageManifest,
    actual: &[u8],
    expected: &[u8],
    mismatch: &PixelMismatch,
) {
    let dir = diagnostics_dir();
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("create diagnostics {}: {error}", dir.display()));
    write_rgb_png(&dir.join(format!("{case_id}.actual.png")), actual);

    let diff = diff_rgb4(actual, expected);
    write_rgb_png(&dir.join(format!("{case_id}.diff.png")), &diff);

    let result = serde_json::json!({
        "schema_version": 1,
        "case": frame_manifest.id,
        "artifact": case_id,
        "status": "pixel-mismatch",
        "producer_id": producer_id,
        "reference": reference_identity(reference),
        "canonical_width": REFERENCE_WIDTH,
        "canonical_height": REFERENCE_HEIGHT,
        "differing_pixels": mismatch.differing_pixels,
        "total_pixels": u64::from(REFERENCE_WIDTH) * u64::from(REFERENCE_HEIGHT),
        "first": {
            "x": mismatch.first_x,
            "y": mismatch.first_y,
            "expected_rgb4": mismatch.first_expected,
            "actual_rgb4": mismatch.first_actual
        },
        "bounding_box": {
            "min_x": mismatch.min_x,
            "min_y": mismatch.min_y,
            "max_x": mismatch.max_x,
            "max_y": mismatch.max_y
        }
    });
    write_result_json(case_id, &result);
}

fn write_temporal_diagnostics(
    case_id: &str,
    frame_manifest: &FrameManifest,
    producer_id: &str,
    first: &[u8],
    second: &[u8],
    status: &str,
) {
    let dir = diagnostics_dir();
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("create diagnostics {}: {error}", dir.display()));
    write_rgb_png(&dir.join(format!("{case_id}.field-a.png")), first);
    write_rgb_png(&dir.join(format!("{case_id}.field-b.png")), second);
    write_rgb_png(
        &dir.join(format!("{case_id}.field-diff.png")),
        &diff_rgb4(first, second),
    );
    let mismatch = pixel_mismatch(first, second);
    let mut result = serde_json::json!({
        "schema_version": 1,
        "case": frame_manifest.id,
        "artifact": case_id,
        "status": status,
        "producer_id": producer_id,
        "references": reference_identities(frame_manifest),
        "canonical_width": REFERENCE_WIDTH,
        "canonical_height": REFERENCE_HEIGHT,
        "field_a_equals_field_b": mismatch.is_none()
    });
    if let Some(mismatch) = mismatch {
        result["differing_pixels"] = serde_json::json!(mismatch.differing_pixels);
        result["total_pixels"] =
            serde_json::json!(u64::from(REFERENCE_WIDTH) * u64::from(REFERENCE_HEIGHT));
        result["first"] = serde_json::json!({
            "x": mismatch.first_x,
            "y": mismatch.first_y,
            "field_b_rgb4": mismatch.first_expected,
            "field_a_rgb4": mismatch.first_actual
        });
        result["bounding_box"] = serde_json::json!({
            "min_x": mismatch.min_x,
            "min_y": mismatch.min_y,
            "max_x": mismatch.max_x,
            "max_y": mismatch.max_y
        });
    }
    write_result_json(case_id, &result);
}

fn write_alternating_diagnostics(
    case_id: &str,
    frame_manifest: &FrameManifest,
    producer_id: &str,
    phase_a: &[u8],
    phase_b: &[u8],
    phase_a2: &[u8],
    phase_b2: &[u8],
) {
    let dir = diagnostics_dir();
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("create diagnostics {}: {error}", dir.display()));
    write_rgb_png(&dir.join(format!("{case_id}.phase-a.png")), phase_a);
    write_rgb_png(&dir.join(format!("{case_id}.phase-b.png")), phase_b);
    write_rgb_png(&dir.join(format!("{case_id}.phase-a2.png")), phase_a2);
    write_rgb_png(&dir.join(format!("{case_id}.phase-b2.png")), phase_b2);
    write_rgb_png(
        &dir.join(format!("{case_id}.phase-a-vs-b.diff.png")),
        &diff_rgb4(phase_a, phase_b),
    );
    write_rgb_png(
        &dir.join(format!("{case_id}.phase-a-vs-a2.diff.png")),
        &diff_rgb4(phase_a, phase_a2),
    );
    write_rgb_png(
        &dir.join(format!("{case_id}.phase-b-vs-b2.diff.png")),
        &diff_rgb4(phase_b, phase_b2),
    );

    let result = serde_json::json!({
        "schema_version": 1,
        "case": frame_manifest.id,
        "artifact": case_id,
        "status": "alternation-invariant-failed",
        "producer_id": producer_id,
        "references": reference_identities(frame_manifest),
        "canonical_width": REFERENCE_WIDTH,
        "canonical_height": REFERENCE_HEIGHT,
        "invariants": {
            "phase_a_differs_from_phase_b": phase_a != phase_b,
            "phase_a_repeats_after_two_fields": phase_a == phase_a2,
            "phase_b_repeats_after_two_fields": phase_b == phase_b2
        },
        "comparisons": {
            "phase_a_vs_phase_b": frame_comparison(phase_a, phase_b),
            "phase_a_vs_phase_a2": frame_comparison(phase_a, phase_a2),
            "phase_b_vs_phase_b2": frame_comparison(phase_b, phase_b2)
        }
    });
    write_result_json(case_id, &result);
}

fn frame_comparison(left: &[u8], right: &[u8]) -> serde_json::Value {
    let Some(mismatch) = pixel_mismatch(left, right) else {
        return serde_json::json!({
            "equal": true,
            "differing_pixels": 0,
            "total_pixels": u64::from(REFERENCE_WIDTH) * u64::from(REFERENCE_HEIGHT)
        });
    };
    serde_json::json!({
        "equal": false,
        "differing_pixels": mismatch.differing_pixels,
        "total_pixels": u64::from(REFERENCE_WIDTH) * u64::from(REFERENCE_HEIGHT),
        "first": {
            "x": mismatch.first_x,
            "y": mismatch.first_y,
            "left_rgb4": mismatch.first_actual,
            "right_rgb4": mismatch.first_expected
        },
        "bounding_box": {
            "min_x": mismatch.min_x,
            "min_y": mismatch.min_y,
            "max_x": mismatch.max_x,
            "max_y": mismatch.max_y
        }
    })
}

fn diff_rgb4(actual: &[u8], expected: &[u8]) -> Vec<u8> {
    let mut diff = Vec::with_capacity(actual.len());
    for (actual_pixel, expected_pixel) in actual.chunks_exact(3).zip(expected.chunks_exact(3)) {
        if actual_pixel == expected_pixel {
            diff.extend_from_slice(&[0, 0, 0]);
        } else {
            diff.extend_from_slice(&[0x0F, 0, 0]);
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

fn write_result_json(case_id: &str, result: &serde_json::Value) {
    let path = diagnostics_dir().join(format!("{case_id}.result.json"));
    let bytes = serde_json::to_vec_pretty(result).expect("encode diagnostic result JSON");
    fs::write(&path, bytes)
        .unwrap_or_else(|error| panic!("write diagnostic result {}: {error}", path.display()));
}

fn write_rgb_png(path: &Path, rgb4: &[u8]) {
    assert_eq!(
        rgb4.len(),
        (REFERENCE_WIDTH * REFERENCE_HEIGHT * 3) as usize
    );
    assert!(
        rgb4.iter().all(|&channel| channel <= 0x0F),
        "diagnostic input must contain RGB4 channels"
    );
    let rgb8: Vec<u8> = rgb4
        .iter()
        .map(|&channel| channel * RUNTIME_CHANNEL_STEP)
        .collect();
    let file = fs::File::create(path)
        .unwrap_or_else(|error| panic!("create diagnostic PNG {}: {error}", path.display()));
    let mut encoder = png::Encoder::new(file, REFERENCE_WIDTH, REFERENCE_HEIGHT);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .unwrap_or_else(|error| panic!("write diagnostic PNG header {}: {error}", path.display()));
    writer
        .write_image_data(&rgb8)
        .unwrap_or_else(|error| panic!("write diagnostic PNG data {}: {error}", path.display()));
}
