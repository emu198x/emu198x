//! Explicit real-media verification with Amiga Test Kit v1.12.
//!
//! These tests are ignored during ordinary workspace runs. They require the
//! exact external fixtures registered in `test-data/amiga-test-kit-v1.12.md`
//! and fail rather than skip when an explicitly requested input is absent or
//! does not match.

use std::collections::BTreeSet;
use std::fs;
use std::hash::Hasher;
use std::path::PathBuf;

use emu198x_shell::{HeadlessSession, MediaImage, MediaKind, MediaSet, read_media_asset};
use motorola_68000::CpuModel;
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaOcsRuntime, AmigaRuntimeKind, AmigaSessionQueryProvider,
    DISPLAY_HEIGHT, DISPLAY_WIDTH, Model, press_key,
};
use twox_hash::XxHash64;

const TEST_KIT_ENV: &str = "EMU198X_AMIGA_TEST_KIT_ADF";
const KICKSTART_ENV: &str = "EMU198X_AMIGA_KICKSTART_13_ROM";
const TEST_KIT_BYTES: usize = 901_120;
const TEST_KIT_XXH64: u64 = 0x79fa_a06b_03da_4ab0;
const KICKSTART_BYTES: usize = 262_144;
const KICKSTART_XXH64: u64 = 0x911e_cc4a_d6e9_89ee;

const BOOT_FIELDS: u32 = 600;
const MEMORY_PAGE_SETTLE_FIELDS: u32 = 48;
const REPLAY_FIELDS: u32 = 8;

const TEST_KIT_VBL_HZ: u32 = 0x0001_3094;
const TEST_KIT_CPU_HZ: u32 = 0x0001_3096;
const TEST_KIT_IS_PAL: u32 = 0x0001_309A;
const TEST_KIT_CHIPSET_TYPE: u32 = 0x0001_309B;
const TEST_KIT_CPU_NAME: u32 = 0x0001_309C;
const TEST_KIT_CPU_NAME_LEN: usize = 31;
const TEST_KIT_CPU_MODEL: u32 = 0x0001_30BB;

type TestSession = HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>;

struct Fixtures {
    kickstart: Vec<u8>,
    test_kit_adf: Vec<u8>,
}

#[test]
#[ignore = "FIXTURE: explicit Amiga Test Kit v1.12 accuracy gate"]
fn amiga_test_kit_v112_a500_reaches_memory_page() {
    let fixtures = load_fixtures();
    let mut session = build_session(Model::A500OcsPal, &fixtures);

    session
        .run_frames(BOOT_FIELDS)
        .expect("Test Kit should boot to its main menu");
    assert_guest_identity(&session, "68000", 0);
    let menu = settled_visible_frame(&mut session, "A500 Test Kit menu");

    press_key(&mut session, "F1", 3).expect("F1 should reach the Test Kit keyboard");
    session
        .run_frames(MEMORY_PAGE_SETTLE_FIELDS)
        .expect("Test Kit memory page should settle");
    let memory_page = settled_visible_frame(&mut session, "A500 Test Kit memory page");

    assert_ne!(
        memory_page, menu,
        "F1 must replace the main menu with a different visible page"
    );
}

