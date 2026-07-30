//! Commodore Lisa (AGA Denise) — thin wrapper over the ECS Denise that
//! advertises the AGA chip identity and carries the AGA-only register
//! state (`BPLCON4`, FMODE-derived sprite width).
//!
//! Lisa's silicon-level deltas vs ECS Super Denise 8373:
//!
//! - **DENISEID = $00F8** (vs ECS's $00FC) — how KS 3.x detects AGA.
//! - **256-entry 24-bit palette** — `palette_24[0..255]`, banked
//!   through BPLCON3 bits 15..13.
//! - **HAM8** — 8-bit HAM mode (vs ECS HAM6).
//! - **BPLCON4** — bitplane colour XOR (BPLAM, bits 15..8) +
//!   sprite colour base (ESPRM/OSPRM, bits 7..0).
//! - **FMODE-driven sprite widths** — 16 / 32 / 64-pixel sprites.
//!
//! Rendering status (per `knowledge/decisions/amiga-machine-rollout-plan.md`):
//! - **24-bit palette resolution** — done (#93): normal indexed modes
//!   resolve through `palette_24` for 8-bit-per-channel colour.
//! - **Wide sprite emit** — done (#95): FMODE feeds the OCS shifter's
//!   `spr_width` (16 / 32 / 64 px).
//! - **HAM8 chaining** — done (#94): 8-plane HAM resolves to a 24-bit
//!   pixel with low-2-bit channel hold; HAM6/EHB stay on the 12-bit path.
//! - **BPLAM bitplane XOR** — done (#96): BPLCON4 bits 15..8 XOR the
//!   playfield colour index before the palette lookup.
//!
//! KS 3.x boot reads DENISEID + writes BPLCON3 / BPLCON4 / FMODE during
//! init; the writes land in AGA-specific state and the reads return the
//! AGA marker ($00F8).
//!
//! Adapted from `Emu198x-Oldest/crates/commodore-denise-aga/`.

use std::ops::{Deref, DerefMut};

pub use commodore_denise_ecs::DeniseEcs as InnerDeniseEcs;
pub use commodore_denise_ocs::{DeniseOcs as InnerDeniseOcs, DeniseOutputPixelDebug};

use common_commodore_amiga::{denise::HorizontalBlanking, denise_chip::DeniseChip};

/// AGA Lisa DENISEID value as the CPU reads it from $DFF07C.
/// WinUAE returns `0x00F8` for A1200 (and `0xFCF8` for A4000).
/// KS 3.x extracts bits 9-8 of the inverted value to derive the
/// sprite-width capability stored at GfxBase+454; `$FFF8` zeroes
/// those bits and breaks the AGA palette layout.
pub const LISA_DENISE_ID: u16 = 0x00F8;

/// Number of palette entries in AGA (vs 32 on OCS / ECS).
pub const PALETTE_ENTRIES_24: usize = 256;

/// Complete read-only snapshot of the state owned by the Lisa wrapper.
///
/// The wrapped ECS/OCS rendering core exposes its own diagnostic snapshot.
/// This type reports only Lisa's outer register mirrors, 24-bit colour state
/// and programmable-blanking latch, so callers can combine the two views
/// without mistaking the common core's compatibility fields for Lisa's live
/// palette or HAM8 hold register.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeniseAgaDiagnosticSnapshot {
    /// Lisa's BPLCON4 register mirror.
    pub bplcon4: u16,
    /// Lisa's complete 256-entry 24-bit palette, stored as `0x00RRGGBB`.
    #[serde(with = "palette_24_serde")]
    pub palette_24: [u32; PALETTE_ENTRIES_24],
    /// Lisa's current 24-bit HAM8 hold colour, stored as `0x00RRGGBB`.
    pub ham_prev_rgb24: u32,
    /// Lisa's FMODE-derived sprite width mirror.
    pub spr_width: u8,
    /// Hidden programmable horizontal-blank comparator level.
    pub programmed_hblank_active: bool,
}

/// Commodore Lisa (AGA Denise). Wraps the ECS Denise core and adds
/// the AGA-only register state.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DeniseAga {
    inner: InnerDeniseEcs,
    /// BPLCON4 ($10C). Bits 15..8 = bitplane colour XOR mask
    /// (BPLAM); bits 7..0 = sprite colour base (ESPRM in bits 7..4,
    /// OSPRM in bits 3..0).
    pub bplcon4: u16,
    /// 256-entry 24-bit palette (8 banks × 32 entries). BPLCON3
    /// bits 15..13 select the active bank for `COLORxx` writes.
    /// Stored as `0x00RRGGBB`.
    #[serde(with = "palette_24_serde")]
    pub palette_24: [u32; PALETTE_ENTRIES_24],
    /// Last resolved RGB24 value, used by HAM8 chaining.
    pub ham_prev_rgb24: u32,
    /// Current sprite display width in pixels (16 / 32 / 64),
    /// driven by FMODE bits 3..2.
    pub spr_width: u8,
    /// Hidden Lisa programmable horizontal-blank level. Comparator events and
    /// the live ECSENA/EXTBLKEN selectors change this state; register writes
    /// do not reconstruct it from the current beam position.
    programmed_hblank_active: bool,
}

