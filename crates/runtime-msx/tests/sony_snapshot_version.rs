//! Approved Sony v11 boundary and unchanged neighbouring-board versions.
use emu198x_shell::{MachineCore, MachineError};
use runtime_msx::{Model, MsxRuntime};
use sha2::{Digest, Sha256};
#[test]
fn all_nine_boards_keep_the_selected_version_and_reject_other_versions_atomically() {
    assert_eq!(Model::ALL.len(), 9);
    for model in Model::ALL {
        let mut r = MsxRuntime::blank(model);
        let expected = if model == Model::SonyHb10p { 11 } else { 10 };
        let before = r.snapshot().expect("current");
        let (version, _) = postcard::take_from_bytes::<u16>(&before).expect("header");
        assert_eq!(version, expected);
        r.restore(&before).expect("current round trip");
        assert_eq!(r.snapshot().expect("same bytes"), before);
        let rejected = if expected == 11 { 10 } else { 11 };
        assert!(
            matches!(r.restore(&[rejected]),Err(MachineError::InvalidSnapshot{reason})
            if reason == format!("unsupported snapshot version {rejected}; expected {expected}"))
        );
        assert_eq!(r.snapshot().expect("atomic rejection"), before);
    }
}
#[test]
fn original_sony_specimens_are_immutable_and_v10_states_reject_before_decode() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/msx/board-split-v10");
    let records = [
        (
            "after.state",
            "e7e24c137195fb55469279117f00b135d855fdcf68912b1e336b74d6d8b50c28",
        ),
        (
            "audio",
            "79edc736f0bbab76fbf66ca90bcb218d23e2839be536dc7095e797e4eaf2fb29",
        ),
        (
            "frames",
            "ea936bf91ccd35cd02753f5380fe2b50670d6e6eb1ef49248a94fcadd4091581",
        ),
        (
            "state",
            "96aee8c00221b51cebf3f4b97eda5c15ec53788508957f92f8e6e020cd7563a8",
        ),
    ];
    let mut r = MsxRuntime::blank(Model::SonyHb10p);
    let before = r.snapshot().expect("before");
    for (suffix, expected) in records {
        let bytes =
            std::fs::read(root.join(format!("sony-hb-10p.{suffix}"))).expect("original specimen");
        let hash = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(hash, expected, "original {suffix}");
        if suffix.ends_with("state") {
            assert!(
                matches!(r.restore(&bytes),Err(MachineError::InvalidSnapshot{reason})
                if reason == "unsupported snapshot version 10; expected 11")
            );
            assert_eq!(r.snapshot().expect("atomic rejection"), before);
        }
    }
}

#[test]
fn aliased_sony_region_does_not_invent_another_sixteen_kib_of_ram() {
    assert_eq!(Model::SonyHb10p.ram_kb(), 64);
}

#[test]
fn sony_decoder_is_reattached_for_restore_and_reset() {
    use emu198x_shell::{HeadlessSession, ResetKind};
    let mut program = vec![0xf3, 0x3e, 0x80, 0xd3, 0xaf, 0x2e, 1];
    let loop_address = program.len() as u16;
    program.extend([
        0x3e, 0xc0, 0xd3, 0xac, 0x7d, 0x32, 0x00, 0xc0, 0x3e, 0, 0xd3, 0xac, 0x3a, 0x00, 0xc0,
        0x47, 0x2c, 0xc3,
    ]);
    program.extend(loop_address.to_le_bytes());
    program.resize(32768, 0);
    let mut s = HeadlessSession::new(MsxRuntime::new(Model::SonyHb10p, program).expect("Sony"), 1);
    s.run_frames(1).expect("guest");
    let saved = s.machine().snapshot().expect("live loop");
    let mut r = MsxRuntime::blank(Model::SonyHb10p);
    r.restore(&saved).expect("restore");
    let mut restored = HeadlessSession::new(r, 1);
    s.run_frames(2).expect("uninterrupted");
    restored.run_frames(2).expect("restored");
    assert_eq!(
        s.machine().snapshot().expect("expected"),
        restored.machine().snapshot().expect("actual")
    );
    assert_ne!(s.machine().snapshot().expect("progress"), saved);
    restored.machine_mut().reset(ResetKind::Hard);
    restored
        .run_frames(3)
        .expect("cold boot with immutable decoder");
    assert_eq!(
        s.machine().snapshot().expect("fresh"),
        restored.machine().snapshot().expect("reset")
    );
}
