//! Real-media SVI cassette regression.
//!
//! This test deliberately keeps the copyrighted ROM and cassette outside the
//! repository. `EMU198X_SVI_328_CAS` may name either a loose `.cas` image or
//! the per-title TOSEC `.zip`; the shell expands the latter before the runtime
//! sees it. The known-good reference is TOSEC's
//! `Mini Golf (1984)(Spectravideo)[CLOAD + RUN].zip`, whose sole member is an
//! 8,126-byte CAS with SHA-256
//! `2655f460ba707e9f9aa37bf80b277f6667041fdbca4e97c1ea64c93302c53f86`.
//!
//! Run with:
//! ```text
//! EMU198X_SVI_328_CAS=/path/to/title.zip cargo test --release \
//!   -p runtime-spectravideo-svi-328 --test real_cassette -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession, InputEvent, MediaImage, MediaKind, MediaSet, read_media_asset,
};
use runtime_spectravideo_svi_328::{Model, Svi328Runtime};

fn required_path(variable: &str) -> PathBuf {
    std::env::var(variable).map_or_else(
        |_| panic!("set {variable} to the required local fixture"),
        PathBuf::from,
    )
}

fn press_key(session: &mut HeadlessSession<Svi328Runtime>, name: &str) {
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: true,
    });
    session.run_frames(3).expect("run with key held");
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: false,
    });
    session.run_frames(8).expect("run after key release");
}

fn type_command(session: &mut HeadlessSession<Svi328Runtime>, command: &str) {
    for character in command.chars() {
        let name = match character {
            '\n' => "return".to_owned(),
            ' ' => "space".to_owned(),
            character if character.is_ascii_alphabetic() => character.to_string(),
            _ => panic!("unsupported test character {character:?}"),
        };
        press_key(session, &name);
    }
}

fn foreground_pixels(frame: &[u32]) -> usize {
    let mut counts = std::collections::HashMap::new();
    for &pixel in frame {
        *counts.entry(pixel).or_insert(0usize) += 1;
    }
    frame.len() - counts.values().copied().max().unwrap_or_default()
}

#[test]
#[ignore = "FIXTURE: needs the SVI-328 BIOS and a real CAS/TOSEC title ZIP"]
fn firmware_loads_and_runs_a_real_tosec_cassette() {
    let bios_path = std::env::var("EMU198X_SVI_328_BIOS").map_or_else(
        |_| {
            let home = std::env::var("HOME").expect("HOME should locate the default BIOS");
            PathBuf::from(home).join(".emu198x/roms/spectravideo-svi-328/svi-328.rom")
        },
        PathBuf::from,
    );
    let bios = std::fs::read(&bios_path).expect("read the 32 KB SVI-328 BIOS");
    let loaded = read_media_asset(&required_path("EMU198X_SVI_328_CAS"), MediaKind::Tape)
        .expect("read the loose CAS or its per-title ZIP");
    assert_eq!(
        loaded.bytes.len(),
        8_126,
        "this evidence gate is pinned to the known Mini Golf CAS"
    );
    assert!(
        loaded
            .archive_member
            .as_deref()
            .is_none_or(|member| member.ends_with(".cas")),
        "a ZIP input should resolve to its CAS member"
    );

    let runtime = Svi328Runtime::new(Model::Svi328Ntsc, bios).expect("construct the runtime");
    // The runtime advances one complete native frame for any positive target,
    // so one tick is the exact headless-session pacing unit here.
    let mut session = HeadlessSession::new(runtime, 1);
    session.run_frames(300).expect("boot to SV-BASIC");
    let boot_foreground = foreground_pixels(
        session
            .machine()
            .machine()
            .expect("machine should be live")
            .framebuffer(),
    );
    let boot_frame = session
        .machine()
        .machine()
        .expect("machine should be live")
        .framebuffer()
        .to_vec();

    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
    session.load_media(&media).expect("insert real CAS media");
    type_command(&mut session, "CLOAD\n");
    session
        .run_frames(4_000)
        .expect("run long enough for the 1,200-baud cassette load");
    type_command(&mut session, "RUN\n");
    session.run_frames(300).expect("run the loaded title");

    let running_frame = session
        .machine()
        .machine()
        .expect("machine should remain live")
        .framebuffer();
    let running_foreground = foreground_pixels(running_frame);
    let changed_pixels = boot_frame
        .iter()
        .zip(running_frame)
        .filter(|(before, after)| before != after)
        .count();
    eprintln!(
        "boot foreground={boot_foreground}, running foreground={running_foreground}, changed={changed_pixels}, vdp={:02x?}, pc={:#06x}",
        session.machine().machine().expect("live").vdp().registers(),
        session.machine().machine().expect("live").cpu().regs.pc,
    );
    let registers = session
        .machine()
        .machine()
        .expect("machine should remain live")
        .vdp()
        .registers();
    assert!(
        changed_pixels > running_frame.len() / 2,
        "RUN should replace the SV-BASIC screen with Mini Golf: changed={changed_pixels}/{}",
        running_frame.len()
    );
    assert_eq!(
        registers[0] & 0x02,
        0x02,
        "Mini Golf should switch the TMS9918 out of the BIOS text mode"
    );
}
