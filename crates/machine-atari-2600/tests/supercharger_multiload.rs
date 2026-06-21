//! Starpath Supercharger (AR) multi-load advance — full machine path.
//!
//! No *binary* multi-load game is available locally (the MAME softlist ships
//! single-load protos + FLAC tapes), so this hand-authors a minimal but faithful
//! two-load image that exercises the real multi-load handshake end-to-end:
//! load 0 asks the dummy BIOS for load 1 (`STA $FA; JMP $F800`), the BIOS
//! re-enters via the `$1850` fast-load hotspot, `load_into_ram` selects load 1
//! by `header[5]`, the RIOT-RAM pokes are applied, and load 1 runs and paints a
//! distinct background. The painted colour is compared against a plain 4K cart
//! running the same paint code, so the check is palette-agnostic and needs no
//! staged media — it runs in CI.

use std::collections::HashMap;

use machine_atari_2600::{Atari2600, Atari2600Region};

/// Build one AR load slot (`8448` bytes). The program is placed so that, under
/// the power-on bank configuration (config 0: low slot = RAM bank 2, high slot =
/// ROM), it executes at `$F400`. The header hands off `bankcfg = 0` and a start
/// address of `$F400`, and tags the slot with `load_num` (header[5]).
fn ar_load(load_num: u8, program: &[u8]) -> Vec<u8> {
    assert!(program.len() <= 256, "one page of program");
    let mut slot = vec![0u8; 8448];
    // The single described page lives in file page 0; the descriptor maps it to
    // RAM bank 2, page 4 — which is $1400 in the low slot (config 0), i.e. the
    // $F400 mirror the start vector points at.
    slot[0..program.len()].copy_from_slice(program);
    let h = 8192; // header offset within the slot
    slot[h] = 0x00; // header[0] start low  → $FE → JMP low byte
    slot[h + 1] = 0xF4; // header[1] start high → $FF → JMP high byte ⇒ $F400
    slot[h + 2] = 0x00; // header[2] bank config → $80 ⇒ config 0 (RAM bank 2 low, ROM high)
    slot[h + 3] = 1; // header[3] page count
    slot[h + 5] = load_num; // header[5] this slot's load number
    // header[0..8] must sum to 0x55 (header[4]/[6] stay 0).
    let partial = 0x00u8
        .wrapping_add(0xF4)
        .wrapping_add(0x00)
        .wrapping_add(1)
        .wrapping_add(load_num);
    slot[h + 7] = 0x55u8.wrapping_sub(partial);
    // Page 0 descriptor: bank 2, page 4 → (page << 2) | bank.
    let desc = (4u8 << 2) | 2;
    slot[h + 16] = desc;
    // Page checksum: sum(page) + descriptor + check == 0x55.
    let page_sum = program.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    slot[h + 64] = 0x55u8.wrapping_sub(page_sum.wrapping_add(desc));
    slot
}

/// A plain 4 KB NROM cart that runs `program` from `$F000` (reset vector).
fn nrom_4k(program: &[u8]) -> Vec<u8> {
    let mut rom = vec![0u8; 4096];
    rom[0..program.len()].copy_from_slice(program);
    rom[0x0FFC] = 0x00; // reset vector → $F000
    rom[0x0FFD] = 0xF0;
    rom
}

/// The most frequent pixel in the framebuffer (the background colour for a cart
/// that only writes COLUBK).
fn dominant_colour(fb: &[u32]) -> u32 {
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for &px in fb {
        *counts.entry(px).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(_, n)| n)
        .map(|(px, _)| px)
        .unwrap_or(0)
}

fn run_dominant(rom: Vec<u8>) -> u32 {
    let mut sys = Atari2600::new(rom, Atari2600Region::Ntsc).expect("init");
    sys.set_joystick_input(0xFF);
    sys.set_switch_input(0xFF);
    for _ in 0..30 {
        sys.run_frame();
    }
    dominant_colour(sys.framebuffer())
}

#[test]
fn multi_load_advances_to_the_next_load_on_the_machine() {
    // The paint program both loads end at: VSYNC/VBLANK off, COLUBK = green,
    // spin. Load 1 paints it; load 0 only asks for load 1.
    let paint_green: [u8; 13] = [
        0xA9, 0x00, // LDA #$00
        0x85, 0x00, // STA VSYNC
        0x85, 0x01, // STA VBLANK
        0xA9, 0xC8, // LDA #$C8  (green)
        0x85, 0x09, // STA COLUBK
        0x4C, 0x00, 0xF4, // JMP $F400  (spin)
    ];
    // Reference: the same paint code as a plain 4K cart (loops at $F000).
    let reference: [u8; 13] = [
        0xA9, 0x00, 0x85, 0x00, 0x85, 0x01, 0xA9, 0xC8, 0x85, 0x09, 0x4C, 0x00, 0xF0,
    ];
    let green = run_dominant(nrom_4k(&reference));
    assert_ne!(green & 0x00FF_FFFF, 0, "sanity: COLUBK $C8 is not black");

    // Load 0 requests load 1 and re-enters the BIOS; load 1 paints green.
    let load0: [u8; 7] = [
        0xA9, 0x01, // LDA #$01   (next load = 1)
        0x85, 0xFA, // STA $FA
        0x4C, 0x00, 0xF8, // JMP $F800  (BIOS multi-load entry)
    ];
    let mut image = ar_load(0, &load0);
    image.extend(ar_load(1, &paint_green));

    let result = run_dominant(image);
    assert_eq!(
        result, green,
        "after the multi-load advance, load 1's paint code ran (got {result:#010x}, \
         expected the reference green {green:#010x})"
    );

    // Negative control: a load 0 that paints red and spins (never asks for
    // load 1) must show red, not green. This proves load 0 really executes —
    // so the green above is a genuine load-0 → load-1 advance, not an artifact.
    let red: u8 = 0x44;
    let paint_red: [u8; 13] = [
        0xA9, 0x00, 0x85, 0x00, 0x85, 0x01, 0xA9, red, 0x85, 0x09, 0x4C, 0x00, 0xF4,
    ];
    let red_ref = run_dominant(nrom_4k(&[
        0xA9, 0x00, 0x85, 0x00, 0x85, 0x01, 0xA9, red, 0x85, 0x09, 0x4C, 0x00, 0xF0,
    ]));
    let no_advance = run_dominant(ar_load(0, &paint_red));
    assert_eq!(
        no_advance, red_ref,
        "load 0 alone paints its own colour (red)"
    );
    assert_ne!(
        no_advance, green,
        "without the advance the screen is not green"
    );
}
