//! Real D64 autoload regressions for the C64 runtime.
//!
//! Every test is `#[ignore]`'d — they require local C64 + 1541 ROMs at
//! `~/.emu198x/roms/commodore-c64/` and the named D64 archives under
//! `~/Projects/Emu198x-Unclean/Reference/commodore/c64/`.

mod common;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{
    HeadlessSession, MachineCore, MediaImage, MediaKind, MediaSet, SessionQueryProvider,
    read_media_asset,
};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_DISK_AUTOLOAD_SLOT,
    DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Model,
    autoload_basic_disk,
};
use serde_json::json;

use common::{
    local_aztec_challenge_d64_zip, local_bomb_jack_d64_zip, local_bruce_lee_d64_zip,
    local_rom_firmware_with_drive, press_button, press_key, screen_text_lines,
};

#[test]
#[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
fn real_d64_mount_bruce_lee_reports_disk_metadata() {
    let mut runtime =
        C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware_with_drive())
            .expect("local ROMs should construct a C64 runtime");
    let provider = C64SessionQueryProvider;
    let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
        .expect("local Bruce Lee D64 archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new("drive-8", MediaKind::Disk, &disk.bytes));

    runtime
        .load_media(&media)
        .expect("Bruce Lee D64 should mount into drive-8");

    assert_eq!(
        provider
            .query(&runtime, "drive8.attached")
            .expect("drive attachment query should not fail")
            .expect("drive attachment query should resolve")
            .value,
        json!(true)
    );
    assert_eq!(
        provider
            .query(&runtime, "drive8.disk.inserted")
            .expect("disk inserted query should not fail")
            .expect("disk inserted query should resolve")
            .value,
        json!(true)
    );
    assert_eq!(
        provider
            .query(&runtime, "drive8.disk.name")
            .expect("disk name query should not fail")
            .expect("disk name query should resolve")
            .value,
        json!("BRUCELEE")
    );
    assert_eq!(
        provider
            .query(&runtime, "drive8.disk.id")
            .expect("disk id query should not fail")
            .expect("disk id query should resolve")
            .value,
        json!("00")
    );
    assert_eq!(
        provider
            .query(&runtime, "drive8.disk.write_protected")
            .expect("disk write-protect query should not fail")
            .expect("disk write-protect query should resolve")
            .value,
        json!(true)
    );
}

