//! The Neo Geo runtime on the stand-in BIOS and a hand-assembled
//! cartridge: media loading, running, queries, reset and snapshots.

use std::io::Write;

use emu198x_shell::{
    AudioPacket, AudioSink, FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, HostIo,
    MachineCore, MachineError, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, ResetKind, SessionQueryProvider, StopReason,
};
use runtime_snk_neo_geo::test_support::{bios, p_rom, runtime_with_program};
use runtime_snk_neo_geo::{
    BIOS_FIRMWARE_ID, Model, NeoGeoRuntime, NeoGeoSessionQueryProvider, ZOOM_FIRMWARE_ID, blank,
};

const RAM: u32 = 0x10_0000;
const FRAME: u64 = 264 * 384 * 4;

/// Enable interrupts and count loop passes at `RAM`; the vertical-blank
/// handler at `$500` counts frames at `RAM + 2`, acknowledges, and sends a
/// sound command.
fn counting_program() -> Vec<u8> {
    let mut p = p_rom(&[
        0x46FC, 0x2000, // MOVE.W #$2000,SR
        0x5279, 0x0010, 0x0000, // loop: ADDQ.W #1,$100000
        0x60F8, // BRA.S loop
    ]);
    p[0x64..0x68].copy_from_slice(&0x500u32.to_be_bytes());
    let handler: [u16; 13] = [
        0x5279, 0x0010, 0x0002, // ADDQ.W #1,$100002
        0x33FC, 0x0004, 0x003C, 0x000C, // MOVE.W #4,$3C000C
        0x13FC, 0x0021, 0x0032, 0x0000, // MOVE.B #$21,$320000
        0x4E73, // RTE
        0x4E71,
    ];
    for (i, word) in handler.iter().enumerate() {
        p[0x500 + i * 2..0x502 + i * 2].copy_from_slice(&word.to_be_bytes());
    }
    p
}

fn loaded() -> NeoGeoRuntime {
    let mut runtime = NeoGeoRuntime::new(Model::Aes, bios(), None).expect("stand-in BIOS");
    runtime
        .insert_cartridge(&counting_program())
        .expect("a bare P-ROM loads");
    runtime
}

fn run_to(runtime: &mut NeoGeoRuntime, target: u64) {
    runtime
        .run_until(
            MachineTime::new(target),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("the runtime should run");
}

fn ram_word(runtime: &NeoGeoRuntime, at: u32) -> u16 {
    let ram = runtime.machine().expect("machine").ram();
    let a = (at & 0xFFFF) as usize;
    u16::from_be_bytes([ram[a], ram[a + 1]])
}

#[test]
fn a_blank_runtime_waits_for_bios_and_cartridge() {
    let mut runtime = blank(Model::Aes);
    let result = runtime
        .run_until(
            MachineTime::new(FRAME),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("run");
    assert_eq!(result.stop_reason, StopReason::WaitingForInput);
    runtime.insert_cartridge(&counting_program()).expect("load");
    assert!(runtime.cartridge_loaded());
    assert!(runtime.machine().is_none(), "no BIOS yet");
    runtime.set_bios(bios()).expect("BIOS");
    assert!(runtime.machine().is_some());
}

#[test]
fn the_bios_must_be_128_kb_and_the_l0_is_optional() {
    assert!(matches!(
        NeoGeoRuntime::new(Model::Aes, vec![0; 0x1000], None),
        Err(MachineError::InvalidFirmware { .. })
    ));
    let bios = bios();
    let zoom = vec![0u8; 0x2_0000];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, bios.as_slice()));
    firmware.push(FirmwareImage::new(ZOOM_FIRMWARE_ID, zoom.as_slice()));
    let runtime = <NeoGeoRuntime as FamilyRuntime>::from_firmware(Model::Aes, &firmware)
        .expect("builds with the L0");
    assert_eq!(runtime.model(), Model::Aes);
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, bios.as_slice()));
    assert!(<NeoGeoRuntime as FamilyRuntime>::from_firmware(Model::Aes, &firmware).is_ok());
    assert!(
        <NeoGeoRuntime as FamilyRuntime>::from_firmware(Model::Aes, &FirmwareSet::new()).is_err()
    );
}

struct Samples(usize);

