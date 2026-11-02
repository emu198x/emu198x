//! A500+ has an onboard clock independently of slow-RAM expansion.
use machine_commodore_amiga_ecs::{AmigaEcs, RamConfig};

#[test]
fn a500_plus_clock_is_guest_visible_with_or_without_slow_ram() {
    for ntsc in [false, true] {
        for slow_kb in [0, 512] {
            let ram = RamConfig {
                chip_kb: 1024,
                slow_kb,
                fast_kb: 0,
            };
            let mut machine = if ntsc {
                AmigaEcs::with_ram_config_ntsc(vec![0; 512 * 1024], ram)
            } else {
                AmigaEcs::with_ram_config(vec![0; 512 * 1024], ram)
            };
            // HOLD the calendar before writing the seconds-one register.
            machine.poke_word(0xDC0034, 1);
            machine.poke_word(0xDC0000, 5);
            assert_eq!(
                machine.read_word(0xDC0000) & 15,
                5,
                "A500+ onboard RTC: ntsc={ntsc}, slow_kb={slow_kb}"
            );
            assert!(machine.gary().rtc_present());
        }
    }
}