#[test]
#[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
fn real_d64_autoload_bruce_lee_starts_drive_motion() {
    let firmware = local_rom_firmware_with_drive();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("local ROMs should construct a C64 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
        .expect("local Bruce Lee D64 archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_DISK_AUTOLOAD_SLOT,
        MediaKind::Disk,
        &disk.bytes,
    ));
    session
        .load_media(&media)
        .expect("Bruce Lee D64 should mount into drive-8");

    let autoload = autoload_basic_disk(
        &mut session,
        DEFAULT_DISK_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("disk autoload should reach SEARCHING FOR");
    assert_eq!(autoload.slot, DEFAULT_DISK_AUTOLOAD_SLOT);

    let start_head = session
        .query("drive8.head_position")
        .expect("head position query should not fail")
        .value
        .as_u64()
        .expect("head position should be numeric");

    session
        .run_frames(2_000)
        .expect("Bruce Lee disk autoload should advance the attached drive");

    let end_head = session
        .query("drive8.head_position")
        .expect("head position query should not fail")
        .value
        .as_u64()
        .expect("head position should stay numeric");

    assert!(
        end_head != start_head,
        "Bruce Lee disk autoload should move the 1541 head after SEARCHING FOR: start={start_head} end={end_head}"
    );
}

#[test]
#[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
fn real_d64_autoload_bruce_lee_reaches_loading() {
    let firmware = local_rom_firmware_with_drive();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("local ROMs should construct a C64 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
        .expect("local Bruce Lee D64 archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_DISK_AUTOLOAD_SLOT,
        MediaKind::Disk,
        &disk.bytes,
    ));
    session
        .load_media(&media)
        .expect("Bruce Lee D64 should mount into drive-8");

    autoload_basic_disk(
        &mut session,
        DEFAULT_DISK_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("disk autoload should reach SEARCHING FOR");

    let loading = session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
        .expect("Bruce Lee disk autoload should reach LOADING");
    assert_eq!(loading.needle, "LOADING");
}

#[test]
#[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
fn real_d64_autoload_bruce_lee_starts_after_run() {
    let firmware = local_rom_firmware_with_drive();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("local ROMs should construct a C64 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
        .expect("local Bruce Lee D64 archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_DISK_AUTOLOAD_SLOT,
        MediaKind::Disk,
        &disk.bytes,
    ));
    session
        .load_media(&media)
        .expect("Bruce Lee D64 should mount into drive-8");

    autoload_basic_disk(
        &mut session,
        DEFAULT_DISK_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("disk autoload should reach SEARCHING FOR");

    session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
        .expect("Bruce Lee disk autoload should reach LOADING");
    session
        .run_frames(2_400)
        .expect("Bruce Lee should return to BASIC after the initial disk stage");

    let ready_frame = session.machine().machine().framebuffer().to_vec();
    let ready_lines = screen_text_lines(&session);
    assert!(
        ready_lines[10].contains("READY."),
        "Bruce Lee should return to BASIC before RUN: {:?}",
        ready_lines[10]
    );
    assert!(
        !session
            .machine()
            .drive8()
            .expect("drive should stay attached")
            .motor_on()
    );

    for key in ["r", "u", "n", "return"] {
        press_key(&mut session, key, 3);
    }

    session
        .run_frames(1_800)
        .expect("Bruce Lee should reach its title screen after RUN");

    let title_lines = screen_text_lines(&session);
    assert_eq!(
        title_lines[0], "????\"QQQQ?????Q1????R????&???L??\"\"R1\"\"\"F",
        "Bruce Lee should replace the BASIC screen with title-screen data"
    );
    assert_eq!(
        title_lines[1], "DDDDDDDD????R????&???L??'QQQQQQQQQQQQQQZ",
        "Bruce Lee should show the stable title-screen top rows after RUN"
    );
    assert_ne!(
        session.machine().machine().framebuffer(),
        ready_frame.as_slice(),
        "Bruce Lee framebuffer should change after RUN starts the title"
    );
    assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 14);
    assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 0);

    let drive = session
        .machine()
        .drive8()
        .expect("drive should stay attached");
    assert!(drive.motor_on());
    assert!(drive.activity_led());
}

#[test]
#[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
fn real_d64_autoload_bruce_lee_advances_after_fire() {
    let firmware = local_rom_firmware_with_drive();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("local ROMs should construct a C64 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
        .expect("local Bruce Lee D64 archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_DISK_AUTOLOAD_SLOT,
        MediaKind::Disk,
        &disk.bytes,
    ));
    session
        .load_media(&media)
        .expect("Bruce Lee D64 should mount into drive-8");

    autoload_basic_disk(
        &mut session,
        DEFAULT_DISK_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("disk autoload should reach SEARCHING FOR");

    session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
        .expect("Bruce Lee disk autoload should reach LOADING");
    session
        .run_frames(2_400)
        .expect("Bruce Lee should return to BASIC after the initial disk stage");

    for key in ["r", "u", "n", "return"] {
        press_key(&mut session, key, 3);
    }

    session
        .run_frames(16_000)
        .expect("Bruce Lee should reach its stable title screen after RUN");

    let title_frame = session.machine().machine().framebuffer().to_vec();
    let title_lines = screen_text_lines(&session);
    assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 6);
    assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 6);
    assert!(
        !session
            .machine()
            .drive8()
            .expect("drive should stay attached")
            .motor_on()
    );

    press_button(&mut session, 2, "fire", 6);
    session
        .run_frames(3_000)
        .expect("Bruce Lee should advance beyond the title after joystick fire");

    let post_fire_lines = screen_text_lines(&session);
    assert_ne!(
        session.machine().machine().framebuffer(),
        title_frame.as_slice(),
        "Bruce Lee framebuffer should change after joystick fire"
    );
    assert_ne!(
        post_fire_lines, title_lines,
        "Bruce Lee screen codes should change after joystick fire"
    );
    assert_eq!(
        post_fire_lines[0], "X?????Q??I?Q???C?CL?D?@?@??P??P???????O?",
        "Bruce Lee should reach the stable post-title scene after joystick fire"
    );
    assert_eq!(
        post_fire_lines[24], "@????????@?????C??G? ??P??A?@??8?X????X?",
        "Bruce Lee should keep the expected lower HUD row after joystick fire"
    );
    assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 0);
    assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 12);

    let drive = session
        .machine()
        .drive8()
        .expect("drive should stay attached");
    assert!(!drive.motor_on());
    assert!(!drive.activity_led());
}

#[test]
#[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
fn real_d64_autoload_bruce_lee_responds_to_joystick_right_after_fire() {
    let firmware = local_rom_firmware_with_drive();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("local ROMs should construct a C64 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
        .expect("local Bruce Lee D64 archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_DISK_AUTOLOAD_SLOT,
        MediaKind::Disk,
        &disk.bytes,
    ));
    session
        .load_media(&media)
        .expect("Bruce Lee D64 should mount into drive-8");

    autoload_basic_disk(
        &mut session,
        DEFAULT_DISK_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("disk autoload should reach SEARCHING FOR");

    session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
        .expect("Bruce Lee disk autoload should reach LOADING");
    session
        .run_frames(2_400)
        .expect("Bruce Lee should return to BASIC after the initial disk stage");

    for key in ["r", "u", "n", "return"] {
        press_key(&mut session, key, 3);
    }

    session
        .run_frames(16_000)
        .expect("Bruce Lee should reach its stable title screen after RUN");

    press_button(&mut session, 2, "fire", 6);
    session
        .run_frames(3_000)
        .expect("Bruce Lee should advance beyond the title after joystick fire");

    let post_fire_frame = session.machine().machine().framebuffer().to_vec();
    let post_fire_lines = screen_text_lines(&session);
    assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 0);
    assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 12);

    press_button(&mut session, 2, "right", 30);
    session
        .run_frames(300)
        .expect("Bruce Lee should keep running after joystick-right input");

    assert_eq!(
        screen_text_lines(&session),
        post_fire_lines,
        "Bruce Lee keeps the same screen-code overlay while the gameplay scene animates"
    );
    assert_ne!(
        session.machine().machine().framebuffer(),
        post_fire_frame.as_slice(),
        "Bruce Lee framebuffer should respond to joystick-right after the post-title scene starts"
    );

    let drive = session
        .machine()
        .drive8()
        .expect("drive should stay attached");
    assert!(!drive.motor_on());
    assert!(!drive.activity_led());
}

