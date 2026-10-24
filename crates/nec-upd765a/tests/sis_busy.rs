use nec_upd765a::{DiskImage, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn controller() -> Upd765a {
    let mut f = Upd765a::new(SPECTRUM_128_CPU_HZ);
    // Explicit fastest programmed rate; this fixture tests framing/PCN, not SRT.
    for byte in [3, 0xf0, 3] {
        f.write_data(byte);
    }
    f.insert_disk(0, DiskImage::default());
    f.insert_disk(1, DiskImage::default());
    for _ in 0..8000 {
        f.tick();
    }
    for unit in 0..2 {
        command(&mut f, &[8]);
        assert_eq!([f.read_data(), f.read_data()], [0xc0 | unit, 0]);
    }
    f
}
fn complete(f: &mut Upd765a) {
    for _ in 0..200_000 {
        f.tick();
    }
    assert!(f.interrupt);
}
fn command(f: &mut Upd765a, bytes: &[u8]) {
    for b in bytes {
        f.write_data(*b);
    }
}
#[test]
fn completed_seek_busy_clears_on_st0_before_pcn() {
    let mut f = controller();
    command(&mut f, &[15, 0, 7]);
    complete(&mut f);
    assert_eq!(f.read_status(), 0x81, "completion awaits acknowledgement");
    command(&mut f, &[8]);
    assert_eq!(f.read_status(), 0xd1, "unread ST0 retains busy");
    assert_eq!(f.read_data(), 0x20);
    assert_eq!(
        f.read_status(),
        0xd0,
        "ST0 acknowledges drive; PCN still unread"
    );
    assert_eq!(f.read_data(), 7);
    assert_eq!(f.read_status(), 0x80);
}
#[test]
fn sis_clears_only_its_own_logical_drive_busy() {
    let mut f = controller();
    command(&mut f, &[15, 0, 3]);
    command(&mut f, &[15, 1, 5]);
    complete(&mut f);
    assert_eq!(f.read_status(), 0x83);
    command(&mut f, &[8]);
    assert_eq!(f.read_status(), 0xd3);
    assert_eq!(f.read_data(), 0x20);
    assert_eq!(f.read_status(), 0xd2);
    assert_eq!(f.read_data(), 3);
    assert_eq!(f.read_status(), 0x82);
    command(&mut f, &[8]);
    assert_eq!(f.read_status(), 0xd2);
    assert_eq!(f.read_data(), 0x21);
    assert_eq!(f.read_status(), 0xd0);
    assert_eq!(f.read_data(), 5);
    assert_eq!(f.read_status(), 0x80);
}
#[test]
fn recalibrate_busy_survives_completion_and_reset_clears_it() {
    let mut f = controller();
    command(&mut f, &[7, 1]);
    complete(&mut f);
    assert_eq!(f.read_status(), 0x82);
    command(&mut f, &[8]);
    assert_eq!(f.read_status(), 0xd2);
    assert_eq!(f.read_data(), 0x21);
    assert_eq!(f.read_status(), 0xd0);
    f.reset();
    assert_eq!(f.read_status(), 0x80);
    assert!(!f.interrupt);
}
