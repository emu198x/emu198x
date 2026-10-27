//! New-media validation must not reset or replace a running cartridge on error.
use emu198x_shell::{HeadlessSession, MachineCore, MachineError, MediaImage, MediaKind, MediaSet};
use runtime_coleco_colecovision::{CvRuntime, Model};

fn cart(signature: [u8; 2]) -> Vec<u8> {
    let mut bytes = vec![0; 0x24];
    bytes[..2].copy_from_slice(&signature);
    bytes
}

fn media(bytes: &[u8]) -> MediaSet<'_> {
    let mut set = MediaSet::new();
    set.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, bytes));
    set
}

#[test]
fn both_bios_header_signatures_load() {
    for signature in [[0xaa, 0x55], [0x55, 0xaa]] {
        let bytes = cart(signature);
        let mut runtime = CvRuntime::new(Model::CvNtsc, vec![0x76; 8192]).expect("BIOS");
        runtime
            .load_media(&media(&bytes))
            .expect("recognised header");
        let machine = runtime.machine().expect("live machine");
        for (offset, byte) in bytes.iter().enumerate() {
            assert_eq!(machine.peek(0x8000 + offset as u16), *byte);
        }
    }
}

#[test]
fn rejected_media_preserves_the_complete_live_snapshot() {
    let mut runtime = CvRuntime::new(Model::CvNtsc, vec![0x76; 8192]).expect("BIOS");
    let good = cart([0xaa, 0x55]);
    runtime.load_media(&media(&good)).expect("mount control");
    let mut session = HeadlessSession::new(runtime, Model::CvNtsc.frame_ticks());
    session.run_frames(1).expect("run before replacement");
    session
        .machine_mut()
        .machine_mut()
        .expect("machine")
        .poke(0x6000, 0x42);
    let before = session.machine().snapshot().expect("live state");
    let mut bad = (0..0x24)
        .map(|len| good[..len].to_vec())
        .collect::<Vec<_>>();
    bad.push(vec![0; 8192]);
    bad.push(cart([0xaa, 0xaa]));
    for bytes in bad {
        let error = session
            .machine_mut()
            .load_media(&media(&bytes))
            .expect_err("invalid cartridge");
        assert!(
            matches!(error, MachineError::InvalidMedia { ref slot, .. } if slot == "cartridge-1")
        );
        assert_eq!(
            session.machine().snapshot().expect("after rejection"),
            before
        );
    }
    // Validate the whole request before applying any replacement.
    let replacement = cart([0x55, 0xaa]);
    let mut mixed = media(&replacement);
    mixed.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &[]));
    assert!(session.machine_mut().load_media(&mixed).is_err());
    assert_eq!(
        session.machine().snapshot().expect("after mixed request"),
        before
    );
}

#[test]
fn raw_insertion_and_saved_diagnostic_cartridges_remain_supported() {
    let mut runtime = CvRuntime::new(Model::CvNtsc, vec![0x76; 8192]).expect("BIOS");
    runtime.insert_cartridge(vec![0x5a; 8192]);
    let state = runtime.snapshot().expect("raw snapshot");
    let mut restored = CvRuntime::blank(Model::CvNtsc);
    restored
        .restore(&state)
        .expect("existing states stay loadable");
    assert_eq!(
        restored.machine().expect("restored machine").peek(0x8000),
        0x5a
    );
}

#[test]
#[ignore = "FIXTURE: requires original ColecoVision BIOS in EMU198X_COLECO_BIOS"]
fn original_bios_executes_a_valid_test_cartridge() {
    let bios = std::fs::read(std::env::var_os("EMU198X_COLECO_BIOS").expect("BIOS path"))
        .expect("BIOS bytes");
    let mut bytes = cart([0x55, 0xaa]);
    bytes[0x0a..0x0c].copy_from_slice(&0x8024_u16.to_le_bytes());
    // Original guest: LD A,$42; LD ($6000),A; HALT.
    bytes.extend_from_slice(&[0x3e, 0x42, 0x32, 0x00, 0x60, 0x76]);
    let mut runtime = CvRuntime::new(Model::CvNtsc, bios).expect("original BIOS");
    runtime
        .load_media(&media(&bytes))
        .expect("normal media path");
    let mut session = HeadlessSession::new(runtime, Model::CvNtsc.frame_ticks());
    session.run_frames(2).expect("BIOS handoff");
    assert_eq!(
        session.machine().machine().expect("machine").peek(0x6000),
        0x42
    );
}
