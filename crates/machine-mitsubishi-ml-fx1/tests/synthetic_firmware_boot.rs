//! This named board executes project-owned firmware through its actual video generation.
use machine_mitsubishi_ml_fx1::Msx;
fn bios() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/synthetic-firmware/msx.rom"),
    )
    .expect("project BIOS")
}
fn boot(bios: Vec<u8>) -> Msx {
    let mut m = Msx::try_new(bios, None).expect("fixed firmware kind");
    for _ in 0..5 {
        m.run_frame();
    }
    m
}
#[test]
fn named_board_runs_firmware_and_paints_its_display() {
    let m = boot(bios());
    assert!(!m.framebuffer().is_empty());
    assert!(m.framebuffer().iter().all(|&p| p == 0xFFFF_FFFF));
}
#[test]
fn unexecuted_firmware_does_not_pass_the_boot_check() {
    let mut b = bios();
    b[..2].copy_from_slice(&[0x18, 0xFE]);
    let m = boot(b);
    assert!(m.framebuffer().iter().any(|&p| p != 0xFFFF_FFFF));
}
