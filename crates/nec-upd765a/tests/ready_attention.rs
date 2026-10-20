//! Attention events through the existing documented image-presence input.
//! This compatibility-input test does not equate insertion with physical READY.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, Upd765a};

#[test]
fn changed_legacy_ready_level_produces_attention_without_a_seek() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    for byte in [3, 0xdf, 1] {
        fdc.write_data(byte);
    }
    for hc in 0..10_000 {
        fdc.tick(hc);
    }
    fdc.insert_disk(0, DiskImage::default());
    for hc in 10_000..20_000 {
        fdc.tick(hc);
    }
    fdc.write_data(4);
    fdc.write_data(0);
    assert_ne!(fdc.read_data() & 0x20, 0, "existing input reports READY");
    fdc.write_data(8);
    assert_eq!(
        fdc.read_data(),
        0xc0,
        "READY attention is not a no-pending Invalid result"
    );
    assert_eq!(fdc.read_status(), 0xd0, "READY includes PCN");
    assert_eq!(fdc.read_data(), 0);
    assert_eq!(fdc.read_status(), 0x80);
}