/// Serde adapter — `[u32; 256]` isn't `Serialize`/`Deserialize` by
/// default in serde without `serde-big-array` or similar. Pack the
/// palette as a `Vec<u32>` of length 256 over the wire.
mod palette_24_serde {
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub fn serialize<S: Serializer>(
        p: &[u32; super::PALETTE_ENTRIES_24],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        s.collect_seq(p.iter())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<[u32; super::PALETTE_ENTRIES_24], D::Error> {
        let v: Vec<u32> = Vec::deserialize(d)?;
        v.try_into().map_err(|v: Vec<u32>| {
            D::Error::custom(format!("palette_24 length {} != 256", v.len()))
        })
    }
}

impl DeniseAga {
    /// Construct a fresh Lisa with the AGA register state zeroed and
    /// sprite width at the AGA default of 16 pixels.
    #[must_use]
    pub fn new() -> Self {
        let mut inner = InnerDeniseEcs::new();
        // Lisa drives up to 8 bitplanes (vs ECS/OCS 6). The OCS core's
        // `num_bitplanes()` only honours the AGA BPU3 bit (BPLCON0 bit 4)
        // when `max_bitplanes > 6`, and 8-plane modes (HAM8, deep CLUT)
        // only compose all planes when this is raised. ECS/OCS Denise
        // stay at 6.
        inner.as_inner_mut().max_bitplanes = 8;
        Self {
            inner,
            bplcon4: 0x0011,
            palette_24: [0; PALETTE_ENTRIES_24],
            ham_prev_rgb24: 0,
            spr_width: 16,
            programmed_hblank_active: false,
        }
    }

    /// Promote an existing ECS Super Denise to AGA Lisa. Carries inner
    /// state across; AGA register state starts at the reset defaults.
    #[must_use]
    pub fn from_ecs(inner: InnerDeniseEcs) -> Self {
        let mut inner = inner;
        // Promotion to Lisa raises the bitplane ceiling to 8 (see `new`).
        inner.as_inner_mut().max_bitplanes = 8;
        Self {
            inner,
            bplcon4: 0x0011,
            palette_24: [0; PALETTE_ENTRIES_24],
            ham_prev_rgb24: 0,
            spr_width: 16,
            programmed_hblank_active: false,
        }
    }

    /// Borrow the wrapped ECS Denise core.
    #[must_use]
    pub const fn as_inner(&self) -> &InnerDeniseEcs {
        &self.inner
    }

