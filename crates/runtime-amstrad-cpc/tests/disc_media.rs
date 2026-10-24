//! Disc media on the CPC664 and CPC6128: load, write-back, reset, eject and
//! export, and the 464's lack of a drive.

use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet, ResetKind};
use machine_amstrad_cpc::DiskImage;
use runtime_amstrad_cpc::{AmstradCpcRuntime, Model};

fn firmware() -> Vec<u8> {
    let mut rom = vec![0x00u8; 0x8000];
    rom[0x4000..].fill(0xC9);
    rom
}

/// A one-track DATA-format disc, sectors `&C1`-`&C9`.
fn dsk() -> Vec<u8> {
    use nec_upd765a::{DiskSector, DiskTrack};
    let track = DiskTrack {
        sectors: (0xC1..=0xC9u8)
            .map(|id| DiskSector {
                c: 0,
                h: 0,
                id,
                n: 2,
                st1: 0,
                st2: 0,
                data: vec![id; 512],
            })
            .collect(),
    };
    let image = DiskImage {
        sides: 1,
        tracks_per_side: 1,
        tracks: vec![vec![track]],
    };
    format_amstrad_dsk::encode_extended(&image).expect("encodes")
}

fn runtime(model: Model) -> AmstradCpcRuntime {
    let mut runtime = AmstradCpcRuntime::new(model, firmware()).expect("firmware");
    runtime.set_amsdos(vec![0; 0x4000]).expect("AMSDOS");
    runtime
}

fn load(
    runtime: &mut AmstradCpcRuntime,
    slot: &'static str,
    bytes: &[u8],
) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new(slot, MediaKind::Disk, bytes));
    runtime.load_media(&media)
}

#[test]
fn a_disc_goes_into_the_named_drive() {
    for model in [Model::Cpc664, Model::Cpc6128] {
        let mut rt = runtime(model);
        load(&mut rt, "disk-b", &dsk()).expect("drive B takes a disc");
        let machine = rt.machine().expect("machine");
        assert!(machine.disk(0).is_none());
        let disk = machine.disk(1).expect("a disc in B");
        assert_eq!(
            disk.sector(0, 0, 0xC5).expect("sector").data,
            vec![0xC5; 512]
        );
    }
}

#[test]
fn reset_keeps_the_discs_and_eject_removes_them() {
    let mut rt = runtime(Model::Cpc6128);
    load(&mut rt, "disk-a", &dsk()).expect("loads");
    rt.reset(ResetKind::Hard);
    assert!(
        rt.machine().expect("machine").disk(0).is_some(),
        "a reset leaves the disc in"
    );
    rt.eject_media("disk-a").expect("ejects");
    assert!(rt.machine().expect("machine").disk(0).is_none());
}

#[test]
fn export_writes_the_live_disc_as_extended_dsk() {
    let mut rt = runtime(Model::Cpc664);
    load(&mut rt, "disk-a", &dsk()).expect("loads");
    let exported = rt.export_media("disk-a", "edsk").expect("exports");
    let reread = format_amstrad_dsk::parse(&exported).expect("parses");
    assert_eq!(
        reread.sector(0, 0, 0xC9).expect("sector").data,
        vec![0xC9; 512]
    );
    assert!(rt.export_media("disk-a", "tap").is_err());
    assert!(
        rt.export_media("disk-b", "edsk").is_err(),
        "drive B is empty"
    );
}

#[test]
fn a_464_has_no_disc_slots() {
    let mut rt = AmstradCpcRuntime::new(Model::Cpc464, firmware()).expect("firmware");
    assert!(matches!(
        load(&mut rt, "disk-a", &dsk()),
        Err(MachineError::UnknownMediaSlot { .. })
    ));
}

#[test]
fn a_malformed_disc_is_refused() {
    let mut rt = runtime(Model::Cpc6128);
    assert!(matches!(
        load(&mut rt, "disk-a", b"not a disc"),
        Err(MachineError::InvalidMedia { .. })
    ));
}
