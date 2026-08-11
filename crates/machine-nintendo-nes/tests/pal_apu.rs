//! blargg's `pal_apu_tests` — the APU suite that only runs on a PAL machine.
//!
//! ⚠ These ten ROMs were ungradeable at any clock the emulator could
//! produce until `Nes::new_with_region` existed: `ricoh-apu-2a03` had
//! `ApuRegion` and a PAL rate table, but the machine built NTSC only, so
//! running them at all meant running them at the wrong frequency.
//!
//! They use the older `$00F8` settle protocol (1 = pass), like their
//! NTSC counterparts in `blargg_apu_2005.07.30`.
//!
//! ⚠ **Seven of the ten discriminate by region** — they settle at 1 on
//! PAL and 2 or 3 on NTSC — so this suite really does test PAL timing
//! rather than merely running under it. Measured, not assumed: see
//! `probe_pal_roms_discriminate`, which was written before the gate was
//! trusted. The three that pass in both regions (`01.len_ctr`,
//! `02.len_table`, `03.irq_flag`) are region-insensitive; they are kept
//! because they still assert real APU behaviour, not because they say
//! anything about PAL.
//!
//! Run with:
//! ```sh
//! cargo test --release -p machine-nintendo-nes --test pal_apu -- --ignored
//! ```

use std::path::PathBuf;

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::{Nes, Region};

/// Generous: these settle within ~12M ticks, and PAL's slower CPU
/// against the same master budget needs a little more headroom.
const MAX_TICKS: u64 = 60_000_000;

/// The `$00F8` byte must hold steady this long to count as settled —
/// same reasoning as the sweep's, riding out inter-sub-test gaps.
const SETTLE_TICKS: u64 = 10_000_000;

fn root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

/// Run one ROM on a PAL machine and return the settled `$00F8` value.
fn run_pal(rel: &str) -> Option<u8> {
    let root = root()?;
    let bytes = std::fs::read(root.join("pal_apu_tests").join(rel)).ok()?;
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new_with_region(parsed.mapper, Region::Pal);

    let mut last = 0u8;
    let mut steady = 0u64;
    let mut saw_nonzero = false;
    while nes.master_clock() < MAX_TICKS {
        nes.tick();
        let v = nes.peek(0x00F8);
        if v == last {
            steady += 1;
        } else {
            last = v;
            steady = 0;
        }
        if v != 0 {
            saw_nonzero = true;
        }
        if saw_nonzero && steady >= SETTLE_TICKS {
            return Some(last);
        }
    }
    None
}

/// Same run on an NTSC machine, for use as a negative control.
fn run_ntsc(rel: &str) -> Option<u8> {
    let root = root()?;
    let bytes = std::fs::read(root.join("pal_apu_tests").join(rel)).ok()?;
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);
    let mut last = 0u8;
    let mut steady = 0u64;
    let mut saw_nonzero = false;
    while nes.master_clock() < MAX_TICKS {
        nes.tick();
        let v = nes.peek(0x00F8);
        if v == last {
            steady += 1;
        } else {
            last = v;
            steady = 0;
        }
        if v != 0 {
            saw_nonzero = true;
        }
        if saw_nonzero && steady >= SETTLE_TICKS {
            return Some(last);
        }
    }
    None
}

/// ⚠ The gate above is only worth having if these ROMs can tell the
/// regions apart. If they passed on an NTSC machine too, ten green
/// tests would prove nothing about PAL — the same trap as a
/// mode-selecting ROM whose held button failed to register.
///
/// Reports which ROMs discriminate rather than asserting a count, so
/// the answer is visible rather than encoded in a threshold.
#[test]
#[ignore = "diagnostic: shows which PAL ROMs discriminate by region"]
fn probe_pal_roms_discriminate() {
    if root().is_none() {
        emu198x_test_skip::skip!("nes-test-roms not found");
    }
    for rel in ROMS {
        let pal = run_pal(rel);
        let ntsc = run_ntsc(rel);
        let discriminates = pal == Some(1) && ntsc != Some(1);
        println!(
            "  {:<24} PAL={:?} NTSC={:?}  {}",
            rel,
            pal,
            ntsc,
            if discriminates {
                "discriminates"
            } else {
                "— same verdict both regions"
            }
        );
    }
}

const ROMS: &[&str] = &[
    "01.len_ctr.nes",
    "02.len_table.nes",
    "03.irq_flag.nes",
    "04.clock_jitter.nes",
    "05.len_timing_mode0.nes",
    "06.len_timing_mode1.nes",
    "07.irq_flag_timing.nes",
    "08.irq_timing.nes",
    "10.len_halt_timing.nes",
    "11.len_reload_timing.nes",
];

fn expect_pass(rel: &str) {
    if root().is_none() {
        eprintln!("nes-test-roms not found; skipping {rel}");
        return;
    }
    match run_pal(rel) {
        Some(1) => {}
        Some(code) => panic!("pal_apu_tests/{rel} failed: $00F8 settled at {code:#04X}"),
        None => panic!("pal_apu_tests/{rel} never settled"),
    }
}

macro_rules! pal_apu_test {
    ($name:ident, $rel:literal) => {
        #[test]
        #[ignore = "ROM run — requires test-suites/nes-test-roms"]
        fn $name() {
            expect_pass($rel);
        }
    };
}

pal_apu_test!(len_ctr, "01.len_ctr.nes");
pal_apu_test!(len_table, "02.len_table.nes");
pal_apu_test!(irq_flag, "03.irq_flag.nes");
pal_apu_test!(clock_jitter, "04.clock_jitter.nes");
pal_apu_test!(len_timing_mode0, "05.len_timing_mode0.nes");
pal_apu_test!(len_timing_mode1, "06.len_timing_mode1.nes");
pal_apu_test!(irq_flag_timing, "07.irq_flag_timing.nes");
pal_apu_test!(irq_timing, "08.irq_timing.nes");
pal_apu_test!(len_halt_timing, "10.len_halt_timing.nes");
pal_apu_test!(len_reload_timing, "11.len_reload_timing.nes");
