//! Tests for blitter fill modes (IFE and EFE).
//!
//! The blitter fill logic scans bits from LSB to MSB within each word,
//! toggling a carry flag that determines the output. These tests verify
//! interior fill (IFE), exclusive fill (EFE), carry propagation, FCI
//! seeding, and descending mode.

use machine_amiga::memory::ROM_BASE;
use machine_amiga::{Amiga, TICKS_PER_CCK};

const REG_DMACON: u16 = 0x096;
const REG_BLTCON0: u16 = 0x040;
const REG_BLTCON1: u16 = 0x042;
const REG_BLTAFWM: u16 = 0x044;
const REG_BLTALWM: u16 = 0x046;
const REG_BLTCPTH: u16 = 0x048;
const REG_BLTCPTL: u16 = 0x04A;
const REG_BLTAPTH: u16 = 0x050;
const REG_BLTAPTL: u16 = 0x052;
const REG_BLTDPTH: u16 = 0x054;
const REG_BLTDPTL: u16 = 0x056;
const REG_BLTSIZE: u16 = 0x058;
const REG_BLTAMOD: u16 = 0x064;
const REG_BLTCMOD: u16 = 0x060;
const REG_BLTDMOD: u16 = 0x066;

const DMACON_BLTEN: u16 = 0x0040;
const DMACON_DMAEN: u16 = 0x0200;

fn make_test_amiga() -> Amiga {
    let mut rom = vec![0u8; 256 * 1024];
    let ssp = 0x0007_FFF0u32;
    rom[0] = (ssp >> 24) as u8;
    rom[1] = (ssp >> 16) as u8;
    rom[2] = (ssp >> 8) as u8;
    rom[3] = ssp as u8;
    let pc = ROM_BASE + 8;
    rom[4] = (pc >> 24) as u8;
    rom[5] = (pc >> 16) as u8;
    rom[6] = (pc >> 8) as u8;
    rom[7] = pc as u8;
    rom[8] = 0x60; // BRA.S *
    rom[9] = 0xFE;
    Amiga::new(rom)
}

fn tick_ccks(amiga: &mut Amiga, ccks: u32) {
    for _ in 0..ccks {
        for _ in 0..TICKS_PER_CCK {
            amiga.tick();
        }
    }
}

fn write_chip_word(amiga: &mut Amiga, addr: u32, val: u16) {
    amiga.memory.write_byte(addr, (val >> 8) as u8);
    amiga.memory.write_byte(addr + 1, val as u8);
}

fn read_chip_word(amiga: &Amiga, addr: u32) -> u16 {
    (u16::from(amiga.memory.read_chip_byte(addr)) << 8)
        | u16::from(amiga.memory.read_chip_byte(addr + 1))
}

fn write_ptr(amiga: &mut Amiga, reg_hi: u16, reg_lo: u16, addr: u32) {
    amiga.write_custom_reg(reg_hi, (addr >> 16) as u16);
    amiga.write_custom_reg(reg_lo, (addr & 0xFFFF) as u16);
}

