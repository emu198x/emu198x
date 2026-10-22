use super::*;

fn command(f: &mut Upd765a, bytes: &[u8]) {
    for &byte in bytes {
        f.write_data(byte);
    }
}
fn prepared() -> Upd765a {
    let mut f = Upd765a::new();
    f.enabled = true;
    for unit in 0..4 {
        f.set_ready(unit, true);
    }
    for _ in 0..100_000 {
        f.tick(0);
    }
    for unit in 0..4 {
        command(&mut f, &[8]);
        assert_eq!([f.read_data(), f.read_data()], [0xc0 | unit, 0]);
    }
    command(&mut f, &[3, 0xf0, 3]);
    f
}
fn advance(f: &mut Upd765a, ticks: u64) {
    for _ in 0..ticks {
        f.tick(0);
    }
}
fn sis(f: &mut Upd765a) -> [u8; 2] {
    command(f, &[8]);
    [f.read_data(), f.read_data()]
}
fn interval() -> u64 {
    2_000 * common_sinclair_zx_spectrum::timing::CPU_HZ_128K
}

#[test]
fn physical_and_logical_position_change_one_step_at_each_rational_boundary() {
    let mut f = prepared();
    command(&mut f, &[15, 0, 3]);
    assert_eq!((f.track[0], f.pcn[0]), (1, 1));
    let first = interval().div_ceil(1_000_000);
    advance(&mut f, first - 1);
    assert_eq!((f.track[0], f.pcn[0]), (1, 1));
    f.tick(0);
    assert_eq!((f.track[0], f.pcn[0]), (2, 2));
    let second = (2 * interval()).div_ceil(1_000_000);
    advance(&mut f, second - first);
    assert_eq!((f.track[0], f.pcn[0]), (3, 3));
    assert!(!f.interrupt, "last step still needs its final interval");
    let end = (3 * interval()).div_ceil(1_000_000);
    advance(&mut f, end - second);
    assert_eq!(sis(&mut f), [0x20, 3]);
    command(&mut f, &[15, 0, 0]);
    assert_eq!((f.track[0], f.pcn[0]), (2, 2));
    advance(&mut f, second);
    assert_eq!((f.track[0], f.pcn[0]), (0, 0));
    assert!(f.head_at_track_zero(0));
    advance(&mut f, end - second);
    assert_eq!(sis(&mut f), [0x20, 0]);
}

#[test]
fn recalibrate_checks_t0_before_steps_and_terminates_after_seventy_seven() {
    let mut f = prepared();
    f.track[0] = 5;
    f.pcn[0] = 9;
    f.set_track_zero(0, true);
    command(&mut f, &[7, 0]);
    assert_eq!((f.track[0], f.pcn[0]), (5, 0));
    assert_eq!(sis(&mut f), [0x20, 0]);
    f.set_track_zero(0, false);
    command(&mut f, &[7, 0]);
    assert_eq!(f.track[0], 4);
    assert_eq!(f.motion[0].expect("active").pulses, 1);
    advance(&mut f, (76 * interval()).div_ceil(1_000_000));
    assert_eq!(f.motion[0].expect("last pulse interval").pulses, 77);
    assert!(!f.interrupt);
    assert_eq!(f.track[0], 0);
    advance(
        &mut f,
        (77 * interval()).div_ceil(1_000_000) - (76 * interval()).div_ceil(1_000_000),
    );
    assert_eq!(sis(&mut f), [0x70, 0]);
    command(&mut f, &[7, 0]);
    f.set_track_zero(0, true);
    advance(&mut f, interval().div_ceil(1_000_000));
    assert_eq!(sis(&mut f), [0x20, 0]);
}

#[test]
fn readiness_and_t0_use_logical_samples_while_aliases_move_one_physical_head() {
    let mut f = prepared();
    f.set_drive_select_mask(1);
    // Distinct supplied slots are useful standalone input controls; +3's
    // adapter supplies the same physical levels to all wired aliases.
    f.set_ready(0, false);
    f.set_track_zero(0, true);
    f.set_track_zero(2, false);
    advance(&mut f, 100_000);
    assert_eq!(sis(&mut f), [0xc8, 0]);
    command(&mut f, &[15, 2, 3]);
    assert_eq!((f.track[0], f.pcn[2]), (1, 1));
    f.set_ready(2, false);
    f.tick(0);
    assert_eq!(sis(&mut f), [0x6a, 1]);
    assert_eq!(f.track[0], 1);
    f.set_ready(2, true);
    command(&mut f, &[7, 2]);
    assert_eq!(f.track[0], 0);
    assert!(f.motion[2].is_some(), "slot0 T0 cannot terminate slot2");
    f.set_track_zero(2, true);
    advance(&mut f, interval().div_ceil(1_000_000));
    assert_eq!(sis(&mut f), [0x22, 0]);
}

