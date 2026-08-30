use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet};
use format198x_spectravideo_svi_cas::{BLOCK_MARKER, CasImage, encode};
use runtime_spectravideo_svi_328::{Model, Svi328Runtime};

fn test_cas() -> Vec<u8> {
    encode(&CasImage::new(vec![vec![0x42, 0x43]]).expect("image")).expect("encode")
}

#[test]
fn production_media_path_accepts_svi_cas() {
    let mut runtime = Svi328Runtime::blank(Model::Svi328Ntsc);
    let bytes = test_cas();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &bytes));
    runtime.load_media(&media).expect("load CAS");
}

#[test]
fn malformed_cas_is_invalid_media() {
    let mut runtime = Svi328Runtime::blank(Model::Svi328Ntsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &[1, 2, 3]));
    assert!(matches!(
        runtime.load_media(&media),
        Err(MachineError::InvalidMedia { slot, .. }) if slot == "tape-1"
    ));
}

#[test]
fn empty_marker_is_invalid_media() {
    let mut runtime = Svi328Runtime::blank(Model::Svi328Ntsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &BLOCK_MARKER));
    assert!(matches!(
        runtime.load_media(&media),
        Err(MachineError::InvalidMedia { .. })
    ));
}
