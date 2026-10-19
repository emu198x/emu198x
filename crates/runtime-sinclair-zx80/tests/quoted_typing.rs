//! The ROM, rather than the legend table, decides whether typed quotes work.
use std::{env, fs, path::PathBuf};

use emu198x_shell::{FamilyRuntime, HeadlessSession, ScriptStep};
use runtime_sinclair_zx80::{Model, Zx80Runtime};

#[test]
#[ignore = "FIXTURE: needs the stock 4 KiB ZX80 ROM — run with --ignored"]
fn typed_quotes_print_a_string_through_the_stock_rom() {
    let path = env::var_os("EMU198X_ZX80_ROM").map_or_else(
        || {
            PathBuf::from(env::var_os("HOME").expect("HOME"))
                .join(".emu198x/roms/sinclair-zx80/zx80.rom")
        },
        PathBuf::from,
    );
    let rom = fs::read(path).expect("provide the ZX80 ROM");
    let runtime = Zx80Runtime::new(Model::Zx80, rom).expect("runtime");
    let ticks = runtime.native_frame_ticks();
    let mut session = HeadlessSession::new(runtime, ticks);
    session.run_frames(150).expect("ROM boot");
    // PRINT is the O keyword on the ZX80. TypeString handles the literal
    // quotes via the public KeyboardTarget, just as MCP and scripts do.
    ScriptStep::PressKey {
        key: "o".into(),
        hold_frames: Some(10),
    }
    .execute_collect(&mut session)
    .expect("PRINT keyword");
    session.run_frames(30).expect("settle PRINT");
    // The stock ROM needs a release window between distinct keys too.
    for text in ["\"", "X", "\""] {
        ScriptStep::TypeString {
            text: text.into(),
            hold_frames: Some(10),
            settle_frames: Some(30),
        }
        .execute_collect(&mut session)
        .expect("one literal character");
    }
    ScriptStep::PressKey {
        key: "enter".into(),
        hold_frames: Some(10),
    }
    .execute_collect(&mut session)
    .expect("execute PRINT");
    session.run_frames(60).expect("finish PRINT");
    let machine = session.machine().machine().expect("loaded");
    let display = u16::from_le_bytes([machine.peek(0x400c), machine.peek(0x400d)]);
    assert_eq!(machine.peek(display), 118, "D_FILE begins with NEWLINE");
    assert_eq!(
        machine.peek(display + 1),
        61,
        "PRINT must render X, not a malformed edit line"
    );
    assert_eq!(
        machine.peek(display + 2),
        118,
        "one-character output ends at NEWLINE"
    );
}
