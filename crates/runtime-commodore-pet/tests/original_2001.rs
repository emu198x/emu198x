//! Original 320008 board: complete firmware identities, physical BASIC input.
use emu198x_shell::{HeadlessSession, InputEvent, MachineCore};
use runtime_commodore_pet::{Model, PetRuntime};
use sha2::{Digest, Sha256};
use std::path::Path;
fn rom(root: &Path, name: &str, hash: &str) -> Vec<u8> {
    let b = std::fs::read(root.join(name)).expect("identified original ROM");
    assert_eq!(b.len(), 2048);
    let actual = Sha256::digest(&b)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    assert_eq!(actual, hash, "complete original {name}");
    b
}
fn runtime(root: &Path, first: bool) -> PetRuntime {
    let mut b = if first {
        rom(
            root,
            "901447-01.h1",
            "6154dca842eab4f8b511752d4c4bd5dafd22ac393c2424947acddf237be8be68",
        )
    } else {
        rom(
            root,
            "901447-09.h1",
            "5554367c64982c4174389a02a8e3d982a914f271f2c152dd740464d43719bcd5",
        )
    };
    for (name, hash) in [
        (
            "901447-02.h5",
            "955ca8cf955bdf222b98ebdd6aa0d9d33740e80e21d7321d98b73e688a86c019",
        ),
        (
            "901447-03.h2",
            "e740b8bf8070baaa5bd91ff5c4e25437c6f7197fb7ac93e570282175e0a943c0",
        ),
        (
            "901447-04.h6",
            "000cfe6ec5ff8f2d60de101c28ee214876d3d4c03c5da6e124eecf958d60beef",
        ),
    ] {
        b.extend(rom(root, name, hash));
    }
    let e = rom(
        root,
        "901447-05.h3",
        "86dc50795651de9f99c886ea61bd412f5372fa4940eec7b550a9fa2d2dfffc13",
    );
    let mut k = rom(
        root,
        "901447-06.h4",
        "860ebc82d1a28465680d4741c328e45b7a25bf2de3cd25974b15a3ce8816be1c",
    );
    k.extend(rom(
        root,
        "901447-07.h7",
        "3534566ff5765274917a265b946504caa1dc325662b6e0dfd31cee15064f4d03",
    ));
    let c = rom(
        root,
        "901447-08.a2",
        "697390d9cd7da9e77d71935b0bf71d1181f624e0be802cbfb472803861443340",
    );
    PetRuntime::new(Model::Pet2001Original8k, k, b, e, c).expect("original socket map")
}
fn screen(r: &PetRuntime) -> Vec<String> {
    let m = r.machine().expect("original PET");
    (0..25u16)
        .map(|row| {
            (0..40u16)
                .map(|col| {
                    let c = m.peek(0x8000 + row * 40 + col) & 127;
                    if (1..=26).contains(&c) {
                        char::from(b'@' + c)
                    } else if (32..=63).contains(&c) {
                        char::from(c)
                    } else {
                        ' '
                    }
                })
                .collect()
        })
        .collect()
}
fn key(s: &mut HeadlessSession<PetRuntime>, name: &'static str) {
    s.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: true,
    });
    s.run_frames(5).expect("held physical key");
    s.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: false,
    });
    s.run_frames(5).expect("released physical key");
}
fn capture(s: &HeadlessSession<PetRuntime>, first: bool, label: &str) {
    let frame = s.latest_frame().expect("native frame");
    let hash = Sha256::digest(&frame.pixels)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let expected = match label {
        "boot" => "cc34f71b5e40a08bf17647994a458f315514bb3593aac4c8f7254f087762f930",
        "basic" => "03b8221cdf4501a8dd097d9012360dc2301db7e59ba77488dd3611817b3807ae",
        _ => panic!("unknown qualified waypoint"),
    };
    assert_eq!(hash, expected, "complete native {label} framebuffer");
    let mut changed = frame.pixels.clone();
    let middle = changed.len() / 2;
    changed[middle] ^= 1;
    let altered = Sha256::digest(&changed)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_ne!(altered, expected, "one changed pixel byte must reject");
    println!("CAPTURE first={first} {label}: {hash}");
    if let Some(root) = std::env::var_os("EMU198X_PET_ORIGINAL_ARTIFACTS") {
        use std::io::Write;
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).expect("artifact directory");
        let name = format!("2001-320008-{}-{label}", if first { "01" } else { "09" });
        let memory: Vec<u8> = (0..1000u16)
            .map(|i| s.machine().machine().expect("board").peek(0x8000 + i))
            .collect();
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(format!("{name}-screen.bin")))
            .expect("new screen capture only")
            .write_all(&memory)
            .expect("screen capture");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(format!(
                "2001-320008-{}-{label}.png",
                if first { "01" } else { "09" }
            )))
            .expect("new capture only")
            .write_all(&frame.png_bytes().expect("PNG"))
            .expect("capture");
    }
}
fn check(first: bool) {
    let root = std::path::PathBuf::from(
        std::env::var_os("EMU198X_PET_ORIGINAL_ROM_DIR").expect("original ROM directory"),
    );
    let mut s = HeadlessSession::new(runtime(&root, first), 1);
    s.run_frames(120).expect("bounded original startup");
    let boot = screen(s.machine()).join("\n");
    println!("BOOT first={first}\n{boot}");
    assert!(boot.contains("COMMODORE BASIC"), "original banner: {boot}");
    assert!(
        boot.contains("7167 BYTES FREE"),
        "physical 8 KiB fitting: {boot}"
    );
    assert!(boot.contains("READY."), "original editor ready: {boot}");
    capture(&s, first, "boot");
    for name in ["p", "r", "i", "n", "t", "3", "+", "4", "enter"] {
        key(&mut s, name);
    }
    s.run_frames(30).expect("BASIC execution");
    let result = screen(s.machine());
    println!("RESULT\n{}", result.join("\n"));
    assert!(
        result.iter().any(|r| r.contains("PRINT3+4")),
        "actual chiclet contacts: {result:?}"
    );
    assert!(
        result.iter().any(|r| r.trim() == "7"),
        "computed result: {result:?}"
    );
    capture(&s, first, "basic");
    let saved = s.machine().snapshot().expect("v6");
    let mut r = PetRuntime::blank(Model::Pet2001Original8k);
    r.restore(&saved).expect("current restore");
    assert_eq!(r.snapshot().expect("round trip"), saved);
    let mut restored = HeadlessSession::new(r, 1);
    s.run_frames(2).expect("original");
    restored.run_frames(2).expect("restored");
    assert_eq!(
        s.latest_frame().expect("picture").pixels,
        restored.latest_frame().expect("resumed picture").pixels
    );
    assert_eq!(
        s.machine().snapshot().expect("original state"),
        restored.machine().snapshot().expect("resumed state")
    );
    // Actual BASIC 1 PRG execution checks its different pointer/buffer map.
    let prg = [
        0x01, 0x04, 0x0e, 0x04, 10, 0, 0x97, b'3', b'2', b'7', b'6', b'8', b',', b'1', 0, 0, 0,
    ];
    s.machine_mut()
        .autoload_prg(&prg)
        .expect("BASIC 1 program injection");
    assert_eq!(
        s.machine().machine().expect("original PET").peek(0x020d),
        4,
        "original keyboard count"
    );
    s.run_frames(60).expect("bounded BASIC 1 RUN");
    assert_eq!(
        s.machine().machine().expect("original PET").peek(0x8000),
        1,
        "original BASIC executes POKE32768,1"
    );
}
#[test]
#[ignore = "FIXTURE: original 2001 ROM sockets at EMU198X_PET_ORIGINAL_ROM_DIR"]
fn initial_basic_one_boots_and_types() {
    check(true);
}
#[test]
#[ignore = "FIXTURE: original 2001 ROM sockets at EMU198X_PET_ORIGINAL_ROM_DIR"]
fn revised_c000_basic_one_boots_and_types() {
    check(false);
}

