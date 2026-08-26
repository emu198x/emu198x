//! MMC5 screen output, and the gate for executing code from ExRAM.
//!
//! ⚠⚠ These three ROMs were briefly recorded as "rendering nothing,
//! possible MMC5 defect". They were not. **MMC5 keeps its nametable RAM
//! inside the mapper** and can map ExRAM or a fill tile into any of the
//! four slots, so the console's CIRAM — `ppu.nametable_ram()` — stays
//! entirely empty for every MMC5 ROM. The probe was reading a buffer
//! that could never contain anything. `Nes::effective_nametable()` asks
//! the mapper first and exists because of this.
//!
//! The framebuffer settled it: all three draw, and their screens match
//! Mesen2's byte for byte.
//!
//! `exram/mmc5exram.nes` is the interesting one. It copies its per-frame
//! bar-position code into MMC5 ExRAM at startup and executes it from
//! `$5C00-$5FFF` during VBLANK — "A proper emulator will be able to
//! handle this without any problems", as the ROM itself puts it. That is
//! worth an assertion rather than a note, so it has one below.
//!
//! Run the diagnostic with:
//! ```sh
//! cargo test --release -p machine-nintendo-nes --test mmc5_screen \
//!     -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;

fn root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

const ROMS: &[&str] = &[
    "mmc5test/mmc5test.nes",
    "mmc5test_v2/mmc5test.nes",
    "exram/mmc5exram.nes",
];

#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_mmc5_screens() {
    let Some(root) = root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    for rel in ROMS {
        let Ok(bytes) = std::fs::read(root.join(rel)) else {
            continue;
        };
        let parsed = parse_ines(&bytes).expect("parse iNES");
        let mut nes = Nes::new(parsed.mapper);

        // Bucket instruction-fetch addresses by 256-byte page, so a
        // spin loop shows up as one dominant page.
        let mut pages: BTreeMap<u16, u64> = BTreeMap::new();
        let mut last_pc: u16 = 0;
        let mut nt_writes = 0u64;
        let mut ppu_writes: BTreeMap<u16, u64> = BTreeMap::new();
        while nes.master_clock() < 40_000_000 {
            nes.tick();
            if nes.cpu.sync {
                last_pc = nes.cpu.addr;
                *pages.entry(nes.cpu.addr & 0xFF00).or_default() += 1;
            }
            // Writes the CPU aims at the PPU register file.
            if !nes.cpu.rw && (0x2000..=0x3FFF).contains(&nes.cpu.addr) {
                *ppu_writes.entry(0x2000 + (nes.cpu.addr & 7)).or_default() += 1;
                if nes.cpu.addr & 7 == 7 {
                    nt_writes += 1;
                }
            }
        }

        // The framebuffer is the ground truth for "did anything render".
        // `ppu.nametable_ram()` is NOT: MMC5 owns its own nametable RAM
        // inside the mapper, and the PPU routes reads and writes through
        // `mapper.nametable_read`/`nametable_write`, so the console's
        // CIRAM copy stays empty for every MMC5 ROM.
        let fb = nes.ppu.framebuffer();
        let distinct: std::collections::BTreeSet<u32> = fb.iter().copied().collect();
        let non_bg = fb.iter().filter(|&&p| p != fb[0]).count();

        let mut top: Vec<(u16, u64)> = pages.into_iter().collect();
        top.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        println!("\n═══ {rel} ═══");
        println!("  last PC   : ${last_pc:04X}");
        println!(
            "  framebuffer: {} distinct colours, {non_bg}/{} px differ from px0",
            distinct.len(),
            fb.len()
        );
        println!(
            "  ppu.nametable_ram() non-blank bytes: {}",
            nes.ppu.nametable_ram().iter().filter(|&&b| b != 0).count()
        );
        println!("  $2007 writes: {nt_writes}");
        let regs: Vec<String> = ppu_writes
            .iter()
            .map(|(a, n)| format!("${a:04X}×{n}"))
            .collect();
        println!("  PPU regs  : {}", regs.join(" "));
        println!("  hot pages :");
        for (page, n) in top.iter().take(6) {
            println!("      ${page:04X}  {n}");
        }
    }
}