    /// Mutably borrow the wrapped ECS Denise core.
    pub fn as_inner_mut(&mut self) -> &mut InnerDeniseEcs {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped ECS Denise core.
    #[must_use]
    pub fn into_inner(self) -> InnerDeniseEcs {
        self.inner
    }

    /// Capture every mutable field owned by the Lisa wrapper without
    /// advancing HAM chaining, changing the blanking comparator or touching
    /// the wrapped ECS/OCS rendering core.
    ///
    /// Use the wrapped OCS core's diagnostic snapshot separately when its
    /// bitplane, sprite and common-register pipeline state is also required.
    #[must_use]
    pub fn diagnostic_snapshot(&self) -> DeniseAgaDiagnosticSnapshot {
        DeniseAgaDiagnosticSnapshot {
            bplcon4: self.bplcon4,
            palette_24: self.palette_24,
            ham_prev_rgb24: self.ham_prev_rgb24,
            spr_width: self.spr_width,
            programmed_hblank_active: self.programmed_hblank_active,
        }
    }

    /// Advance Lisa's programmable horizontal-blank comparator over the two
    /// output samples produced by one Denise phase.
    ///
    /// The coarse comparator occupies the low byte of HBSTRT/HBSTOP. Lisa's
    /// three fine bits are paired onto the renderer's four-sample-per-CCK
    /// grid. ECSENA and EXTBLKEN are sampled live; disabling either clears the
    /// hidden level, so enabling a selector after HBSTRT cannot synthesize a
    /// start event. BEAMCON0.BLANKEN is not part of the Lisa path.
    #[must_use]
    pub fn programmed_hblank_for_output_phase(
        &mut self,
        hpos: u16,
        phase: u8,
        bplcon0: u16,
        hbstrt: u16,
        hbstop: u16,
    ) -> HorizontalBlanking {
        const BPLCON0_ECSENA: u16 = 0x0001;
        const BPLCON3_EXTBLKEN: u16 = 0x0001;
        const OUTPUT_SAMPLES_PER_CCK: u16 = 4;

        debug_assert!(phase < 2);
        let selectors_enabled =
            (bplcon0 & BPLCON0_ECSENA) != 0 && (self.inner.bplcon3 & BPLCON3_EXTBLKEN) != 0;
        let fine_sample =
            |word: u16| (word & 0x00FF) * OUTPUT_SAMPLES_PER_CCK + ((word >> 8) & 0x0007) / 2;
        let start_sample = fine_sample(hbstrt);
        let stop_sample = fine_sample(hbstop);
        let phase_sample = (hpos & 0x00FF) * OUTPUT_SAMPLES_PER_CCK + u16::from(phase) * 2;
        let mut output_samples = [false; 2];

        for (subpixel, output) in output_samples.iter_mut().enumerate() {
            if !selectors_enabled {
                self.programmed_hblank_active = false;
                continue;
            }

            let sample = phase_sample + subpixel as u16;
            // Start precedes stop, so equal edges describe an empty interval.
            if sample == start_sample {
                self.programmed_hblank_active = true;
            }
            if sample == stop_sample {
                self.programmed_hblank_active = false;
            }
            *output = self.programmed_hblank_active;
        }

        HorizontalBlanking::from_output_samples(output_samples)
    }

    /// Current hidden Lisa programmable horizontal-blank level.
    #[must_use]
    pub const fn programmed_hblank_active(&self) -> bool {
        self.programmed_hblank_active
    }

    /// AGA Lisa ID register value, as reported by DENISEID ($DFF07C).
    /// Real silicon returns $F8 in the low byte; the high byte is $00
    /// (WinUAE returns $00F8 for A1200, $FCF8 for A4000).
    #[must_use]
    pub const fn deniseid(&self) -> u16 {
        LISA_DENISE_ID
    }

    /// Update the sprite display width from the FMODE register value
    /// (FMODE lives on Alice, not Lisa, so the machine layer forwards
    /// the value when FMODE is written).
    ///
    /// FMODE bits 3..2: 00 → 16 px, 01/10 → 32 px, 11 → 64 px.
    pub fn set_sprite_width_from_fmode(&mut self, fmode: u16) {
        let width = match (fmode >> 2) & 0x0003 {
            0 => 16,
            1 | 2 => 32,
            _ => 64,
        };
        self.spr_width = width;
        // Propagate into the OCS sprite shifter that actually emits the
        // pixels (`spr_data`/`spr_shift_data` are u64, and the shifter
        // outputs `spr_width` lores pixels per line). DeniseAga's own
        // `spr_width` is only a diagnostic mirror the render path never
        // reads — without this write, AGA sprites stayed 16 px wide (#95).
        self.inner.as_inner_mut().spr_width = width;
    }

    /// Handle a CPU/copper write to one of the COLOR registers
    /// (`$DFF180..$DFF1BE`). On AGA, the 32 base color indices are
    /// banked through `BPLCON3[15:13]` (8 banks × 32 = 256 entries),
    /// and `BPLCON3[9]` (LOCT) selects which 4-bit half of each
    /// component the write addresses.
    ///
    /// Semantics:
    ///
    /// - `LOCT=0` (high write): the four-bit value for each channel
    ///   is placed in the high nybble *and mirrored into the low
    ///   nybble*, so an OS that only writes once per colour gets a
    ///   full-precision 8-bit value (matches the AGA HRM / WinUAE).
    /// - `LOCT=1` (low write): only the low nybble of each channel
    ///   is updated; the high nybble keeps its previous value.
    ///
    /// The OCS palette is also updated for bank 0 / LOCT=0 writes so
    /// the existing ECS 12-bit render path keeps producing pixels
    /// while the AGA 24-bit render path is still pending.
    pub fn handle_color_write(&mut self, offset: u16, val: u16) {
        let idx = ((offset - 0x180) / 2) as usize;
        let bplcon3 = self.inner.bplcon3;
        let bank = ((bplcon3 >> 13) & 0x7) as usize;
        let loct = (bplcon3 & 0x0200) != 0;
        let slot = bank * 32 + idx;
        if slot < PALETTE_ENTRIES_24 {
            let r4 = u32::from((val >> 8) & 0xF);
            let g4 = u32::from((val >> 4) & 0xF);
            let b4 = u32::from(val & 0xF);
            if loct {
                // Update only the low nybble of each 8-bit channel.
                let lo = (r4 << 16) | (g4 << 8) | b4;
                self.palette_24[slot] = (self.palette_24[slot] & 0x00F0_F0F0) | lo;
            } else {
                // High write mirrors into the low nybble so a single
                // write produces a full 8-bit value per channel.
                let r8 = (r4 << 4) | r4;
                let g8 = (g4 << 4) | g4;
                let b8 = (b4 << 4) | b4;
                self.palette_24[slot] = (r8 << 16) | (g8 << 8) | b8;
            }
        }
        // Keep the OCS 12-bit palette in sync for the existing render
        // path — but only for bank 0 / high writes, so LOCT=1 passes
        // don't corrupt the 12-bit value we'll still resolve through.
        if bank == 0 && !loct {
            self.inner.write_word(offset, val);
        }
    }
}

impl Default for DeniseAga {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for DeniseAga {
    type Target = InnerDeniseEcs;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for DeniseAga {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<DeniseAga> for InnerDeniseEcs {
    fn from(denise: DeniseAga) -> Self {
        denise.into_inner()
    }
}

// ── DeniseChip impl ───────────────────────────────────────────────
// Stage A: delegate everything to the ECS layer. AGA-specific
// behaviour (24-bit palette resolution, HAM8 chaining done, BPLCON4 XOR,
// wide sprite emit) is added incrementally as catalogue entries
// surface the requirement.

impl DeniseChip for DeniseAga {
    fn new() -> Self {
        DeniseAga::new()
    }

    fn write_word(&mut self, offset: u16, val: u16) {
        // AGA-only registers land here. The ECS layer (`self.inner`)
        // handles BPLCON3 + its 12-bit COLOR semantics; AGA layers on
        // BANK + LOCT decoding into the 24-bit palette_24 table.
        const BPLCON4: u16 = 0x010C;
        const FMODE: u16 = 0x01FC;
        match offset {
            BPLCON4 => {
                self.bplcon4 = val;
                // Forward to the OCS core, which owns pixel composition:
                // bits 15-8 (BPLAM) XOR the playfield colour index (#96)
                // and bits 7-0 (ESPRM/OSPRM) the sprite colour base. Both
                // read `bplcon4` off the OCS chip; DeniseAga.bplcon4 is the
                // diagnostic mirror.
                self.inner.as_inner_mut().bplcon4 = val;
            }
            FMODE => {
                // Lisa cares about FMODE bits 3..2 for sprite width.
                // Alice (the Agnus side) owns FMODE storage; Lisa
                // receives the value when the machine layer forwards
                // the write here.
                self.set_sprite_width_from_fmode(val);
            }
            0x180..=0x1BE => {
                self.handle_color_write(offset, val);
            }
            _ => self.inner.write_word(offset, val),
        }
    }

    fn load_bitplane(&mut self, idx: usize, val: u16) {
        self.inner.as_inner_mut().load_bitplane(idx, val);
    }

    fn push_bpl_fifo(&mut self, idx: usize, val: u16) {
        self.inner.as_inner_mut().push_bpl_fifo(idx, val);
    }

    fn queue_shift_load_from_bpl1dat(&mut self) {
        self.inner.as_inner_mut().queue_shift_load_from_bpl1dat();
    }

    fn write_sprite_pos(&mut self, sprite: usize, val: u16) {
        self.inner.as_inner_mut().write_sprite_pos(sprite, val);
    }

    fn write_sprite_ctl(&mut self, sprite: usize, val: u16) {
        self.inner.as_inner_mut().write_sprite_ctl(sprite, val);
    }

    fn write_sprite_data(&mut self, sprite: usize, val: u16) {
        self.inner.as_inner_mut().write_sprite_data(sprite, val);
    }

    fn write_sprite_datb(&mut self, sprite: usize, val: u16) {
        self.inner.as_inner_mut().write_sprite_datb(sprite, val);
    }

    fn begin_beam_line(&mut self) {
        self.inner.as_inner_mut().begin_beam_line();
        // HAM8 holds a 24-bit running colour across the line; reset it to
        // the AGA background (COLOR00) at the start of each scanline, the
        // same way the OCS layer resets its 12-bit HAM hold register.
        self.ham_prev_rgb24 = self.palette_24[0] & 0x00FF_FFFF;
    }

    fn output_pixel_with_beam_and_playfield_gate(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug {
        self.inner
            .as_inner_mut()
            .output_pixel_with_beam_and_playfield_gate(x, y, beam_x, beam_y, playfield_visible_gate)
    }

    fn output_pixel_with_beam_sprite_coords(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        spr_beam_x: u32,
        spr_beam_y: u32,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug {
        self.inner
            .as_inner_mut()
            .output_pixel_with_beam_sprite_coords(
                x,
                y,
                beam_x,
                beam_y,
                spr_beam_x,
                spr_beam_y,
                playfield_visible_gate,
            )
    }

    fn resolve_color_rgb12(&mut self, color_idx: u8) -> u16 {
        self.inner.resolve_color_rgb12(color_idx)
    }

    /// Resolve to a final ARGB8888 pixel through the AGA 24-bit palette.
    ///
    /// - **Normal indexed** (#93): `palette_24[idx]` (8-bit-per-channel).
    /// - **HAM8** (#94): 8-plane hold-and-modify, resolved to a full
    ///   24-bit pixel — see below.
    /// - **HAM6 / EHB**: derive their colour from the 12-bit palette
    ///   (4-bit channels, nibble-replicated to 8 bits), so they stay on
    ///   the existing 12-bit path.
    fn resolve_color_argb(&mut self, color_idx: u8) -> u32 {
        let ocs = self.inner.as_inner();
        let bplcon0 = ocs.bplcon0;
        let ham = bplcon0 & 0x0800 != 0;
        let dual_playfield = bplcon0 & 0x0400 != 0;
        let planes = ocs.num_bitplanes();

        // HAM8: hold-and-modify with 8 bitplanes resolves to a 24-bit
        // pixel. Unlike HAM6, the control select is the LOW two bits and
        // the data is the HIGH six bits. control=00 reads a 24-bit colour
        // register (bank 0, entries 0..63); a modify replaces the top six
        // bits of one 8-bit channel and HOLDS that channel's low two bits
        // from the previous pixel. Confirmed against two references:
        // Minimig-AGA `denise_hamgenerator.v` (control `select_r[1:0]`,
        // data `select_r[7:2]`) and WinUAE/fs-uae `decode_ham_pixel_aga`
        // (control `pv & 0x3`, modify `pix & 0xFC`).
        //
        // `color_idx` already has the BPLCON4 BPLAM XOR applied upstream
        // in `compose_playfield_pixel` (#96), so control + data are taken
        // post-XOR (Minimig's behaviour). WinUAE XORs only the control and
        // colour-register index, taking modify data from the raw pixel —
        // the two diverge only when BPLAM is non-zero in HAM8, which real
        // software effectively never does.
        if ham && !dual_playfield && planes == 8 {
            let control = color_idx & 0x03;
            let data6 = u32::from(color_idx >> 2);
            let prev = self.ham_prev_rgb24 & 0x00FF_FFFF;
            let rgb = match control {
                0b00 => self.palette_24[data6 as usize] & 0x00FF_FFFF,
                0b01 => {
                    // modify blue
                    let blue = (data6 << 2) | (prev & 0x03);
                    (prev & 0x00FF_FF00) | blue
                }
                0b10 => {
                    // modify red
                    let red = (data6 << 2) | ((prev >> 16) & 0x03);
                    (prev & 0x0000_FFFF) | (red << 16)
                }
                _ => {
                    // 0b11: modify green
                    let green = (data6 << 2) | ((prev >> 8) & 0x03);
                    (prev & 0x00FF_00FF) | (green << 8)
                }
            };
            self.ham_prev_rgb24 = rgb;
            return 0xFF00_0000 | rgb;
        }

        // HAM6 (≥5 planes) and EHB (6 planes, no HAM) derive their colour
        // from the 12-bit palette rather than the 24-bit indexed table.
        let derived_mode = !dual_playfield && ((ham && planes >= 5) || (!ham && planes == 6));
        if derived_mode {
            InnerDeniseOcs::rgb12_to_argb32(
                self.inner.as_inner_mut().resolve_color_rgb12(color_idx),
            )
        } else {
            0xFF00_0000 | (self.palette_24[color_idx as usize] & 0x00FF_FFFF)
        }
    }

    fn palette(&self) -> &[u16; 32] {
        &self.inner.as_inner().palette
    }

    fn interlace_active(&self) -> bool {
        self.inner.as_inner().interlace_active
    }

    fn lof(&self) -> bool {
        self.inner.as_inner().lof
    }

    fn bplcon0(&self) -> u16 {
        self.inner.as_inner().bplcon0
    }

    fn set_bplcon0(&mut self, v: u16) {
        self.inner.as_inner_mut().bplcon0 = v;
    }

    fn set_interlace_active(&mut self, v: bool) {
        self.inner.as_inner_mut().interlace_active = v;
    }

    fn set_lof(&mut self, v: bool) {
        self.inner.as_inner_mut().lof = v;
    }

    fn deniseid(&self) -> u16 {
        DeniseAga::deniseid(self)
    }
    fn read_clxdat(&mut self) -> u16 {
        self.inner.as_inner_mut().read_clxdat()
    }
    fn peek_clxdat(&self) -> u16 {
        self.inner.as_inner().peek_clxdat()
    }
}

#[cfg(test)]
mod tests {
    use super::{DeniseAga, LISA_DENISE_ID};
    use common_commodore_amiga::{denise::HorizontalBlanking, denise_chip::DeniseChip};

    #[test]
    fn new_starts_with_aga_register_defaults() {
        let denise = DeniseAga::new();
        // BPLCON4 resets to $0011 (Minimig denise.v): ESPRM/OSPRM = 1 so
        // sprites default to the OCS $10–$1F colour range, BPLAM = 0.
        assert_eq!(denise.bplcon4, 0x0011);
        assert_eq!(denise.spr_width, 16);
        assert_eq!(denise.ham_prev_rgb24, 0);
        assert!(denise.palette_24.iter().all(|&c| c == 0));
        assert!(!denise.programmed_hblank_active());
    }

    #[test]
    fn diagnostic_snapshot_reports_complete_lisa_state_without_using_core_mirrors() {
        let mut denise = DeniseAga::new();

        for (index, color) in denise.palette_24.iter_mut().enumerate() {
            *color = ((index as u32) * 0x0001_0203) & 0x00FF_FFFF;
        }
        denise.write_word(0x0106, 0xA000); // BANK=5, LOCT=0
        denise.write_word(0x019A, 0x0A5C); // COLOR13 in bank 5 -> slot 173
        denise.write_word(0x010C, 0x5A3C);
        denise.write_word(0x01FC, 0x000C);
        denise.set_bplcon0(0x0810); // HAM + BPU=8
        denise.palette_24[1] = 0x0012_3457;
        assert_eq!(denise.resolve_color_argb(0x04), 0xFF12_3457);

        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        let _ = denise.programmed_hblank_for_output_phase(0x0040, 0, 0x0001, 0x0040, 0x0080);

        // The common OCS core carries compatibility mirrors. Deliberately
        // make them disagree so this test proves the snapshot reads Lisa's
        // actual outer state rather than the older common diagnostic view.
        let core = denise.as_inner_mut().as_inner_mut();
        core.bplcon4 = 0xBEEF;
        core.palette_24 = [0x00DE_ADBE; 256];
        core.ham_prev_rgb24 = 0x00CA_FEBE;
        core.spr_width = 16;

        let expected_palette = denise.palette_24;
        let first = denise.diagnostic_snapshot();
        let second = denise.diagnostic_snapshot();

        assert_eq!(first, second);
        assert_eq!(first.bplcon4, 0x5A3C);
        assert_eq!(first.palette_24, expected_palette);
        assert_eq!(first.palette_24[173], 0x00AA_55CC);
        assert_eq!(first.ham_prev_rgb24, 0x0012_3457);
        assert_eq!(first.spr_width, 64);
        assert!(first.programmed_hblank_active);
        assert!(denise.programmed_hblank_active());
    }

    #[test]
    fn lisa_hblank_fine_stop_can_split_one_output_pair() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x0001); // ECSENA
        denise.write_word(0x0106, 0x0001); // EXTBLKEN

        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0030, 0, 0x0001, 0x0030, 0x0740,),
            HorizontalBlanking::from_output_samples([true, true]),
        );
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0040, 1, 0x0001, 0x0030, 0x0740,),
            HorizontalBlanking::from_output_samples([true, false]),
        );
        assert!(!denise.programmed_hblank_active());
    }

