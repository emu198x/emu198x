//! ATR disk images through the runtime media surface: the `disk-1` slot,
//! the drive keeping its disk across a reset, and eject.

use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet, ResetKind};
use runtime_atari_800xl::{Atari800xlRuntime, Model};

/// A single-density ATR of `sectors` sectors, every byte of sector `n`
/// holding `n`.
fn atr(sectors: u16) -> Vec<u8> {
    let data_len = usize::from(sectors) * 128;
    let mut image = vec![0u8; 16];
    image[0..2].copy_from_slice(&0x0296u16.to_le_bytes());
    image[2..4].copy_from_slice(&((data_len / 16) as u16).to_le_bytes());
    image[4..6].copy_from_slice(&128u16.to_le_bytes());
    for sector in 1..=sectors {
        image.extend(std::iter::repeat_n(sector as u8, 128));
    }
    image
}

fn load(runtime: &mut Atari800xlRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("disk-1", MediaKind::Disk, bytes));
    runtime.load_media(&media)
}

fn disk_in_d1(runtime: &Atari800xlRuntime) -> bool {
    runtime
        .machine()
        .expect("machine")
        .sio()
        .drive(1)
        .is_some_and(|drive| drive.has_disk())
}

fn cart_only() -> Atari800xlRuntime {
    Atari800xlRuntime::new(Model::A800xlNtsc, None, None, Some(vec![0u8; 8192]), false)
        .expect("cart-only machine builds")
}

#[test]
fn a_disk_goes_into_d1_and_stays_there_through_a_reset() {
    let mut runtime = cart_only();
    assert!(runtime.machine().expect("machine").sio().drive(1).is_none());

    load(&mut runtime, &atr(720)).expect("disk loads");
    assert!(disk_in_d1(&runtime));

    runtime.reset(ResetKind::Hard);
    assert!(
        disk_in_d1(&runtime),
        "a reset of the computer does not empty the drive"
    );
}

#[test]
fn a_disk_loaded_before_the_machine_exists_is_in_the_drive_once_it_does() {
    let mut runtime = Atari800xlRuntime::blank(Model::A800xlNtsc);
    assert!(runtime.machine().is_none());
    load(&mut runtime, &atr(720)).expect("disk loads into a blank runtime");

    runtime
        .insert_cartridge(Some(vec![0u8; 8192]))
        .expect("cart builds the machine");
    assert!(disk_in_d1(&runtime));
}

#[test]
fn eject_empties_the_drive_but_leaves_it_on_the_bus() {
    let mut runtime = cart_only();
    load(&mut runtime, &atr(720)).expect("disk loads");
    runtime.eject_media("disk-1").expect("disk ejects");
    let machine = runtime.machine().expect("machine");
    assert!(
        machine
            .sio()
            .drive(1)
            .is_some_and(|drive| !drive.has_disk())
    );
}

#[test]
fn a_file_that_is_not_an_atr_is_refused_by_name() {
    let mut runtime = cart_only();
    let err = load(&mut runtime, b"not a disk image at all").expect_err("refused");
    assert!(
        matches!(&err, MachineError::InvalidMedia { slot, .. } if slot == "disk-1"),
        "{err:?}"
    );
    assert!(runtime.machine().expect("machine").sio().drive(1).is_none());
}

#[test]
fn a_disk_for_a_slot_the_profile_does_not_have_is_refused() {
    let mut runtime = cart_only();
    let image = atr(720);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("disk-2", MediaKind::Disk, &image));
    let err = runtime.load_media(&media).expect_err("refused");
    assert!(
        matches!(&err, MachineError::UnknownMediaSlot { slot } if slot == "disk-2"),
        "{err:?}"
    );
}
