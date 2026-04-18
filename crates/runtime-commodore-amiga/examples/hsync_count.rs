//! Count how often hpos==0 fires during one frame to debug CIA-B TOD undercounting.

use machine_commodore_amiga::Amiga;
use std::fs;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);
    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);
    println!("ccks_per_frame={ccks_per_frame}");

    // Sample every 10 frames: CIA-B TOD advance + halted state.
    let mut prev_tod = 0u32;
    for frame in 1..=200 {
        for _ in 0..ccks_per_frame {
            amiga.tick_cck();
        }
        if frame % 10 == 0 {
            let tod = amiga.cia_b.tod_counter();
            let halted = amiga.cia_b.tod_halted();
            println!("frame={frame:>3}  CIA-B TOD=${tod:06X} ({tod:>6}) Δ{:+>6}  halted={halted}",
                tod as i64 - prev_tod as i64);
            prev_tod = tod;
        }
    }
}
