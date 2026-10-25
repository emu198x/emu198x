//! Up to four floppy drives. DF1 to DF3 are fitted when a disk goes into
//! their slot or when asked for, and stay fitted, with their disks,
//! across eject, reset and snapshot restore on every chipset tier.

use emu198x_peripheral_commodore_amiga_floppy::DD;
use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet, ResetKind};
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};

const MODELS: [Model; 3] = [Model::A500OcsPal, Model::A500PlusEcsPal, Model::A1200AgaPal];

fn disk(fill: u8) -> Vec<u8> {
    vec![fill; DD.len()]
}

fn load(
    runtime: &mut AmigaRuntimeKind,
    slot: &'static str,
    bytes: &[u8],
) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new(slot, MediaKind::Disk, bytes));
    runtime.load_media(&media)
}

fn fitted(runtime: &AmigaRuntimeKind) -> Vec<usize> {
    (0..4)
        .filter(|&unit| runtime.floppy_drive_fitted(unit))
        .collect()
}

fn image(runtime: &AmigaRuntimeKind, unit: usize) -> Option<Vec<u8>> {
    runtime.drives().drive(unit)?.save_adf()
}

#[test]
fn a_new_machine_has_only_df0() {
    for model in MODELS {
        assert_eq!(fitted(&AmigaRuntimeKind::blank(model)), [0], "{model:?}");
    }
}

#[test]
fn a_disk_in_a_slot_fits_that_drive() {
    for model in MODELS {
        let mut runtime = AmigaRuntimeKind::blank(model);
        load(&mut runtime, "floppy-2", &disk(0xA5)).expect("DF2 takes an ADF");
        assert_eq!(fitted(&runtime), [0, 2], "{model:?}");
        assert_eq!(image(&runtime, 2), Some(disk(0xA5)), "{model:?}");
        assert_eq!(image(&runtime, 0), None, "{model:?}: DF0 stays empty");
    }
}

#[test]
fn ejecting_leaves_the_drive_fitted() {
    for model in MODELS {
        let mut runtime = AmigaRuntimeKind::blank(model);
        load(&mut runtime, "floppy-1", &disk(0)).expect("DF1 takes an ADF");
        runtime.eject_media("floppy-1").expect("DF1 ejects");
        assert_eq!(fitted(&runtime), [0, 1], "{model:?}");
        assert_eq!(image(&runtime, 1), None, "{model:?}");
    }
}

#[test]
fn drives_can_be_fitted_empty_up_to_four() {
    let mut runtime = AmigaRuntimeKind::blank(Model::A500OcsPal);
    runtime.fit_floppy_drives(3).expect("three drives");
    assert_eq!(fitted(&runtime), [0, 1, 2]);
    for count in [0, 5] {
        assert!(
            matches!(
                runtime.fit_floppy_drives(count),
                Err(MachineError::InvalidRequest { .. })
            ),
            "{count} drives"
        );
    }
    assert_eq!(fitted(&runtime), [0, 1, 2]);
}

#[test]
fn an_unknown_slot_is_refused() {
    let mut runtime = AmigaRuntimeKind::blank(Model::A500OcsPal);
    let error = load(&mut runtime, "floppy-4", &disk(0)).expect_err("there is no DF4");
    assert!(matches!(error, MachineError::UnknownMediaSlot { ref slot } if slot == "floppy-4"));
    assert_eq!(fitted(&runtime), [0]);
}

#[test]
fn reset_keeps_fitted_drives_and_their_disks() {
    for model in MODELS {
        let mut runtime = AmigaRuntimeKind::blank(model);
        load(&mut runtime, "floppy-1", &disk(0x3C)).expect("DF1 takes an ADF");
        runtime.fit_floppy_drives(4).expect("four drives");
        runtime.reset(ResetKind::Hard);
        assert_eq!(fitted(&runtime), [0, 1, 2, 3], "{model:?}");
        assert_eq!(image(&runtime, 1), Some(disk(0x3C)), "{model:?}");
        assert_eq!(image(&runtime, 3), None, "{model:?}");
    }
}

#[test]
fn snapshot_restore_keeps_fitted_drives_and_their_disks() {
    for model in MODELS {
        let mut source = AmigaRuntimeKind::blank(model);
        load(&mut source, "floppy-0", &disk(0x11)).expect("DF0 takes an ADF");
        load(&mut source, "floppy-1", &disk(0x22)).expect("DF1 takes an ADF");
        source.fit_floppy_drives(3).expect("three drives");
        let snapshot = source.snapshot().expect("snapshot");

        let mut restored = AmigaRuntimeKind::blank(model);
        restored.restore(&snapshot).expect("restore");
        assert_eq!(fitted(&restored), [0, 1, 2], "{model:?}");
        assert_eq!(image(&restored, 0), Some(disk(0x11)), "{model:?}");
        assert_eq!(image(&restored, 1), Some(disk(0x22)), "{model:?}");
        assert_eq!(image(&restored, 2), None, "{model:?}");
        assert_eq!(
            restored.snapshot().expect("snapshot again"),
            snapshot,
            "{model:?}: restore must be a byte-level fixed point"
        );
    }
}
