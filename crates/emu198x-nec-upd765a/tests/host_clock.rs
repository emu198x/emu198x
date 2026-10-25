//! The datasheet's intervals are real time, whatever rate the host ticks at.
//!
//! The +3 ticks the controller once per 3.5469 MHz CPU T-state and the CPC
//! once per 4 MHz CPU cycle. The same programmed step rate must take the same
//! number of milliseconds on both, so the tick counts differ by the ratio of
//! the two clocks.
use emu198x_nec_upd765a::Upd765a;

const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;
const AMSTRAD_CPC_CPU_HZ: u64 = 4_000_000;

fn command(f: &mut Upd765a, bytes: &[u8]) {
    for &byte in bytes {
        f.write_data(byte);
    }
}

fn ready(host_tick_hz: u64) -> Upd765a {
    let mut f = Upd765a::new(host_tick_hz);
    assert_eq!(f.host_tick_hz(), host_tick_hz);
    f.set_ready(0, true);
    for _ in 0..200_000 {
        f.tick();
    }
    command(&mut f, &[8]);
    assert_eq!([f.read_data(), f.read_data()], [0xc0, 0]);
    f
}

/// Host ticks from issuing Seek to its interrupt.
fn seek_ticks(f: &mut Upd765a, target: u8) -> u64 {
    command(f, &[15, 0, target]);
    let mut ticks = 0;
    while !f.interrupt {
        assert!(ticks < 4_000_000, "seek never finished");
        f.tick();
        ticks += 1;
    }
    command(f, &[8]);
    assert_eq!([f.read_data(), f.read_data()], [0x20, target]);
    ticks
}

#[test]
fn a_programmed_step_takes_the_same_time_at_any_host_clock() {
    for srt in [0u8, 8, 15] {
        let mut elapsed_ns = Vec::new();
        for host_tick_hz in [SPECTRUM_128_CPU_HZ, AMSTRAD_CPC_CPU_HZ] {
            let mut f = ready(host_tick_hz);
            command(&mut f, &[3, srt << 4, 3]);
            let ticks = seek_ticks(&mut f, 2);
            // Two steps of (16 - SRT) ms each at a 4 MHz FDC clock.
            let expected = (2 * 2_000 * u64::from(16 - srt) * host_tick_hz).div_ceil(1_000_000);
            assert_eq!(ticks, expected, "SRT {srt} at {host_tick_hz} Hz");
            elapsed_ns.push(ticks * 1_000_000_000 / host_tick_hz);
        }
        // Rounding up to whole host ticks costs at most one tick at either rate.
        let gap = elapsed_ns[0].abs_diff(elapsed_ns[1]);
        assert!(gap <= 300, "SRT {srt}: {elapsed_ns:?} differ by {gap} ns");
    }
}

#[test]
fn a_zero_host_rate_degrades_instead_of_panicking() {
    let f = Upd765a::new(0);
    assert_eq!(f.host_tick_hz(), 1);
}