#[test]
#[ignore = "requires local C64 ROMs, 1541 ROM, and Aztec Challenge D64 archive"]
fn real_d64_autoload_aztec_challenge_reaches_instruction_screen() {
    let firmware = local_rom_firmware_with_drive();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("local ROMs should construct a C64 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let disk = read_media_asset(&local_aztec_challenge_d64_zip(), MediaKind::Disk)
        .expect("local Aztec Challenge D64 archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_DISK_AUTOLOAD_SLOT,
        MediaKind::Disk,
        &disk.bytes,
    ));
    session
        .load_media(&media)
        .expect("Aztec Challenge D64 should mount into drive-8");

    autoload_basic_disk(
        &mut session,
        DEFAULT_DISK_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("disk autoload should reach SEARCHING FOR");

    session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
        .expect("Aztec Challenge disk autoload should reach LOADING");
    session
        .run_frames(4_000)
        .expect("Aztec Challenge should return to BASIC after the initial disk stage");

    let ready_lines = screen_text_lines(&session);
    assert!(
        ready_lines[10].contains("READY."),
        "Aztec Challenge should return to BASIC before RUN: {:?}",
        ready_lines[10]
    );

    for key in ["r", "u", "n", "return"] {
        press_key(&mut session, key, 3);
    }

    session
        .run_frames(5_000)
        .expect("Aztec Challenge should reach the player-select screen after RUN");
    press_key(&mut session, "f1", 3);
    session
        .run_frames(2_000)
        .expect("Aztec Challenge should reach its instruction screen after F1");

    let lines = screen_text_lines(&session);
    assert_eq!(
        lines[3], "  PLAYER 1                  PLAYER 2    ",
        "Aztec Challenge should show the player headers on its instruction screen"
    );
    assert_eq!(
        lines[17], "            THE GAUNTLET                ",
        "Aztec Challenge should identify the first phase after F1"
    );
    assert_eq!(
        lines[24], "      PRESS FIRE BUTTON TO START        ",
        "Aztec Challenge should show the readable start prompt after F1"
    );

    let drive = session
        .machine()
        .drive8()
        .expect("drive should stay attached");
    assert!(!drive.motor_on());
    assert!(!drive.activity_led());
    assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 0);
    assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 0);
}

#[test]
#[ignore = "requires local C64 ROMs, 1541 ROM, and Bomb Jack D64 archive"]
fn real_d64_autoload_bomb_jack_responds_to_port1_fire() {
    let firmware = local_rom_firmware_with_drive();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("local ROMs should construct a C64 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let disk = read_media_asset(&local_bomb_jack_d64_zip(), MediaKind::Disk)
        .expect("local Bomb Jack D64 archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_DISK_AUTOLOAD_SLOT,
        MediaKind::Disk,
        &disk.bytes,
    ));
    session
        .load_media(&media)
        .expect("Bomb Jack D64 should mount into drive-8");

    autoload_basic_disk(
        &mut session,
        DEFAULT_DISK_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("disk autoload should reach SEARCHING FOR");

    session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
        .expect("Bomb Jack disk autoload should reach LOADING");
    session
        .run_frames(50_000)
        .expect("Bomb Jack should settle into its title screen after the multi-stage loader");

    let title_frame = session.machine().machine().framebuffer().to_vec();
    let drive = session
        .machine()
        .drive8()
        .expect("drive should stay attached");
    assert!(!drive.motor_on());
    assert!(!drive.activity_led());
    assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 0);
    assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 6);

    press_button(&mut session, 1, "fire", 6);
    session
        .run_frames(4_000)
        .expect("Bomb Jack should advance after joystick port-1 fire");

    assert_ne!(
        session.machine().machine().framebuffer(),
        title_frame.as_slice(),
        "Bomb Jack framebuffer should change after joystick port-1 fire"
    );

    let drive = session
        .machine()
        .drive8()
        .expect("drive should stay attached");
    assert!(!drive.motor_on());
    assert!(!drive.activity_led());
    assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 0);
    assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 6);
}