/// Run a single-row fill blit with given BLTCON1 flags.
/// Source A contains the edge bits, C is pass-through, D is output.
/// Returns the output words.
fn run_fill_blit(
    amiga: &mut Amiga,
    width_words: u16,
    height_rows: u16,
    bltcon1: u16,
    source_a: &[u16],
    source_c: &[u16],
) -> Vec<u16> {
    let base_a = 0x1000u32;
    let base_c = 0x2000u32;
    let base_d = 0x3000u32;

    for (i, &w) in source_a.iter().enumerate() {
        write_chip_word(amiga, base_a + (i as u32) * 2, w);
    }
    for (i, &w) in source_c.iter().enumerate() {
        write_chip_word(amiga, base_c + (i as u32) * 2, w);
    }
    let total = (width_words as u32) * (height_rows as u32);
    for i in 0..total {
        write_chip_word(amiga, base_d + i * 2, 0);
    }

    // BLTCON0: use A+C+D, LF=0xCA (D = A ? C : D, effectively copy C through edges).
    // For fill testing, we use LF=0xF0 (D = A) so fill operates on A's edge bits.
    let bltcon0 = 0x09F0u16; // channels A+D, LF=0xF0

    amiga.write_custom_reg(REG_BLTCON0, bltcon0);
    amiga.write_custom_reg(REG_BLTCON1, bltcon1);
    amiga.write_custom_reg(REG_BLTAFWM, 0xFFFF);
    amiga.write_custom_reg(REG_BLTALWM, 0xFFFF);
    amiga.write_custom_reg(REG_BLTAMOD, 0);
    amiga.write_custom_reg(REG_BLTCMOD, 0);
    amiga.write_custom_reg(REG_BLTDMOD, 0);

    write_ptr(amiga, REG_BLTAPTH, REG_BLTAPTL, base_a);
    write_ptr(amiga, REG_BLTCPTH, REG_BLTCPTL, base_c);
    write_ptr(amiga, REG_BLTDPTH, REG_BLTDPTL, base_d);

    // Enable blitter DMA.
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_BLTEN);

    // Start the blit.
    let bltsize = (height_rows << 6) | width_words;
    amiga.write_custom_reg(REG_BLTSIZE, bltsize);

    // Run enough CCKs for the blit to complete.
    let max_ccks = total * 4 + 200;
    tick_ccks(amiga, max_ccks);

    let mut result = Vec::new();
    for i in 0..total {
        result.push(read_chip_word(amiga, base_d + i * 2));
    }
    result
}

#[test]
fn ife_basic_fill_single_word() {
    let mut amiga = make_test_amiga();
    // Edge bits: bits 2 and 6 set → 0b0100_0100 = 0x0044.
    // Interior fill (IFE) fills between edges (inclusive of carry toggle).
    // Starting carry = 0. Scanning from bit 0:
    //   bits 0,1: carry=0, out=0
    //   bit 2: carry^=1→1, out=1 (carry)
    //   bits 3,4,5: carry=1, out=1
    //   bit 6: carry^=1→0, out=0 (carry after toggle)
    //   bits 7-15: carry=0, out=0
    // Result: 0b0011_1100 = 0x003C
    let bltcon1 = 0x0008; // IFE bit
    let result = run_fill_blit(&mut amiga, 1, 1, bltcon1, &[0x0044], &[0x0000]);
    assert_eq!(result, vec![0x003C], "IFE basic fill between bits 2 and 6");
}

#[test]
fn efe_basic_fill_single_word() {
    let mut amiga = make_test_amiga();
    // Exclusive fill (EFE): out = carry XOR d_bit.
    // Edge bits: 0x0044 (bits 2 and 6).
    // Starting carry = 0. Scanning from bit 0:
    //   bits 0,1: carry=0, out=0^0=0
    //   bit 2: carry^=1→1, out=1^1=0
    //   bits 3,4,5: carry=1, out=1^0=1
    //   bit 6: carry^=1→0, out=0^1=1
    //   bits 7-15: carry=0, out=0^0=0
    // Result: 0b0111_1000 = 0x0078
    // Wait — let me recalculate. EFE: out = fill_carry ^ d_bit, but fill_carry
    // is already toggled by d_bit before the XOR:
    //   bit 2: carry ^= 1 → 1, out = 1 ^ 1 = 0
    //   bit 3: carry ^= 0 → 1, out = 1 ^ 0 = 1
    //   bit 5: carry ^= 0 → 1, out = 1 ^ 0 = 1
    //   bit 6: carry ^= 1 → 0, out = 0 ^ 1 = 1
    // So bits 3,4,5,6 are set: 0b0111_1000 = 0x0078
    let bltcon1 = 0x0010; // EFE bit
    let result = run_fill_blit(&mut amiga, 1, 1, bltcon1, &[0x0044], &[0x0000]);
    assert_eq!(result, vec![0x0078], "EFE basic fill between bits 2 and 6");
}