#[test]
#[ignore = "FIXTURE: explicit Amiga Test Kit v1.12 accuracy gate"]
fn amiga_test_kit_v112_a530_identifies_cpu_and_replays() {
    let fixtures = load_fixtures();
    let mut session = build_session(Model::A500OcsPalGvpA530, &fixtures);

    session
        .run_frames(BOOT_FIELDS)
        .expect("Test Kit should boot to its main menu on the A530");
    assert_guest_identity(&session, "68030", 3);
    assert_a530_configuration(&session);
    let menu = settled_visible_frame(&mut session, "A530 Test Kit menu");

    press_key(&mut session, "F1", 3).expect("F1 should reach the Test Kit keyboard");
    session
        .run_frames(MEMORY_PAGE_SETTLE_FIELDS)
        .expect("Test Kit memory page should settle on the A530");
    let memory_page = settled_visible_frame(&mut session, "A530 Test Kit memory page");
    assert_ne!(
        memory_page, menu,
        "F1 must replace the main menu with a different visible page"
    );

    let checkpoint = session
        .snapshot_bytes()
        .expect("encode A530 Test Kit checkpoint");
    let mut replay = build_empty_session(Model::A500OcsPalGvpA530, &fixtures.kickstart);
    replay
        .restore_snapshot(&checkpoint)
        .expect("restore A530 Test Kit checkpoint into a fresh runtime");
    assert_eq!(
        replay
            .snapshot_bytes()
            .expect("re-encode restored A530 Test Kit checkpoint"),
        checkpoint,
        "A530 Test Kit checkpoint must be an immediate snapshot fixed point"
    );
    assert_a530_configuration(&replay);

    session
        .run_frames(REPLAY_FIELDS)
        .expect("advance original A530 Test Kit run");
    replay
        .run_frames(REPLAY_FIELDS)
        .expect("advance restored A530 Test Kit run");

    assert_eq!(
        replay
            .snapshot_bytes()
            .expect("encode replayed A530 Test Kit state"),
        session
            .snapshot_bytes()
            .expect("encode original A530 Test Kit state"),
        "original and restored Test Kit runs must converge byte-for-byte"
    );
    assert_eq!(
        ocs_runtime(&replay).machine().denise().framebuffer(),
        ocs_runtime(&session).machine().denise().framebuffer(),
        "original and restored Test Kit runs must render the same framebuffer"
    );
    assert_eq!(
        a530_mapping_and_ram(&replay),
        a530_mapping_and_ram(&session),
        "original and restored Test Kit runs must preserve A530 mapping and local RAM"
    );
}

