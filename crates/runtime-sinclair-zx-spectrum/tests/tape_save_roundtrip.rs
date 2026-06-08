//! Real tape SAVE for the 48K Spectrum runtime.
//!
//! `#[ignore]`'d — requires the local 48K ROM at
//! `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`. Proves the full SAVE
//! capture path end to end: the real ROM's `SA-BYTES` routine toggles the MIC
//! line, the recorder captures and decodes that signal, and the flush turns it
//! into a standard `.tap` the loader's own parser accepts.

use common_sinclair_zx_spectrum::timing::TIMING_48K;
use emu198x_shell::HeadlessSession;
use format_sinclair_zx_spectrum_tap::parse_tap;
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Spectrum48kRuntime, SpectrumSessionQueryProvider, tap_key,
    tap_symbol_combo,
};

fn local_48k_rom() -> Vec<u8> {
    let path = std::path::PathBuf::from(std::env::var("HOME").expect("HOME for the local ROM"))
        .join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    std::fs::read(&path).expect("local 48K ROM should exist")
}

#[test]
#[ignore = "requires local 48K ROM at ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
fn save_captures_a_reloadable_tap() {
    let runtime = Spectrum48kRuntime::from_rom_bytes(&local_48k_rom())
        .expect("local 48K ROM should construct a runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    session
        .wait_for_boot(DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
        .expect("the 48K ROM should boot to its BASIC prompt");

    // Enter `10 REM`. At the start of a line the cursor is K, so the `e` key
    // enters the REM keyword (not the letter).
    tap_key(&mut session, "1").expect("type 1");
    tap_key(&mut session, "0").expect("type 0");
    tap_key(&mut session, "e").expect("type REM");
    tap_key(&mut session, "enter").expect("enter the line");

    // `SAVE "A"`: S enters the SAVE keyword (K cursor); the cursor is then L, so
    // the quotes (SYMBOL SHIFT + P) and the name letter are entered literally.
    tap_key(&mut session, "s").expect("type SAVE");
    tap_symbol_combo(&mut session, "p").expect("open quote");
    tap_key(&mut session, "a").expect("type the file name");
    tap_symbol_combo(&mut session, "p").expect("close quote");
    tap_key(&mut session, "enter").expect("run the SAVE command");

    // The ROM now shows "Start tape, then press any key." and waits. Clear any
    // pre-SAVE MIC noise so only the SAVE signal is captured, then press a key
    // to start it and let the pilot + header + data + pilot + data lay down.
    session.run_frames(20).expect("let the SAVE prompt appear");
    session.machine_mut().clear_tape_recording();
    tap_key(&mut session, "enter").expect("press a key to start the SAVE");
    session
        .run_frames(800)
        .expect("lay down the full SAVE signal");

    // Decode the captured MIC signal into a .tap and confirm it is a valid
    // standard program save: a header block (type 0) naming the program, then a
    // data block.
    let tap = session
        .machine()
        .flush_tape_image()
        .expect("a SAVE should have produced a tape image");
    let blocks = parse_tap(&tap).expect("the flushed image should parse as a .tap");

    assert!(
        blocks.len() >= 2,
        "a program SAVE should produce a header and a data block; got {} block(s)",
        blocks.len()
    );

    let header = &blocks[0];
    assert!(header.is_header(), "the first block should be a header");
    assert_eq!(
        header.data.len(),
        17,
        "a program header payload is 17 bytes; got {}",
        header.data.len()
    );
    assert_eq!(
        header.data.first(),
        Some(&0),
        "the header type byte should be 0 (Program)"
    );
    assert_eq!(
        header.data.get(1).map(u8::to_ascii_lowercase),
        Some(b'a'),
        "the header file name should start with the typed 'a'"
    );
    assert_eq!(
        blocks[1].flag, 0xFF,
        "the second block should be a data block (flag 0xFF)"
    );
}
