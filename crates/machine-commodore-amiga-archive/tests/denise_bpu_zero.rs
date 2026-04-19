//! BPU=0 must blank the playfield.
//!
//! Per the Hardware Reference Manual, BPLCON0 BPU bits 14:12 == 0 means
//! "no bitplanes displayed". Real Denise (per WinUAE `getlinetype()` in
//! `drawing.cpp`) treats `GET_PLANES(bplcon0) == 0` as `LINETYPE_BORDER` —
//! the entire line is background colour (COLOR00) only. Bitplane shift
//! registers must not feed the colour lookup when BPU=0.
//!
//! See `wiki/decisions/amiga-denise-bpu-zero-rendering.md`.
//!
//! Strategy: bypass the CPU and Copper entirely. Build a minimal Amiga,
//! halt the CPU with `STOP #$2700`, set BPLCON0 to BPU=0+COLOR=1, and
//! seed Denise's bitplane shift registers with a recognisable pattern
//! at the start of every scanline. The framebuffer must be uniformly
//! COLOR00 — any non-COLOR00 pixel proves stale shift-register data
//! leaked through despite BPU=0.

use machine_commodore_amiga::Amiga;

/// Build a minimal kickstart that immediately halts the CPU.
/// Reset vector: SSP = $0010_0000, PC = $0000_0008.
/// Instruction at PC: STOP #$2700 (`0x4E72_2700`).
fn halting_kickstart() -> Vec<u8> {
    // 256 KiB ROM image, mostly zeros (overlay maps low chip RAM to ROM
    // for the boot reset vector fetch).
    let mut rom = vec![0u8; 256 * 1024];
    // SSP at offset 0..4 = $0010_0000 (just below ROM in chip RAM).
    rom[0x00] = 0x00;
    rom[0x01] = 0x10;
    rom[0x02] = 0x00;
    rom[0x03] = 0x00;
    // PC at offset 4..8 = $0000_0008
    rom[0x04] = 0x00;
    rom[0x05] = 0x00;
    rom[0x06] = 0x00;
    rom[0x07] = 0x08;
    // Instruction at $0000_0008 = STOP #$2700
    rom[0x08] = 0x4E;
    rom[0x09] = 0x72;
    rom[0x0A] = 0x27;
    rom[0x0B] = 0x00;
    rom
}

/// Configure a clean palette and zero BPU so that any non-COLOR00 pixel
/// in the framebuffer can only come from the bitplane shift registers.
fn arm_bpu_zero_palette(amiga: &mut Amiga) {
    // COLOR00 = white, COLOR01 = black. If BPU=0 is honoured the entire
    // framebuffer must be pure white. Any black pixel proves bitplane
    // data leaked through.
    amiga.denise.palette[0] = 0x0FFF;
    amiga.denise.palette[1] = 0x0000;

    // BPU=0, no HAM, no DBLPF, no LACE, no HIRES. Bit 9 (COLOR) on to
    // mirror the actual Kickstart 1.3 insert-disk state.
    amiga.agnus.bplcon0 = 0x0200;
    amiga.denise.bplcon0 = 0x0200;

    // Standard PAL display window (Workbench-ish).
    amiga.agnus.diwstrt = 0x2C81;
    amiga.agnus.diwstop = 0xF4C1;

    // No bitplane DMA — we want to prove that even without DMA, stale
    // shift-register contents must not bleed through.
    amiga.agnus.dmacon = 0x0200; // DMAEN only

    // Standard fetch window (irrelevant when BPLEN is off, but set for
    // a clean configuration).
    amiga.agnus.ddfstrt = 0x0038;
    amiga.agnus.ddfstop = 0x00D0;
    amiga.agnus.bpl1mod = 0;
    amiga.agnus.bpl2mod = 0;
}

/// Seed Denise's bitplane shift registers with data, simulating the
/// state after a previous frame fetched bitplanes via DMA. This models
/// the realistic case where software programs BPU=N, fetches a frame,
/// then sets BPU=0 — the shift registers still hold stale data.
///
/// On real Denise, BPU=0 means "no bitplanes displayed" — the colour
/// lookup must NOT consume from the shift registers regardless of what
/// they contain. Per WinUAE `drawing.cpp::getlinetype()`,
/// `GET_PLANES(bplcon0) == 0` produces `LINETYPE_BORDER` (background only).
///
/// We seed only the public `bpl_shift` words and `shift_count`; the
/// per-plane `bpl_shift_count` is private but `shift_one_playfield_source_pixel`
/// already mirrors `shift_count` into the per-plane state via
/// `ensure_legacy_shift_state_compat`.
fn seed_bitplane_shift_registers(amiga: &mut Amiga) {
    // Fill plane 0 with alternating bits. If Denise feeds bpl_shift[0]
    // through the colour lookup despite BPU=0 we will see alternating
    // COLOR00/COLOR01 (white/black) pixels.
    for plane in 0..8 {
        amiga.denise.bpl_shift[plane] = if plane == 0 { 0xAAAA } else { 0x0000 };
        amiga.denise.bpl_data[plane] = amiga.denise.bpl_shift[plane];
    }
    amiga.denise.shift_count = 16;
}

#[test]
fn bpu_zero_renders_only_color00_with_stale_shift_registers() {
    let mut amiga = Amiga::new(halting_kickstart());

    // Let the CPU settle into supervisor STOP state.
    for _ in 0..2 {
        amiga.run_frame();
    }

    arm_bpu_zero_palette(&mut amiga);

    // PAL frame ≈ 312 lines × 227.5 CCK = 70980 CCK. Tick CCK-by-CCK
    // and re-seed shift registers at the start of every scanline so
    // the renderer always has fresh stale data to incorrectly leak —
    // proving Denise must blank regardless of shift-register contents.
    let cck_per_line = 227u32;
    let lines_per_frame = 312u32;
    for _line in 0..lines_per_frame {
        seed_bitplane_shift_registers(&mut amiga);
        for _ in 0..cck_per_line {
            amiga.tick_cck();
        }
    }

    let (fb_w, _fb_h) = amiga.framebuffer_size();
    let fb = amiga.framebuffer();

    // COLOR00 = white = ARGB $FFFF_FFFF.
    let expected = 0xFFFF_FFFFu32;

    // The bug, if present, leaks half the visible-area pixels every
    // line. Tolerance covers any tiny edge-case at the absolute raster
    // borders without hiding the real symptom.
    let tolerance = 64usize;

    let leaked: Vec<(u32, u32, u32)> = fb
        .iter()
        .enumerate()
        .filter_map(|(idx, &px)| {
            if px == expected {
                None
            } else {
                let x = (idx as u32) % fb_w;
                let y = (idx as u32) / fb_w;
                Some((x, y, px))
            }
        })
        .collect();

    if leaked.len() > tolerance {
        eprintln!(
            "BPU=0 leaked {} non-COLOR00 pixels out of {} (fb width {})",
            leaked.len(),
            fb.len(),
            fb_w,
        );
        for (x, y, px) in leaked.iter().take(8) {
            eprintln!("  ({x:4}, {y:4}) = ${px:08X}");
        }
        panic!(
            "Denise rendered bitplane data with BPU=0 — see \
             wiki/decisions/amiga-denise-bpu-zero-rendering.md"
        );
    }
}
