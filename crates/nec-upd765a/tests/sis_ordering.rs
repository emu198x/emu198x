//! NEC Dec1978p16 / A/B Invalid section: acknowledge seek IRQ before another command.
//! Fuse deliberately relaxes this rule; its compatibility exception is retained
//! as a reference disagreement, not silently copied into the documented contract.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, Upd765a};

#[test]
fn unacknowledged_seek_rejects_other_command_without_losing_status() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    // Explicit fastest programmed rate; this fixture tests framing/PCN, not SRT.
    for byte in [3, 0xf0, 3] {
        fdc.write_data(byte);
    }
    fdc.insert_disk(0, DiskImage::default());
    for b in [0x0f, 0, 7] {
        fdc.write_data(b);
    }
    for hc in 0..200_000 {
        fdc.tick(hc);
    }
    fdc.write_data(4);
    fdc.write_data(0);
    assert_eq!(
        fdc.read_data(),
        0x80,
        "SDS must be Invalid before seek acknowledgement"
    );
    assert_eq!(
        fdc.read_status(),
        0x81,
        "Invalid preserves unacknowledged Seek busy"
    );
    fdc.write_data(8);
    assert_eq!(fdc.read_data(), 0x20);
    assert_eq!(
        fdc.read_data(),
        7,
        "rejection must preserve the pending record"
    );
    fdc.write_data(4);
    fdc.write_data(0);
    assert_ne!(fdc.read_data() & 0x20, 0, "SDS is accepted after SIS");
}
