use super::*;

fn at(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> DateTime {
    DateTime {
        year,
        month,
        day,
        weekday: 0,
        hour,
        minute,
        second,
    }
}

/// The time block as the CPU reads it, register by register.
fn time_block(rtc: &mut Rp5c01) -> [u8; 13] {
    rtc.write(MODE_REG, MODE_TIMER_ENABLE);
    std::array::from_fn(|reg| rtc.read(reg as u8))
}

fn run_seconds(rtc: &mut Rp5c01, seconds: u32) {
    for _ in 0..seconds * TICK_HZ {
        rtc.tick();
    }
}

#[test]
fn power_on_is_1980_in_twelve_hour_mode() {
    let mut rtc = Rp5c01::new();
    assert_eq!(
        rtc.date_time(),
        DateTime {
            weekday: 2,
            ..at(1980, 1, 1, 0, 0, 0)
        }
    );
    assert_eq!(
        time_block(&mut rtc),
        [0, 0, 0, 0, 0, 0, 2, 1, 0, 1, 0, 0, 0]
    );
    assert_eq!(rtc.read(MODE_REG), MODE_TIMER_ENABLE);
    assert_eq!(Rp5c01::default(), rtc);
}

#[test]
fn a_second_is_16384_ticks() {
    let mut rtc = Rp5c01::new();
    for _ in 0..TICK_HZ - 1 {
        rtc.tick();
    }
    assert_eq!(rtc.date_time().second, 0);
    rtc.tick();
    assert_eq!(rtc.date_time().second, 1);
    assert_eq!(rtc.read(0), 1);
}

#[test]
fn time_stops_without_the_timer_enable_bit() {
    let mut rtc = Rp5c01::new();
    rtc.write(MODE_REG, 0);
    run_seconds(&mut rtc, 2);
    assert_eq!(rtc.date_time().second, 0);
}

#[test]
fn the_time_block_is_bcd_low_digit_first() {
    let mut rtc = Rp5c01::new();
    rtc.write(MODE_REG, MODE_TIMER_ENABLE | 1);
    rtc.write(10, 1); // 24-hour mode
    rtc.set_date_time(DateTime {
        weekday: 5,
        ..at(2026, 10, 24, 17, 38, 59)
    });
    assert_eq!(
        time_block(&mut rtc),
        [9, 5, 8, 3, 7, 1, 5, 4, 2, 0, 1, 6, 4]
    );
}

#[test]
fn twelve_hour_mode_flags_the_afternoon() {
    let mut rtc = Rp5c01::new();
    rtc.set_date_time(at(1986, 1, 1, 13, 0, 0));
    let block = time_block(&mut rtc);
    assert_eq!((block[4], block[5]), (1, 2), "1 p.m. reads as 21");
    // Writing 12-hour digits back reads them as the afternoon.
    rtc.write(4, 3);
    assert_eq!(rtc.date_time().hour, 15);
}

#[test]
fn seconds_carry_through_to_a_new_year() {
    let mut rtc = Rp5c01::new();
    rtc.write(MODE_REG, MODE_TIMER_ENABLE | 1);
    rtc.write(10, 1);
    rtc.set_date_time(DateTime {
        weekday: 6,
        ..at(1999, 12, 31, 23, 59, 59)
    });
    run_seconds(&mut rtc, 1);
    assert_eq!(rtc.date_time(), at(2000, 1, 1, 0, 0, 0));
}

#[test]
fn february_has_29_days_only_in_a_leap_year() {
    let mut rtc = Rp5c01::new();
    rtc.set_date_time(at(1988, 2, 28, 23, 59, 59));
    run_seconds(&mut rtc, 1);
    assert_eq!((rtc.date_time().month, rtc.date_time().day), (2, 29));

    rtc.set_date_time(at(1987, 2, 28, 23, 59, 59));
    run_seconds(&mut rtc, 1);
    assert_eq!((rtc.date_time().month, rtc.date_time().day), (3, 1));
}

#[test]
fn writing_a_digit_moves_the_clock() {
    let mut rtc = Rp5c01::new();
    rtc.write(MODE_REG, MODE_TIMER_ENABLE);
    rtc.write(3, 4); // tens of minutes
    rtc.write(2, 2);
    assert_eq!(rtc.date_time().minute, 42);
    // Bits the register lacks are dropped.
    rtc.write(1, 0xF);
    assert_eq!(rtc.read(1), 0x7);
}

#[test]
fn the_alarm_block_has_its_own_masks() {
    let mut rtc = Rp5c01::new();
    rtc.write(MODE_REG, 1);
    rtc.write(0, 0xF);
    assert_eq!(rtc.read(0), 0, "the alarm has no seconds");
    rtc.write(2, 0x9);
    rtc.write(11, 0xF);
    assert_eq!((rtc.read(2), rtc.read(11)), (9, 3));
    assert_eq!(
        rtc.date_time().year,
        1980,
        "the leap counter is not the year"
    );
    rtc.write(RESET_REG, RESET_ALARM);
    assert_eq!(rtc.read(2), 0, "a reset clears the alarm");
}

#[test]
fn ram_blocks_hold_26_nibbles() {
    let mut rtc = Rp5c01::new();
    rtc.write(MODE_REG, 2);
    rtc.write(0, 0xFA);
    rtc.write(12, 0x3);
    rtc.write(MODE_REG, 3);
    rtc.write(12, 0x7);
    assert_eq!(rtc.read(12), 7);
    rtc.write(MODE_REG, 2);
    assert_eq!((rtc.read(0), rtc.read(12)), (0xA, 3));

    let ram = rtc.ram().to_vec();
    assert_eq!(ram.len(), 26);
    let mut other = Rp5c01::new();
    other.load_ram(&ram);
    assert_eq!(other.ram(), ram.as_slice());
}

#[test]
fn test_and_reset_registers_read_as_all_ones() {
    let mut rtc = Rp5c01::new();
    rtc.write(TEST_REG, 0);
    assert_eq!(rtc.read(TEST_REG), 0xF);
    assert_eq!(rtc.read(RESET_REG), 0xF);
}

#[test]
fn test_mode_advances_every_tick() {
    let mut rtc = Rp5c01::new();
    rtc.write(TEST_REG, TEST_SECONDS);
    for _ in 0..3 {
        rtc.tick();
    }
    assert_eq!(rtc.date_time().second, 3);

    rtc.write(TEST_REG, TEST_MINUTES | TEST_DAYS | TEST_YEARS);
    rtc.tick();
    let now = rtc.date_time();
    assert_eq!((now.minute, now.day, now.year), (1, 2, 1981));
}

#[test]
fn resetting_the_fraction_restarts_the_second() {
    let mut rtc = Rp5c01::new();
    for _ in 0..TICK_HZ - 1 {
        rtc.tick();
    }
    rtc.write(RESET_REG, RESET_FRACTION);
    rtc.tick();
    assert_eq!(rtc.date_time().second, 0);
}

#[test]
fn set_date_time_clamps() {
    let mut rtc = Rp5c01::new();
    rtc.set_date_time(DateTime {
        year: 3000,
        month: 0,
        day: 40,
        weekday: 9,
        hour: 30,
        minute: 70,
        second: 99,
    });
    assert_eq!(
        rtc.date_time(),
        DateTime {
            year: 2079,
            month: 1,
            day: 31,
            weekday: 6,
            hour: 23,
            minute: 59,
            second: 59,
        }
    );
}
