//! Register-set conformance for the Sega VDP, derived from SMS Power's
//! "Development: VDP Registers" page (mirrored at
//! `reference/by-topic/vdp-sms/smspower-vdp-registers.txt`, retrieved
//! 2026-04-18, page last modified 2026-02-04) and the distillation at
//! `reference/by-topic/vdp-sms/vdp-sms-reference.md`.
//!
//! Every test drives the chip through its ports using the two write idioms
//! the reference gives, so what is under test is the register decode a game
//! actually exercises rather than our own field layout.

use sega_vdp::{SegaVdp, VdpRegion, VdpVariant};

const REGION: VdpRegion = VdpRegion::Ntsc;

fn vdp() -> SegaVdp {
    SegaVdp::new(REGION, VdpVariant::Sms2)
}

/// Reference, "Write idiom (register)": the first write carries the data,
/// the second is `10` plus the register number.
fn write_register(vdp: &mut SegaVdp, reg: u8, value: u8) {
    vdp.write_control(value);
    vdp.write_control(0x80 | (reg & 0x0F));
}

/// Reference, "Write idiom (VRAM address)": command code 01 sets a write
/// address, 00 a read address.
fn set_vram_address(vdp: &mut SegaVdp, addr: u16, write: bool) {
    vdp.write_control(addr as u8);
    let cmd = if write { 0x40 } else { 0x00 };
    vdp.write_control(((addr >> 8) as u8 & 0x3F) | cmd);
}

fn poke_vram(vdp: &mut SegaVdp, addr: u16, bytes: &[u8]) {
    set_vram_address(vdp, addr, true);
    for &b in bytes {
        vdp.write_data(b);
    }
}

/// Command code 11 sets a CRAM write address.
fn poke_cram(vdp: &mut SegaVdp, index: u8, value: u8) {
    vdp.write_control(index);
    vdp.write_control(0xC0);
    vdp.write_data(value);
}

/// One tile row in Mode 4's four-plane format — one byte per bitplane, bit 7
/// leftmost — with all eight pixels set to `colour`.
fn solid_row(colour: u8) -> [u8; 4] {
    let plane = |bit: u8| if colour & (1 << bit) != 0 { 0xFF } else { 0x00 };
    [plane(0), plane(1), plane(2), plane(3)]
}

fn solid_tile(colour: u8) -> [u8; 32] {
    let row = solid_row(colour);
    let mut tile = [0u8; 32];
    for r in 0..8 {
        tile[r * 4..r * 4 + 4].copy_from_slice(&row);
    }
    tile
}

/// Run out whatever frame the chip is part-way through, then scan the next
/// one from the top down to `line`, so the border fill and every line read
/// back afterwards belong to the same frame.
fn render_to(vdp: &mut SegaVdp, line: u32) {
    while !vdp.tick_scanline() {}
    for _ in 0..=line {
        vdp.tick_scanline();
    }
}

fn pixel(vdp: &SegaVdp, line: u32, x: u32) -> u32 {
    let index =
        (REGION.border_top() + line) * REGION.framebuffer_width() + REGION.border_left() + x;
    vdp.framebuffer()[index as usize]
}

