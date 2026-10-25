//! The Plus 3's drives as media slots: load, query, snapshot, reset, export
//! and eject, with synthetic ROMs and a disc built by the ADFS format crate.

use emu198x_shell::{
    HostIo, MachineCore, MachineError, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, ResetKind, SessionQueryProvider,
};
use format_acorn_adfs::{Format, blank};
use runtime_acorn_electron::{ElectronRuntime, ElectronSessionQueryProvider, Model};

fn plus3() -> ElectronRuntime {
    let mut rt = ElectronRuntime::new(Model::ElectronPlus3, vec![0u8; 16384], vec![0u8; 16384])
        .expect("synthetic OS + BASIC ROMs build the machine");
    rt.set_adfs(vec![0u8; 16384])
        .expect("a 16 KB ADFS ROM fits");
    rt
}

fn load(rt: &mut ElectronRuntime, slot: &'static str, bytes: &[u8]) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new(slot, MediaKind::Disk, bytes).writable(true));
    rt.load_media(&media)
}

fn query(rt: &ElectronRuntime, path: &str) -> serde_json::Value {
    ElectronSessionQueryProvider
        .query(rt, path)
        .expect("query does not error")
        .unwrap_or_else(|| panic!("path `{path}` resolves"))
        .value
}

fn run(rt: &mut ElectronRuntime, ticks: u64) {
    let target = MachineTime::new(rt.time().get() + ticks);
    rt.run_until(
        target,
        &mut HostIo {
            input_events: &[],
            frame_sink: &mut NullFrameSink,
            audio_sink: &mut NullAudioSink,
            trace_sink: &mut NullTraceSink,
        },
    )
    .expect("runs");
}

#[test]
fn a_disc_loads_into_each_drive_and_shows_in_the_queries() {
    let mut rt = plus3();
    assert_eq!(query(&rt, "plus3.fitted"), true);
    assert_eq!(query(&rt, "plus3.drive0.loaded"), false);
    load(&mut rt, "drive-0", &blank(Format::M, "ZERO")).expect("an M disc");
    load(&mut rt, "drive-1", &blank(Format::L, "ONE")).expect("an L disc");
    assert_eq!(query(&rt, "plus3.drive0.loaded"), true);
    let group = query(&rt, "plus3");
    assert_eq!(group["adfs"], true);
    assert_eq!(group["drives"][0]["tracks"], 80);
    assert_eq!(group["drives"][0]["sides"], 1);
    assert_eq!(group["drives"][1]["sides"], 2);
    assert_eq!(group["drives"][1]["write_protected"], false);
}

#[test]
fn a_disc_exports_as_the_image_it_is_and_ejects() {
    let mut rt = plus3();
    let image = blank(Format::L, "EXPORT");
    load(&mut rt, "drive-1", &image).expect("an L disc");
    assert_eq!(rt.export_media("drive-1", "adl").expect("exports"), image);
    assert!(matches!(
        rt.export_media("drive-1", "adf"),
        Err(MachineError::InvalidMedia { .. })
    ));
    assert!(matches!(
        rt.export_media("drive-0", "adf"),
        Err(MachineError::InvalidMedia { .. })
    ));
    rt.eject_media("drive-1").expect("ejects");
    assert_eq!(query(&rt, "plus3.drive1.loaded"), false);
}

#[test]
fn a_trimmed_image_is_padded_and_a_bad_one_refused() {
    let mut rt = plus3();
    let mut image = blank(Format::M, "TRIM");
    image.truncate(7 * 256);
    load(&mut rt, "drive-0", &image).expect("a trimmed disc");
    let exported = rt.export_media("drive-0", "adf").expect("exports");
    assert_eq!(exported.len(), Format::M.image_len());
    assert!(matches!(
        load(&mut rt, "drive-0", &[]),
        Err(MachineError::InvalidMedia { .. })
    ));
    assert!(matches!(
        load(&mut rt, "drive-2", &image),
        Err(MachineError::UnknownMediaSlot { .. })
    ));
}

#[test]
fn reset_and_snapshot_keep_the_discs() {
    let mut rt = plus3();
    let image = blank(Format::M, "KEEP");
    load(&mut rt, "drive-0", &image).expect("an M disc");
    run(&mut rt, 40_000);
    rt.reset(ResetKind::Hard);
    assert_eq!(
        query(&rt, "plus3.drive0.loaded"),
        true,
        "reset keeps the disc"
    );
    assert_eq!(query(&rt, "plus3")["adfs"], true, "and the ADFS ROM");

    let snap = rt.snapshot().expect("snapshots");
    let mut restored = ElectronRuntime::blank(Model::ElectronPlus3);
    restored.restore(&snap).expect("restores");
    assert_eq!(
        restored.export_media("drive-0", "adf").expect("exports"),
        image
    );
    assert_eq!(restored.snapshot().expect("re-snapshots"), snap);
}

#[test]
fn a_plain_electron_has_no_drives() {
    let mut rt =
        ElectronRuntime::new(Model::Electron, vec![0u8; 16384], vec![0u8; 16384]).expect("builds");
    assert!(matches!(
        load(&mut rt, "drive-0", &blank(Format::S, "NO")),
        Err(MachineError::UnknownMediaSlot { .. })
    ));
    assert_eq!(query(&rt, "plus3.fitted"), false);
    assert!(ElectronSessionQueryProvider.query(&rt, "plus3").is_err());
    assert!(rt.set_adfs(vec![0; 16384]).is_err());
}