impl AudioSink for Samples {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        assert_eq!(packet.channels, 2);
        self.0 += packet.samples.len();
        Ok(())
    }
}

#[test]
fn the_cartridge_runs_takes_interrupts_and_makes_sound() {
    let mut runtime = loaded();
    let mut samples = Samples(0);
    runtime
        .run_until(
            MachineTime::new(10 * FRAME),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut samples,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("run");
    assert!(ram_word(&runtime, RAM + 2) >= 8, "vertical blanks counted");
    assert!(ram_word(&runtime, RAM) > 100, "the loop ran");
    // 48 kHz stereo for the time run, to within a sample or two.
    let expected = runtime.time().get() * 48_000 * 2 / 24_000_000;
    assert!(
        samples.0.abs_diff(expected as usize) <= 4,
        "{} vs {expected}",
        samples.0
    );
    // The first frame is short: power-on is at the top of the frame.
    assert_eq!(runtime.frames(), 11);
    assert!(runtime.display().is_some());
}

#[test]
fn a_zip_rom_set_loads_through_the_cartridge_slot() {
    // The P-ROM as MAME stores it: words byte-swapped.
    let swapped: Vec<u8> = counting_program()
        .chunks(2)
        .flat_map(|w| [w[1], w[0]])
        .collect();
    let mut zip = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut zip);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("test-p1.p1", options).expect("start");
        writer.write_all(&swapped).expect("write");
        writer.start_file("test-m1.m1", options).expect("start");
        writer.write_all(&[0x76]).expect("write"); // HALT
        writer.finish().expect("finish");
    }
    let bytes = zip.into_inner();
    let mut runtime = NeoGeoRuntime::new(Model::Aes, bios(), None).expect("BIOS");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "cartridge-1",
        MediaKind::Cartridge,
        bytes.as_slice(),
    ));
    runtime.load_media(&media).expect("loads");
    assert_eq!(runtime.machine().expect("machine").cartridge().m, [0x76]);
    run_to(&mut runtime, 3 * FRAME);
    assert!(ram_word(&runtime, RAM + 2) >= 2);

    let mut bad = MediaSet::new();
    bad.push(MediaImage::new(
        "cartridge-1",
        MediaKind::Cartridge,
        &b"PK\x03\x04broken"[..],
    ));
    assert!(matches!(
        runtime.load_media(&bad),
        Err(MachineError::InvalidMedia { .. })
    ));
    let mut wrong = MediaSet::new();
    wrong.push(MediaImage::new(
        "cartridge-2",
        MediaKind::Cartridge,
        &[0u8][..],
    ));
    assert!(runtime.load_media(&wrong).is_err());
    let mut disk = MediaSet::new();
    disk.push(MediaImage::new("cartridge-1", MediaKind::Disk, &[0u8][..]));
    assert!(runtime.load_media(&disk).is_err());
}