#[test]
#[ignore = "FIXTURE: original 2001 ROM sockets at EMU198X_PET_ORIGINAL_ROM_DIR"]
fn original_basic_one_confirms_every_printable_chiclet_contact() {
    let root = std::path::PathBuf::from(
        std::env::var_os("EMU198X_PET_ORIGINAL_ROM_DIR").expect("original ROM directory"),
    );
    let mut boot = HeadlessSession::new(runtime(&root, true), 1);
    boot.run_frames(120).expect("original boot");
    assert_eq!(
        screen(boot.machine())[4].trim(),
        "READY.",
        "original double-spaced startup"
    );
    let baseline = boot.machine().snapshot().expect("ready baseline");
    let mut cases: Vec<(String, u8)> =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!\"#$%&'()*+,-./:;<=>?@[\\]^"
            .bytes()
            .map(|ascii| {
                (
                    char::from(ascii).to_string(),
                    if ascii >= 64 { ascii - 64 } else { ascii },
                )
            })
            .collect();
    cases.push(("left-arrow".into(), 31));
    assert_eq!(cases.len(), 63, "all printable keycaps plus back arrow");
    for (name, expected) in cases {
        let mut r = PetRuntime::blank(Model::Pet2001Original8k);
        r.restore(&baseline).expect("fresh keyboard case");
        let mut s = HeadlessSession::new(r, 1);
        s.queue_input(InputEvent::Key {
            name: name.clone().into(),
            pressed: true,
        });
        s.run_frames(5).expect("press");
        s.queue_input(InputEvent::Key {
            name: name.clone().into(),
            pressed: false,
        });
        s.run_frames(5).expect("release");
        let actual = s
            .machine()
            .machine()
            .expect("original board")
            .peek(0x8000 + 5 * 40)
            & 127;
        assert_eq!(
            actual, expected,
            "original firmware decodes physical key {name}"
        );
    }
    let mut r = PetRuntime::blank(Model::Pet2001Original8k);
    r.restore(&baseline).expect("space control");
    let mut s = HeadlessSession::new(r, 1);
    key(&mut s, "space");
    key(&mut s, "p");
    assert_eq!(
        s.machine()
            .machine()
            .expect("board")
            .peek(0x8000 + 5 * 40 + 1)
            & 127,
        16,
        "SPACE advances before P"
    );
}
