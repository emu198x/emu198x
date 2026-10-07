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
//! - **CLXCON2** — BP7/BP8 collision enables and match values.
//!
//! Rendering status (per `knowledge/decisions/amiga-machine-rollout-plan.md`):
//! - **24-bit palette resolution** — done (#93): normal indexed modes
//!   resolve through `palette_24` for 8-bit-per-channel colour.
//! - **Wide sprite emit** — done (#95): FMODE feeds the OCS shifter's
//!   `spr_width` (16 / 32 / 64 px).
//! - **HAM chaining and EHB** — done (#94): AGA HAM6/HAM8 and EHB resolve
//!   through Lisa's 24-bit palette and hold state.
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

use common_commodore_amiga::denise_counter::DeniseStrobe;
use common_commodore_amiga::{denise::HorizontalBlanking, denise_chip::DeniseChip};

/// Lisa's strobe-driven vertical blanking, retained across horizontal edges.
/// Fixed and programmed events are independent (WinUAE `handle_strobes`,
/// `do_hbstrt/stop`, `do_phbstrt/stop_aga`). No viewport coordinate owns this state.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeniseAgaVerticalBlanking {
    pub previous_horizontal_strobe: bool,
    pub pending_fixed: Option<bool>,
    pub pending_programmed: Option<bool>,
    pub fixed_active: bool,
    pub programmed_active: bool,
}

impl DeniseAgaVerticalBlanking {
    fn strobe(&mut self, strobe: DeniseStrobe) {
        let horizontal = strobe == DeniseStrobe::Horizontal;
        if horizontal != self.previous_horizontal_strobe {
            self.pending_fixed = Some(!horizontal);
            self.pending_programmed = Some(!horizontal);
        }
        self.previous_horizontal_strobe = horizontal;
    }

    fn edge(&mut self, programmed: bool, start: bool) {
        let (pending, active) = if programmed {
            (&mut self.pending_programmed, &mut self.programmed_active)
        } else {
            (&mut self.pending_fixed, &mut self.fixed_active)
        };
        if *pending == Some(start) {
            *active = start;
            *pending = None;
        }
    }
}

/// AGA Lisa DENISEID value as the CPU reads it from $DFF07C.
/// WinUAE returns `0x00F8` for A1200 (and `0xFCF8` for A4000).
/// KS 3.x extracts bits 9-8 of the inverted value to derive the
/// sprite-width capability stored at GfxBase+454; `$FFF8` zeroes
/// those bits and breaks the AGA palette layout.
pub const LISA_DENISE_ID: u16 = 0x00F8;

/// Number of palette entries in AGA (vs 32 on OCS / ECS).
pub const PALETTE_ENTRIES_24: usize = 256;

const BPLCON2_RDRAM: u16 = 0x0100;
const BPLCON3_LOCT: u16 = 0x0200;

/// One AGA `COLORxx` write waiting to cross Lisa's one-hires-pixel output
/// delay. The palette register mirrors already contain the new values; these
/// fields preserve the values visible to the immediately following sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeniseAgaDelayedColorWrite {
    /// AGA palette slot selected by BPLCON3 BANK and the COLOR register.
    pub palette_index: u8,
    /// Previous 24-bit value, stored as `0x00RRGGBB`.
    pub previous_rgb24: u32,
    /// Previous bank-zero 12-bit value when the write also updated the
    /// compatibility palette. Other banks and LOCT writes leave this absent.
    pub previous_rgb12: Option<u16>,
    /// Previous transparency/genlock flag for the selected palette slot.
    pub previous_genlock: bool,
}

/// Lisa-side copies of Alice's programmable horizontal-blank registers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeniseAgaProgrammedHblankRegisters {
    /// HBSTRT coarse/fine comparator word.
    pub hbstrt: u16,
    /// HBSTOP coarse/fine comparator word.
    pub hbstop: u16,
}

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
    /// Per-entry AGA palette transparency/genlock flags.
    #[serde(with = "palette_genlock_serde")]
    pub palette_genlock: [bool; PALETTE_ENTRIES_24],
    /// Lisa's current 24-bit HAM8 hold colour, stored as `0x00RRGGBB`.
    pub ham_prev_rgb24: u32,
    /// Lisa's FMODE-derived sprite width mirror.
    pub spr_width: u8,
    /// Hidden programmable horizontal-blank comparator level.
    pub programmed_hblank_active: bool,
    pub vertical_blanking: DeniseAgaVerticalBlanking,
    /// Pending one-hires-pixel AGA palette-output delay, if any.
    pub delayed_color_write: Option<DeniseAgaDelayedColorWrite>,
    /// Previous palette value retained across the current master/4 output tick.
    pub pending_early_color_write: Option<DeniseAgaDelayedColorWrite>,
    /// Alice register values presented to Lisa's normal input stage.
    pub programmed_hblank_input: DeniseAgaProgrammedHblankRegisters,
    /// Comparator words currently visible inside Lisa.
    pub programmed_hblank_visible: DeniseAgaProgrammedHblankRegisters,
    /// Pending normal-stage comparator copies, nearest output stage first.
    pub programmed_hblank_pipeline: [DeniseAgaProgrammedHblankRegisters; 2],
    /// Raw BPLCON0 presented to Lisa.
    pub bplcon0_input: u16,
    /// Pending normal-stage serializer control copies.
    pub bplcon0_pipeline: [u16; 2],
    pub bplcon1_visible: u16,
    pub bplcon1_pipeline: [u16; 2],
    /// Raw Alice fetch-width input, normal-stage copy and pending copies.
    pub bitplane_fmode_input: u16,
    pub bitplane_fmode_visible: u16,
    pub bitplane_fmode_pipeline: [u16; 2],
    /// BPLAM sample history; the palette selector reads the six-sample tap.
    pub playfield_xor_pipeline: [u8; 10],
    /// DIW gate for the current group of native samples.
    pub playfield_output_active: bool,
    pub playfield_output_gates: [bool; 4],
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
    /// Per-entry transparency/genlock flag written through COLOR bit 15.
    #[serde(default = "default_palette_genlock", with = "palette_genlock_serde")]
    pub palette_genlock: [bool; PALETTE_ENTRIES_24],
    /// Last resolved RGB24 value, used by HAM8 chaining.
    pub ham_prev_rgb24: u32,
    /// Current sprite display width in pixels (16 / 32 / 64),
    /// driven by FMODE bits 3..2.
    pub spr_width: u8,
    /// Hidden Lisa programmable horizontal-blank level. Comparator events and
    /// the live ECSENA/EXTBLKEN selectors change this state; register writes
    /// do not reconstruct it from the current beam position.
    programmed_hblank_active: bool,
    vertical_blanking: DeniseAgaVerticalBlanking,
    /// The previous palette value visible for one hires output sample after
    /// an AGA COLOR write. Register and inspection reads see the new value
    /// immediately; only pixel output is delayed.
    #[serde(default)]
    delayed_color_write: Option<DeniseAgaDelayedColorWrite>,
    /// Previous palette value held through the current early RGA output stage.
    #[serde(default)]
    pending_early_color_write: Option<DeniseAgaDelayedColorWrite>,
    /// Current Alice HBSTRT/HBSTOP values presented to Lisa.
    programmed_hblank_input: DeniseAgaProgrammedHblankRegisters,
    /// HBSTRT/HBSTOP values currently visible to Lisa's comparator.
    programmed_hblank_visible: DeniseAgaProgrammedHblankRegisters,
    /// Two normal-stage comparator copies between Alice and Lisa.
    programmed_hblank_pipeline: [DeniseAgaProgrammedHblankRegisters; 2],
    bplcon0_input: u16,
    bplcon0_pipeline: [u16; 2],
    bplcon1_pipeline: [u16; 2],
    bitplane_fmode_input: u16,
    bitplane_fmode_visible: u16,
    bitplane_fmode_pipeline: [u16; 2],
    // Retain the ten-sample history for snapshot 54; output reads tap 4.
    playfield_xor_pipeline: [u8; 10],
    playfield_output_gates: [bool; 4],
}

