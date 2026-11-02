//! Onboard RTC decoding is independent of trapdoor RAM.
use common_commodore_amiga_ecs::{AgnusRegion, Configuration, EcsCore, RamConfig};
fn check<C: Configuration>() {
    for slow_kb in [0, 512] {
        let mut board = EcsCore::<C>::with_ram_config(
            vec![0; 512 * 1024],
            RamConfig {
                chip_kb: 1024,
                slow_kb,
                fast_kb: 0,
            },
        );
        board.poke_word(0xDC0034, 1);
        board.poke_word(0xDC0000, 5);
        assert_eq!(board.read_word(0xDC0000) & 15, 5);
        assert!(board.gary().rtc_present());
    }
}
#[test]
fn a500_plus_clock_is_guest_visible_with_or_without_slow_ram() {
    check::<LegacyPal>();
    check::<LegacyNtsc>();
}

struct LegacyPal;
impl Configuration for LegacyPal {
    const REGION: AgnusRegion = AgnusRegion::Pal;
    const HAS_GAYLE: bool = false;
    const MACHINE_ID: &'static str = "test-rtc-pal";
}

pub struct LegacyNtsc;
impl Configuration for LegacyNtsc {
    const REGION: AgnusRegion = AgnusRegion::Ntsc;
    const HAS_GAYLE: bool = false;
    const MACHINE_ID: &'static str = "test-ecs-ntsc";
}
