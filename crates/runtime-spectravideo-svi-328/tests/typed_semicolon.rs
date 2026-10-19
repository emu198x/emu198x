//! Character entry must preserve punctuation in the program BASIC stores.

use std::{env, fs, path::PathBuf};

use emu198x_shell::{HeadlessScript, HeadlessSession};
use runtime_spectravideo_svi_328::{Model, Svi328Runtime};

#[test]
#[ignore = "FIXTURE: needs the 32 KB SVI-328 BIOS/BASIC — run with --ignored"]
fn typed_semicolon_survives_inside_a_basic_string() {
    let path = env::var_os("EMU198X_SVI_328_BIOS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").expect("HOME"))
                .join(".emu198x/roms/spectravideo-svi-328/svi-328.rom")
        });
    let bios = fs::read(path).expect("read SVI-328 firmware");
    let model = Model::Svi328Ntsc;
    let runtime = Svi328Runtime::new(model, bios).expect("construct SVI-328");
    let mut session = HeadlessSession::new(runtime, model.frame_ticks());
    session.run_frames(900).expect("boot firmware");
    assert_ne!(
        session
            .machine()
            .machine()
            .expect("machine")
            .vdp()
            .registers()[1]
            & 0x40,
        0,
        "BASIC display must be running before typing"
    );
    let script = HeadlessScript::from_json_str(
        r#"[{"action":"type_string","text":"10 PRINT \";\"\n20 END\n"}]"#,
    )
    .expect("parse keyboard script");
    script
        .execute(&mut session)
        .expect("enter BASIC through keyboard");
    let machine = session
        .machine()
        .machine()
        .expect("firmware-backed machine");
    let text: Vec<u8> = (0x8000..0x8100).map(|addr| machine.peek(addr)).collect();
    assert!(
        text.windows(3).any(|bytes| bytes == b"\";\""),
        "The string the reader typed must contain a semicolon, not a colon: {text:?}"
    );
}