const fn default_palette_genlock() -> [bool; PALETTE_ENTRIES_24] {
    [false; PALETTE_ENTRIES_24]
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

/// Serde adapter for the fixed 256-entry transparency table.
mod palette_genlock_serde {
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub fn serialize<S: Serializer>(
        flags: &[bool; super::PALETTE_ENTRIES_24],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(flags.iter())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[bool; super::PALETTE_ENTRIES_24], D::Error> {
        let values: Vec<bool> = Vec::deserialize(deserializer)?;
        values.try_into().map_err(|values: Vec<bool>| {
            D::Error::custom(format!("palette_genlock length {} != 256", values.len()))
        })
    }
}

impl DeniseAga {
    /// Construct a fresh Lisa with the AGA register reset defaults and
    /// sprite width at the AGA default of 16 pixels.
    #[must_use]
    pub fn new() -> Self {
        let mut inner = InnerDeniseEcs::new();
        // Lisa resets PF2OF to 3, retaining the OCS playfield-2 offset 8.
        // WinUAE drawing.cpp reset and Minimig denise.v agree on $0C00.
        inner.bplcon3 = 0x0C00;
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
            palette_genlock: [false; PALETTE_ENTRIES_24],
            ham_prev_rgb24: 0,
            spr_width: 16,
            programmed_hblank_active: false,
            vertical_blanking: DeniseAgaVerticalBlanking::default(),
            delayed_color_write: None,
            pending_early_color_write: None,
            programmed_hblank_input: DeniseAgaProgrammedHblankRegisters::default(),
            programmed_hblank_visible: DeniseAgaProgrammedHblankRegisters::default(),
            programmed_hblank_pipeline: [DeniseAgaProgrammedHblankRegisters::default(); 2],
            bplcon0_input: 0,
            bplcon0_pipeline: [0; 2],
            bplcon1_pipeline: [0; 2],
            bitplane_fmode_input: 0,
            bitplane_fmode_visible: 0,
            bitplane_fmode_pipeline: [0; 2],
            playfield_xor_pipeline: [0; 10],
            playfield_output_gates: [false; 4],
        }
    }

    /// Promote an existing ECS Super Denise to AGA Lisa. Carries inner
    /// state across; AGA register state starts at the reset defaults.
    #[must_use]
    pub fn from_ecs(inner: InnerDeniseEcs) -> Self {
        let mut inner = inner;
        // Promotion to Lisa raises the bitplane ceiling to 8 (see `new`).
        inner.as_inner_mut().max_bitplanes = 8;
        let bplcon0 = inner.as_inner().bplcon0;
        let fmode = inner.as_inner().bitplane_fmode;
        let bplcon1 = inner.as_inner().bplcon1;
        Self {
            inner,
            bplcon4: 0x0011,
            palette_24: [0; PALETTE_ENTRIES_24],
            palette_genlock: [false; PALETTE_ENTRIES_24],
            ham_prev_rgb24: 0,
            spr_width: 16,
            programmed_hblank_active: false,
            vertical_blanking: DeniseAgaVerticalBlanking::default(),
            delayed_color_write: None,
            pending_early_color_write: None,
            programmed_hblank_input: DeniseAgaProgrammedHblankRegisters::default(),
            programmed_hblank_visible: DeniseAgaProgrammedHblankRegisters::default(),
            programmed_hblank_pipeline: [DeniseAgaProgrammedHblankRegisters::default(); 2],
            bplcon0_input: bplcon0,
            bplcon0_pipeline: [bplcon0; 2],
            bplcon1_pipeline: [bplcon1; 2],
            bitplane_fmode_input: fmode,
            bitplane_fmode_visible: fmode,
            bitplane_fmode_pipeline: [fmode; 2],
            playfield_xor_pipeline: [0; 10],
            playfield_output_gates: [false; 4],
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
            palette_genlock: self.palette_genlock,
            ham_prev_rgb24: self.ham_prev_rgb24,
            spr_width: self.spr_width,
            programmed_hblank_active: self.programmed_hblank_active,
            vertical_blanking: self.vertical_blanking,
            delayed_color_write: self.delayed_color_write,
            pending_early_color_write: self.pending_early_color_write,
            programmed_hblank_input: self.programmed_hblank_input,
            programmed_hblank_visible: self.programmed_hblank_visible,
            programmed_hblank_pipeline: self.programmed_hblank_pipeline,
            bplcon0_input: self.bplcon0_input,
            bplcon0_pipeline: self.bplcon0_pipeline,
            bplcon1_visible: self.inner.as_inner().bitplane_scroll_selector,
            bplcon1_pipeline: self.bplcon1_pipeline,
            bitplane_fmode_input: self.bitplane_fmode_input,
            bitplane_fmode_visible: self.bitplane_fmode_visible,
            bitplane_fmode_pipeline: self.bitplane_fmode_pipeline,
            playfield_xor_pipeline: self.playfield_xor_pipeline,
            playfield_output_active: self.playfield_output_gates[3],
            playfield_output_gates: self.playfield_output_gates,
        }
    }

    fn advance_playfield_xor_sample(&mut self) -> u8 {
        // Early RGA accepts BPLAM before the first lores half of a CCK.
        // The reference selects it halfway through the second half: six
        // native samples later. The old ten-sample tap included one lores
        // period from the reference framebuffer's padded storage origin.
        let visible = self.playfield_xor_pipeline[4];
        self.playfield_xor_pipeline.copy_within(1.., 0);
        self.playfield_xor_pipeline[9] = (self.bplcon4 >> 8) as u8;
        visible
    }

    /// Present Alice's live HBSTRT/HBSTOP register words to Lisa's normal
    /// display-side input stage.
    pub fn set_programmed_hblank_input(&mut self, hbstrt: u16, hbstop: u16) {
        self.programmed_hblank_input = DeniseAgaProgrammedHblankRegisters { hbstrt, hbstop };
    }

    /// Alice register words currently presented to Lisa.
    #[must_use]
    pub const fn programmed_hblank_input(&self) -> DeniseAgaProgrammedHblankRegisters {
        self.programmed_hblank_input
    }

    /// HBSTRT/HBSTOP words currently visible to Lisa's comparator.
    #[must_use]
    pub const fn programmed_hblank_visible(&self) -> DeniseAgaProgrammedHblankRegisters {
        self.programmed_hblank_visible
    }

    /// Pending normal-stage HBSTRT/HBSTOP copies, nearest output stage first.
    #[must_use]
    pub const fn programmed_hblank_pipeline(&self) -> [DeniseAgaProgrammedHblankRegisters; 2] {
        self.programmed_hblank_pipeline
    }

    /// Advance Alice's comparator words through Lisa's normal display path.
    pub fn advance_programmed_hblank_pipeline(&mut self) {
        self.programmed_hblank_visible = self.programmed_hblank_pipeline[0];
        self.programmed_hblank_pipeline[0] = self.programmed_hblank_pipeline[1];
        self.programmed_hblank_pipeline[1] = self.programmed_hblank_input;
    }

    /// Compose Lisa's fixed or programmable horizontal and vertical blanking over the four
    /// 35 ns samples produced by one Denise phase.
    ///
    /// The coarse comparator occupies the low byte of HBSTRT/HBSTOP. Lisa's
    /// three fine bits address the eight 35 ns samples per CCK. ECSENA and EXTBLKEN are sampled live; disabling either clears the
    /// hidden level, so enabling a selector after HBSTRT cannot synthesize a
    /// start event. The fixed $10/$5D comparators observe the next lores
    /// counter, so their output interval is $0F..$5C.
    /// BEAMCON0.BLANKEN is not part of the Lisa path.
    #[must_use]
    pub fn programmed_hblank_for_output_phase(
        &mut self,
        hpos: u16,
        phase: u8,
        bplcon0: u16,
        hbstrt: u16,
        hbstop: u16,
    ) -> HorizontalBlanking {
        debug_assert!(phase < 2);
        self.programmed_hblank_for_next_counter(
            ((hpos & 0x00FF) * 2 + u16::from(phase) + 1) & 511,
            bplcon0,
            hbstrt,
            hbstop,
        )
    }

    /// Compose blanking using the next counter selected by the live strobe
    /// stage. At a reset it differs from the current output counter plus one.
    #[must_use]
    pub fn programmed_hblank_for_next_counter(
        &mut self,
        next_counter: u16,
        bplcon0: u16,
        hbstrt: u16,
        hbstop: u16,
    ) -> HorizontalBlanking {
        const OUTPUT_SAMPLES_PER_CCK: u16 = 8;

        self.bplcon0_input = bplcon0;
        self.set_programmed_hblank_input(hbstrt, hbstop);
        let selectors_enabled =
            self.inner.output_ecsena_enabled() && self.inner.output_extblken_enabled();
        let fine_sample =
            |word: u16| (word & 0x00FF) * OUTPUT_SAMPLES_PER_CCK + ((word >> 8) & 0x0007);
        let start_sample = fine_sample(self.programmed_hblank_visible.hbstrt);
        let stop_sample = fine_sample(self.programmed_hblank_visible.hbstop);
        let mut output_samples = [false; 4];

        for (subpixel, output) in output_samples.iter_mut().enumerate() {
            let next_sample = (next_counter & 511) * 4 + subpixel as u16;
            if !selectors_enabled {
                self.programmed_hblank_active = false;
                if next_sample == 0x10 * 4 {
                    self.vertical_blanking.edge(false, true);
                }
                if next_sample == 0x5D * 4 {
                    self.vertical_blanking.edge(false, false);
                }
                *output = self.vertical_blanking.fixed_active
                    || (0x10 * 4..0x5D * 4).contains(&next_sample);
                continue;
            }
            // Lisa compares programmed edges with the next lores counter,
            // including its four fine samples (UAE checkhorizontal1_aga and
            // lts_unaligned_aga). Counter-traced SPHX edges establish this
            // phase independently of the framebuffer's retained padding.
            // Start precedes stop, so equal edges describe an empty interval.
            if next_sample == start_sample {
                self.programmed_hblank_active = true;
                self.vertical_blanking.edge(true, true);
            }
            if next_sample == stop_sample {
                self.programmed_hblank_active = false;
                self.vertical_blanking.edge(true, false);
            }
            *output = self.programmed_hblank_active || self.vertical_blanking.programmed_active;
        }

        HorizontalBlanking::from_superhires_samples(output_samples)
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

    /// Handle an ordinary post-output write to one of the COLOR registers
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
    /// - with `BPLCON2.RDRAM` set, the write is ignored.
    ///
    /// The OCS compatibility palette is also updated for bank 0 / LOCT=0
    /// writes. Register mirrors change immediately, but Lisa keeps the
    /// previous 24-bit value at its output for one hires pixel.
    pub fn handle_color_write(&mut self, offset: u16, val: u16) {
        if let Some(delayed) = self.apply_color_write(offset, val) {
            self.delayed_color_write = Some(delayed);
        }
    }

    fn handle_color_write_with_early_output_delay(&mut self, offset: u16, val: u16) {
        if let Some(delayed) = self.apply_color_write(offset, val) {
            // Copper is dispatched on the early RGA phase already. Only
            // Lisa's two-sample palette delay remains before composition.
            self.pending_early_color_write = None;
            self.delayed_color_write = Some(delayed);
        }
    }

    fn take_color_output_delay(&mut self) -> Option<DeniseAgaDelayedColorWrite> {
        self.pending_early_color_write
            .or_else(|| self.delayed_color_write.take())
    }

    fn apply_color_write(&mut self, offset: u16, val: u16) -> Option<DeniseAgaDelayedColorWrite> {
        // Lisa samples RDRAM with the write before placing the one-pixel
        // delayed colour change into its output pipeline. A protected write
        // therefore changes neither the register mirrors nor pending output.
        if self.inner.as_inner().bplcon2 & BPLCON2_RDRAM != 0 {
            return None;
        }
        let idx = ((offset - 0x180) / 2) as usize;
        let bplcon3 = self.inner.bplcon3;
        let bank = ((bplcon3 >> 13) & 0x7) as usize;
        let loct = (bplcon3 & BPLCON3_LOCT) != 0;
        let slot = bank * 32 + idx;
        if slot < PALETTE_ENTRIES_24 {
            let previous_rgb24 = self.palette_24[slot];
            let previous_genlock = self.palette_genlock[slot];
            let previous_rgb12 = (bank == 0 && !loct).then(|| self.inner.as_inner().palette[idx]);
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
                self.palette_genlock[slot] = val & 0x8000 != 0;
            }

            let delayed = DeniseAgaDelayedColorWrite {
                palette_index: slot as u8,
                previous_rgb24,
                previous_rgb12,
                previous_genlock,
            };
            // Keep the OCS 12-bit palette in sync for the existing render
            // path — but only for bank 0 / high writes, so LOCT=1 passes
            // don't corrupt the 12-bit value we'll still resolve through.
            if bank == 0 && !loct {
                self.inner.write_word(offset, val);
            }
            return Some(delayed);
        }

        // Keep the OCS 12-bit palette in sync for the existing render
        // path — but only for bank 0 / high writes, so LOCT=1 passes
        // don't corrupt the 12-bit value we'll still resolve through.
        if bank == 0 && !loct {
            self.inner.write_word(offset, val);
        }
        None
    }

    /// Read one banked AGA colour-table register through `BPLCON2.RDRAM`.
    ///
    /// The selected `BPLCON3.BANK` chooses one of the eight 32-colour banks.
    /// `BPLCON3.LOCT` chooses the low or high nibble of each stored eight-bit
    /// channel. Without RDRAM, colour registers remain write-only.
    #[must_use]
    pub fn read_color_register(&self, offset: u16) -> u16 {
        if self.inner.as_inner().bplcon2 & BPLCON2_RDRAM == 0
            || !(0x0180..=0x01BE).contains(&offset)
            || offset & 1 != 0
        {
            return 0xFFFF;
        }

        let color = usize::from((offset - 0x0180) / 2);
        let bank = usize::from((self.inner.bplcon3 >> 13) & 0x0007);
        let rgb24 = self.palette_24[bank * 32 + color];
        let shift = if self.inner.bplcon3 & BPLCON3_LOCT != 0 {
            0
        } else {
            4
        };
        let red = ((rgb24 >> (16 + shift)) & 0x0F) as u16;
        let green = ((rgb24 >> (8 + shift)) & 0x0F) as u16;
        let blue = ((rgb24 >> shift) & 0x0F) as u16;
        let genlock = if shift == 4 && self.palette_genlock[bank * 32 + color] {
            0x8000
        } else {
            0
        };
        genlock | (red << 8) | (green << 4) | blue
    }

    fn resolve_rgb12_with_delayed_write(
        &mut self,
        color_idx: u8,
        delayed: Option<DeniseAgaDelayedColorWrite>,
    ) -> u16 {
        let Some(delayed) = delayed else {
            return self.inner.resolve_color_rgb12(color_idx);
        };
        let Some(previous_rgb12) = delayed.previous_rgb12 else {
            return self.inner.resolve_color_rgb12(color_idx);
        };

        let palette_index = usize::from(delayed.palette_index);
        // A valid write only records RGB12 for bank zero. Treat malformed
        // restored state as having no compatible 12-bit delay instead of
        // indexing beyond the fixed OCS palette.
        if palette_index >= 32 {
            return self.inner.resolve_color_rgb12(color_idx);
        }
        let current_rgb12 = self.inner.as_inner().palette[palette_index];
        self.inner.as_inner_mut().palette[palette_index] = previous_rgb12;
        let resolved = self.inner.resolve_color_rgb12(color_idx);
        self.inner.as_inner_mut().palette[palette_index] = current_rgb12;
        resolved
    }

    fn palette_rgb24_with_delayed_write(
        &self,
        palette_index: usize,
        delayed: Option<DeniseAgaDelayedColorWrite>,
    ) -> u32 {
        delayed
            .filter(|entry| usize::from(entry.palette_index) == palette_index)
            .map_or(self.palette_24[palette_index], |entry| entry.previous_rgb24)
            & 0x00FF_FFFF
    }

    fn resolve_playfield_color_argb_with_delayed_write(
        &mut self,
        color_idx: u8,
        delayed: Option<DeniseAgaDelayedColorWrite>,
    ) -> u32 {
        let ocs = self.inner.as_inner();
        let bplcon0 = ocs.bplcon0;
        let ham = bplcon0 & 0x0800 != 0;
        let dual_playfield = bplcon0 & 0x0400 != 0;
        let planes = ocs.num_bitplanes();
        let kill_ehb = ocs.bplcon2 & 0x0200 != 0;

        // Lisa HAM resolves through the 24-bit palette and hold register.
        // HAM8 uses the low two control bits and high six data bits; HAM6
        // uses the high two control bits and replicates its four data bits
        // across the selected eight-bit channel. Confirmed against
        // Minimig-AGA's `denise_hamgenerator.v` and WinUAE/FS-UAE's
        // `decode_ham_pixel_aga`.
        //
        // `color_idx` already has the BPLCON4 BPLAM XOR applied upstream
        // in `compose_playfield_pixel` (#96), so control + data are taken
        // post-XOR (Minimig's behaviour). WinUAE XORs only the control and
        // colour-register index, taking modify data from the raw pixel —
        // the two can diverge in both HAM6 and HAM8 when BPLAM changes
        // modification data. This path currently follows Minimig; that
        // disagreement has not been resolved by a physical capture.
        if ham && !dual_playfield && planes >= 5 {
            let prev = self.ham_prev_rgb24 & 0x00FF_FFFF;
            let rgb = if planes >= 7 {
                let control = color_idx & 0x03;
                let data6 = u32::from(color_idx >> 2);
                match control {
                    0b00 => self.palette_rgb24_with_delayed_write(data6 as usize, delayed),
                    0b01 => {
                        let blue = (data6 << 2) | (prev & 0x03);
                        (prev & 0x00FF_FF00) | blue
                    }
                    0b10 => {
                        let red = (data6 << 2) | ((prev >> 16) & 0x03);
                        (prev & 0x0000_FFFF) | (red << 16)
                    }
                    _ => {
                        let green = (data6 << 2) | ((prev >> 8) & 0x03);
                        (prev & 0x00FF_00FF) | (green << 8)
                    }
                }
            } else {
                let control = color_idx & 0x30;
                let data8 = u32::from(color_idx & 0x0F) * 0x11;
                match control {
                    0x00 => self
                        .palette_rgb24_with_delayed_write(usize::from(color_idx & 0x0F), delayed),
                    0x10 => (prev & 0x00FF_FF00) | data8,
                    0x20 => (prev & 0x0000_FFFF) | (data8 << 16),
                    _ => (prev & 0x00FF_00FF) | (data8 << 8),
                }
            };
            self.ham_prev_rgb24 = rgb;
            return 0xFF00_0000 | rgb;
        }

        // KILLEHB selects ordinary indexed output on Lisa. Unlike ECS,
        // Lisa has independent entries above COLOR31, so the complete
        // post-BPLAM address must reach the palette in this mode.
        // WinUAE setbplmode() selects CMODE_NORMAL; Minimig deasserts
        // ehb_en and passes select_xored unchanged to rd_adr.
        if !ham && !dual_playfield && planes == 6 && !kill_ehb {
            let palette_index = usize::from(color_idx & 0x1F);
            let mut rgb24 = self.palette_rgb24_with_delayed_write(palette_index, delayed);
            if color_idx & 0x20 != 0 {
                rgb24 = (rgb24 >> 1) & 0x007F_7F7F;
            }
            return 0xFF00_0000 | rgb24;
        }

        let palette_index = usize::from(color_idx);
        0xFF00_0000 | self.palette_rgb24_with_delayed_write(palette_index, delayed)
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
    fn retire_timing_strobe(&mut self, strobe: DeniseStrobe) {
        self.vertical_blanking.strobe(strobe);
    }

    const SUPPORTS_DIWHIGH: bool = true;
    const OUTPUT_SAMPLES_PER_LORES: u32 = 4;
    const RESETS_COUNTER_ON_EQUALISATION: bool = true;

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
            0x0100 => {
                let visible = self.inner.as_inner().bplcon0;
                self.inner.write_word(offset, val);
                self.bplcon0_input = val;
                self.inner.as_inner_mut().bplcon0 = visible;
            }
            0x0102 => {
                let visible = self.inner.as_inner().bitplane_scroll_selector;
                self.inner.write_word(offset, val);
                self.inner.as_inner_mut().bitplane_scroll_selector = visible;
            }
            0x0110 => {
                self.inner.as_inner_mut().bitplane_fmode = self.bitplane_fmode_visible;
                self.inner.write_word(offset, val);
            }
            0x010E => self.inner.as_inner_mut().clxcon2 = val,
            // The OCS dispatcher ends at BPL7DAT; Lisa also decodes the
            // eighth holding register. BPL1DAT remains the copy strobe.
            0x011E => self.inner.as_inner_mut().load_bitplane(7, val),
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
                // Lisa uses FMODE for sprite width and serial scroll depth.
                // Alice (the Agnus side) owns DMA scheduling; Lisa
                // receives the value when the machine layer forwards
                // the write here.
                self.set_sprite_width_from_fmode(val);
                self.bitplane_fmode_input = val;
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
        // WinUAE latches bpldat_fmode on BPL1DAT, separately from FMODE's
        // normal RGA selector. Existing data keeps its original copy width.
        self.inner.as_inner_mut().bitplane_fmode = self.bitplane_fmode_visible;
        self.inner.as_inner_mut().queue_shift_load_from_bpl1dat();
    }

    fn enable_sprites_from_bpl1dat(&mut self) {
        self.inner.as_inner_mut().enable_sprites_from_bpl1dat();
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
        // AGA HAM6 and HAM8 hold a 24-bit running colour across the line;
        // reset it to COLOR00 at the start of each scanline, the same way
        // the OCS layer resets its 12-bit HAM hold register.
        self.ham_prev_rgb24 = self.palette_24[0] & 0x00FF_FFFF;
    }

    fn border_blanking_for_output(&self, playfield_visible_gate: bool) -> bool {
        let ocs = self.inner.as_inner();
        ocs.bplcon0 & 1 != 0
            && self.inner.bplcon3 & 0x20 != 0
            && (!playfield_visible_gate || !ocs.sprite_bpl1dat_enabled())
    }

    fn output_pixel_with_beam_and_playfield_gate(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug {
        self.output_pixel_with_sample_gates(
            x,
            y,
            beam_x,
            beam_y,
            beam_x,
            beam_y,
            [playfield_visible_gate; 4],
        )
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
        self.output_pixel_with_sample_gates(
            x,
            y,
            beam_x,
            beam_y,
            spr_beam_x,
            spr_beam_y,
            [playfield_visible_gate; 4],
        )
    }

    fn output_pixel_with_sample_gates(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        spr_beam_x: u32,
        spr_beam_y: u32,
        gates: [bool; 4],
    ) -> DeniseOutputPixelDebug {
        self.playfield_output_gates = gates;
        let selector = ((self.inner.bplcon3 >> 10) & 7) as u8;
        let pf2_palette_offset = if selector == 0 { 0 } else { 1 << selector };
        let border_sprites_enabled =
            self.inner.as_inner().bplcon0 & 1 != 0 && self.inner.bplcon3 & 2 != 0;
        let sprite_sample_period = match (self.inner.bplcon3 >> 6) & 3 {
            0 if self.inner.as_inner().bplcon0 & 0x40 != 0 => 2,
            2 => 2,
            3 => 1,
            _ => 4,
        };
        let mut output = self
            .inner
            .as_inner_mut()
            .output_pixel_with_sample_gates_and_display_controls(
                x,
                y,
                beam_x,
                beam_y,
                spr_beam_x,
                spr_beam_y,
                gates,
                pf2_palette_offset,
                border_sprites_enabled,
                sprite_sample_period,
            );
        output.beam_x = beam_x;
        output
    }

    fn resolve_color_rgb12(&mut self, color_idx: u8) -> u16 {
        let delayed = self.take_color_output_delay();
        self.resolve_rgb12_with_delayed_write(color_idx, delayed)
    }

    /// Resolve to a final ARGB8888 pixel through the AGA 24-bit palette.
    ///
    /// - **Normal indexed** (#93): `palette_24[idx]` (8-bit-per-channel).
    /// - **HAM6 / HAM8**: Lisa hold-and-modify through its 24-bit palette
    ///   and 24-bit running colour.
    /// - **EHB**: halve each eight-bit component from the 24-bit base entry,
    ///   unless AGA `BPLCON2.KILLEHB` suppresses half-brite selection.
    fn resolve_color_argb(&mut self, color_idx: u8) -> u32 {
        let delayed = self.take_color_output_delay();
        self.resolve_playfield_color_argb_with_delayed_write(color_idx, delayed)
    }

    fn resolve_output_color_argb(
        &mut self,
        playfield_color_idx: u8,
        output_color_idx: u8,
        is_sprite: bool,
    ) -> u32 {
        // One delayed palette view feeds both the underlying playfield decode
        // and a sprite that subsequently wins priority for this sample.
        let delayed = self.take_color_output_delay();
        let playfield =
            self.resolve_playfield_color_argb_with_delayed_write(playfield_color_idx, delayed);
        if is_sprite {
            let rgb24 =
                self.palette_rgb24_with_delayed_write(usize::from(output_color_idx), delayed);
            0xFF00_0000 | rgb24
        } else {
            playfield
        }
    }

    fn resolve_output_sample_argb(
        &mut self,
        playfield_color_idx: u8,
        output_color_idx: u8,
        is_sprite: bool,
        sample: u8,
    ) -> u32 {
        let visible_xor = self.advance_playfield_xor_sample();
        // Composition uses the raw mirror for diagnostics and sprite priority.
        // BPLAM changes only palette selection; replace its raw mask with the
        // mask that has crossed the timed output stage. Border is COLOR00.
        let playfield_color_idx = if self.playfield_output_gates[usize::from(sample)] {
            playfield_color_idx ^ (self.bplcon4 >> 8) as u8 ^ visible_xor
        } else {
            playfield_color_idx
        };
        // A normal COLOR write is delayed by one hires period: two 35 ns
        // samples. Consume the retained palette view only on the second.
        let delayed = if sample & 1 == 0 {
            self.pending_early_color_write.or(self.delayed_color_write)
        } else {
            self.take_color_output_delay()
        };
        let playfield =
            self.resolve_playfield_color_argb_with_delayed_write(playfield_color_idx, delayed);
        if is_sprite {
            0xFF00_0000
                | self.palette_rgb24_with_delayed_write(usize::from(output_color_idx), delayed)
        } else {
            playfield
        }
    }

    fn advance_color_output_samples(&mut self, samples: u8) {
        for _ in 0..samples {
            self.advance_playfield_xor_sample();
        }
        if samples != 0 {
            self.delayed_color_write = None;
        }
    }

    fn write_color_before_output_tick(&mut self, offset: u16, value: u16) -> bool {
        self.handle_color_write_with_early_output_delay(offset, value);
        true
    }

    fn advance_early_color_output_pipeline(&mut self) {
        if let Some(delayed) = self.pending_early_color_write.take() {
            self.delayed_color_write = Some(delayed);
        }
    }

    fn advance_register_output_pipeline(&mut self) {
        self.inner.as_inner_mut().bitplane_scroll_selector = self.bplcon1_pipeline[0];
        self.bplcon1_pipeline[0] = self.bplcon1_pipeline[1];
        self.bplcon1_pipeline[1] = self.inner.as_inner().bplcon1;
        self.bitplane_fmode_visible = self.bitplane_fmode_pipeline[0];
        self.bitplane_fmode_pipeline[0] = self.bitplane_fmode_pipeline[1];
        self.bitplane_fmode_pipeline[1] = self.bitplane_fmode_input;
        // The existing ECS selector stage samples the raw input, while
        // the serializer consumes the copy leaving this same normal stage.
        self.inner.as_inner_mut().bplcon0 = self.bplcon0_input;
        self.inner.advance_output_selector_pipeline();
        self.inner.as_inner_mut().bplcon0 = self.bplcon0_pipeline[0];
        self.bplcon0_pipeline[0] = self.bplcon0_pipeline[1];
        self.bplcon0_pipeline[1] = self.bplcon0_input;
        self.advance_programmed_hblank_pipeline();
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
        self.bplcon0_input = v;
        self.bplcon0_pipeline = [v; 2];
    }

    fn sync_bplcon0_input(&mut self, value: u16) {
        self.bplcon0_input = value;
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
    use super::{BPLCON2_RDRAM, DeniseAga, LISA_DENISE_ID};
    use common_commodore_amiga::{denise::HorizontalBlanking, denise_chip::DeniseChip};

    fn settle_programmed_hblank_inputs(
        denise: &mut DeniseAga,
        bplcon0: u16,
        hbstrt: u16,
        hbstop: u16,
    ) {
        denise.set_bplcon0(bplcon0);
        denise.set_programmed_hblank_input(hbstrt, hbstop);
        for _ in 0..3 {
            denise.advance_register_output_pipeline();
        }
    }

    #[test]
    fn new_starts_with_aga_register_defaults() {
        let denise = DeniseAga::new();
        // BPLCON4 resets to $0011 (Minimig denise.v): ESPRM/OSPRM = 1 so
        // sprites default to the OCS $10–$1F colour range, BPLAM = 0.
        assert_eq!(denise.bplcon3, 0x0C00); // PF2OF=3, default offset 8
        assert_eq!(denise.bplcon4, 0x0011);
        assert_eq!(denise.spr_width, 16);
        assert_eq!(denise.ham_prev_rgb24, 0);
        assert!(denise.palette_24.iter().all(|&c| c == 0));
        assert!(denise.palette_genlock.iter().all(|flag| !flag));
        assert!(!denise.programmed_hblank_active());
    }

    #[test]
    fn lisa_window_matches_reach_output_at_the_counter_equality() {
        let denise = DeniseAga::new();

        let mut window = common_commodore_amiga::denise_window::DeniseWindow::default();
        window.queue_write(0x08E, 129, true, true, true);
        window.queue_write(0x090, 193, true, true, true);
        for _ in 0..3 {
            window.begin_output_tick();
        }
        let mut active = false;
        // Independent UAE counter trace: the start match is composed at 129,
        // while the stop match at 449 already selects border.
        for position in 128..=450 {
            let gates = window.output_gates(
                &mut active,
                position,
                true,
                denise.horizontal_diw_comparator_phase(),
            );
            assert_eq!(
                gates,
                [(129..449).contains(&position); 4],
                "counter {position}"
            );
        }
    }

    #[test]
    fn fractional_window_gates_playfield_sprite_and_xor_per_sample() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x1000);
        denise.write_word(0x104, 4); // sprite group zero in front of playfield
        denise.begin_beam_line();
        denise.load_bitplane(0, 0xFFFF);
        denise.write_sprite_pos(0, 0);
        denise.write_sprite_ctl(0, 0);
        denise.write_sprite_datb(0, 0);
        denise.write_sprite_data(0, 0xFFFF);
        denise.queue_shift_load_from_bpl1dat();
        for x in 0..2 {
            denise.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, true);
        }
        let output =
            denise.output_pixel_with_sample_gates(2, 0, 2, 0, 2, 0, [false, true, true, false]);
        assert_eq!(output.quad_playfield_color_idx, [0, 1, 1, 0]);
        assert_eq!(output.quad_is_sprite, [false, true, true, false]);
        // A pending XOR must never turn a closed-window sample into a
        // playfield colour. Palette entries distinguish the two outcomes.
        denise.palette_24[0] = 0x112233;
        denise.bplcon4 ^= 0x0100;
        assert_eq!(
            denise.resolve_output_sample_argb(0, 0, false, 0),
            0xFF112233
        );
        assert_eq!(
            denise.resolve_output_sample_argb(0, 0, false, 3),
            0xFF112233
        );
    }

    #[test]
    fn lisa_parallel_copy_reaches_output_on_the_following_counter() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x1000); // one lowres bitplane
        denise.begin_beam_line();
        denise.load_bitplane(0, 0x8000);
        denise.queue_shift_load_from_bpl1dat();

        let first = denise.output_pixel_with_beam_and_playfield_gate(0, 0, 0, 0, true);
        let second = denise.output_pixel_with_beam_and_playfield_gate(1, 0, 1, 0, true);
        let third = denise.output_pixel_with_beam_and_playfield_gate(2, 0, 2, 0, true);

        assert_eq!(first.quad_playfield_color_idx[0], 0);
        // UAE composes the held pixel, copies at counter 0 and loads the
        // next held pixel there. Counter 1 emits it; no second lores delay.
        assert_eq!(second.quad_playfield_color_idx, [1; 4]);
        assert_eq!(third.quad_playfield_color_idx, [0; 4]);
    }

    #[test]
    fn lisa_sprite_emits_one_lores_period_after_the_comparator() {
        let mut denise = DeniseAga::new();
        denise.begin_beam_line();
        denise.write_sprite_pos(0, 0x0000); // HSTART=0
        denise.write_sprite_ctl(0, 0x0000);
        denise.write_sprite_datb(0, 0x0000);
        denise.write_sprite_data(0, 0x8000);
        denise.queue_shift_load_from_bpl1dat();

        let at_hstart = denise.output_pixel_with_beam_sprite_coords(0, 0, 0, 0, 0, 0, true);
        let following = denise.output_pixel_with_beam_sprite_coords(1, 0, 1, 0, 1, 0, true);

        let second = denise.output_pixel_with_beam_sprite_coords(2, 0, 2, 0, 2, 0, true);
        assert!(!at_hstart.quad_is_sprite[0]);
        assert!(following.quad_is_sprite[0]);
        assert!(!second.quad_is_sprite[0]);
    }

    #[test]
    fn lisa_reset_comparison_retains_all_eight_fine_edge_positions() {
        for fine in 0..8 {
            for stop in [false, true] {
                let mut denise = DeniseAga::new();
                denise.write_word(0x106, 1);
                let edge = 1 | (fine << 8);
                let (start, end) = if stop { (0x80, edge) } else { (edge, 0xA0) };
                settle_programmed_hblank_inputs(&mut denise, 1, start, end);
                if stop {
                    assert_eq!(
                        denise.programmed_hblank_for_next_counter(256, 1, start, end),
                        HorizontalBlanking::from_level(true),
                    );
                }
                // Reset selects next=2 during the preceding counter's output;
                // next=3 is the first output after the reset commits.
                for (half, next) in [2, 3].into_iter().enumerate() {
                    let expected = std::array::from_fn(|sample| {
                        ((half * 4 + sample) >= usize::from(fine)) != stop
                    });
                    assert_eq!(
                        denise.programmed_hblank_for_next_counter(next, 1, start, end),
                        HorizontalBlanking::from_superhires_samples(expected),
                        "fine={fine}, stop={stop}, next={next}",
                    );
                }
            }
        }
    }

    #[test]
    fn lisa_fixed_hblank_compares_the_next_counter() {
        let mut denise = DeniseAga::new();
        for (hpos, phase, blank) in [
            (7, 0, false),
            (7, 1, true),
            (8, 0, true),
            (45, 1, true),
            (46, 0, false),
            (46, 1, false),
            (47, 0, false),
        ] {
            assert_eq!(
                denise.programmed_hblank_for_output_phase(hpos, phase, 0, 0, 0),
                HorizontalBlanking::from_level(blank),
                "fixed blanking at CCK {hpos}, phase {phase}",
            );
        }
        denise.set_bplcon0(1);
        denise.write_word(0x0106, 1);
        // Both propagated selectors must be enabled to replace the fixed comparator.
        settle_programmed_hblank_inputs(&mut denise, 0, 0, 0);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(46, 0, 0, 0, 0),
            HorizontalBlanking::from_level(false)
        );
        settle_programmed_hblank_inputs(&mut denise, 1, 0, 0);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(46, 0, 1, 0, 0),
            HorizontalBlanking::disabled()
        );
    }

    #[test]
    fn lisa_vertical_blank_waits_for_selected_horizontal_edges() {
        use common_commodore_amiga::denise_counter::DeniseStrobe;
        for programmed in [false, true] {
            let mut denise = DeniseAga::new();
            denise.write_word(0x106, u16::from(programmed));
            settle_programmed_hblank_inputs(&mut denise, 1, 0x80, 0xA0);
            denise.retire_timing_strobe(DeniseStrobe::Horizontal);
            // Starting and ending VBL are distinct events. Repeated vertical
            // strobes (including STREQU) must not erase the pending change.
            denise.retire_timing_strobe(DeniseStrobe::VerticalBlank);
            denise.retire_timing_strobe(DeniseStrobe::Equalisation);
            let (start, stop) = if programmed { (255, 319) } else { (15, 92) };
            let output = |d: &mut DeniseAga, counter: u16| {
                d.programmed_hblank_for_output_phase(
                    counter / 2,
                    (counter & 1) as u8,
                    1,
                    0x80,
                    0xA0,
                )
            };
            assert_eq!(
                output(&mut denise, start - 1),
                HorizontalBlanking::disabled()
            );
            output(&mut denise, start);
            // Horizontal stop alone cannot release vertical blank.
            assert_eq!(
                output(&mut denise, stop),
                HorizontalBlanking::from_level(true)
            );
            denise.retire_timing_strobe(DeniseStrobe::Horizontal);
            denise.retire_timing_strobe(DeniseStrobe::Horizontal);
            assert_eq!(
                output(&mut denise, 100),
                HorizontalBlanking::from_level(true)
            );
            assert_eq!(output(&mut denise, stop), HorizontalBlanking::disabled());
        }
    }

    #[test]
    fn lisa_strobe_uses_the_existing_normal_stage_once_per_cck() {
        use common_commodore_amiga::denise::Denise;
        use common_commodore_amiga::denise_counter::DeniseStrobe;
        let mut board = Denise::<DeniseAga>::new();
        board.service_timing_strobe(DeniseStrobe::Horizontal);
        for _ in 0..2 {
            board.prepare_output_timing_strobe();
            assert!(!board.ocs.vertical_blanking.previous_horizontal_strobe);
            board.begin_counter_output_tick();
            board.end_counter_output_tick();
        }
        board.prepare_output_timing_strobe();
        assert!(board.ocs.vertical_blanking.previous_horizontal_strobe);
        assert_eq!(board.ocs.vertical_blanking.pending_programmed, Some(false));
        board.begin_counter_output_tick();
        board.end_counter_output_tick();
        // A stop comparator in the first half consumes the event. Delivering
        // the strobe again in the second half must not resurrect it.
        board.ocs.vertical_blanking.edge(true, false);
        board.prepare_output_timing_strobe();
        assert_eq!(board.ocs.vertical_blanking.pending_programmed, None);
    }

    #[test]
    fn lisa_vertical_blank_release_retains_all_eight_fine_stop_phases() {
        use common_commodore_amiga::denise_counter::DeniseStrobe;
        for fine in 0..8 {
            let mut denise = DeniseAga::new();
            let stop = 0xA0 | fine << 8;
            denise.write_word(0x106, 1);
            settle_programmed_hblank_inputs(&mut denise, 1, 0x80, stop);
            denise.retire_timing_strobe(DeniseStrobe::Horizontal);
            denise.retire_timing_strobe(DeniseStrobe::VerticalBlank);
            let _ = denise.programmed_hblank_for_output_phase(127, 1, 1, 0x80, stop);
            denise.retire_timing_strobe(DeniseStrobe::Horizontal);
            // Reference stop $A0.fine compares with the next lores counter.
            // The interval below straddles every possible quarter-pixel phase.
            for sample in (159 * 8)..(162 * 8) {
                if sample % 4 != 0 {
                    continue;
                }
                let output = denise.programmed_hblank_for_output_phase(
                    sample / 8,
                    ((sample / 4) & 1) as u8,
                    1,
                    0x80,
                    stop,
                );
                let expected =
                    std::array::from_fn(|sub| sample + (sub as u16) < 160 * 8 + fine - 4);
                assert_eq!(
                    output,
                    HorizontalBlanking::from_superhires_samples(expected),
                    "fine={fine} sample={sample}"
                );
            }
        }
    }

    #[test]
    fn lisa_fixed_and_programmed_vertical_events_survive_selector_changes() {
        use common_commodore_amiga::denise_counter::DeniseStrobe;
        let mut denise = DeniseAga::new();
        settle_programmed_hblank_inputs(&mut denise, 1, 0x80, 0xA0);
        denise.retire_timing_strobe(DeniseStrobe::Horizontal);
        denise.retire_timing_strobe(DeniseStrobe::VerticalBlank);
        let _ = denise.programmed_hblank_for_output_phase(7, 1, 1, 0x80, 0xA0);
        assert_eq!(denise.vertical_blanking.pending_fixed, None);
        assert_eq!(denise.vertical_blanking.pending_programmed, Some(true));
        denise.write_word(0x106, 1);
        settle_programmed_hblank_inputs(&mut denise, 1, 0x80, 0xA0);
        let _ = denise.programmed_hblank_for_output_phase(127, 1, 1, 0x80, 0xA0);
        assert!(denise.vertical_blanking.programmed_active);
        denise.retire_timing_strobe(DeniseStrobe::Horizontal);
        let _ = denise.programmed_hblank_for_output_phase(159, 1, 1, 0x80, 0xA0);
        assert!(!denise.vertical_blanking.programmed_active);
        assert!(denise.vertical_blanking.fixed_active);
        assert_eq!(denise.vertical_blanking.pending_fixed, Some(false));
        denise.write_word(0x106, 0);
        settle_programmed_hblank_inputs(&mut denise, 1, 0x80, 0xA0);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(100, 0, 1, 0x80, 0xA0),
            HorizontalBlanking::from_level(true)
        );
        assert_eq!(
            denise.programmed_hblank_for_output_phase(46, 0, 1, 0x80, 0xA0),
            HorizontalBlanking::disabled()
        );
    }

    #[test]
    fn lisa_equal_and_wrapping_edges_consume_vertical_events() {
        use common_commodore_amiga::denise_counter::DeniseStrobe;
        for (start, stop) in [(0xA0, 0xA0), (0xFF, 0)] {
            let mut denise = DeniseAga::new();
            denise.write_word(0x106, 1);
            settle_programmed_hblank_inputs(&mut denise, 1, start, stop);
            denise.retire_timing_strobe(DeniseStrobe::Horizontal);
            denise.retire_timing_strobe(DeniseStrobe::VerticalBlank);
            let _ = denise.programmed_hblank_for_output_phase(start - 1, 1, 1, start, stop);
            denise.retire_timing_strobe(DeniseStrobe::Horizontal);
            // A repeated strobe is not a new event. The retained stop must
            // survive a 9-bit counter wrap, including HBSTOP zero.
            let stop_counter = (stop * 2).wrapping_sub(1) & 511;
            let output = denise.programmed_hblank_for_output_phase(
                stop_counter / 2,
                (stop_counter & 1) as u8,
                1,
                start,
                stop,
            );
            assert_eq!(output, HorizontalBlanking::disabled());
            assert!(!denise.vertical_blanking.programmed_active);
        }
    }

    #[test]
    fn diagnostic_snapshot_reports_complete_lisa_state_without_using_core_mirrors() {
        let mut denise = DeniseAga::new();

        for (index, color) in denise.palette_24.iter_mut().enumerate() {
            *color = ((index as u32) * 0x0001_0203) & 0x00FF_FFFF;
        }
        denise.palette_genlock[172] = true;
        denise.write_word(0x0106, 0xA000); // BANK=5, LOCT=0
        denise.write_word(0x019A, 0x0A5C); // COLOR13 in bank 5 -> slot 173
        denise.write_word(0x010C, 0x5A3C);
        denise.write_word(0x01FC, 0x000C);
        denise.set_bplcon0(0x0810); // HAM + BPU=8
        denise.palette_24[1] = 0x0012_3457;
        assert_eq!(denise.resolve_color_argb(0x04), 0xFF12_3457);

        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0040, 0x0080);
        let _ = denise.programmed_hblank_for_output_phase(0x003F, 1, 0x0001, 0x0040, 0x0080);

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
        assert!(first.palette_genlock[172]);
        assert!(!first.palette_genlock[173]);
        assert_eq!(first.ham_prev_rgb24, 0x0012_3457);
        assert_eq!(first.spr_width, 64);
        assert!(first.programmed_hblank_active);
        assert!(denise.programmed_hblank_active());
    }

    #[test]
    fn lisa_programmed_hblank_matches_counter_traced_edges() {
        // SPHX blanking guests, reference counter 100 -> raw sample 36.
        // HBSTRT=$80 begins at counter 255 (raw x=656), not 256.
        // HBSTOP=$07A0 ends at counter 320.75 (raw x=919).
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(1);
        denise.write_word(0x0106, 1);
        settle_programmed_hblank_inputs(&mut denise, 1, 0x80, 0x07A0);
        for hpos in 0x70..=0xA1 {
            for phase in 0..2 {
                let blank = denise.programmed_hblank_for_output_phase(hpos, phase, 1, 0x80, 0x07A0);
                let expected = match (hpos, phase) {
                    (0x70..=0x7E, _) | (0x7F, 0) | (0xA0, 1) | (0xA1, _) => [false; 4],
                    (0xA0, 0) => [true, true, true, false],
                    _ => [true; 4],
                };
                assert_eq!(
                    blank,
                    HorizontalBlanking::from_superhires_samples(expected),
                    "counter CCK {hpos:#x} half {phase}"
                );
            }
        }
    }

    #[test]
    fn lisa_hblank_fine_stop_retains_all_four_output_samples() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x0001); // ECSENA
        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0030, 0x0740);

        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x002F, 1, 0x0001, 0x0030, 0x0740,),
            HorizontalBlanking::from_output_samples([true, true]),
        );
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0040, 0, 0x0001, 0x0030, 0x0740,),
            HorizontalBlanking::from_superhires_samples([true, true, true, false]),
        );
        assert!(!denise.programmed_hblank_active());
    }

    #[test]
    fn lisa_hbstrt_write_behind_beam_does_not_synthesize_start() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0070, 0x00C0);

        let _ = denise.programmed_hblank_for_output_phase(0x0060, 0, 0x0001, 0x0070, 0x00C0);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0061, 0, 0x0001, 0x0050, 0x00C0,),
            HorizontalBlanking::disabled(),
        );
        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0050, 0x00C0);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x004F, 1, 0x0001, 0x0050, 0x00C0,),
            HorizontalBlanking::from_level(true),
            "the rewritten edge takes effect when the following line reaches it",
        );
    }

    #[test]
    fn lisa_hbstop_write_ahead_after_stop_does_not_reassert() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0060, 0x0070);
        let _ = denise.programmed_hblank_for_output_phase(0x005F, 1, 0x0001, 0x0060, 0x0070);
        let _ = denise.programmed_hblank_for_output_phase(0x006F, 1, 0x0001, 0x0060, 0x0070);

        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0071, 0, 0x0001, 0x0060, 0x00B0,),
            HorizontalBlanking::disabled(),
        );
        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0060, 0x00B0);
        let _ = denise.programmed_hblank_for_output_phase(0x005F, 1, 0x0001, 0x0060, 0x00B0);
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
        settle_programmed_hblank_inputs(&mut denise, 0x0000, 0x0040, 0x0080);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x003F, 1, 0x0000, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
        );
        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0040, 0x0080);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0050, 0, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
        );
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x003F, 1, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::from_level(true),
        );
    }

    #[test]
    fn lisa_extblken_enable_after_start_waits_for_the_next_start() {
        let mut denise = DeniseAga::new();
        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0040, 0x0080);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x003F, 1, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
        );
        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0050, 0, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
        );
        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0040, 0x0080);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x003F, 1, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::from_level(true),
        );
    }

    #[test]
    fn lisa_equal_hblank_edges_leave_level_clear() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0040, 0x0040);

        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x003F, 1, 0x0001, 0x0040, 0x0040,),
            HorizontalBlanking::disabled(),
        );
        assert!(!denise.programmed_hblank_active());
    }

    #[test]
    fn disabling_a_lisa_selector_clears_the_active_level() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0x0001); // EXTBLKEN
        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0040, 0x0080);
        let _ = denise.programmed_hblank_for_output_phase(0x003F, 1, 0x0001, 0x0040, 0x0080);
        assert!(denise.programmed_hblank_active());

        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0050, 0, 0x0000, 0x0040, 0x0080,),
            HorizontalBlanking::from_level(true),
            "the raw selector write has not yet reached Lisa's output stage",
        );
        for _ in 0..3 {
            denise.advance_register_output_pipeline();
        }
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0060, 0, 0x0000, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
        );
        assert!(!denise.programmed_hblank_active());

        settle_programmed_hblank_inputs(&mut denise, 0x0001, 0x0040, 0x0080);
        assert_eq!(
            denise.programmed_hblank_for_output_phase(0x0061, 0, 0x0001, 0x0040, 0x0080,),
            HorizontalBlanking::disabled(),
            "re-enabling after HBSTRT must not synthesize an active level",
        );
        assert!(!denise.programmed_hblank_active());
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
    fn color_write_reaches_aga_output_one_hires_pixel_late() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x1000); // one plane, normal indexed mode

        denise.write_word(0x0180, 0x0123);
        denise.advance_color_output_samples(1);
        denise.write_word(0x0180, 0x0ABC);

        // Register and inspection state changes immediately.
        assert_eq!(denise.palette_24[0], 0x00AA_BBCC);
        assert_eq!(denise.palette()[0], 0x0ABC);
        let pending = denise
            .diagnostic_snapshot()
            .delayed_color_write
            .expect("COLOR00 write must be pending at the output");
        assert_eq!(pending.palette_index, 0);
        assert_eq!(pending.previous_rgb24, 0x0011_2233);
        assert_eq!(pending.previous_rgb12, Some(0x0123));
        assert!(!pending.previous_genlock);

        // Lisa retains the previous colour for exactly one hires sample.
        assert_eq!(denise.resolve_color_argb(0), 0xFF11_2233);
        assert!(denise.diagnostic_snapshot().delayed_color_write.is_none());
        assert_eq!(denise.resolve_color_argb(0), 0xFFAA_BBCC);
    }

    #[test]
    fn copper_color_write_retains_only_the_lisa_hires_output_delay() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0180, 0x0123);
        denise.advance_color_output_samples(1);
        assert!(denise.write_color_before_output_tick(0x0180, 0x0ABC));
        // Counter-traced adjacent MOVEs change at 292.5, 300.5, 308.5,
        // 316.5: the early write retains two 35 ns samples, not six.
        let output: Vec<_> = (0..4)
            .map(|sample| denise.resolve_output_sample_argb(0, 0, false, sample))
            .collect();
        assert_eq!(output, [0xFF11_2233, 0xFF11_2233, 0xFFAA_BBCC, 0xFFAA_BBCC]);
    }

    #[test]
    fn aga_color_delay_expires_on_a_different_palette_index() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x1000);
        denise.palette_24[1] = 0x0044_5566;

        denise.write_word(0x0180, 0x0ABC);
        assert_eq!(denise.resolve_color_argb(1), 0xFF44_5566);
        assert_eq!(denise.resolve_color_argb(0), 0xFFAA_BBCC);
    }

    #[test]
    fn aga_color_delay_expires_outside_the_recorded_viewport() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x1000);

        denise.write_word(0x0180, 0x0ABC);
        denise.advance_color_output_samples(1);

        assert!(denise.diagnostic_snapshot().delayed_color_write.is_none());
        assert_eq!(denise.resolve_color_argb(0), 0xFFAA_BBCC);
    }

    #[test]
    fn rdram_protected_color_write_changes_neither_palette_nor_output_delay() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x1000);
        denise.write_word(0x0180, 0x0123);
        denise.advance_color_output_samples(1);
        denise.write_word(0x0104, 0x0100); // BPLCON2 RDRAM

        denise.write_word(0x0180, 0x0ABC);

        assert_eq!(denise.palette_24[0], 0x0011_2233);
        assert_eq!(denise.palette()[0], 0x0123);
        assert!(denise.diagnostic_snapshot().delayed_color_write.is_none());
        assert_eq!(denise.resolve_color_argb(0), 0xFF11_2233);
    }

    #[test]
    fn rdram_reads_the_selected_bank_and_loct_half() {
        let mut denise = DeniseAga::new();
        denise.write_word(0x0106, 0xA000); // BANK=5, high nibbles
        denise.write_word(0x018A, 0x8A5C); // COLOR05 -> palette $A5, T=1
        denise.write_word(0x0106, 0xA200); // BANK=5, LOCT
        denise.write_word(0x018A, 0x0123);

        assert_eq!(denise.read_color_register(0x018A), 0xFFFF);
        denise.write_word(0x0104, BPLCON2_RDRAM);

        denise.write_word(0x0106, 0xA000);
        assert_eq!(denise.read_color_register(0x018A), 0x8A5C);
        denise.write_word(0x0106, 0xA200);
        assert_eq!(denise.read_color_register(0x018A), 0x0123);

        denise.write_word(0x018A, 0x0FED);
        denise.write_word(0x0106, 0xA000);
        assert_eq!(denise.read_color_register(0x018A), 0x8A5C);

        denise.write_word(0x0106, 0x8000); // BANK=4
        assert_eq!(denise.read_color_register(0x018A), 0x0000);
        assert_eq!(denise.read_color_register(0x018B), 0xFFFF);
    }

    #[test]
    fn aga_ehb_uses_delayed_full_24bit_palette_with_loct_precision() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x6000); // six planes, EHB

        denise.write_word(0x0182, 0x0123);
        denise.advance_color_output_samples(1);
        denise.write_word(0x0106, 0x0200); // BPLCON3 LOCT
        denise.write_word(0x0182, 0x0456);

        // Index 33 is half-brite COLOR01. LOCT must not act as ECS KILLEHB,
        // and the first sample must halve the complete previous RGB8 value.
        assert_eq!(denise.resolve_color_argb(0x21), 0xFF08_1119);
        assert_eq!(denise.resolve_color_argb(0x21), 0xFF0A_121B);
    }

    #[test]
    fn aga_killehb_uses_the_full_palette_address() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x6000); // six planes, EHB candidate
        denise.write_word(0x0182, 0x0123);
        denise.advance_color_output_samples(1);
        denise.write_word(0x0104, 0x0200); // BPLCON2 KILLEHB

        denise.write_word(0x0106, 0x2000); // BANK=1: COLOR33
        denise.write_word(0x0182, 0x0ABC);
        denise.advance_color_output_samples(1);
        assert_eq!(denise.resolve_color_argb(0x01), 0xFF11_2233);
        assert_eq!(denise.resolve_color_argb(0x21), 0xFFAA_BBCC);
    }

    #[test]
    fn killehb_retains_all_eight_post_xor_address_bits_and_color_delay() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x6000);
        denise.write_word(0x0104, 0x0200);
        // Distinct RGB8 entries catch folding into either 32 or 64 colours.
        for index in 0..256usize {
            denise.palette_24[index] = (index as u32) * 0x010101;
        }
        for index in 0..=255u8 {
            assert_eq!(
                denise.resolve_color_argb(index),
                0xFF00_0000 | (u32::from(index) * 0x010101),
                "post-BPLAM palette address {index:02x}"
            );
        }
        denise.write_word(0x0106, 0xA000); // BANK=5
        denise.write_word(0x0182, 0x0123); // COLOR161
        assert_eq!(denise.resolve_color_argb(0xA1), 0xFFA1_A1A1);
        assert_eq!(denise.resolve_color_argb(0xA1), 0xFF11_2233);
        denise.write_word(0x0104, 0); // EHB re-enabled: colour33 halves COLOR01
        assert_eq!(denise.resolve_color_argb(0x21), 0xFF00_0000);
    }

    #[test]
    fn six_plane_killehb_output_uses_serial_pixels_and_bplam() {
        for xor in [0u8, 0x40, 0x80, 0xC0] {
            let mut denise = DeniseAga::new();
            denise.set_bplcon0(0x6000);
            denise.write_word(0x0104, 0x0200);
            denise.write_word(0x010C, (u16::from(xor) << 8) | 0x0011);
            let address = 0x21 ^ xor;
            denise.palette_24[usize::from(address)] = 0x123456;
            denise.begin_beam_line();
            for plane in 0..6 {
                denise.load_bitplane(plane, if 0x21 & (1 << plane) != 0 { 0xFFFF } else { 0 });
            }
            denise.as_inner_mut().as_inner_mut().trigger_shift_load();
            for x in 0..8 {
                let sample = denise.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
                let rgb = denise.resolve_output_color_argb(
                    sample.quad_playfield_color_idx[0],
                    sample.quad_color_idx[0],
                    sample.quad_is_sprite[0],
                );
                if x >= 4 {
                    assert_eq!(sample.plane_bits_mask, 0x21);
                    assert_eq!(sample.quad_playfield_color_idx[0], address);
                    assert!(!sample.quad_is_sprite[0]);
                    assert_eq!(rgb, 0xFF12_3456, "BPLAM={xor:02x}, pixel={x}");
                }
            }
        }
    }

    #[test]
    fn consecutive_aga_color_writes_flush_the_earlier_delay() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x1000);

        denise.write_word(0x0180, 0x0123);
        denise.advance_color_output_samples(1);
        denise.write_word(0x0180, 0x0456);
        denise.write_word(0x0180, 0x0789);

        // The second write makes $445566 live before queueing its own delay.
        assert_eq!(denise.resolve_color_argb(0), 0xFF44_5566);
        assert_eq!(denise.resolve_color_argb(0), 0xFF77_8899);
    }

    #[test]
    fn ham8_direct_color_observes_the_aga_output_delay() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x0800 | 0x0010); // HAM + BPU=8

        denise.write_word(0x0182, 0x0123);
        denise.advance_color_output_samples(1);
        denise.write_word(0x0182, 0x0ABC);

        assert_eq!(denise.resolve_color_argb(0x04), 0xFF11_2233);
        assert_eq!(denise.resolve_color_argb(0x04), 0xFFAA_BBCC);
    }

    #[test]
    fn ham6_uses_delayed_24bit_palette_and_eight_bit_hold_channels() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x6000 | 0x0800); // BPU=6 + HAM
        denise.write_word(0x0182, 0x0123);
        denise.advance_color_output_samples(1);
        denise.write_word(0x0106, 0x0200); // BPLCON3 LOCT
        denise.write_word(0x0182, 0x0456);

        assert_eq!(denise.resolve_color_argb(0x01), 0xFF11_2233);
        assert_eq!(denise.resolve_color_argb(0x01), 0xFF14_2536);
        assert_eq!(denise.resolve_color_argb(0x2A), 0xFFAA_2536); // red = $AA
        assert_eq!(denise.resolve_color_argb(0x1C), 0xFFAA_25CC); // blue = $CC
        assert_eq!(denise.resolve_color_argb(0x3D), 0xFFAA_DDCC); // green = $DD
    }

    #[test]
    fn ham6_sprite_overlays_the_advancing_playfield_hold() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x6800); // six planes, HAM
        denise.palette_24[0] = 0x0011_2233;
        denise.palette_24[0x21] = 0x00DE_ADBE;
        denise.begin_beam_line();

        assert_eq!(
            denise.resolve_output_color_argb(0x2A, 0x21, true),
            0xFFDE_ADBE,
        );
        assert_eq!(denise.ham_prev_rgb24, 0x00AA_2233);
    }

    #[test]
    fn ham8_sprite_overlays_the_advancing_playfield_hold() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x0810); // eight planes, HAM
        denise.palette_24[0] = 0x0012_3456;
        denise.palette_24[0xA5] = 0x0065_43AB;
        denise.begin_beam_line();

        assert_eq!(
            denise.resolve_output_color_argb(0xAA, 0xA5, true),
            0xFF65_43AB,
        );
        assert_eq!(denise.ham_prev_rgb24, 0x00AA_3456);
    }

    #[test]
    fn ehb_sprite_bank_bypasses_half_brite_and_observes_color_delay() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x6000); // six-plane EHB playfield
        denise.write_word(0x0106, 0xA000); // BANK=5, LOCT=0
        denise.write_word(0x018A, 0x0123); // palette $A5
        denise.advance_color_output_samples(1);
        denise.palette_24[5] = 0x00FE_DCBA;
        denise.write_word(0x018A, 0x0ABC);

        assert_eq!(
            denise.resolve_output_color_argb(0x25, 0xA5, true),
            0xFF11_2233,
            "the first sprite sample sees the delayed direct banked colour",
        );
        assert_eq!(
            denise.resolve_output_color_argb(0x25, 0xA5, true),
            0xFFAA_BBCC,
            "subsequent sprite output sees the new direct colour",
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
        // BPLCON1 ($102) — standard scroll register goes through to OCS.
        // BPLCON0 uses Lisa's separately timed normal stage.
        denise.write_word(0x0102, 0x0034);
        assert_eq!(denise.as_inner().as_inner().bplcon1, 0x0034);
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
    #[test]
    fn full_sample_clock_keeps_one_hires_period_color_delay() {
        let mut denise = DeniseAga::new();
        denise.palette_24[1] = 0x00123456;
        denise.handle_color_write(0x182, 0x0F00);
        let samples: Vec<_> = (0..4)
            .map(|sample| denise.resolve_output_sample_argb(1, 1, false, sample))
            .collect();
        assert_eq!(samples, [0xFF123456, 0xFF123456, 0xFFFF0000, 0xFFFF0000]);
        assert!(denise.diagnostic_snapshot().delayed_color_write.is_none());
    }

    #[test]
    fn superhires_ham8_decodes_each_35ns_playfield_sample() {
        let mut denise = DeniseAga::new();
        denise.set_bplcon0(0x0850);
        denise.palette_24[1] = 0x00FF8043;
        let codes = [0x04, (0x2A << 2) | 1, (0x15 << 2) | 2, (0x10 << 2) | 3];
        let samples: Vec<_> = codes
            .into_iter()
            .enumerate()
            .map(|(sample, code)| {
                denise.resolve_output_sample_argb(code, code, false, sample as u8)
            })
            .collect();
        assert_eq!(samples, [0xFFFF8043, 0xFFFF80AB, 0xFF5780AB, 0xFF5740AB]);
    }
}