    #[test]
    fn lisa_hbstrt_write_behind_beam_does_not_synthesize_start() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0x0001); // EXTBLKEN

        let _ = denise.programmed_hblank_for_output_phase(0x0060, 0, 0x0001, 0x0070, 0x00C0);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0061, 0, 0x0001, 0x0050, 0x00C0,),
            HorizontalBlanking::disabled(),
        );
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0050, 0, 0x0001, 0x0050, 0x00C0,),
            HorizontalBlanking::from_level(true),
            "the rewritten edge takes effect when the following line reaches it",
        );
    }

    #[test]
    fn lisa_hbstop_write_ahead_after_stop_does_not_reassert() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        let _ = denise.programmed_hblank_for_output_phase(0x0060, 0, 0x0001, 0x0060, 0x0070);
        let _ = denise.programmed_hblank_for_output_phase(0x0070, 0, 0x0001, 0x0060, 0x0070);

        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0071, 0, 0x0001, 0x0060, 0x00B0,),
            HorizontalBlanking::disabled(),
        );
        let _ = denise.programmed_hblank_for_output_phase(0x0060, 0, 0x0001, 0x0060, 0x00B0);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x00A0, 0, 0x0001, 0x0060, 0x00B0,),
            HorizontalBlanking::from_level(true),
            "the future stop remains active on the following line",
        );
    }

    #[test]
    fn lisa_ecsena_enable_after_start_waits_for_the_next_start() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0040, 0, 0x0000, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
        );
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0050, 0, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
        );
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0040, 0, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::from_level(true),
        );
    }

    #[test]
    fn lisa_extblken_enable_after_start_waits_for_the_next_start() {
        let mut denise = DeniseAga::new();
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0040, 0, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
        );
        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0050, 0, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
        );
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0040, 0, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::from_level(true),
        );
    }

    #[test]
    fn lisa_equal_hblank_edges_leave_level_clear() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0x0001); // EXTBLKEN

        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0040, 0, 0x0001, 0x0040, 0x0040,),
            HorizontalBlanking::disabled(),
        );
        assert!(!denise.programmed_hblank_active());
    }

    #[test]
    fn disabling_a_lisa_selector_clears_the_active_level() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        let _ = denise.programmed_hblank_for_output_phase(0x0040, 0, 0x0001, 0x0040, 0x0080);
        assert!(denise.programmed_hblank_active());

        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0050, 0, 0x0000, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
        );
        assert!(!denise.programmed_hblank_active());
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0060, 0, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
        );
    }

    #[test]
    fn deniseid_returns_lisa_marker() {
        let denise = DeniseAga::new();
        assert_eq!(denise.deniseid(), LISA_DENISE_ID);
        assert_eq!(denise.deniseid(), 0x00F8);
    }

    #[test]
    fn bplcon4_write_via_trait_lands_on_aga_state() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x010C, 0x5A3C);
        assert_eq!(denise.bplcon4, 0x5A3C);
        // Must also reach the OCS core, which owns pixel composition: the
        // BPLAM XOR (#96) and sprite ESPRM/OSPRM both read bplcon4 there.
        assert_eq!(
            denise.as_inner().as_inner().bplcon4,
            0x5A3C,
            "BPLCON4 must forward to the OCS composition core"
        );
    }

    #[test]
    fn normal_mode_resolves_through_24bit_palette() {
        // #93: a normal indexed AGA screen resolves colours through the
        // 24-bit palette, giving true 8-bit-per-channel output — not the
        // 12-bit-quantised value the OCS/ECS path would produce.
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x1000); // BPU=1, lores, no HAM → normal indexed
        denise.palette_24[1] = 0x0012_3456; // R=$12 G=$34 B=$56 (low ≠ high nibble)

        assert_eq!(
            denise.resolve_color_argb(1),
            0xFF12_3456,
            "24-bit palette gives the exact 8-bit colour, not 12-bit $FF11_3355"
        );
        // COLOR00 (background) resolves the same way.
        denise.palette_24[0] = 0x00AB_CDEF;
        assert_eq!(denise.resolve_color_argb(0), 0xFFAB_CDEF);
    }

    #[test]
    fn ham6_mode_keeps_the_12bit_path() {
        // HAM6 (≤6 planes) derives colours from the 12-bit palette, not
        // the indexed 24-bit table, so palette_24 must NOT be consulted.
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x5000 | 0x0800); // BPU=5 + HAM (bit 11)
        denise.palette_24[1] = 0x0012_3456; // would show if 24-bit path were used
        assert_ne!(
            denise.resolve_color_argb(1),
            0xFF12_3456,
            "HAM6 mode must not resolve through the 24-bit palette"
        );
    }

    #[test]
    fn aga_denise_supports_eight_bitplanes() {
        // Lisa composes up to 8 planes; the OCS BPU3 bit (BPLCON0 bit 4)
        // is only honoured when max_bitplanes > 6, which DeniseAga raises.
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x0010); // BPU=8 (bit 4), lores, no HAM
        assert_eq!(denise.as_inner().as_inner().num_bitplanes(), 8);
    }

    #[test]
    fn ham8_control00_reads_24bit_color_register() {
        // #94: HAM8 control=00 loads a full 24-bit colour register. The
        // control select is the LOW two bits, the data is the HIGH six —
        // so register 5 is addressed by idx = 5 << 2.
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x0800 | 0x0010); // HAM + BPU=8
        denise.palette_24[5] = 0x0012_3456;
        assert_eq!(denise.resolve_color_argb(0x14), 0xFF12_3456);
    }

    #[test]
    fn ham8_modify_replaces_top6_holds_low2_and_other_channels() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x0800 | 0x0010); // HAM + BPU=8
        denise.palette_24[1] = 0x00FF_8043; // R=$FF G=$80 B=$43

        // Seed the hold register: control=00, data6=1 → idx 0x04.
        assert_eq!(denise.resolve_color_argb(0x04), 0xFFFF_8043);

        // Modify blue (control=01, data6=$2A): new B = ($2A<<2)|($43&3)
        // = $A8|$3 = $AB; red and green held.
        assert_eq!(denise.resolve_color_argb((0x2A << 2) | 0x01), 0xFFFF_80AB);

        // Re-seed, modify red (control=10, data6=$15): new R =
        // ($15<<2)|($FF&3) = $54|$3 = $57; green and blue held.
        denise.resolve_color_argb(0x04);
        assert_eq!(denise.resolve_color_argb((0x15 << 2) | 0x02), 0xFF57_8043);

        // Re-seed, modify green (control=11, data6=$10): new G =
        // ($10<<2)|($80&3) = $40|$0 = $40; red and blue held.
        denise.resolve_color_argb(0x04);
        assert_eq!(denise.resolve_color_argb((0x10 << 2) | 0x03), 0xFFFF_4043);
    }

    #[test]
    fn ham8_line_start_resets_hold_to_color00() {
        // The HAM hold register starts each scanline at COLOR00, not at
        // whatever colour the previous line ended on.
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x0800 | 0x0010); // HAM + BPU=8
        denise.palette_24[0] = 0x0011_2233; // background
        denise.palette_24[1] = 0x00FF_FFFF;
        denise.resolve_color_argb(0x04); // pollute hold = $FFFFFF

        denise.begin_beam_line();

        // Modify blue from background (control=01, data6=0 → idx 0x01):
        // new B = (0<<2)|($33&3) = 3; red+green from COLOR00.
        assert_eq!(denise.resolve_color_argb(0x01), 0xFF11_2203);
    }

    #[test]
    fn fmode_write_via_trait_updates_sprite_width() {
        let mut denise = DeniseAga::new();
        for (fmode, expected) in [(0x0000, 16u8), (0x0004, 32), (0x0008, 32), (0x000C, 64)] {
            denise.write_word(0x01FC, fmode);
            assert_eq!(denise.spr_width, expected, "FMODE={fmode:#06x}");
            // The width must reach the OCS sprite shifter (the field the
            // render path actually reads), not just the wrapper mirror (#95).
            assert_eq!(
                denise.as_inner().as_inner().spr_width,
                expected,
                "FMODE={fmode:#06x}: OCS shifter spr_width"
            );
        }
    }

    #[test]
    fn deref_exposes_inner_ecs_state() {
        let mut denise = DeniseAga::new();
        denise.bplcon3 = 0x6000; // BANK = 3
        assert_eq!(denise.bplcon3, 0x6000);
    }

    #[test]
    fn non_aga_register_writes_delegate_to_inner_ocs() {
        let mut denise = DeniseAga::new();
        // BPLCON0 ($100) — standard register, goes through to OCS.
        denise.write_word(0x0100, 0x1234);
        assert_eq!(denise.bplcon0(), 0x1234);
    }

    #[test]
    fn bplcon3_write_via_trait_lands_on_ecs_layer() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0xE200); // BANK=7, LOCT=1
        assert_eq!(denise.bplcon3, 0xE200);
    }

    #[test]
    fn color_write_high_mirrors_into_low_nybble() {
        let mut denise = DeniseAga::new();
        // BPLCON3 = 0 (bank 0, LOCT=0). COLOR00 = $FFF should make
        // palette_24[0] = $FFFFFF.
        denise.write_word(0x0180, 0x0FFF);
        assert_eq!(denise.palette_24[0], 0x00FF_FFFF);
        // OCS 12-bit palette also gets the value for bank-0 high
        // writes so the existing render path keeps working.
        assert_eq!(denise.palette()[0], 0x0FFF);
    }

    #[test]
    fn color_write_loct_only_updates_low_nybble() {
        let mut denise = DeniseAga::new();
        // First the high write sets palette_24[0] = $FFFFFF.
        denise.write_word(0x0180, 0x0FFF);
        // Switch to LOCT=1 and clear the low nybble of each channel.
        denise.write_word(0x0106, 0x0200);
        denise.write_word(0x0180, 0x0000);
        // High nybble preserved, low nybble zeroed.
        assert_eq!(denise.palette_24[0], 0x00F0_F0F0);
        // OCS palette must keep the high-write value — LOCT=1 must
        // not propagate to the 12-bit table.
        assert_eq!(denise.palette()[0], 0x0FFF);
    }

    #[test]
    fn color_write_bank_offset_into_palette_24() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0x4000); // BANK=2, LOCT=0
        denise.write_word(0x0184, 0x0A50); // COLOR02 in bank 2 → slot 66
        assert_eq!(denise.palette_24[66], 0x00AA_5500);
        // OCS palette[2] must NOT receive non-bank-0 writes.
        assert_eq!(denise.palette()[2], 0x0000);
    }
}
