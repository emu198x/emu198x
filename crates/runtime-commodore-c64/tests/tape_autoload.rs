//! Real TAP autoload regressions for the C64 runtime.
//!
//! Every test is `#[ignore]`'d — they require local C64 ROMs at
//! `~/.emu198x/roms/commodore-c64/` and the named TAP archives under
//! `~/Projects/Emu198x-Unclean/Reference/commodore/c64/`.

mod common;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{
    HeadlessSession, InputEvent, MediaImage, MediaKind, MediaSet, read_media_asset,
};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    DEFAULT_TAPE_AUTOLOAD_SLOT, DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES, Model, autoload_basic_tape,
};

use common::{
    local_ghostbusters_tap_zip, local_rom_firmware, local_thing_on_a_spring_tap_zip,
    local_thinker_tap_zip, local_thomas_tap_zip, screen_text_lines, wait_for_screen_line_contains,
};

#[test]
#[ignore = "requires local C64 ROMs and Thinker TAP archive"]
fn real_tap_autoload_reaches_post_load_ready() {
    let firmware = local_rom_firmware();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("real PAL C64 firmware should construct a runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let tape = read_media_asset(&local_thinker_tap_zip(), MediaKind::Tape)
        .expect("local Thinker TAP archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        MediaKind::Tape,
        &tape.bytes,
    ));
    session
        .load_media(&media)
        .expect("local Thinker TAP should insert");

    let autoload = autoload_basic_tape(
        &mut session,
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("autoload should reach PRESS PLAY ON TAPE and start transport");
    assert_eq!(autoload.slot, DEFAULT_TAPE_AUTOLOAD_SLOT);

    let found = session
        .wait_for_query_text_contains("screen.text.lines", "FOUND THINKER", 1500)
        .expect("Thinker tape should reach FOUND banner");
    assert_eq!(found.line, Some(12));

    let loading = session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 3000)
        .expect("Thinker tape should reach LOADING banner");
    assert_eq!(loading.line, Some(13));

    wait_for_screen_line_contains(&mut session, 14, "READY.", 5000);
    let lines = screen_text_lines(&session);
    assert!(
        lines[12].contains("FOUND THINKER"),
        "post-load screen should retain FOUND banner: {:?}",
        lines[12]
    );
    assert!(
        lines[13].contains("LOADING"),
        "post-load screen should retain LOADING banner: {:?}",
        lines[13]
    );
    assert!(
        lines[14].contains("READY."),
        "post-load screen should reach READY. line: {:?}",
        lines[14]
    );
    assert!(session.machine().machine().tape_is_playing());
}

#[test]
#[ignore = "requires local C64 ROMs and Thomas the Tank Engine TAP archive"]
fn real_tap_autoload_reaches_thomas_loading_ready_banner() {
    let firmware = local_rom_firmware();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("real PAL C64 firmware should construct a runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let tape = read_media_asset(&local_thomas_tap_zip(), MediaKind::Tape)
        .expect("local Thomas TAP archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        MediaKind::Tape,
        &tape.bytes,
    ));
    session
        .load_media(&media)
        .expect("local Thomas TAP should insert");

    let autoload = autoload_basic_tape(
        &mut session,
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("autoload should reach PRESS PLAY ON TAPE and start transport");
    assert_eq!(autoload.slot, DEFAULT_TAPE_AUTOLOAD_SLOT);

    let found = session
        .wait_for_query_text_contains("screen.text.lines", "FOUND THOMAS", 1500)
        .expect("Thomas tape should reach FOUND banner");
    assert_eq!(found.line, Some(12));

    let loading = session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 3000)
        .expect("Thomas tape should reach LOADING banner");
    assert_eq!(loading.line, Some(13));

    wait_for_screen_line_contains(&mut session, 14, "READY.", 3000);
    let lines = screen_text_lines(&session);
    assert!(
        lines[12].contains("FOUND THOMAS"),
        "Thomas screen should retain FOUND banner: {:?}",
        lines[12]
    );
    assert!(
        lines[13].contains("LOADING"),
        "Thomas screen should retain LOADING banner: {:?}",
        lines[13]
    );
    assert!(
        lines[14].contains("READY."),
        "Thomas screen should reach READY. line: {:?}",
        lines[14]
    );
    assert!(session.machine().machine().tape_is_playing());
}