/// Executing code out of MMC5 ExRAM works.
///
/// `mmc5exram.nes` copies its colour-bar routine into ExRAM at startup and
/// runs it from `$5C00-$5FFF` every VBLANK. Two things are asserted, and
/// both matter:
///
/// * the banner reaches the screen, read through the *effective*
///   nametable — via CIRAM it is invisible;
/// * the framebuffer is not a flat colour, which is what proves the
///   ExRAM-resident code actually ran. A ROM that drew its banner and
///   then died in ExRAM would still pass the first check alone.
#[test]
#[ignore = "FIXTURE: ROM run — requires test-suites/nes-test-roms"]
fn mmc5_executes_code_from_exram() {
    let Some(root) = root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let path = root.join("exram/mmc5exram.nes");
    let Ok(bytes) = std::fs::read(&path) else {
        emu198x_test_skip::skip!("ROM not present at {path:?}; skipping");
    };
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);
    while nes.master_clock() < 40_000_000 {
        nes.tick();
    }

    let nt = nes.effective_nametable();
    let text: String = nt
        .iter()
        .map(|&b| {
            if (0x20..=0x7E).contains(&b) {
                b as char
            } else {
                ' '
            }
        })
        .collect();
    assert!(
        text.contains("MMC5 Executable ExRAM Test"),
        "ExRAM test banner missing from the effective nametable"
    );

    let fb = nes.ppu.framebuffer();
    let distinct = fb.iter().collect::<std::collections::BTreeSet<_>>().len();
    assert!(
        distinct > 2,
        "framebuffer has only {distinct} distinct colours — the colour-bar \
         code running from ExRAM did not produce output"
    );
}

/// What the `dmc_tests` ROMs actually write.
///
/// ⚠ Mesen2 writes NOTHING to either nametable across 2400 frames on
/// these, so the long-held note that they "draw tile indices against a
/// CHR font" is wrong — there is no screen output to compare at all.
/// This records where their output does go.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_dmc_tests_output_channels() {
    let Some(root) = root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    for name in ["latency", "buffer_retained", "status", "status_irq"] {
        let Ok(bytes) = std::fs::read(root.join(format!("dmc_tests/{name}.nes"))) else {
            continue;
        };
        let parsed = parse_ines(&bytes).expect("parse iNES");
        let mut nes = Nes::new(parsed.mapper);
        let (mut ppu_w, mut apu_w, mut ram_w) = (0u64, 0u64, 0u64);
        let mut mask_writes = 0u64;
        while nes.master_clock() < 60_000_000 {
            nes.tick();
            if !nes.cpu.rw {
                match nes.cpu.addr {
                    0x2000..=0x3FFF => {
                        ppu_w += 1;
                        if nes.cpu.addr & 7 == 1 {
                            mask_writes += 1;
                        }
                    }
                    0x4000..=0x4017 => apu_w += 1,
                    0x0000..=0x1FFF => ram_w += 1,
                    _ => {}
                }
            }
        }
        let nt_nonzero = nes
            .effective_nametable()
            .iter()
            .filter(|&&b| b != 0)
            .count();
        println!(
            "  {name:<16} ppu_reg={ppu_w:<6} ($2001={mask_writes}) apu_reg={apu_w:<7} \
             ram={ram_w:<8} nametable_nonzero={nt_nonzero}"
        );
    }
}

/// What CRC do the dmc_dma_during_read4 ROMs print?
///
/// ⚠ These ROMs have several legal outputs (their source headers list
/// them), so a screen diff against one reference capture cannot grade
/// them. They end with `jsr print_crc`, and the header lists every
/// acceptable checksum — that is the real verdict channel.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_dma_read4_crcs() {
    let Some(root) = root() else { return };
    for name in [
        "dma_2007_read",
        "double_2007_read",
        "dma_4016_read",
        "dma_2007_write",
        "read_write_2007",
    ] {
        let Ok(bytes) = std::fs::read(root.join(format!("dmc_dma_during_read4/{name}.nes"))) else {
            continue;
        };
        let parsed = parse_ines(&bytes).expect("parse iNES");
        let mut nes = Nes::new(parsed.mapper);
        for _ in 0..300 {
            nes.run_frame();
        }
        let text: String = nes
            .effective_nametable()
            .iter()
            .map(|&b| {
                if (0x20..=0x7E).contains(&b) {
                    b as char
                } else {
                    ' '
                }
            })
            .collect();
        let compact: Vec<&str> = text.split_whitespace().collect();
        println!("  {name:<18} {}", compact.join(" "));
    }
}
