//! The C128 in the family runtime, and the dispatcher that holds either
//! machine.
//!
//! The real-ROM tests are gated `#[ignore]`: name VICE's `data/C128` with
//! EMU198X_C128_ROM_DIR, as the machine crate's own boot tests do.
//!
//! ```text
//! EMU198X_C128_ROM_DIR=... cargo test --release -p runtime-commodore-c64 --test c128 -- --ignored
//! ```

use std::path::PathBuf;

use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, FramePacket, FrameSink, HeadlessSession, HostIo,
    LoaderError, MachineCore, MachineError, MachineTime, MediaImage, MediaKind, MediaSet,
    NullAudioSink, NullTraceSink, ResetKind, SessionQueryProvider,
};
use runtime_commodore_c64::{
    AsC64Runtime, C64RuntimeKind, C64SessionQueryProvider, C64Variant, C128Model, C128Runtime,
    Model, type_string,
};
use serde_json::json;

/// The size of each frame pushed, last first.
#[derive(Default)]
struct FrameSizes(Vec<(u32, u32)>);

impl FrameSink for FrameSizes {
    fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
        self.0.push((frame.width, frame.height));
        Ok(())
    }
}

fn run_frames<M: MachineCore>(runtime: &mut M, frames: u64, per_frame: u64) -> FrameSizes {
    let mut sizes = FrameSizes::default();
    let mut audio = NullAudioSink;
    let mut trace = NullTraceSink;
    let target = MachineTime::new(runtime.time().get() + frames * per_frame);
    runtime
        .run_until(
            target,
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut sizes,
                audio_sink: &mut audio,
                trace_sink: &mut trace,
            },
        )
        .expect("run");
    sizes
}

fn query<M>(runtime: &M, path: &str) -> Option<serde_json::Value>
where
    C64SessionQueryProvider: SessionQueryProvider<M>,
{
    C64SessionQueryProvider
        .query(runtime, path)
        .expect("query")
        .map(|result| result.value)
}

#[test]
fn a_blank_c128_reports_its_own_profile_and_pacing() {
    let pal = C128Runtime::blank(C128Model::C128Pal);
    assert_eq!(pal.profile().machine_id.as_str(), "commodore-c128");
    assert_eq!(pal.native_frame_ticks(), 312 * 63);
    assert!(pal.profile().media_slots.is_empty());
    let ntsc = C128Runtime::blank(C128Model::C128Ntsc);
    assert_eq!(ntsc.native_frame_ticks(), 263 * 65);
    assert_eq!(C128Runtime::variant_ids(), ["c128-pal", "c128-ntsc"]);
    assert_eq!(C128Runtime::model_from_id("c128"), Some(C128Model::C128Pal));
}

#[test]
fn the_80_column_screen_replaces_the_vic_picture_when_chosen() {
    let mut c128 = C128Runtime::blank(C128Model::C128Pal);
    let vic = *run_frames(&mut c128, 2, 312 * 63)
        .0
        .last()
        .expect("a frame");
    c128.set_alternate_screen(true);
    assert!(c128.alternate_screen());
    let vdc = *run_frames(&mut c128, 2, 312 * 63)
        .0
        .last()
        .expect("a frame");
    assert_eq!(vdc, (704, 270), "the VDC's picture");
    assert_ne!(vic, vdc);
    assert_eq!(c128.rgba_frame().1, 704);
    assert_eq!(query(&c128, "c128.screen.80_column"), Some(json!(true)));
    c128.set_alternate_screen(false);
    assert_eq!(
        *run_frames(&mut c128, 1, 312 * 63)
            .0
            .last()
            .expect("a frame"),
        vic
    );
}

#[test]
fn a_c64_ignores_a_request_for_a_second_screen() {
    let mut c64 = runtime_commodore_c64::C64Runtime::blank(Model::C64PalBreadbin);
    c64.set_alternate_screen(true);
    assert!(!c64.alternate_screen());
}

#[test]
fn the_c128_answers_shared_and_its_own_queries_but_not_the_c64s() {
    let c128 = C128Runtime::blank(C128Model::C128Pal);
    let paths = C64SessionQueryProvider.query_paths(&c128, None);
    for path in [
        "cpu.pc",
        "vic.border_colour",
        "screen.text.lines",
        "c128.z80.pc",
    ] {
        assert!(paths.iter().any(|p| p == path), "{path} is advertised");
        assert!(query(&c128, path).is_some(), "{path} answers");
    }
    for path in ["drive8.attached", "tape.loaded", "iec.cpu_port"] {
        assert!(!paths.iter().any(|p| p == path), "{path} is not advertised");
        assert_eq!(query(&c128, path), None, "{path} is unknown");
    }
    assert_eq!(query(&c128, "c128.cpu_8502_active"), Some(json!(false)));
    assert!(query(&c128, "memory.ram.0400").is_some());
}

