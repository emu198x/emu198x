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
//! For Stage A of the A1200 wiring (per
//! `knowledge/decisions/amiga-machine-rollout-plan.md`), Lisa is
//! structurally complete (wrapper + AGA register storage + DeniseChip
//! trait impl + DENISEID = $00F8) but the rendering paths (24-bit
//! palette resolution, HAM8 chaining, wide sprite emit) are
//! deferred to the first AGA catalogue entry that exercises them.
//! KS 3.x boot reads DENISEID + writes BPLCON3 / BPLCON4 / FMODE
//! during init; the writes land in AGA-specific state, the reads
//! return the AGA marker, and rendering continues through the ECS
//! 12-bit palette path until the AGA rendering work lands.
//!
//! Adapted from `Emu198x-Oldest/crates/commodore-denise-aga/`.

use std::ops::{Deref, DerefMut};

pub use commodore_denise_ecs::DeniseEcs as InnerDeniseEcs;
pub use commodore_denise_ocs::{DeniseOcs as InnerDeniseOcs, DeniseOutputPixelDebug};

use common_commodore_amiga::denise_chip::DeniseChip;

/// AGA Lisa ID register value (low byte). Set in the silicon mask
/// programming; matches WinUAE's `aga_denise = 0xFFF8` (high byte is
/// open-bus `$FF`; low byte is `$F8` for Lisa).
pub const LISA_DENISE_ID: u16 = 0x00F8;

/// Number of palette entries in AGA (vs 32 on OCS / ECS).
pub const PALETTE_ENTRIES_24: usize = 256;

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
}

/// Serde adapter — `[u32; 256]` isn't `Serialize`/`Deserialize` by
/// default in serde without `serde-big-array` or similar. Pack the
/// palette as a `Vec<u32>` of length 256 over the wire.
mod palette_24_serde {
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub fn serialize<S: Serializer>(p: &[u32; super::PALETTE_ENTRIES_24], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(p.iter())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<[u32; super::PALETTE_ENTRIES_24], D::Error> {
        let v: Vec<u32> = Vec::deserialize(d)?;
        v.try_into()
            .map_err(|v: Vec<u32>| D::Error::custom(format!("palette_24 length {} != 256", v.len())))
    }
}

impl DeniseAga {
    /// Construct a fresh Lisa with the AGA register state zeroed and
    /// sprite width at the AGA default of 16 pixels.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: InnerDeniseEcs::new(),
            bplcon4: 0,
            palette_24: [0; PALETTE_ENTRIES_24],
            ham_prev_rgb24: 0,
            spr_width: 16,
        }
    }

    /// Promote an existing ECS Super Denise to AGA Lisa. Carries inner
    /// state across; AGA register state starts at the reset defaults.
    #[must_use]
    pub fn from_ecs(inner: InnerDeniseEcs) -> Self {
        Self {
            inner,
            bplcon4: 0,
            palette_24: [0; PALETTE_ENTRIES_24],
            ham_prev_rgb24: 0,
            spr_width: 16,
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

    /// AGA Lisa ID register value, as reported by DENISEID ($DFF07C).
    /// Real silicon returns $00F8 in the low byte; the high byte is
    /// open bus ($FF on a typical board).
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
        self.spr_width = match (fmode >> 2) & 0x0003 {
            0 => 16,
            1 | 2 => 32,
            _ => 64,
        };
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
// behaviour (24-bit palette resolution, HAM8 chaining, BPLCON4 XOR,
// wide sprite emit) is added incrementally as catalogue entries
// surface the requirement.

impl DeniseChip for DeniseAga {
    fn new() -> Self {
        DeniseAga::new()
    }

    fn write_word(&mut self, offset: u16, val: u16) {
        // BPLCON4 lands here on its own offset (AGA-only register).
        // Other writes pass through to the ECS layer unchanged.
        const BPLCON4: u16 = 0x010C;
        const FMODE: u16 = 0x01FC;
        match offset {
            BPLCON4 => {
                self.bplcon4 = val;
            }
            FMODE => {
                // Lisa cares about FMODE bits 3..2 for sprite width.
                // Alice (the Agnus side) owns FMODE storage; Lisa
                // receives the value when the machine layer forwards
                // the write here.
                self.set_sprite_width_from_fmode(val);
            }
            _ => self.inner.as_inner_mut().write_word(offset, val),
        }
    }

    fn load_bitplane(&mut self, idx: usize, val: u16) {
        self.inner.as_inner_mut().load_bitplane(idx, val);
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

    fn resolve_color_rgb12(&mut self, color_idx: u8) -> u16 {
        self.inner.resolve_color_rgb12(color_idx)
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
}

#[cfg(test)]
mod tests {
    use super::{DeniseAga, LISA_DENISE_ID};
    use common_commodore_amiga::denise_chip::DeniseChip;

    #[test]
    fn new_starts_with_aga_register_defaults() {
        let denise = DeniseAga::new();
        assert_eq!(denise.bplcon4, 0);
        assert_eq!(denise.spr_width, 16);
        assert_eq!(denise.ham_prev_rgb24, 0);
        assert!(denise.palette_24.iter().all(|&c| c == 0));
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
    }

    #[test]
    fn fmode_write_via_trait_updates_sprite_width() {
        let mut denise = DeniseAga::new();
        for (fmode, expected) in [(0x0000, 16u8), (0x0004, 32), (0x0008, 32), (0x000C, 64)] {
            denise.write_word(0x01FC, fmode);
            assert_eq!(denise.spr_width, expected, "FMODE={fmode:#06x}");
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
}
