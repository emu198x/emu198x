//! Project-owned cartridge reaches the physical board's display path.
use machine_sega_master_system_2_ntsc::Sms;
fn boot(rom: Vec<u8>) -> Sms {
    let mut m = Sms::new(rom);
    for _ in 0..5 {
        m.run_frame();
    }
    m
}
#[test]
fn cartridge_paints_this_boards_display() {
    let m = boot(
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../test-data/sega/synthetic-cart/master-system.sms"),
        )
        .expect("project cartridge"),
    );
    assert_eq!((m.framebuffer_width(), m.framebuffer_height()), (280, 240));
    assert!(!m.framebuffer().is_empty());
    assert!(m.framebuffer().iter().all(|&p| p == 0xFF00_FF00));
}
#[test]
fn empty_cartridge_does_not_pass_the_boot_check() {
    let m = boot(vec![0; 32768]);
    assert!(m.framebuffer().iter().any(|&p| p != 0xFF00_FF00));
}