#[test]
fn the_c128_has_no_media_slots() {
    let mut c128 = C128Runtime::blank(C128Model::C128Pal);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &[0; 4]));
    assert!(matches!(
        c128.load_media(&media),
        Err(MachineError::UnknownMediaSlot { .. })
    ));
}

#[test]
fn a_c128_snapshot_resumes_in_lockstep_and_refuses_a_c64() {
    let mut c128 = C128Runtime::blank(C128Model::C128Pal);
    run_frames(&mut c128, 1, 1234);
    let bytes = c128.snapshot().expect("snapshot");
    let mut restored = C128Runtime::blank(C128Model::C128Pal);
    restored.restore(&bytes).expect("restore");
    assert_eq!(restored.time(), c128.time());
    run_frames(&mut c128, 3, 312 * 63);
    run_frames(&mut restored, 3, 312 * 63);
    assert_eq!(
        restored.snapshot().expect("snapshot"),
        c128.snapshot().expect("snapshot")
    );
    let mut c64 = runtime_commodore_c64::C64Runtime::blank(Model::C64PalBreadbin);
    assert!(
        c64.restore(&bytes).is_err(),
        "a C128 snapshot is not a C64's"
    );
}

#[test]
fn a_reset_rebuilds_the_c128_from_its_roms() {
    let mut c128 = C128Runtime::blank(C128Model::C128Pal);
    run_frames(&mut c128, 2, 312 * 63);
    c128.reset(ResetKind::Hard);
    assert_eq!(c128.time(), MachineTime::default());
}

#[test]
fn the_dispatcher_names_every_variant_and_reaches_the_c64_only_through_as_c64() {
    assert_eq!(
        C64RuntimeKind::variant_ids(),
        [
            "pal",
            "ntsc",
            "c64c-pal",
            "c64c-ntsc",
            "c128-pal",
            "c128-ntsc"
        ]
    );
    assert_eq!(C64Variant::all().len(), 6);
    for variant in C64Variant::all() {
        assert_eq!(
            C64Variant::from_variant_id(variant.variant_id()),
            Some(variant)
        );
        assert_eq!(
            C64RuntimeKind::blank(variant).profile().profile_id,
            variant.profile().profile_id
        );
    }
    let mut c64 = C64RuntimeKind::blank(Model::C64PalBreadbin.into());
    assert!(c64.as_c64().is_some() && c64.as_c128().is_none());
    assert!(query(&c64, "tape.loaded").is_some());
    let mut c128 = C64RuntimeKind::blank(C128Model::C128Ntsc.into());
    assert!(c128.as_c64_mut().is_none() && c128.as_c128_mut().is_some());
    assert_eq!(c128.variant(), C64Variant::C128(C128Model::C128Ntsc));
    assert_eq!(c128.native_frame_ticks(), 263 * 65);
    assert!(query(&c128, "c128.z80.pc").is_some());
    c128.set_alternate_screen(true);
    assert!(c128.alternate_screen());
    c64.set_alternate_screen(true);
    assert!(!c64.alternate_screen());
}

#[test]
fn the_c64_loaders_decline_a_c128() {
    let runtime = C64RuntimeKind::blank(C128Model::C128Pal.into());
    let mut session = HeadlessSession::new_with_query_provider(runtime, 1, C64SessionQueryProvider);
    assert!(matches!(
        C64RuntimeKind::load_basic_program(&mut session, "10 PRINT 1\n", false),
        Err(LoaderError::Unsupported { .. })
    ));
    assert!(matches!(
        C64RuntimeKind::autoload_tape(&mut session, "tape-1", 1),
        Err(LoaderError::Unsupported { .. })
    ));
}

#[test]
fn firmware_of_the_wrong_size_is_named_by_its_id() {
    let mut firmware = FirmwareSet::new();
    for (id, size) in [
        ("commodore-c128-kernal-rom", 0x4000),
        ("commodore-c128-basic-lo-rom", 0x4000),
        ("commodore-c128-basic-hi-rom", 0x2000),
        ("commodore-c128-character-rom", 0x2000),
    ] {
        firmware.push(FirmwareImage::new(
            id,
            Box::leak(vec![0; size].into_boxed_slice()),
        ));
    }
    match C128Runtime::from_firmware(C128Model::C128Pal, &firmware).map(|_| ()) {
        Err(MachineError::InvalidFirmware { id, .. }) => {
            assert_eq!(id, "commodore-c128-basic-hi-rom");
        }
        other => panic!("expected InvalidFirmware, got {other:?}"),
    }
}

