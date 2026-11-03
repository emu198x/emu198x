//! Original shared TC2068/TS2068 firmware: real boot and keyboard BASIC.
use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, MachineCore,
};
use runtime_sinclair_zx_spectrum::{Model, SpectrumLiveAccess, SpectrumRuntimeKind};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
const MAIN: &str = "6338f94b2ee5000b81a64a4f055b8f1d2db6e669e52cafc11233ff63896fae13";
const EXROM: &str = "e52e861bd30c144f174a596233c6f3e5804cff170fdc50d44ba61a1683b8174c";
fn identity(bytes: &[u8], size: usize, hash: &str) -> bool {
    bytes.len() == size
        && Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
            == hash
}
fn screen(runtime: &SpectrumRuntimeKind, rom: &[u8]) -> Vec<String> {
    (0..24u16)
        .map(|row| {
            (0..32u16)
                .map(|col| {
                    let glyph: Vec<_> = (0..8u16)
                        .map(|i| {
                            let y = row * 8 + i;
                            runtime.read_byte(
                                0x4000
                                    | ((y & 0xc0) << 5)
                                    | ((y & 7) << 8)
                                    | ((y & 0x38) << 2)
                                    | col,
                            )
                        })
                        .collect();
                    (32..128u8)
                        .find(|c| {
                            rom[0x3c00 + usize::from(*c) * 8..0x3c00 + usize::from(*c) * 8 + 8]
                                == glyph
                        })
                        .map_or('?', char::from)
                })
                .collect()
        })
        .collect()
}
fn assert_native_screen(session: &HeadlessSession<SpectrumRuntimeKind>, label: &str) {
    let frame = session.latest_frame().expect("completed frame");
    assert_eq!(frame.format, emu198x_shell::PixelFormat::Indexed8);
    assert_eq!((frame.width, frame.height), (704, 296));
    let machine = session.machine();
    for y in 0..192u16 {
        for column in 0..32u16 {
            let bitmap = machine.read_byte(
                0x4000 | ((y & 0xc0) << 5) | ((y & 7) << 8) | ((y & 0x38) << 2) | column,
            );
            let attr = machine.read_byte(0x5800 + (y / 8) * 32 + column);
            assert_eq!(
                attr & 0x80,
                0,
                "reference BASIC screen has no flashing attributes"
            );
            let bright = (attr >> 3) & 8;
            let ink = (attr & 7) | bright;
            let paper = ((attr >> 3) & 7) | bright;
            for bit in 0..8u16 {
                let expected = if bitmap & (0x80 >> bit) != 0 {
                    ink
                } else {
                    paper
                };
                let offset = usize::from(y + 48) * 704 + 96 + usize::from(column * 8 + bit) * 2;
                assert_eq!(
                    &frame.pixels[offset..offset + 2],
                    &[expected, expected],
                    "{label}: native y={y}, column={column}, bit={bit}"
                );
            }
        }
    }
    if let Some(root) = std::env::var_os("EMU198X_TIMEX_BOOT_ARTIFACTS") {
        use std::io::Write;
        let root = PathBuf::from(root);
        std::fs::create_dir_all(&root).expect("artifact directory");
        let file = root.join(format!(
            "{}-{label}.png",
            machine.profile().machine_id.as_str()
        ));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(file)
            .expect("new capture only")
            .write_all(&frame.png_bytes().expect("original native capture"))
            .expect("capture");
    }
}
fn key(session: &mut HeadlessSession<SpectrumRuntimeKind>, name: &'static str, symbol: bool) {
    if symbol {
        session.queue_input(InputEvent::Key {
            name: "symbol".into(),
            pressed: true,
        });
    }
    session.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: true,
    });
    session.run_frames(4).expect("held physical key");
    session.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: false,
    });
    if symbol {
        session.queue_input(InputEvent::Key {
            name: "symbol".into(),
            pressed: false,
        });
    }
    session.run_frames(4).expect("released physical key");
}
fn check(model: Model) {
    let root = std::env::var_os("EMU198X_TIMEX_REFERENCE_ROM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join(".emu198x/roms/timex-ts2068")
        });
    // Fuse distributes the same original pair for both models. Independently
    // checked against MAME ts2068_h/ts2068_x full SHA-1 identities; no ROM bundled.
    let main = std::fs::read(root.join("ts2068.rom")).expect("original main ROM");
    let exrom = std::fs::read(root.join("exrom.rom")).expect("original extension ROM");
    assert!(
        identity(&main, 16384, MAIN),
        "complete main-ROM identity before execution"
    );
    assert!(
        identity(&exrom, 8192, EXROM),
        "complete EXROM identity before execution"
    );
    assert!(!identity(&exrom, 16384, MAIN), "wrong-ROM control");
    let mut mutated = main.clone();
    mutated[8192] ^= 1;
    assert!(
        !identity(&mutated, 16384, MAIN),
        "one changed firmware byte must reject"
    );
    let mut fw = FirmwareSet::new();
    fw.push(FirmwareImage::new("timex-ts2068-rom-0", &main));
    fw.push(FirmwareImage::new("timex-ts2068-rom-1", &exrom));
    let rt = SpectrumRuntimeKind::from_firmware(model, &fw).expect("selected original firmware");
    let mut s = HeadlessSession::new(rt, 1);
    s.run_frames(240).expect("bounded native boot");
    let boot = screen(s.machine(), &main).join("\n");
    assert!(
        boot.contains("1982 Sinclair Research Ltd"),
        "normal startup screen: {boot}"
    );
    assert!(
        boot.contains("1983 Timex Computer Corp"),
        "normal Timex startup screen: {boot}"
    );
    assert_native_screen(&s, "boot");
    for (name, symbol) in [("p", false), ("3", false), ("k", true), ("4", false)] {
        key(&mut s, name, symbol);
    }
    let echo = screen(s.machine(), &main).join("\n");
    assert!(
        echo.contains("PRINT 3+4"),
        "physical keyboard must enter the command: {echo}"
    );
    key(&mut s, "enter", false);
    s.run_frames(10).expect("BASIC execution");
    let result = screen(s.machine(), &main);
    assert_eq!(result[0].trim(), "7", "computed BASIC result");
    assert!(
        result[23].contains("0 OK, 0:1"),
        "successful execution: {result:?}"
    );
    assert_native_screen(&s, "basic");
    let saved = s.machine().snapshot().expect("live boot snapshot");
    let mut restored = SpectrumRuntimeKind::from_firmware(model, &fw).expect("same board");
    restored.restore(&saved).expect("restore");
    assert_eq!(restored.snapshot().expect("round trip"), saved);
    let mut replay = HeadlessSession::new(restored, 1);
    s.run_frames(2).expect("original continuation");
    replay.run_frames(2).expect("restored continuation");
    assert_eq!(
        s.latest_frame().expect("original").pixels,
        replay.latest_frame().expect("restored").pixels
    );
    assert_eq!(
        s.machine().snapshot().expect("original"),
        replay.machine().snapshot().expect("restored")
    );
    println!(
        "{model:?}: exact ROMs; both banners; typed PRINT 3+4 = 7; exact snapshot continuation; wrong-ROM/mutation controls"
    );
}
fn on_stack(model: Model) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || check(model))
        .expect("test thread")
        .join()
        .expect("test result");
}
#[test]
#[ignore = "FIXTURE: original TS2068/TC2068 ROM pair in EMU198X_TIMEX_REFERENCE_ROM_DIR or conventional TS2068 directory"]
fn ts2068_boots_and_executes_typed_basic() {
    on_stack(Model::TimexTS2068);
}
#[test]
#[ignore = "FIXTURE: original TS2068/TC2068 ROM pair in EMU198X_TIMEX_REFERENCE_ROM_DIR or conventional TS2068 directory"]
fn tc2068_boots_and_executes_typed_basic() {
    on_stack(Model::TimexTC2068);
}
