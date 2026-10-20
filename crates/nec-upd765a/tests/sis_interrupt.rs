use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, Upd765a};

fn seek(fdc: &mut Upd765a, drive: u8, cylinder: u8) {
    for byte in [0x0f, drive, cylinder] {
        fdc.write_data(byte);
    }
}

fn complete_seeks(fdc: &mut Upd765a) {
    // Bound by observed drive-busy state, rather than restating SEEK_TICKS.
    for hc in 0..4096 {
        fdc.tick(hc);
        if fdc.read_status() & 0x0f == 0 {
            return;
        }
    }
    panic!("seek did not complete within 4096 controller ticks");
}

#[test]
fn sis_acknowledges_irq_without_discarding_other_drive_status() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    fdc.insert_disk(0, DiskImage::default());
    fdc.insert_disk(1, DiskImage::default());
    seek(&mut fdc, 1, 7);
    seek(&mut fdc, 0, 3);
    complete_seeks(&mut fdc);
    assert!(fdc.interrupt, "completed seeks must raise IRQ");

    fdc.write_data(0x08);
    assert!(!fdc.interrupt, "issuing SIS must acknowledge IRQ");
    assert_eq!(fdc.read_status(), 0xd0, "SIS enters result phase");
    assert_eq!(fdc.read_data(), 0x20);
    assert_eq!(fdc.read_status(), 0xd0, "PCN remains unread");
    assert_eq!(fdc.read_data(), 3);
    assert_eq!(fdc.read_status(), 0x80, "both results drain to idle");

    fdc.write_data(0x08);
    assert!(!fdc.interrupt);
    assert_eq!(
        fdc.read_data(),
        0x21,
        "drive 1 status survives acknowledgement"
    );
    assert_eq!(fdc.read_data(), 7);
    assert_eq!(fdc.read_status(), 0x80);
    fdc.write_data(0x08);
    assert_eq!(fdc.read_data(), 0x80, "no phantom pending status");
    assert_eq!(fdc.read_status(), 0x80);

    seek(&mut fdc, 1, 9);
    assert!(!fdc.interrupt, "a new seek has not completed yet");
    complete_seeks(&mut fdc);
    assert!(fdc.interrupt, "a later completion must reassert IRQ");
    fdc.write_data(0x08);
    assert!(!fdc.interrupt);
    assert_eq!(fdc.read_data(), 0x21);
    assert_eq!(fdc.read_data(), 9);
    assert_eq!(fdc.read_status(), 0x80);
}