#[test]
fn concurrent_aliases_keep_independent_pcns_completions_and_fractional_phase() {
    let mut f = prepared();
    f.set_drive_select_mask(1);
    command(&mut f, &[15, 0, 2]);
    command(&mut f, &[15, 2, 3]);
    assert_eq!(f.track[0], 2);
    assert_eq!(f.read_status(), 0x85);
    advance(&mut f, (2 * interval()).div_ceil(1_000_000));
    assert_eq!(f.track[0], 5);
    assert_eq!(sis(&mut f), [0x20, 2]);
    assert_eq!(f.read_status(), 0x84);
    assert!(!f.interrupt);
    advance(
        &mut f,
        (3 * interval()).div_ceil(1_000_000) - (2 * interval()).div_ceil(1_000_000),
    );
    assert_eq!(sis(&mut f), [0x22, 3]);
    assert_eq!(f.read_status(), 0x80);
}

#[test]
fn reprogramming_preserves_current_interval_and_reset_aborts_without_more_steps() {
    let mut f = prepared();
    command(&mut f, &[3, 0x10, 3]);
    command(&mut f, &[15, 0, 3]);
    advance(&mut f, 1000);
    let remaining = f.motion[0].expect("active").remaining;
    command(&mut f, &[3, 0xf0, 3]);
    assert_eq!(f.motion[0].expect("active").remaining, remaining);
    let slow = 30_000 * common_sinclair_zx_spectrum::timing::CPU_HZ_128K;
    advance(&mut f, slow.div_ceil(1_000_000) - 1000);
    assert_eq!(f.pcn[0], 2);
    assert_eq!(
        f.motion[0].expect("next").remaining,
        interval() - (1_000_000 - slow % 1_000_000) % 1_000_000
    );
    let position = f.track[0];
    let pcn = f.pcn[0];
    f.reset();
    assert_eq!((f.track[0], f.pcn[0]), (position, pcn));
    advance(&mut f, 100_000);
    assert_eq!((f.track[0], f.pcn[0]), (position, pcn));
    assert!(f.motion.iter().all(Option::is_none));
    assert_eq!(f.step_rate, 0);
    // Reset may rediscover READY, but must not invent a seek completion.
    assert!(
        f.seek_pending
            .iter()
            .all(|st0| st0.is_none_or(|st0| st0 & 0x20 == 0))
    );
}

#[test]
fn asynchronous_seek_completion_cannot_change_an_overlapping_read_result() {
    let mut f = prepared();
    f.insert_disk(
        0,
        DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![DiskTrack {
                sectors: vec![DiskSector {
                    c: 0,
                    h: 0,
                    id: 1,
                    n: 0,
                    st1: 0,
                    st2: 0,
                    data: vec![0x5a; 128],
                }],
            }]],
        },
    );
    command(&mut f, &[15, 1, 1]);
    command(&mut f, &[0x46, 0, 0, 0, 1, 0, 1, 42, 128]);
    for _ in 0..128 {
        let mut waited = 0;
        while f.read_status() & 0xc0 != 0xc0 {
            assert!(waited < 10_000);
            f.tick(0);
            waited += 1;
        }
        assert_eq!(f.read_data(), 0x5a);
    }
    let mut result = [0; 7];
    for byte in &mut result {
        *byte = f.read_data();
    }
    assert_eq!(
        result,
        [0x40, 0x80, 0, 0, 0, 1, 0],
        "read retains unit0 and its own status"
    );
    assert_eq!(
        sis(&mut f),
        [0x21, 1],
        "independent unit1 completion survives"
    );
}

#[test]
fn no_track_zero_search_emits_exactly_seventy_seven_physical_steps() {
    let mut f = prepared();
    f.track[0] = 100;
    f.pcn[0] = 100;
    f.set_track_zero(0, false);
    command(&mut f, &[7, 0]);
    assert_eq!(f.track[0], 99);
    advance(&mut f, (77 * interval()).div_ceil(1_000_000));
    assert_eq!(
        f.track[0], 23,
        "100 minus77 physical steps, without saturation"
    );
    assert_eq!(sis(&mut f), [0x70, 0]);
}