#[test]
fn ife_carry_across_word_boundary() {
    let mut amiga = make_test_amiga();
    // Two words: first word has edge at bit 4, second has edge at bit 4.
    // Carry should propagate from word 0 to word 1.
    // Word 0 = 0x0010 (bit 4): carry starts 0, toggles to 1 at bit 4.
    //   IFE output: bits 0-3 = 0, bit 4 carry→1 out=1, bits 5-15 carry=1 out=1
    //   = 0xFFF0
    // Word 1 = 0x0010 (bit 4): carry starts 1 (from word 0).
    //   bits 0-3: carry=1, out=1
    //   bit 4: carry^=1→0, out=0
    //   bits 5-15: carry=0, out=0
    //   = 0x000F
    let bltcon1 = 0x0008; // IFE
    let result = run_fill_blit(&mut amiga, 2, 1, bltcon1, &[0x0010, 0x0010], &[0; 2]);
    assert_eq!(
        result,
        vec![0xFFF0, 0x000F],
        "IFE carry propagates across word boundary"
    );
}

#[test]
fn ife_carry_resets_at_row_start() {
    let mut amiga = make_test_amiga();
    // Two rows of 1 word each. First row edge at bit 0 toggles carry to 1.
    // Carry must reset to FCI (0) at the start of row 2.
    // Row 0: 0x0001 (bit 0). Carry 0→1 at bit 0, out=1. Bits 1-15: carry=1, out=1.
    //   = 0xFFFF
    // Row 1: 0x0001 (bit 0). Carry reset to 0 at row start.
    //   Same as row 0: 0xFFFF
    let bltcon1 = 0x0008; // IFE
    let result = run_fill_blit(&mut amiga, 1, 2, bltcon1, &[0x0001, 0x0001], &[0; 2]);
    assert_eq!(
        result,
        vec![0xFFFF, 0xFFFF],
        "IFE carry resets to FCI at each row start"
    );
}

#[test]
fn ife_fci_seed_bit_starts_carry_at_one() {
    let mut amiga = make_test_amiga();
    // FCI = 1 (BLTCON1 bit 2): carry starts at 1.
    // Source: 0x0010 (bit 4 edge).
    // carry=1 at start. Bits 0-3: carry=1, out=1. Bit 4: carry^=1→0, out=0.
    // Bits 5-15: carry=0, out=0.
    // = 0x000F
    let bltcon1 = 0x0008 | 0x0004; // IFE + FCI
    let result = run_fill_blit(&mut amiga, 1, 1, bltcon1, &[0x0010], &[0x0000]);
    assert_eq!(
        result,
        vec![0x000F],
        "IFE with FCI=1 starts fill carry at 1"
    );
}

#[test]
fn ife_descending_mode() {
    let mut amiga = make_test_amiga();
    // Descending mode (BLTCON1 bit 1): pointers decrement.
    // Fill still scans bits 0→15 within each word, but words are processed
    // in reverse address order. We set up the same pattern and verify the
    // output is correct.
    // Single word: 0x0044 edges at bits 2 and 6.
    // Same IFE result: 0x003C
    let base_a = 0x1000u32;
    let base_d = 0x3000u32;
    write_chip_word(&mut amiga, base_a, 0x0044);
    write_chip_word(&mut amiga, base_d, 0);

    // Descending: pointers start at last word and walk backward.
    let bltcon0 = 0x09F0u16; // A+D, LF=0xF0
    let bltcon1 = 0x0008 | 0x0002; // IFE + DESC

    amiga.write_custom_reg(REG_BLTCON0, bltcon0);
    amiga.write_custom_reg(REG_BLTCON1, bltcon1);
    amiga.write_custom_reg(REG_BLTAFWM, 0xFFFF);
    amiga.write_custom_reg(REG_BLTALWM, 0xFFFF);
    amiga.write_custom_reg(REG_BLTAMOD, 0);
    amiga.write_custom_reg(REG_BLTDMOD, 0);

    // Descending: set pointers to last word.
    write_ptr(&mut amiga, REG_BLTAPTH, REG_BLTAPTL, base_a);
    write_ptr(&mut amiga, REG_BLTDPTH, REG_BLTDPTL, base_d);

    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_BLTEN);
    amiga.write_custom_reg(REG_BLTSIZE, (1 << 6) | 1); // 1 row x 1 word

    tick_ccks(&mut amiga, 200);
    let result = read_chip_word(&amiga, base_d);
    assert_eq!(result, 0x003C, "IFE descending mode fills correctly");
}
