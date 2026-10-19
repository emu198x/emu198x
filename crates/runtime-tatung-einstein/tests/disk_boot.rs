//! The runtime keyboard must reach the MOS disk boot route, not just core tests.
use std::{env, fs, path::PathBuf};

use emu198x_shell::{HeadlessSession, InputEvent, MachineCore, MediaImage, MediaKind, MediaSet};
use runtime_tatung_einstein::{EinsteinRuntime, Model};

// One track, ten 512-byte sectors, with an original MOS boot header in sector 0.
fn marker_disk() -> Vec<u8> {
    let mut dsk = vec![0; 256 + 256 + 10 * 512];
    dsk[..23].copy_from_slice(b"EXTENDED CPC DSK File\r\n");
    dsk[0x30] = 1;
    dsk[0x31] = 1;
    dsk[0x34] = 21;
    dsk[0x100..0x10c].copy_from_slice(b"Track-Info\r\n");
    dsk[0x114] = 2;
    dsk[0x115] = 10;
    for sector in 0..10 {
        let descriptor = 0x118 + 8 * sector;
        dsk[descriptor + 2] = sector as u8;
        dsk[descriptor + 3] = 2;
        dsk[descriptor + 7] = 2;
    }
    // MOS header: destination $9000, end $9200, entry $9007, disk mode 0.
    // DI; LD A,$5A; LD ($8000),A; JR $ — marker proves CPU execution.
    let program = [
        0x00, 0x90, 0x00, 0x92, 0x07, 0x90, 0x00, 0xf3, 0x3e, 0x5a, 0x32, 0x00, 0x80, 0x18, 0xfe,
    ];
    dsk[0x200..0x200 + program.len()].copy_from_slice(&program);
    dsk
}

#[test]
#[ignore = "FIXTURE: needs Einstein MOS — run with --ignored"]
fn ctrl_break_runs_a_disk_program_through_runtime_input() {
    let path = env::var_os("EMU198X_EINSTEIN_BIOS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").expect("HOME"))
                .join(".emu198x/roms/tatung-einstein/einstein.rom")
        });
    let runtime = EinsteinRuntime::new(Model::Einstein, fs::read(path).expect("read MOS"))
        .expect("construct runtime");
    let mut session = HeadlessSession::new(runtime, Model::Einstein.frame_ticks());
    session.run_frames(300).expect("boot MOS");
    assert_ne!(
        session.machine().machine().expect("machine").peek(0x8000),
        0x5a
    );
    let disk = marker_disk();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
    session
        .machine_mut()
        .load_media(&media)
        .expect("mount original disk");
    for (name, pressed, frames) in [
        ("control", true, 3),
        ("break", true, 6),
        ("break", false, 30),
        ("control", false, 300),
    ] {
        session.queue_input(InputEvent::Key {
            name: name.into(),
            pressed,
        });
        session.run_frames(frames).expect("deliver runtime key");
    }
    assert_eq!(
        session.machine().machine().expect("machine").peek(0x8000),
        0x5a,
        "Ctrl-BREAK must execute the disk program, not merely transfer a sector"
    );
}