#[test]
fn input_events_reach_the_controller() {
    let mut runtime = runtime_with_program(&[
        0x13F9, 0x0030, 0x0000, 0x0010, 0x0000, // MOVE.B $300000,$100000
        0x60F4, // BRA.S back
    ]);
    let events = [emu198x_shell::InputEvent::Button {
        port: 1,
        name: "a".to_owned().into(),
        pressed: true,
    }];
    runtime
        .run_until(
            MachineTime::new(2 * FRAME),
            &mut HostIo {
                input_events: &events,
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("run");
    assert_eq!(runtime.machine().expect("machine").ram()[0], 0xEF);
}

#[test]
fn queries_resolve_grouped_and_leaves() {
    let mut session = HeadlessSession::new(loaded(), Model::Aes.frame_ticks());
    session.run_frames(3).expect("run");
    let rt = session.into_machine();
    let resolve = |path: &str| {
        NeoGeoSessionQueryProvider
            .query(&rt, path)
            .expect("no error")
            .unwrap_or_else(|| panic!("`{path}` resolves"))
            .value
    };
    for path in NeoGeoSessionQueryProvider.query_paths(&rt, None) {
        resolve(&path);
    }
    assert_eq!(resolve("machine.variant"), "aes");
    assert_eq!(resolve("cartridge.loaded"), true);
    assert_eq!(resolve("sound.command"), 0x21);
    assert_eq!(
        resolve("sound")["pending"],
        true,
        "the cartridge has no Z80 code"
    );
    assert!(resolve("cpu.pc").as_u64().expect("pc") >= 0x400);
    assert_eq!(resolve("lspc.line"), resolve("lspc")["line"]);
    assert!(
        NeoGeoSessionQueryProvider
            .query(&rt, "nope")
            .expect("ok")
            .is_none()
    );
    assert_eq!(
        NeoGeoSessionQueryProvider.query_paths(&rt, Some("sound")),
        ["sound", "sound.command", "sound.reply"]
    );
    assert!(
        NeoGeoSessionQueryProvider
            .query(&blank(Model::Aes), "cpu.pc")
            .is_err()
    );
}

#[test]
fn resets_restart_from_the_bios() {
    let mut runtime = loaded();
    run_to(&mut runtime, 3 * FRAME);
    let frames = ram_word(&runtime, RAM + 2);
    runtime.reset(ResetKind::Soft);
    assert_eq!(
        ram_word(&runtime, RAM + 2),
        frames,
        "RAM survives a soft reset"
    );
    assert_eq!(runtime.machine().expect("machine").peek(5), 0xC0);
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.time(), MachineTime::default());
    assert_eq!(ram_word(&runtime, RAM + 2), 0);
    let bios = bios();
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, bios.as_slice()));
    let replacement = runtime
        .replacement(Model::Aes, &firmware)
        .expect("replacement");
    assert!(
        replacement.machine().is_some(),
        "the cartridge carries over"
    );
}

#[test]
fn snapshot_round_trip_preserves_live_state() {
    let mut runtime = loaded();
    run_to(&mut runtime, 3 * FRAME + 1234);
    let snap = runtime.snapshot().expect("snapshot");

    // Media do not travel in a snapshot.
    assert!(blank(Model::Aes).restore(&snap).is_err());

    let mut restored = loaded();
    restored.restore(&snap).expect("restore");
    assert_eq!(restored.time(), runtime.time());
    assert_eq!(restored.snapshot().expect("snapshot"), snap);

    run_to(&mut runtime, 9 * FRAME);
    run_to(&mut restored, 9 * FRAME);
    assert_eq!(
        restored.snapshot().expect("snapshot"),
        runtime.snapshot().expect("snapshot"),
        "a restored machine must evolve identically",
    );
    assert!(ram_word(&restored, RAM + 2) >= 7);

    // A blank snapshot restores into a blank runtime.
    let empty = blank(Model::Aes).snapshot().expect("snapshot");
    blank(Model::Aes).restore(&empty).expect("restore");
}

/// Native events, rather than the session's pacing hint, determine both count
/// and elapsed time. The independent machine path advances the same fields.
#[test]
fn exact_native_frames_match_machine_boundaries_for_every_profile() {
    for model in Model::ALL {
        for pacing in [1, model.frame_ticks(), model.frame_ticks() * 3] {
            assert_eq!(model, Model::Aes);
            for count in [0, 1, 2, 5] {
                for traced in [false, true] {
                    let mut expected = loaded();
                    for _ in 0..count {
                        expected.machine_mut().expect("machine").run_frame();
                    }
                    let mut session = HeadlessSession::new(loaded(), pacing);
                    let mut trace = emu198x_shell::NullTraceSink;
                    if traced {
                        session
                            .run_frames_with_trace_sink(count, &mut trace)
                            .expect("frames");
                    } else {
                        session.run_frames(count).expect("frames");
                    }
                    assert_eq!(
                        session.frames_emitted(),
                        u64::from(count),
                        "{model:?}, traced={traced}"
                    );
                    assert_eq!(
                        session.time().get(),
                        expected.machine().expect("reference").mclk()
                    );
                    assert_eq!(
                        session.time().get(),
                        session.machine().machine().expect("machine").mclk()
                    );
                    // Repeated calls start at the actual prior boundary.
                    for _ in 0..3 {
                        expected.machine_mut().expect("machine").run_frame();
                    }
                    session.run_frames(3).expect("successive frames");
                    assert_eq!(session.frames_emitted(), u64::from(count + 3));
                    assert_eq!(
                        session.time().get(),
                        expected.machine().expect("reference").mclk()
                    );
                }
            }
        }
    }
}
