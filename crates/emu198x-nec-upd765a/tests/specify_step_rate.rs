use emu198x_nec_upd765a::Upd765a;

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn command(f: &mut Upd765a, bytes: &[u8]) {
    for &byte in bytes {
        f.write_data(byte);
    }
}
fn ready() -> Upd765a {
    let mut f = Upd765a::new(SPECTRUM_128_CPU_HZ);
    f.set_ready(0, true);
    for _ in 0..100_000 {
        f.tick();
    }
    command(&mut f, &[8]);
    assert_eq!([f.read_data(), f.read_data()], [0xc0, 0]);
    assert!(!f.interrupt);
    f
}
fn journey(f: &mut Upd765a, target: u8) -> u64 {
    command(f, &[15, 0, target]);
    let mut ticks = 0;
    while !f.interrupt {
        assert!(ticks < 2_000_000);
        f.tick();
        ticks += 1;
    }
    assert_eq!(f.read_status(), 0x81);
    command(f, &[8]);
    assert_eq!([f.read_data(), f.read_data()], [0x20, target]);
    assert_eq!(f.read_status(), 0x80);
    assert!(!f.interrupt);
    ticks
}
#[test]
fn specify_changes_seek_rate_and_distance_using_real_elapsed_ticks() {
    for srt in [0u8, 1, 14, 15] {
        let mut f = ready();
        command(&mut f, &[3, srt << 4, 3]);
        assert!(!f.interrupt);
        assert_eq!(f.read_status(), 0x80);
        let interval = 2_000 * u64::from(16 - srt) * SPECTRUM_128_CPU_HZ;
        let one = journey(&mut f, 1);
        let two = journey(&mut f, 3);
        let reverse = journey(&mut f, 1);
        let zero = journey(&mut f, 1);
        assert_eq!(one, interval.div_ceil(1_000_000));
        assert_eq!(two, (2 * interval).div_ceil(1_000_000));
        assert_eq!(reverse, two);
        assert_eq!(zero, 0);
    }
}