fn load_fixtures() -> Fixtures {
    let test_kit_path = required_path(TEST_KIT_ENV);
    let loaded = read_media_asset(&test_kit_path, MediaKind::Disk)
        .unwrap_or_else(|error| panic!("read {}: {error}", test_kit_path.display()));
    assert_fixture(
        "Amiga Test Kit v1.12 ADF",
        &loaded.bytes,
        TEST_KIT_BYTES,
        TEST_KIT_XXH64,
    );

    let kickstart_path = required_path(KICKSTART_ENV);
    let kickstart = fs::read(&kickstart_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", kickstart_path.display()));
    assert_fixture(
        "Kickstart 1.3 r34.005",
        &kickstart,
        KICKSTART_BYTES,
        KICKSTART_XXH64,
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

fn assert_fixture(label: &str, bytes: &[u8], expected_len: usize, expected_hash: u64) {
    assert_eq!(
        bytes.len(),
        expected_len,
        "{label} has the wrong byte length"
    );
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(bytes);
    assert_eq!(
        hasher.finish(),
        expected_hash,
        "{label} does not match the registered v1.12 fixture"
    );
}

fn build_session(model: Model, fixtures: &Fixtures) -> TestSession {
    let mut session = build_empty_session(model, &fixtures.kickstart);
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "floppy-0",
        MediaKind::Disk,
        &fixtures.test_kit_adf,
    ));
    session
        .load_media(&media)
        .expect("insert registered Test Kit ADF into DF0");
    session
}

fn build_empty_session(model: Model, kickstart: &[u8]) -> TestSession {
    let runtime = AmigaRuntimeKind::new(model, kickstart.to_vec())
        .unwrap_or_else(|error| panic!("construct {model:?} Test Kit runtime: {error}"));
    HeadlessSession::new_with_query_provider(
        runtime,
        A500_PAL_FRAME_TICKS,
        AmigaSessionQueryProvider,
    )
}

fn ocs_runtime(session: &TestSession) -> &AmigaOcsRuntime {
    match session.machine() {
        AmigaRuntimeKind::Ocs(runtime) => runtime,
        AmigaRuntimeKind::Ecs(_) | AmigaRuntimeKind::Aga(_) => {
            panic!("Test Kit gate requires an OCS-shaped runtime")
        }
    }
}

fn guest_byte(session: &TestSession, address: u32) -> u8 {
    ocs_runtime(session).machine().read_chip_ram_byte(address)
}

fn guest_u32(session: &TestSession, address: u32) -> u32 {
    u32::from_be_bytes([
        guest_byte(session, address),
        guest_byte(session, address + 1),
        guest_byte(session, address + 2),
        guest_byte(session, address + 3),
    ])
}

fn guest_cpu_name(session: &TestSession) -> String {
    let bytes: Vec<u8> = (0..TEST_KIT_CPU_NAME_LEN)
        .map(|offset| guest_byte(session, TEST_KIT_CPU_NAME + offset as u32))
        .take_while(|&byte| byte != 0)
        .collect();
    String::from_utf8(bytes).expect("Test Kit CPU name should be ASCII")
}

fn assert_guest_identity(session: &TestSession, expected_cpu_name: &str, expected_model: u8) {
    assert_eq!(guest_byte(session, TEST_KIT_VBL_HZ), 50);
    assert_eq!(guest_u32(session, TEST_KIT_CPU_HZ), 7_093_790);
    assert_eq!(guest_byte(session, TEST_KIT_IS_PAL), 1);
    assert_eq!(
        guest_byte(session, TEST_KIT_CHIPSET_TYPE),
        0,
        "Test Kit should identify the original chipset"
    );
    assert_eq!(guest_cpu_name(session), expected_cpu_name);
    assert_eq!(guest_byte(session, TEST_KIT_CPU_MODEL), expected_model);
}

fn settled_visible_frame(session: &mut TestSession, label: &str) -> Vec<u8> {
    assert_eq!(
        ocs_runtime(session).machine().bplcon0(),
        0xB200,
        "{label} should retain Test Kit's three-bitplane display mode"
    );
    let first = current_rgba(session);
    assert_visible_text_screen(&first, label);

    session
        .run_frames(1)
        .unwrap_or_else(|error| panic!("run stability field for {label}: {error}"));
    let second = current_rgba(session);
    assert_visible_text_screen(&second, label);
    assert_eq!(
        second, first,
        "{label} should be stable across settled fields"
    );
    second
}

fn current_rgba(session: &TestSession) -> Vec<u8> {
    let frame = session
        .latest_frame()
        .expect("Test Kit should have emitted a framebuffer");
    assert_eq!(frame.width, DISPLAY_WIDTH);
    assert_eq!(frame.height, DISPLAY_HEIGHT);
    frame
        .rgba_pixels()
        .expect("Test Kit framebuffer should be valid RGBA")
}

fn assert_visible_text_screen(rgba: &[u8], label: &str) {
    let width = DISPLAY_WIDTH as usize;
    let mut non_black = 0_usize;
    let mut bright = 0_usize;
    let mut colours = BTreeSet::new();

    for y in 80_usize..500 {
        for x in 128_usize..640 {
            let offset = (y * width + x) * 4;
            let rgb = [rgba[offset], rgba[offset + 1], rgba[offset + 2]];
            if rgb != [0, 0, 0] {
                non_black += 1;
            }
            if rgb.into_iter().all(|component| component >= 0xC0) {
                bright += 1;
            }
            colours.insert(rgb);
        }
    }

    assert!(
        non_black >= 30_000,
        "{label} has too little visible central-screen content: {non_black} non-black pixels"
    );
    assert!(
        bright >= 200,
        "{label} has too little bright glyph content: {bright} bright pixels"
    );
    assert!(
        colours.len() >= 4,
        "{label} has too little colour structure: {} colours",
        colours.len()
    );
}

fn assert_a530_configuration(session: &TestSession) {
    let runtime = ocs_runtime(session);
    assert_eq!(runtime.machine().active_cpu().model(), CpuModel::M68EC030);
    assert_eq!(runtime.config().cpu().clock_hz(), 40_000_000);
    let board = runtime
        .machine()
        .gvp_a530()
        .expect("A530 profile must retain its accelerator");
    assert_eq!(board.ram_size(), 1024 * 1024);
    assert!(board.configuration_is_coherent());
    let base = board
        .mapped_base()
        .expect("Kickstart should configure A530 local RAM");
    assert!(
        (0x0020_0000..0x00A0_0000).contains(&base),
        "A530 local RAM base ${base:06X} is outside Zorro-II space"
    );
}

fn a530_mapping_and_ram(session: &TestSession) -> (u32, Vec<u8>) {
    let board = ocs_runtime(session)
        .machine()
        .gvp_a530()
        .expect("A530 profile must retain its accelerator");
    (
        board
            .mapped_base()
            .expect("A530 local RAM must remain configured"),
        board.storage().to_vec(),
    )
}
