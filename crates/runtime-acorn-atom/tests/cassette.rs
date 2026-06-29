//! Cassette `load_media` wiring for the Acorn Atom (#371).
//!
//! Drives the runtime's UEF tape path end to end with a synthetic 24 KiB ROM, so
//! no external firmware is needed.

use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet, ResetKind};
use runtime_acorn_atom::{AtomRuntime, Model};

/// A minimal valid UEF: magic + version, a carrier tone, and one data byte.
fn uef_with_byte(byte: u8) -> Vec<u8> {
    let mut image = b"UEF File!\0".to_vec();
    image.extend_from_slice(&[0x0a, 0x00]); // version 0.10
    image.extend_from_slice(&0x0110u16.to_le_bytes()); // carrier tone
    image.extend_from_slice(&2u32.to_le_bytes());
    image.extend_from_slice(&256u16.to_le_bytes());
    image.extend_from_slice(&0x0100u16.to_le_bytes()); // implicit data
    image.extend_from_slice(&1u32.to_le_bytes());
    image.push(byte);
    image
}

fn runtime() -> AtomRuntime {
    AtomRuntime::new(Model::AtomFull, vec![0u8; 24 * 1024])
        .expect("synthetic ROM builds the machine")
}

fn load(rt: &mut AtomRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, bytes));
    rt.load_media(&media)
}

#[test]
fn load_media_parses_a_uef_and_mounts_the_tape() {
    let mut rt = runtime();
    load(&mut rt, &uef_with_byte(0x41)).expect("UEF tape loads");
    assert!(rt.machine().expect("machine present").tape_loaded());
}

#[test]
fn the_tape_survives_a_reset() {
    let mut rt = runtime();
    load(&mut rt, &uef_with_byte(0x41)).expect("UEF tape loads");
    rt.reset(ResetKind::Hard);
    assert!(rt.machine().expect("machine present").tape_loaded());
}

#[test]
fn a_corrupt_uef_reports_invalid_media() {
    let mut rt = runtime();
    match load(&mut rt, b"not a uef file") {
        Err(MachineError::InvalidMedia { slot, .. }) => assert_eq!(slot, "tape-1"),
        other => panic!("expected InvalidMedia, got {other:?}"),
    }
}

#[test]
fn an_unknown_slot_is_rejected() {
    let mut rt = runtime();
    let uef = uef_with_byte(0x41);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-2", MediaKind::Tape, &uef));
    match rt.load_media(&media) {
        Err(MachineError::UnknownMediaSlot { slot }) => assert_eq!(slot, "tape-2"),
        other => panic!("expected UnknownMediaSlot, got {other:?}"),
    }
}