#[test]
#[ignore = "requires local C64 ROMs and Ghostbusters TAP archive"]
fn real_tap_autoload_ghostbusters_reaches_later_loader_state() {
    let firmware = local_rom_firmware();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("real PAL C64 firmware should construct a runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let tape = read_media_asset(&local_ghostbusters_tap_zip(), MediaKind::Tape)
        .expect("local Ghostbusters TAP archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        MediaKind::Tape,
        &tape.bytes,
    ));
    session
        .load_media(&media)
        .expect("local Ghostbusters TAP should insert");

    let autoload = autoload_basic_tape(
        &mut session,
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("autoload should reach PRESS PLAY ON TAPE and start transport");
    assert_eq!(autoload.slot, DEFAULT_TAPE_AUTOLOAD_SLOT);

    let found = session
        .wait_for_query_text_contains("screen.text.lines", "FOUND MAIN", 1500)
        .expect("Ghostbusters tape should reach FOUND MAIN banner");
    assert_eq!(found.line, Some(12));

    session
        .run_frames(25_000)
        .expect("Ghostbusters loader should run past the first-stage banner");

    let lines = screen_text_lines(&session);
    assert!(
        !lines.iter().any(|line| line.contains("FOUND MAIN")),
        "Ghostbusters should move past FOUND MAIN: {:?}",
        lines
    );
    assert!(
        !lines.iter().any(|line| line.contains("LOADING")),
        "Ghostbusters should move past LOADING banner: {:?}",
        lines
    );

    let machine = session.machine().machine();
    assert!(
        machine.memory().is_io_visible(),
        "Ghostbusters loader should keep CIA/VIC I/O visible in the later state"
    );
    assert_eq!(
        machine.cia2().timer_a_latch(),
        280,
        "Ghostbusters later loader should have programmed CIA2 Timer A"
    );
    assert!(!machine.tape_is_playing());
    assert!(!machine.tape_motor_on());
    assert!(
        machine.tape_pulse_index() > 460_000,
        "Ghostbusters should consume almost the entire TAP before the later state"
    );
}

#[test]
#[ignore = "requires local C64 ROMs and Thing on a Spring TAP archive"]
fn real_tap_autoload_thing_on_a_spring_reaches_menu() {
    let firmware = local_rom_firmware();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("real PAL C64 firmware should construct a runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let tape = read_media_asset(&local_thing_on_a_spring_tap_zip(), MediaKind::Tape)
        .expect("local Thing on a Spring TAP archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        MediaKind::Tape,
        &tape.bytes,
    ));
    session
        .load_media(&media)
        .expect("local Thing on a Spring TAP should insert");

    let autoload = autoload_basic_tape(
        &mut session,
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("autoload should reach PRESS PLAY ON TAPE and start transport");
    assert_eq!(autoload.slot, DEFAULT_TAPE_AUTOLOAD_SLOT);

    session
        .run_frames(25_000)
        .expect("Thing on a Spring should reach its post-load menu state");

    let lines = screen_text_lines(&session);
    assert!(
        lines[16].contains("600-MICRO"),
        "Thing on a Spring should show the score table: {:?}",
        lines[16]
    );
    assert!(
        lines[17].contains("500-PROJECTS"),
        "Thing on a Spring should show the score table: {:?}",
        lines[17]
    );
    assert!(
        lines[20].contains("200-GREMLIN"),
        "Thing on a Spring should show the publisher line: {:?}",
        lines[20]
    );
    assert!(
        lines[17].contains("RIGHT - X"),
        "Thing on a Spring should show the control legend: {:?}",
        lines[17]
    );
    assert!(
        lines[20].contains("FIRE  - SPACE"),
        "Thing on a Spring should show the fire control: {:?}",
        lines[20]
    );

    let machine = session.machine().machine();
    assert!(!machine.tape_is_playing());
    assert_eq!(
        machine.tape_pulse_index(),
        machine.tape_pulse_count(),
        "Thing on a Spring should consume the full TAP by the menu state"
    );
}

#[test]
#[ignore = "requires local C64 ROMs and Thing on a Spring TAP archive"]
fn real_tap_autoload_thing_on_a_spring_starts_after_space() {
    let firmware = local_rom_firmware();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("real PAL C64 firmware should construct a runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let tape = read_media_asset(&local_thing_on_a_spring_tap_zip(), MediaKind::Tape)
        .expect("local Thing on a Spring TAP archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        MediaKind::Tape,
        &tape.bytes,
    ));
    session
        .load_media(&media)
        .expect("local Thing on a Spring TAP should insert");

    autoload_basic_tape(
        &mut session,
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("autoload should reach PRESS PLAY ON TAPE and start transport");

    session
        .run_frames(25_000)
        .expect("Thing on a Spring should reach its post-load menu state");

    let menu_lines = screen_text_lines(&session);
    assert!(
        menu_lines[16].contains("600-MICRO"),
        "Thing on a Spring should show the score table before start: {:?}",
        menu_lines[16]
    );
    let menu_frame = session.machine().machine().framebuffer().to_vec();

    session.queue_input(InputEvent::Key {
        name: "space".into(),
        pressed: true,
    });
    // Hold SPACE for a human-realistic duration. The game's own scan loop
    // debounces across IRQ scans, so a 3-frame tap is phase-sensitive — it
    // happened to register with the pre-pipeline CIA timing and stopped
    // with the cycle-exact timers (#17). ~0.6 s is robust.
    session
        .run_frames(30)
        .expect("Thing on a Spring should advance with SPACE held");
    session.queue_input(InputEvent::Key {
        name: "space".into(),
        pressed: false,
    });
    session
        .run_frames(480)
        .expect("Thing on a Spring should settle into its started state");

    let started_lines = screen_text_lines(&session);
    assert_eq!(
        started_lines[0], " @A!!!!!!!!!!!!DE  JKLMN  @A!!!!!!!!!DE ",
        "Thing on a Spring should replace the menu banner after SPACE"
    );
    assert_eq!(
        started_lines[8], " HI############LM QRSTUVW HI#########LM ",
        "Thing on a Spring should reach its stable started screen after SPACE"
    );
    assert!(
        !started_lines[16].contains("600-MICRO"),
        "Thing on a Spring should leave the score table after SPACE: {:?}",
        started_lines[16]
    );
    assert_ne!(
        session.machine().machine().framebuffer(),
        menu_frame.as_slice(),
        "Thing on a Spring framebuffer should change after SPACE starts the title"
    );

    let machine = session.machine().machine();
    assert!(!machine.tape_is_playing());
    assert_eq!(
        machine.tape_pulse_index(),
        machine.tape_pulse_count(),
        "Thing on a Spring should still have consumed the full TAP after SPACE"
    );
}