/// VICE's C128 ROMs from EMU198X_C128_ROM_DIR.
fn c128_firmware() -> FirmwareSet<'static> {
    let dir = PathBuf::from(std::env::var("EMU198X_C128_ROM_DIR").expect("EMU198X_C128_ROM_DIR"));
    let mut firmware = FirmwareSet::new();
    for (id, file) in [
        ("commodore-c128-kernal-rom", "kernal-318020-05.bin"),
        ("commodore-c128-basic-lo-rom", "basiclo-318018-04.bin"),
        ("commodore-c128-basic-hi-rom", "basichi-318019-04.bin"),
        ("commodore-c128-character-rom", "chargen-390059-01.bin"),
    ] {
        let bytes = std::fs::read(dir.join(file)).unwrap_or_else(|err| panic!("{file}: {err}"));
        firmware.push(FirmwareImage::new(id, Box::leak(bytes.into_boxed_slice())));
    }
    firmware
}

fn screen(lines: Option<serde_json::Value>) -> String {
    lines
        .and_then(|value| {
            value.as_array().map(|lines| {
                lines
                    .iter()
                    .filter_map(|line| line.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        })
        .unwrap_or_default()
}

#[test]
#[ignore = "FIXTURE: needs the C128 ROMs — run with --ignored"]
fn the_c128_boots_basic_7_and_runs_typed_basic() {
    for model in C128Model::ALL {
        let runtime = C128Runtime::from_firmware(model, &c128_firmware()).expect("firmware");
        let ticks = runtime.native_frame_ticks();
        let mut session =
            HeadlessSession::new_with_query_provider(runtime, ticks, C64SessionQueryProvider);
        session.wait_for_boot(400).expect("READY.");
        let text = screen(session.query("screen.text.lines").ok().map(|r| r.value));
        assert!(text.contains("BASIC V7.0"), "{model:?}:\n{text}");
        type_string(&mut session, "PRINT 6*7\n", 3, 20).expect("typing");
        let text = screen(session.query("screen.text.lines").ok().map(|r| r.value));
        assert!(
            text.lines().any(|line| line.trim() == "42"),
            "{model:?}:\n{text}"
        );
        assert_eq!(
            session.query("c128.cpu_8502_active").expect("query").value,
            json!(true)
        );
    }
}

#[test]
#[ignore = "FIXTURE: needs the C128 ROMs — run with --ignored"]
fn with_40_80_down_basic_comes_up_on_the_80_column_screen() {
    let mut runtime =
        C128Runtime::from_firmware(C128Model::C128Pal, &c128_firmware()).expect("firmware");
    runtime.machine_mut().set_80_column_key(true);
    runtime.set_alternate_screen(true);
    let sizes = run_frames(&mut runtime, 200, 312 * 63);
    assert_eq!(sizes.0.last(), Some(&(704, 270)));
    let text = screen(query(&runtime, "c128.vdc.text.lines"));
    assert!(text.contains("BASIC V7.0"), "{text}");
    assert!(text.lines().any(|line| line.trim() == "READY."), "{text}");
    assert_eq!(
        query(&runtime, "boot.detected"),
        Some(json!(true)),
        "boot detection reads the 80-column screen too"
    );
}

#[test]
#[ignore = "FIXTURE: needs the C128 ROMs — run with --ignored"]
fn a_mid_boot_snapshot_of_a_real_c128_resumes_in_lockstep() {
    let mut runtime =
        C128Runtime::from_firmware(C128Model::C128Pal, &c128_firmware()).expect("firmware");
    run_frames(&mut runtime, 37, 312 * 63);
    let bytes = runtime.snapshot().expect("snapshot");
    let mut restored =
        C128Runtime::from_firmware(C128Model::C128Pal, &c128_firmware()).expect("firmware");
    restored.restore(&bytes).expect("restore");
    run_frames(&mut runtime, 100, 312 * 63);
    run_frames(&mut restored, 100, 312 * 63);
    assert_eq!(
        restored.snapshot().expect("snapshot"),
        runtime.snapshot().expect("snapshot")
    );
    assert!(
        screen(query(&restored, "screen.text.lines")).contains("READY."),
        "the restored machine finished booting"
    );
}
