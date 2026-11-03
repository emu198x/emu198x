//! Identified Sony HB-10P manufacturer ROM, physical keys and native restore.
use emu198x_shell::{HeadlessSession, InputEvent, MachineCore};
use runtime_msx::{Model, MsxRuntime};
use sha2::{Digest, Sha256};
const ROM_HASH: &str = "88eff516d7d706b8a4e5a512697444bd5a851e403c82006bcbfc537be3c47976";
fn identity(bytes: &[u8]) -> bool {
    bytes.len() == 32768
        && Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
            == ROM_HASH
}
fn text(r: &MsxRuntime) -> Vec<String> {
    let v = r
        .machine()
        .expect("Sony")
        .tms9918()
        .expect("fitted VDP family");
    let columns = if v.registers()[1] & 0x10 != 0 { 40 } else { 32 };
    let base = usize::from(v.registers()[2] & 15) << 10;
    v.vram()[base..base + columns * 24]
        .chunks(columns)
        .map(|row| {
            row.iter()
                .map(|&b| {
                    if b.is_ascii_graphic() || b == b' ' {
                        char::from(b)
                    } else {
                        ' '
                    }
                })
                .collect()
        })
        .collect()
}
fn native_text(s: &HeadlessSession<MsxRuntime>, rows: usize, label: &str) {
    let m = s.machine().machine().expect("Sony");
    let v = m.tms9918().expect("VDP");
    let r = v.registers();
    assert_eq!(r[1] & 0x58, 0x50, "documented 40-column text mode");
    assert_eq!(r[7], 0xf4, "white ink, blue background");
    let name_base = usize::from(r[2] & 15) * 1024;
    let pattern_base = usize::from(r[4] & 7) * 2048;
    let mut foreground = 0;
    // MSX Technical Data Book: text mode is 40 six-dot columns, eight
    // scanlines per glyph, using the upper six pattern bits. The offsets
    // below qualify the current PAL presentation, not analogue T6950 timing.
    assert_eq!((m.framebuffer_width(), m.framebuffer_height()), (278, 288));
    for row in 0..rows {
        for col in 0..40 {
            let code = usize::from(v.vram()[name_base + row * 40 + col]);
            for scan in 0..8 {
                let bits = v.vram()[pattern_base + code * 8 + scan];
                for bit in 0..6 {
                    let ink = bits & (0x80 >> bit) != 0;
                    foreground += usize::from(ink);
                    let expected = if ink { 0xffff_ffff } else { 0xff54_55ed };
                    assert_eq!(
                        m.framebuffer()[(51 + row * 8 + scan) * 278 + 19 + col * 6 + bit],
                        expected,
                        "{label}: row={row}, col={col}, scan={scan}, bit={bit}"
                    );
                }
            }
        }
    }
    assert!(foreground > 100, "native text includes real glyphs");
    if let Some(root) = std::env::var_os("EMU198X_SONY_BOOT_ARTIFACTS") {
        use std::io::Write;
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).expect("artifact directory");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(format!("sony-hb10p-{label}.png")))
            .expect("new capture only")
            .write_all(
                &s.latest_frame()
                    .expect("native frame")
                    .png_bytes()
                    .expect("PNG"),
            )
            .expect("capture");
    }
}
fn key(s: &mut HeadlessSession<MsxRuntime>, name: &'static str, shift: bool) {
    if shift {
        s.queue_input(InputEvent::Key {
            name: "shift".into(),
            pressed: true,
        });
    }
    s.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: true,
    });
    s.run_frames(4).expect("held physical key");
    s.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: false,
    });
    if shift {
        s.queue_input(InputEvent::Key {
            name: "shift".into(),
            pressed: false,
        });
    }
    s.run_frames(4).expect("released physical key");
}
#[test]
#[ignore = "FIXTURE: identified original HB-10P ROM at EMU198X_MSX_HB10P_ROM"]
fn original_hb10p_boots_accepts_basic_and_resumes_native_output() {
    let path = std::env::var_os("EMU198X_MSX_HB10P_ROM").expect("explicit original Sony ROM path");
    let bytes = std::fs::read(path).expect("original ROM");
    assert!(identity(&bytes), "complete original HB-10P ROM identity");
    let mut changed = bytes.clone();
    changed[16384] ^= 1;
    assert!(!identity(&changed), "one-byte mutation rejects");
    assert!(!identity(&bytes[..16384]), "truncation rejects");
    let runtime = MsxRuntime::new(Model::SonyHb10p, bytes).expect("Sony firmware");
    let mut s = HeadlessSession::new(runtime, 1);
    s.run_frames(400).expect("bounded original startup");
    let boot = text(s.machine()).join("\n");
    println!(
        "BOOT\n{boot}\nREGS {:?}",
        s.machine()
            .machine()
            .expect("Sony")
            .tms9918()
            .expect("VDP")
            .registers()
    );
    assert!(
        boot.contains("MSX BASIC version 1.0"),
        "manufacturer BASIC: {boot}"
    );
    assert!(
        boot.lines().any(|r| r.trim() == "Ok"),
        "ready prompt: {boot}"
    );
    native_text(&s, 3, "boot");
    for name in ["p", "r", "i", "n", "t", "space", "3"] {
        key(&mut s, name, false);
    }
    key(&mut s, "=", true);
    key(&mut s, "4", false);
    let echo = text(s.machine()).join("\n");
    assert!(
        echo.to_ascii_uppercase().contains("PRINT 3+4"),
        "keyboard echo: {echo}"
    );
    key(&mut s, "enter", false);
    s.run_frames(10).expect("BASIC execution");
    let result = text(s.machine());
    println!("RESULT\n{}", result.join("\n"));
    assert!(
        result.iter().any(|r| r.trim() == "7"),
        "computed result: {result:?}"
    );
    native_text(&s, 6, "basic");
    let saved = s.machine().snapshot().expect("v11 state");
    let mut restored = MsxRuntime::blank(Model::SonyHb10p);
    restored.restore(&saved).expect("current Sony restore");
    assert_eq!(restored.snapshot().expect("round trip"), saved);
    let mut replay = HeadlessSession::new(restored, 1);
    s.run_frames(2).expect("original continuation");
    replay.run_frames(2).expect("restored continuation");
    assert_eq!(
        s.latest_frame().expect("original").pixels,
        replay.latest_frame().expect("restored").pixels
    );
    assert_eq!(
        s.machine().snapshot().expect("original state"),
        replay.machine().snapshot().expect("restored state")
    );
}