/// SMS CRAM is `%00BBGGRR`; each two-bit channel expands `cc -> cccccccc`.
fn sms_argb(entry: u8) -> u32 {
    let level = |c: u32| c * 85;
    let r = level(u32::from(entry) & 3);
    let g = level((u32::from(entry) >> 2) & 3);
    let b = level((u32::from(entry) >> 4) & 3);
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

// ---------------------------------------------------------------------------
// Table base addresses
// ---------------------------------------------------------------------------

/// R2, Mode 4 192-line: "bits 1-3 of this register are multiplied by $800 to
/// get the base address". Move the table and the tile a pixel is fetched from
/// has to move with it.
#[test]
fn the_name_table_base_is_register_2_bits_3_to_1_times_0x800() {
    for bits in 1u8..8 {
        // Base $0000 would put the table on top of the patterns, so start at
        // the first base that clears them.
        let base = u16::from(bits) * 0x0800;
        let mut vdp = vdp();
        write_register(&mut vdp, 0, 0x04); // Mode 4
        write_register(&mut vdp, 1, 0x40); // display on
        write_register(&mut vdp, 2, bits << 1);
        poke_cram(&mut vdp, 1, 0x03); // background colour 1 = red
        poke_vram(&mut vdp, 0x0020, &solid_tile(1)); // pattern 1
        poke_vram(&mut vdp, base, &[0x01, 0x00]); // tile (0,0) -> pattern 1

        render_to(&mut vdp, 0);
        assert_eq!(
            pixel(&vdp, 0, 0),
            sms_argb(0x03),
            "R2 bits 3-1 = {bits} should put the name table at {base:#06X}"
        );
    }
}

/// R2 bit 0 is the SMS1 mask bit and bits 7-4 are unused, so neither may move
/// the table.
#[test]
fn the_unused_and_mask_bits_of_register_2_do_not_move_the_name_table() {
    for extra in [0x00u8, 0x01, 0xF0, 0xF1] {
        let reg2 = 0x0E | extra;
        let mut vdp = vdp();
        write_register(&mut vdp, 0, 0x04);
        write_register(&mut vdp, 1, 0x40);
        write_register(&mut vdp, 2, reg2); // bits 3-1 set -> $3800
        poke_cram(&mut vdp, 1, 0x03);
        poke_vram(&mut vdp, 0x0020, &solid_tile(1));
        poke_vram(&mut vdp, 0x3800, &[0x01, 0x00]);

        render_to(&mut vdp, 0);
        assert_eq!(
            pixel(&vdp, 0, 0),
            sms_argb(0x03),
            "R2 = {reg2:#04X} must still address $3800"
        );
    }
}

/// R5: "--bb bbbb 00iii iii : y-coordinate" — bits 6-1 supply address bits
/// 13-8, so the attribute table can sit on any 256-byte boundary.
#[test]
fn the_sprite_attribute_table_base_is_register_5_bits_6_to_1() {
    for page in [0x00u8, 0x01, 0x20, 0x3E, 0x3F] {
        let base = u16::from(page) << 8;
        let mut vdp = vdp();
        write_register(&mut vdp, 0, 0x04);
        write_register(&mut vdp, 1, 0x40);
        write_register(&mut vdp, 5, (page << 1) | 0x01); // bits 6-1 = page
        write_register(&mut vdp, 6, 0x00); // sprite patterns at $0000
        poke_cram(&mut vdp, 16 + 5, 0x03); // sprite colour 5 = red
        poke_vram(&mut vdp, 0x0020, &solid_tile(5)); // sprite pattern 1
        poke_vram(&mut vdp, base, &[7, 0xD0]); // sprite 0 at y=7, then end
        poke_vram(&mut vdp, base + 0x80, &[0, 1]); // x = 0, pattern 1

        render_to(&mut vdp, 8);
        assert_eq!(
            pixel(&vdp, 8, 0),
            sms_argb(0x03),
            "R5 page {page:#04X} should put the attribute table at {base:#06X}"
        );
    }
}

/// R6 bit 2 alone chooses the sprite pattern base: 0 -> $0000, 1 -> $2000.
#[test]
fn the_sprite_pattern_base_is_register_6_bit_2() {
    for (reg6, base) in [(0x00u8, 0x0000u16), (0x04, 0x2000)] {
        let mut vdp = vdp();
        write_register(&mut vdp, 0, 0x04);
        write_register(&mut vdp, 1, 0x40);
        write_register(&mut vdp, 5, 0xFF); // attribute table at $3F00
        write_register(&mut vdp, 6, reg6);
        poke_cram(&mut vdp, 16 + 5, 0x03);
        poke_vram(&mut vdp, base + 0x20, &solid_tile(5));
        poke_vram(&mut vdp, 0x3F00, &[7, 0xD0]);
        poke_vram(&mut vdp, 0x3F80, &[0, 1]);

        render_to(&mut vdp, 8);
        assert_eq!(
            pixel(&vdp, 8, 0),
            sms_argb(0x03),
            "R6 = {reg6:#04X} should fetch sprite patterns from {base:#06X}"
        );
    }
}

/// R7: "It is thus a 4-bit number, the upper 4 bits having no effect", and
/// the entry it names comes from the sprite half of CRAM (entries 16-31).
#[test]
fn the_backdrop_is_a_sprite_palette_entry_named_by_the_low_nibble_of_register_7() {
    let mut vdp = vdp();
    write_register(&mut vdp, 0, 0x04);
    write_register(&mut vdp, 1, 0x40);
    for entry in 0..16u8 {
        // Each sprite-palette entry gets a different colour and the
        // background half the opposite one, so a lookup in the wrong half of
        // CRAM cannot land on the right answer.
        poke_cram(&mut vdp, 16 + entry, entry);
        poke_cram(&mut vdp, entry, 0x3F - entry);
    }

    for entry in 0..16u8 {
        write_register(&mut vdp, 7, 0xF0 | entry); // upper nibble is noise
        render_to(&mut vdp, 0);
        assert_eq!(
            pixel(&vdp, 0, 0),
            sms_argb(entry),
            "R7 low nibble {entry} should select CRAM entry {}",
            16 + entry
        );
    }
}

/// "The VDP is unaffected when register $0B and above are written to."
#[test]
fn registers_0x0b_and_above_have_no_effect() {
    let mut vdp = vdp();
    write_register(&mut vdp, 0, 0x04);
    write_register(&mut vdp, 1, 0x40);
    let before = *vdp.registers();

    for reg in 0x0B..=0x0Fu8 {
        write_register(&mut vdp, reg, 0xFF);
    }
    assert_eq!(
        *vdp.registers(),
        before,
        "writes to $0B-$0F must not disturb R0-R10"
    );
}

// ---------------------------------------------------------------------------
// Colour RAM
// ---------------------------------------------------------------------------

/// SMS CRAM is six bits, `%00BBGGRR`, and each two-bit channel expands by
/// repeating its bits — so the four levels are $00, $55, $AA, $FF, and full
/// scale reaches white rather than stopping short of it.
#[test]
fn each_two_bit_cram_channel_expands_by_repeating_its_bits() {
    let mut vdp = vdp();
    write_register(&mut vdp, 0, 0x04);
    write_register(&mut vdp, 1, 0x40);
    write_register(&mut vdp, 7, 0x00); // backdrop = CRAM entry 16

    for level in 0..4u8 {
        let expanded = u32::from(level) * 0x55;
        for (shift, name, mask) in [
            (0u8, "red", 0x00FF_0000u32),
            (2, "green", 0x0000_FF00),
            (4, "blue", 0x0000_00FF),
        ] {
            poke_cram(&mut vdp, 16, level << shift);
            render_to(&mut vdp, 0);
            let argb = pixel(&vdp, 0, 0);
            assert_eq!(
                (argb & mask) >> mask.trailing_zeros(),
                expanded,
                "{name} level {level} should expand to {expanded:#04X}"
            );
            assert_eq!(
                argb & !mask,
                0xFF00_0000,
                "{name} level {level} must not bleed into the other channels"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// R0 scroll locks
// ---------------------------------------------------------------------------

/// A striped scene: even name-table rows draw colour 1, odd rows colour 2.
/// A vertical scroll of 8 swaps the two everywhere, so any line of the
/// picture reports which scroll value was in force when it was drawn.
fn scroll_scene(reg0: u8, reg9: u8) -> SegaVdp {
    let mut vdp = vdp();
    write_register(&mut vdp, 0, 0x04 | reg0);
    write_register(&mut vdp, 1, 0x40);
    write_register(&mut vdp, 2, 0x0E); // name table $3800
    write_register(&mut vdp, 9, reg9);
    poke_cram(&mut vdp, 1, 0x03); // red
    poke_cram(&mut vdp, 2, 0x0C); // green
    poke_vram(&mut vdp, 0x0020, &solid_tile(1));
    poke_vram(&mut vdp, 0x0040, &solid_tile(2));
    for row in 0..28u16 {
        let tile = if row % 2 == 0 { 0x01 } else { 0x02 };
        for col in 0..32u16 {
            poke_vram(&mut vdp, 0x3800 + row * 64 + col * 2, &[tile, 0x00]);
        }
    }
    vdp
}

/// R0 bit 7: "make the rightmost eight columns of the screen fixed with
/// vertical scroll value 0". Columns 24-31 are pixels 192-255.
#[test]
fn the_vertical_scroll_lock_holds_the_rightmost_eight_columns_at_zero() {
    let mut unlocked = scroll_scene(0x00, 8);
    render_to(&mut unlocked, 0);
    assert_eq!(
        pixel(&unlocked, 0, 200),
        sms_argb(0x0C),
        "without the lock the right-hand columns scroll like the rest"
    );

    let mut locked = scroll_scene(0x80, 8);
    render_to(&mut locked, 0);
    assert_eq!(
        pixel(&locked, 0, 8),
        sms_argb(0x0C),
        "the lock must leave columns 0-23 scrolling"
    );
    assert_eq!(
        pixel(&locked, 0, 184),
        sms_argb(0x0C),
        "column 23 is the last one that still scrolls"
    );
    assert_eq!(
        pixel(&locked, 0, 192),
        sms_argb(0x03),
        "column 24 is the first one held at vertical scroll 0"
    );
    assert_eq!(
        pixel(&locked, 0, 248),
        sms_argb(0x03),
        "column 31 is held too"
    );
}

/// R0 bit 6, the horizontal twin: "make the top two rows (16 pixels) fixed
/// with horizontal scroll value 0".
#[test]
fn the_horizontal_scroll_lock_holds_the_top_two_rows_at_zero() {
    // The name table alternates two tiles every 8 pixels, so a horizontal
    // scroll of 8 swaps which one a given column shows.
    fn striped(reg0: u8) -> SegaVdp {
        let mut vdp = vdp();
        write_register(&mut vdp, 0, 0x04 | reg0);
        write_register(&mut vdp, 1, 0x40);
        write_register(&mut vdp, 2, 0x0E);
        write_register(&mut vdp, 8, 8);
        poke_cram(&mut vdp, 1, 0x03);
        poke_cram(&mut vdp, 2, 0x0C);
        poke_vram(&mut vdp, 0x0020, &solid_tile(1));
        poke_vram(&mut vdp, 0x0040, &solid_tile(2));
        for row in 0..4u16 {
            for col in 0..32u16 {
                let tile = if col % 2 == 0 { 0x01 } else { 0x02 };
                poke_vram(&mut vdp, 0x3800 + row * 64 + col * 2, &[tile, 0x00]);
            }
        }
        vdp
    }

    let mut unlocked = striped(0x00);
    render_to(&mut unlocked, 0);
    assert_eq!(
        pixel(&unlocked, 0, 0),
        sms_argb(0x0C),
        "unlocked, column 0 shows the tile the scroll brought in"
    );

    let mut locked = striped(0x40);
    render_to(&mut locked, 16);
    assert_eq!(
        pixel(&locked, 0, 0),
        sms_argb(0x03),
        "row 0 is held at horizontal scroll 0"
    );
    assert_eq!(
        pixel(&locked, 15, 0),
        sms_argb(0x03),
        "row 1 is held too - the lock covers the top 16 pixels"
    );
    assert_eq!(
        pixel(&locked, 16, 0),
        sms_argb(0x0C),
        "row 2 scrolls normally"
    );
}

// ---------------------------------------------------------------------------
// R0 bit 5 - hide the leftmost eight pixels
// ---------------------------------------------------------------------------

/// Two sprites side by side in the top-left corner, the left one inside the
/// masked column and the right one just clear of it.
fn two_corner_sprites(reg0: u8) -> SegaVdp {
    let mut vdp = vdp();
    write_register(&mut vdp, 0, 0x04 | reg0);
    write_register(&mut vdp, 1, 0x40);
    write_register(&mut vdp, 5, 0xFF); // attribute table $3F00
    write_register(&mut vdp, 6, 0x00); // sprite patterns $0000
    write_register(&mut vdp, 7, 0x00); // backdrop = CRAM entry 16
    poke_cram(&mut vdp, 16, 0x30); // backdrop = blue
    poke_cram(&mut vdp, 16 + 5, 0x03); // sprite colour 5 = red
    poke_vram(&mut vdp, 0x0020, &solid_tile(5));
    poke_vram(&mut vdp, 0x3F00, &[7, 7, 0xD0]); // two sprites at y=7, then end
    poke_vram(&mut vdp, 0x3F80, &[0, 1, 8, 1]); // x = 0 and x = 8
    vdp
}

/// R0 bit 5 blanks the leftmost eight pixels, and the reference's rendering
/// order puts that overwrite *after* sprites are composited. That ordering is
/// the whole point of the bit: SMS Power notes that showing the column is
/// what stops sprites scrolling smoothly off either edge, so what the mask
/// hides is sprites, not just background.
#[test]
fn hiding_the_left_column_hides_the_sprites_inside_it() {
    let mut shown = two_corner_sprites(0x00);
    render_to(&mut shown, 8);
    assert_eq!(
        pixel(&shown, 8, 0),
        sms_argb(0x03),
        "with the column shown, the sprite in it is visible"
    );

    let mut hidden = two_corner_sprites(0x20);
    render_to(&mut hidden, 8);
    for x in 0..8 {
        assert_eq!(
            pixel(&hidden, 8, x),
            sms_argb(0x30),
            "pixel {x} is inside the masked column and must read as backdrop"
        );
    }
    assert_eq!(
        pixel(&hidden, 8, 8),
        sms_argb(0x03),
        "the mask covers eight pixels and no more"
    );
}

// ---------------------------------------------------------------------------
// Sprites
// ---------------------------------------------------------------------------

/// "Y is (actual Y - 1)": a sprite whose attribute byte reads 7 starts on
/// scanline 8, not 7.
#[test]
fn the_sprite_y_attribute_is_one_less_than_the_first_line_drawn() {
    let mut vdp = two_corner_sprites(0x00);
    render_to(&mut vdp, 20);
    assert_eq!(
        pixel(&vdp, 7, 0),
        sms_argb(0x30),
        "scanline 7 is above a sprite whose Y attribute is 7"
    );
    assert_eq!(pixel(&vdp, 8, 0), sms_argb(0x03), "the sprite starts on 8");
    assert_eq!(
        pixel(&vdp, 15, 0),
        sms_argb(0x03),
        "an 8x8 sprite covers eight lines, 8 through 15"
    );
    assert_eq!(pixel(&vdp, 16, 0), sms_argb(0x30), "and stops after them");
}

/// "Y = $D0 in 192-line mode terminates the list - any sprites at indices >=
/// the first $D0 are skipped."
#[test]
fn a_y_of_0xd0_ends_the_sprite_list() {
    let mut vdp = two_corner_sprites(0x00);
    // Sprite 1 sits on the same line as sprite 0 and would draw at x = 8;
    // the terminator in its Y byte has to stop the whole list instead.
    poke_vram(&mut vdp, 0x3F01, &[0xD0]);
    render_to(&mut vdp, 8);
    assert_eq!(
        pixel(&vdp, 8, 0),
        sms_argb(0x03),
        "sprite 0 comes before the terminator and still draws"
    );
    assert_eq!(
        pixel(&vdp, 8, 8),
        sms_argb(0x30),
        "sprite 1 is at the terminator and must be skipped"
    );
}

/// "Max 8 sprites per scanline. 9th and beyond are not drawn and set the OVR
/// flag in status."
#[test]
fn the_ninth_sprite_on_a_line_is_dropped_and_flags_overflow() {
    fn line_of(count: u8) -> SegaVdp {
        let mut vdp = vdp();
        write_register(&mut vdp, 0, 0x04);
        write_register(&mut vdp, 1, 0x40);
        write_register(&mut vdp, 5, 0xFF);
        write_register(&mut vdp, 6, 0x00);
        write_register(&mut vdp, 7, 0x00);
        poke_cram(&mut vdp, 16, 0x30);
        poke_cram(&mut vdp, 16 + 5, 0x03);
        poke_vram(&mut vdp, 0x0020, &solid_tile(5));
        for i in 0..count {
            poke_vram(&mut vdp, 0x3F00 + u16::from(i), &[7]);
            poke_vram(&mut vdp, 0x3F80 + u16::from(i) * 2, &[i * 8, 1]);
        }
        poke_vram(&mut vdp, 0x3F00 + u16::from(count), &[0xD0]);
        vdp
    }

    let mut eight = line_of(8);
    render_to(&mut eight, 8);
    assert_eq!(
        pixel(&eight, 8, 56),
        sms_argb(0x03),
        "the eighth sprite draws"
    );
    assert_eq!(
        eight.read_status() & 0x40,
        0,
        "eight sprites on a line is the limit, not past it"
    );

    let mut nine = line_of(9);
    render_to(&mut nine, 8);
    assert_eq!(
        pixel(&nine, 8, 56),
        sms_argb(0x03),
        "the first eight still draw"
    );
    assert_eq!(
        pixel(&nine, 8, 64),
        sms_argb(0x30),
        "the ninth is dropped rather than drawn"
    );
    assert_eq!(
        nine.read_status() & 0x40,
        0x40,
        "a ninth sprite on the line sets the overflow flag"
    );
}

// ---------------------------------------------------------------------------
// Status port side effects
// ---------------------------------------------------------------------------

/// "Reading the status port ($BF) or asserting RES clears the first-write
/// latch." The idiom that depends on it is read-status-then-write-register:
/// with a half-finished command word still latched, the register write would
/// be taken as that word's second byte instead.
#[test]
fn reading_status_resets_the_control_port_latch() {
    let mut vdp = vdp();
    write_register(&mut vdp, 1, 0x40);
    vdp.write_control(0x12); // first half of a command word, abandoned
    let _ = vdp.read_status();
    write_register(&mut vdp, 0, 0x04);
    assert_eq!(
        vdp.registers()[0],
        0x04,
        "after a status read the next control write must start a fresh command"
    );
}

/// "Reading $BF clears INT, OVR, COL, and the line-IRQ pending flag." There
/// is no way to read one flag without clearing the others.
#[test]
fn reading_status_clears_every_flag_at_once() {
    let mut vdp = vdp();
    write_register(&mut vdp, 0, 0x14); // Mode 4 + line interrupts
    write_register(&mut vdp, 1, 0x60); // display on + frame interrupts
    write_register(&mut vdp, 10, 0); // a line interrupt every line

    for _ in 0..200 {
        vdp.tick_scanline();
    }
    assert_ne!(
        vdp.read_status() & 0x80,
        0,
        "past line 192 the frame flag is set"
    );
    assert_eq!(
        vdp.read_status() & 0xE0,
        0,
        "the read cleared the frame, overflow and collision flags together"
    );
}

// ---------------------------------------------------------------------------
// V counter
// ---------------------------------------------------------------------------

/// The V counter reports the scanline but repeats a run so the value stays
/// inside a byte: NTSC 192-line counts 0..$DA then $D5..$FF, PAL 0..$F2 then
/// $BA..$FF. Both maps have to account for every line of the frame exactly
/// once. Which line publishes which value is a phase question this does not
/// pin down - only the sequence and its one discontinuity.
#[test]
fn the_v_counter_maps_every_scanline_of_the_frame() {
    for (region, lines, runs) in [
        (
            VdpRegion::Ntsc,
            262usize,
            [(0x00u32, 0xDAu32), (0xD5, 0xFF)],
        ),
        (VdpRegion::Pal, 313, [(0x00, 0xF2), (0xBA, 0xFF)]),
    ] {
        let expected: Vec<u8> = runs
            .iter()
            .flat_map(|&(lo, hi)| (lo..=hi).map(|v| v as u8))
            .collect();
        assert_eq!(
            expected.len(),
            lines,
            "{region:?}: the documented runs should cover the whole frame"
        );

        let mut vdp = SegaVdp::new(region, VdpVariant::Sms2);
        write_register(&mut vdp, 1, 0x40);
        while !vdp.tick_scanline() {} // settle on a frame boundary
        let mut seen = Vec::with_capacity(lines);
        for _ in 0..lines {
            seen.push(vdp.read_v_counter());
            vdp.tick_scanline();
        }

        assert!(
            (0..lines).any(|offset| {
                let mut rotated = expected.clone();
                rotated.rotate_left(offset);
                rotated == seen
            }),
            "{region:?} V counter sequence is not the documented map"
        );
    }
}

// ---------------------------------------------------------------------------
// R9 is latched once per frame
// ---------------------------------------------------------------------------

/// R9 is "sampled only at the start of the active display", so a mid-frame
/// write does not take effect until the next frame. This is why the Master
/// System has no vertical raster-scroll trick: split scrolling is done with
/// R8 and the line counter, and a game that writes R9 mid-frame is aiming at
/// the frame after.
///
/// Genesis Plus GX agrees, and its variable says so: `vscroll` is declared
/// "Latched vertical scroll value" in `vdp_ctrl.c`, assigned once in
/// `system.c` on the line before the active display starts, and read
/// unchanged by `render_bg_m4` for every line of the frame.
#[test]
fn the_vertical_scroll_register_is_latched_once_per_frame() {
    let mut vdp = scroll_scene(0x00, 0);

    // Scan into a frame with no vertical scroll, then move R9 by a whole
    // tile row while the picture is still being drawn. Line 100 falls in an
    // even name-table row at scroll 0 and an odd one at scroll 8, so it
    // reports which value was in force when it was scanned.
    while !vdp.tick_scanline() {}
    for _ in 0..8 {
        vdp.tick_scanline();
    }
    write_register(&mut vdp, 9, 8);
    for _ in 8..192 {
        vdp.tick_scanline();
    }

    assert_eq!(
        pixel(&vdp, 100, 0),
        sms_argb(0x03),
        "a mid-frame write to R9 must not move the rest of this frame"
    );

    // The next whole frame picks it up.
    while !vdp.tick_scanline() {}
    for _ in 0..192 {
        vdp.tick_scanline();
    }
    assert_eq!(
        pixel(&vdp, 100, 0),
        sms_argb(0x0C),
        "the write takes effect from the next frame"
    );
}

// ---------------------------------------------------------------------------
// Status bits 4-0
// ---------------------------------------------------------------------------

/// Bits 4-0 of the status register are the fifth-sprite number in the TMS9918
/// modes this chip inherited. Mode 4 has no such field and reads them back as
/// ones.
///
/// Genesis Plus GX does this and names the title that proves it — `else if
/// (reg[0] & 0x04) { /* Mode 4 unused bits (fixes PGA Tour Golf) */ temp |=
/// 0x1F; }` — so a game does read them and does expect them set. That also
/// settles a claim in our own distillation, which had the 315-5246 reporting
/// the ninth sprite's index here: a game that needs ones would break on real
/// hardware if an index were there.
///
/// The second half of this test pins that the fill is conditional on Mode 4
/// rather than unconditional. The TMS-compatibility modes themselves are a
/// placeholder in this crate, so what it checks is the gate, not the field.
#[test]
fn mode_4_reads_the_unused_status_bits_back_as_ones() {
    let mut vdp = vdp();
    write_register(&mut vdp, 0, 0x04); // Mode 4
    write_register(&mut vdp, 1, 0x40);
    assert_eq!(
        vdp.read_status() & 0x1F,
        0x1F,
        "Mode 4 has no fifth-sprite field and reads ones there"
    );

    write_register(&mut vdp, 0, 0x00); // leave Mode 4
    assert_eq!(
        vdp.read_status() & 0x1F,
        0x00,
        "outside Mode 4 the bits are a field, not a fill"
    );
}
