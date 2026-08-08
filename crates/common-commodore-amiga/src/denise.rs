//! Denise — board-level wrapper, generic over the Denise chip variant.
//!
//! Register + pixel-pipeline state lives in the concrete chip
//! (`commodore_denise_ocs::DeniseOcs` for OCS, `commodore_denise_ecs::DeniseEcs`
//! for ECS, future variants for AGA Lisa / CD32). This module is the
//! narrow wiring layer that the machine uses to:
//!
//!   - dispatch custom-register writes into the chip's `write_word`
//!     (BPLCON0/1/2, CLXCON, BPL*DAT, SPR*, COLOR00..COLOR31),
//!   - drive the per-CCK bitplane DMA fetch + shift-load cycle that
//!     the chip leaves to the caller,
//!   - copy the chip's per-pixel output into the board's own ARGB
//!     framebuffer for display.
//!
//! DDF / legacy OCS DIW decoding helpers (`ddf_window`,
//! `diw_vertical_window`) live here because Agnus owns those registers
//! on real silicon. The live vertical gate is supplied by the concrete
//! Agnus/Alice variant so ECS/AGA `DIWHIGH` and comparator-latch state
//! cannot be lost through an OCS base view. The per-CCK DMA slot schedule
//! itself lives in Agnus's `current_slot` / `cck_bus_plan` (#30).
//!
//! HIRES / HAM / EHB / DPF / sprites / collisions all flow through
//! the chip's `output_pixel_with_beam_and_playfield_gate` unchanged.

use crate::denise_chip::{DeniseChip, HorizontalDiwComparatorPhase};
use crate::memory::Memory;

/// Display dimensions for PAL Standard (line-doubled, lores → 4:3).
pub const FB_WIDTH: u32 = 768;
pub const FB_HEIGHT: u32 = 576;

/// One Agnus-granted bitplane transfer for the current CCK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitplaneDmaFetch {
    pub plane: u8,
    pub width_words: u8,
}

/// External horizontal-blank levels for the two output samples emitted by one
/// master/4 tick.
///
/// The machine layer advances the chipset-specific comparator latches and
/// supplies their resulting levels. The renderer therefore has no register,
/// comparator, or selector policy of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HorizontalBlanking {
    output_samples: [bool; 2],
}

impl HorizontalBlanking {
    /// Disable external programmable horizontal blanking.
    #[must_use]
    pub const fn disabled() -> Self {
        Self::from_level(false)
    }

    /// Apply one horizontal-blank level to both samples in this output tick.
    #[must_use]
    pub const fn from_level(active: bool) -> Self {
        Self {
            output_samples: [active; 2],
        }
    }

    /// Apply independently composed levels to the two output samples in this
    /// tick. Lisa uses this form because one fine comparator can split the
    /// pair.
    #[must_use]
    pub const fn from_output_samples(output_samples: [bool; 2]) -> Self {
        Self { output_samples }
    }

    fn contains_output_sample(self, subpixel: u8) -> bool {
        debug_assert!(subpixel < 2);
        self.output_samples[usize::from(subpixel)]
    }
}

/// Display signals composed by the concrete machine around the shared Denise
/// renderer for one output tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeniseOutputSignals {
    vertical_diw_active: bool,
    horizontal_blanking: HorizontalBlanking,
}

impl DeniseOutputSignals {
    /// Compose enhanced display signals at the machine boundary.
    #[must_use]
    pub const fn new(vertical_diw_active: bool, horizontal_blanking: HorizontalBlanking) -> Self {
        Self {
            vertical_diw_active,
            horizontal_blanking,
        }
    }

    /// Compose the OCS-compatible output path with no programmable horizontal
    /// blanking.
    #[must_use]
    pub const fn unblanked(vertical_diw_active: bool) -> Self {
        Self::new(vertical_diw_active, HorizontalBlanking::disabled())
    }
}

/// Display context retained while the horizontal counter has wrapped but
/// Denise is still completing the preceding raster row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PriorLineRasterContext {
    vpos: u16,
    line_ccks: u16,
    /// Raw Agnus field count while this physical line was current.
    vbl_count: u64,
    ddf_start: Option<u16>,
    pipeline_y: u32,
    vertical_diw_active: bool,
    /// `None` writes both line-doubled rows. Interlaced fields retain the
    /// selected row offset (`0` for LOF, `1` for the alternate field).
    interlace_row: Option<u8>,
}

/// Side-effect-free view of the preceding physical line retained while Denise
/// completes its post-wrap raster tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DenisePriorLineRasterDiagnosticSnapshot {
    pub vpos: u16,
    pub line_ccks: u16,
    pub vbl_count: u64,
    pub ddf_start: Option<u16>,
    pub pipeline_y: u32,
    pub vertical_diw_active: bool,
    pub interlace_row: Option<u8>,
}

/// Complete bounded view of mutable line state owned by the board-level
/// Denise wrapper.
///
/// The concrete chip pipeline is exposed by its own diagnostic snapshot. The
/// framebuffer remains available through [`Denise::framebuffer`] and is not
/// copied into this snapshot.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeniseBoardPipelineDiagnosticSnapshot {
    pub bytes_this_line: u32,
    pub last_begin_line: Option<u16>,
    pub prior_line_raster: Option<DenisePriorLineRasterDiagnosticSnapshot>,
    /// Early-stage COLOR writes waiting for the current output tick to retire.
    pub pending_early_writes: Vec<DenisePendingRegisterWrite>,
}

/// One Denise/Lisa register write retained until the early output stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DenisePendingRegisterWrite {
    /// Custom-register offset in the `$DFFxxx` window.
    pub register: u16,
    /// Word presented to the display chip.
    pub value: u16,
}

/// Decode a DIWSTRT/DIWSTOP register pair into the effective vertical
/// display window `[vstart, vstop)` in absolute line numbers.
///
/// Per HRM 3rd ed., Table 3-9 and the surrounding "Setting the
/// Display Window" prose:
///
/// - VSTART is the low 8 bits of DIWSTRT's high byte; bit 8 is
///   implicitly 0 (VSTART is in the upper half of the frame).
/// - VSTOP is the low 8 bits of DIWSTOP's high byte; bit 8 is
///   **the complement of bit 7** ("forcing the MSB of the stop
///   position to be the complement of the next MSB"). So a byte
///   value >=$80 means VSTOP is in the upper half (bit 8 = 0),
///   and <$80 means VSTOP is in the lower half (bit 8 = 1,
///   i.e. add $100).
#[must_use]
pub fn diw_vertical_window(diwstrt: u16, diwstop: u16) -> (u16, u16) {
    let vstart = (diwstrt >> 8) & 0xFF;
    let vstop_byte = (diwstop >> 8) & 0xFF;
    let vstop = if vstop_byte & 0x80 != 0 {
        vstop_byte
    } else {
        vstop_byte | 0x100
    };
    (vstart, vstop)
}

/// Decode a DDFSTRT/DDFSTOP register pair into a CCK-aligned fetch
/// window `[ddf_start, ddf_stop)`. The low 2 bits of each register
/// are forced to zero per HRM — fetch block boundaries must align to
/// 4 CCKs for lores. This helper only decodes register values; runtime
/// comparator history and fixed hardware limits belong to Agnus.
#[must_use]
pub fn ddf_window(ddfstrt: u16, ddfstop: u16) -> (u16, u16) {
    (ddfstrt & 0x00FC, ddfstop & 0x00FC)
}

/// Return whether the horizontal display-window gate is active for one
/// lores output tick.
///
/// OCS/ECS comparator matches control the current tick (`[HSTART, HSTOP)`).
/// Lisa's additional output stage applies each match after the current tick
/// (`(HSTART, HSTOP]`). This is the steady-state transfer relation for stable
/// window registers; it does not claim a history-sensitive latch model for
/// mid-line DIWSTRT or DIWSTOP rewrites.
#[inline]
fn horizontal_diw_active(
    beam_x_lores: u32,
    diwstrt: u16,
    diwstop: u16,
    comparator_phase: HorizontalDiwComparatorPhase,
) -> bool {
    let hstart = u32::from(diwstrt & 0x00FF);
    let hstop = 0x0100u32 | u32::from(diwstop & 0x00FF);
    match comparator_phase {
        HorizontalDiwComparatorPhase::BeforeOutput => {
            beam_x_lores >= hstart && beam_x_lores < hstop
        }
        HorizontalDiwComparatorPhase::AfterOutput => beam_x_lores > hstart && beam_x_lores <= hstop,
    }
}

/// Board-level Denise wrapper, generic over the concrete chip
/// variant via [`DeniseChip`]. Each per-chipset machine crate
/// instantiates this with its specific Denise type
/// (`Denise<DeniseOcs>`, `Denise<DeniseEcs>`, future `Denise<DeniseAga>`).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(bound(serialize = "C: serde::Serialize"))]
#[serde(bound(deserialize = "C: serde::de::DeserializeOwned"))]
pub struct Denise<C: DeniseChip> {
    /// The concrete Denise chip — owns BPLCON1/2, COLOR palette,
    /// sprite registers, shift registers, HAM prev-RGB, collision
    /// state.
    pub ocs: C,
    /// ARGB8888 framebuffer (FB_WIDTH × FB_HEIGHT pixels) for the
    /// frontend. We resolve the chip's `final_color_idx` through
    /// its palette for each pixel and fill a 2×2 block here (pixel-
    /// doubling + line-doubling for square-pixel 4:3 output).
    pub framebuffer: Vec<u32>,
    /// Bytes fetched on the current line (used to decide when to
    /// apply the end-of-line modulo).
    bytes_this_line: u32,
    /// vpos of the most recent `begin_beam_line()` call — guards
    /// against multiple resets per line.
    last_begin_line: Option<u16>,
    /// Previous physical line retained through the post-wrap interval before
    /// Denise reaches fixed HBLANK start and begins the new display line.
    prior_line_raster: Option<PriorLineRasterContext>,
    /// COLOR writes cross Denise's early RGA stage after the current output
    /// tick. Lisa then applies its own one-hires-sample palette-output delay.
    pending_early_writes: Vec<DenisePendingRegisterWrite>,
}

impl<C: DeniseChip> Default for Denise<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: DeniseChip> Denise<C> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ocs: C::new(),
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
            bytes_this_line: 0,
            last_begin_line: None,
            prior_line_raster: None,
            pending_early_writes: Vec::new(),
        }
    }

    /// CPU or debugger write to a Denise-owned custom register after the
    /// current output tick. The register is therefore available to the next
    /// output tick immediately; Lisa retains its own one-hires-sample COLOR
    /// delay inside the concrete chip.
    pub fn write_word(&mut self, offset: u16, val: u16) {
        self.ocs.write_word(offset, val);
    }

    /// Copper write to a Denise-owned custom register before the current
    /// output tick. COLOR writes cross Denise's early display-side RGA stage,
    /// so the current output retains the previous colour. Other registers
    /// retain their existing concrete-chip propagation rules.
    pub fn write_word_before_output_tick(&mut self, offset: u16, val: u16) {
        if (0x0180..=0x01BE).contains(&offset) && offset.is_multiple_of(2) {
            if !self.ocs.write_color_with_early_output_delay(offset, val) {
                self.pending_early_writes.push(DenisePendingRegisterWrite {
                    register: offset,
                    value: val,
                });
            }
        } else {
            self.ocs.write_word(offset, val);
        }
    }

    /// DENISEID register read ($DFF07C). Each chip variant returns
    /// its own marker — KS uses the value to discriminate OCS / ECS
    /// / AGA at boot.
    #[must_use]
    pub fn deniseid(&self) -> u16 {
        self.ocs.deniseid()
    }

    /// CLXDAT register read ($DFF00E) — latched sprite/playfield
    /// collision bits, cleared on read. Forwarded to the concrete
    /// chip's collision latch.
    pub fn read_clxdat(&mut self) -> u16 {
        self.ocs.read_clxdat()
    }

    /// Non-destructive CLXDAT read for the debug / inspection bus
    /// (`&self`); does not clear the latch.
    #[must_use]
    pub fn peek_clxdat(&self) -> u16 {
        self.ocs.peek_clxdat()
    }

    /// Framebuffer dimensions (width, height).
    #[must_use]
    pub fn framebuffer_size(&self) -> (u32, u32) {
        (FB_WIDTH, FB_HEIGHT)
    }

    /// Read-only framebuffer access.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Inspect board-owned line and raster-tail state without advancing the
    /// renderer or copying framebuffer pixels.
    #[must_use]
    pub fn board_pipeline_diagnostic_snapshot(&self) -> DeniseBoardPipelineDiagnosticSnapshot {
        DeniseBoardPipelineDiagnosticSnapshot {
            bytes_this_line: self.bytes_this_line,
            last_begin_line: self.last_begin_line,
            prior_line_raster: self.prior_line_raster.map(|prior| {
                DenisePriorLineRasterDiagnosticSnapshot {
                    vpos: prior.vpos,
                    line_ccks: prior.line_ccks,
                    vbl_count: prior.vbl_count,
                    ddf_start: prior.ddf_start,
                    pipeline_y: prior.pipeline_y,
                    vertical_diw_active: prior.vertical_diw_active,
                    interlace_row: prior.interlace_row,
                }
            }),
            pending_early_writes: self.pending_early_writes.clone(),
        }
    }

    /// Number of fields whose final displayed raster row is complete.
    ///
    /// Agnus increments `vbl_count` when its raw counters enter line zero,
    /// before Denise has consumed the post-wrap CCKs that still belong to the
    /// preceding displayed row. Hold that increment back while the retained
    /// context came from an older raw field. Ordinary carries between lines
    /// have the same count as Agnus and therefore do not affect this value.
    #[must_use]
    pub fn completed_display_field_count(&self, vbl_count: u64) -> u64 {
        let previous_field_pending = self
            .prior_line_raster
            .is_some_and(|prior| prior.vbl_count.checked_add(1) == Some(vbl_count));
        vbl_count.saturating_sub(u64::from(previous_field_pending))
    }

    /// Read one COLOR palette entry from the chip.
    #[must_use]
    pub fn color(&self, idx: usize) -> u16 {
        if idx < 32 { self.ocs.palette()[idx] } else { 0 }
    }

    /// Tick one master/4 period (= 1 lores pixel, = half a CCK).
    /// `phase` selects which half of the CCK this tick belongs to:
    ///   - `0`: first lores pixel of the CCK. CCK-boundary events
    ///     (fetch, end-of-line modulo, HBLANK-start reset) fire here.
    ///   - `1`: second lores pixel of the CCK.
    ///
    /// `line_ccks` is the actual length of the current physical line from the
    /// outermost Agnus or Alice variant. Denise needs it to project physical
    /// positions after counter wrap onto the preceding displayed row.
    ///
    /// Every tick advances the chip's shift register by the mode-
    /// appropriate number of source pixels and writes one lores pixel
    /// (pixel-doubled) to the framebuffer when the beam is inside the
    /// display window.
    pub fn tick(
        &mut self,
        phase: u8,
        bitplane_dma_fetch: Option<BitplaneDmaFetch>,
        vertical_diw_active: bool,
        agnus: &mut commodore_agnus_ocs::Agnus,
        memory: &Memory,
        line_ccks: u16,
    ) {
        self.tick_with_output_signals(
            phase,
            bitplane_dma_fetch,
            DeniseOutputSignals::unblanked(vertical_diw_active),
            agnus,
            memory,
            line_ccks,
        );
    }

    /// Tick one master/4 period while applying a machine-composed external
    /// programmable-horizontal-blank signal to final video output.
    ///
    /// Pixel, HAM, sprite, and collision state still advances while blanking
    /// is active. Blanking replaces only the final external sample with
    /// opaque black.
    pub fn tick_with_output_signals(
        &mut self,
        phase: u8,
        bitplane_dma_fetch: Option<BitplaneDmaFetch>,
        output_signals: DeniseOutputSignals,
        agnus: &mut commodore_agnus_ocs::Agnus,
        memory: &Memory,
        line_ccks: u16,
    ) {
        let DeniseOutputSignals {
            vertical_diw_active,
            horizontal_blanking,
        } = output_signals;
        let vpos = agnus.vpos;
        let hpos = agnus.hpos;
        let dmacon = agnus.dmacon;
        let (vstart, _) = diw_vertical_window(agnus.diwstrt, agnus.diwstop);
        let ddf_start = agnus.ddf_start_match();

        let in_visible_line = vertical_diw_active;
        let bpl_dma_on = dmacon & 0x0300 == 0x0300;
        let bpu = agnus.num_bitplanes();
        const HBLANK_START_CCK: u16 = 0x12;
        // An ordinary DDFSTOP completes the active fetch unit containing
        // it and one additional unit; Agnus derives that unit from the
        // installed fetch mode. Agnus also owns fixed-limit termination
        // and passes only actual grants here.
        // Keep the chip's BPLCON0 copy in lockstep with Agnus's —
        // Agnus owns the primary storage (it consumes BPU for the DMA
        // scheduler); Denise reads HIRES/HOMOD/DBLPF/LACE from it.
        self.ocs.set_bplcon0(agnus.bplcon0);
        // Mirror interlace state: Agnus toggles `lof` each frame when
        // BPLCON0 LACE (bit 2) is set; Denise consumes both for
        // per-field row interleaving.
        let lace = (agnus.bplcon0 & 0x0004) != 0;
        self.ocs.set_interlace_active(lace);
        self.ocs.set_lof(agnus.lof);

        // ── CCK-boundary events (phase 0 only) ──────────────────
        if phase == 0 {
            // Denise's display line begins at fixed HBLANK start, not when
            // Agnus's horizontal counter wraps. Until $12, physical pixels
            // still complete the preceding displayed row. Perform the
            // one-time line reset before servicing this CCK's fetch so an
            // enhanced-chipset DDF comparator coincident with the boundary
            // contributes to the new line rather than being cleared as
            // previous-line state.
            if hpos >= HBLANK_START_CCK {
                self.prior_line_raster = None;
                if in_visible_line && self.last_begin_line != Some(vpos) {
                    self.ocs.begin_beam_line();
                    self.last_begin_line = Some(vpos);
                } else if !in_visible_line {
                    self.last_begin_line = None;
                }
            }

            // Bitplane fetch — the concrete Agnus/Alice grant already
            // incorporates DMA enable, DDF cadence, and its variant's
            // vertical display-window decode.
            if let Some(fetch) = bitplane_dma_fetch {
                let plane = fetch.plane as usize;
                let width = u32::from(fetch.width_words);
                let addr = agnus.bpl_pt[plane];
                // First word feeds the normal shift-register load path.
                let word = memory.read_chip_ram_word(addr);
                self.ocs.load_bitplane(plane, word);
                if plane == 0 {
                    self.ocs.queue_shift_load_from_bpl1dat();
                }
                // AGA wide fetch (FMODE > 0): a single DMA slot transfers
                // 2 (32-bit) or 4 (64-bit) words. The extra words queue in
                // Denise's per-plane FIFO and reload the shift register as
                // it drains. Width 1 (OCS / ECS) skips this loop entirely.
                for w in 1..width {
                    let extra = memory.read_chip_ram_word(addr.wrapping_add(2 * w));
                    self.ocs.push_bpl_fifo(plane, extra);
                }
                let bytes = 2 * width;
                agnus.bpl_pt[plane] = agnus.bpl_pt[plane].wrapping_add(bytes);
                self.bytes_this_line += bytes;
            }

            // End-of-line modulo — applied the moment hpos wraps to
            // zero (i.e. at the start of the next line).
            if hpos == 0 && self.bytes_this_line > 0 && bpl_dma_on {
                for p in 0..bpu as usize {
                    let modulo = if p & 1 == 0 {
                        i32::from(agnus.bpl1mod)
                    } else {
                        i32::from(agnus.bpl2mod)
                    };
                    agnus.bpl_pt[p] = agnus.bpl_pt[p].wrapping_add(modulo as u32);
                }
                self.bytes_this_line = 0;
            }

            // Sprite DMA is serviced per colour-clock slot by the machine
            // (Agnus owns the control/data state machine + the SPRxPT
            // pointers; see `Agnus::service_sprite_dma_cyc`). The earlier
            // per-line implementation here advanced the pointer every line
            // — including idle and vblank lines — which desynced VSTART/
            // VSTOP from the data stream after the first control fetch, so
            // DMA-driven sprites never displayed (gap #162). The control/
            // data words land in this chip via the same `write_sprite_*`
            // helpers, so the render path is unchanged.
        }

        // ── Per-tick: output one lores pixel ────────────────────
        const VIEWPORT_H_START_CCK: u16 = 0x2C;
        const VIEWPORT_V_START_LINE: u16 = 0x19;
        const VIEWPORT_H_END_CCK: u16 = 0xEC;
        const VIEWPORT_V_END_LINE: u16 = 0x139;
        let pipeline_y = u32::from(vpos.saturating_sub(vstart)) * 2;
        let interlace_row = if lace {
            Some(if agnus.lof { 0 } else { 1 })
        } else {
            None
        };

        // vAmiga's registered raster mapping treats physical positions before
        // HBLANK start as the tail of the preceding displayed row. Extend the
        // horizontal coordinate by that line's actual length while retaining
        // its vertical, DDF and interlace context. Agnus time and bus ownership
        // remain on the current physical position.
        let projection = if hpos < HBLANK_START_CCK {
            self.prior_line_raster.map(|prior| {
                (
                    prior.vpos,
                    prior.line_ccks.saturating_add(hpos),
                    prior.ddf_start,
                    prior.pipeline_y,
                    prior.vertical_diw_active,
                    prior.interlace_row,
                )
            })
        } else {
            Some((
                vpos,
                hpos,
                ddf_start,
                pipeline_y,
                in_visible_line,
                interlace_row,
            ))
        };

        let mut output_samples_advanced = false;
        if let Some((
            raster_vpos,
            raster_hpos,
            raster_ddf_start,
            raster_pipeline_y,
            raster_vertical_diw_active,
            raster_interlace_row,
        )) = projection
        {
            let in_viewport_h = (VIEWPORT_H_START_CCK..VIEWPORT_H_END_CCK).contains(&raster_hpos);
            let in_viewport_v = (VIEWPORT_V_START_LINE..VIEWPORT_V_END_LINE).contains(&raster_vpos);
            let pipeline_x = match raster_ddf_start {
                Some(start) if raster_hpos >= start => {
                    u32::from(raster_hpos - start) * 2 + u32::from(phase)
                }
                _ => 0,
            };

            // Horizontal visibility uses the extended Denise coordinate,
            // not the wrapped physical counter. This lets genuine bitplane
            // and sprite tails reach the right edge.
            let beam_x_lores = u32::from(raster_hpos) * 2 + u32::from(phase);
            let in_visible_h = horizontal_diw_active(
                beam_x_lores,
                agnus.diwstrt,
                agnus.diwstop,
                self.ocs.horizontal_diw_comparator_phase(),
            );
            let playfield_gate = raster_vertical_diw_active && in_visible_h;

            // The Denise pipeline runs across the complete projected raster,
            // including positions outside the host framebuffer. Early DDF
            // windows can load and shift bitplane data before retained output
            // begins; pausing here would let a later fetch overwrite that
            // pending word. Only framebuffer storage is viewport-clipped.
            // The bitplane pipeline uses DDF-relative coordinates, while the
            // sprite comparator consumes the extended absolute beam position.
            let dbg = self.ocs.output_pixel_with_beam_sprite_coords(
                pipeline_x,
                raster_pipeline_y,
                pipeline_x,
                raster_pipeline_y,
                beam_x_lores,
                u32::from(raster_vpos),
                playfield_gate,
            );
            let samples = if dbg.called {
                match dbg.source_pixels_per_fb_pixel.min(2) {
                    0 => [(0, 0, false), (0, 0, false)],
                    1 => [
                        (
                            dbg.quad_playfield_color_idx[0],
                            dbg.final_color_idx,
                            dbg.quad_is_sprite[0],
                        ),
                        (
                            dbg.quad_playfield_color_idx[0],
                            dbg.final_color_idx,
                            dbg.quad_is_sprite[0],
                        ),
                    ],
                    _ => [
                        (
                            dbg.quad_playfield_color_idx[0],
                            dbg.quad_color_idx[0],
                            dbg.quad_is_sprite[0],
                        ),
                        (
                            dbg.quad_playfield_color_idx[1],
                            dbg.quad_color_idx[1],
                            dbg.quad_is_sprite[1],
                        ),
                    ],
                }
            } else {
                [(0, 0, false), (0, 0, false)]
            };

            // Resolve each hires output sample once even when it is not
            // retained. HAM and Lisa's delayed COLOR-write path are stateful,
            // so composition is part of raster advancement rather than host
            // framebuffer storage.
            let composed_pixels =
                samples.map(|(playfield_color_idx, output_color_idx, is_sprite)| {
                    self.ocs.resolve_output_color_argb(
                        playfield_color_idx,
                        output_color_idx,
                        is_sprite,
                    )
                });
            output_samples_advanced = true;

            if in_viewport_h && in_viewport_v {
                let local_y = u32::from(raster_vpos - VIEWPORT_V_START_LINE) * 2;
                let local_x = u32::from(raster_hpos - VIEWPORT_H_START_CCK) * 2 + u32::from(phase);
                let row_offsets: &[u32] = match raster_interlace_row {
                    Some(0) => &[0],
                    Some(_) => &[1],
                    None => &[0, 1],
                };

                // Non-interlaced rendering duplicates the already-composed
                // samples onto two host rows without advancing them twice.
                for &row_offset in row_offsets {
                    let dy = local_y + row_offset;
                    for (dx, composed_pixel) in composed_pixels.iter().copied().enumerate() {
                        let pixel = if horizontal_blanking.contains_output_sample(dx as u8) {
                            0xFF00_0000
                        } else {
                            composed_pixel
                        };
                        let x = local_x * 2 + dx as u32;
                        let idx = (dy * FB_WIDTH + x) as usize;
                        if idx < self.framebuffer.len() {
                            self.framebuffer[idx] = pixel;
                        }
                    }
                }
            }
        }

        // At startup there is no previous-line raster context for physical
        // positions before HBLANK start, so there is no bitplane or sprite
        // output to consume. Still advance the standalone colour-output
        // delay so a register write cannot remain pending indefinitely.
        if !output_samples_advanced {
            self.ocs.advance_color_output_samples(2);
        }

        // COLOR writes become chip-visible only after this output tick. This
        // is Denise's early RGA stage; AGA Lisa's existing colour-output
        // delay remains inside `DeniseAga::handle_color_write`.
        for pending in self.pending_early_writes.drain(..) {
            self.ocs.write_word(pending.register, pending.value);
        }
        self.ocs.advance_early_color_output_pipeline();

        // Enhanced-chipset selector/comparator copies use the normal display
        // path and therefore advance independently of the early COLOR stage.
        self.ocs.advance_register_output_pipeline();

        // Capture after the terminal pixel has been composed. The following
        // physical line's pre-$12 interval will finish this displayed row.
        if phase == 1 && line_ccks != 0 && hpos.saturating_add(1) == line_ccks {
            self.prior_line_raster = Some(PriorLineRasterContext {
                vpos,
                line_ccks,
                vbl_count: agnus.vbl_count,
                ddf_start,
                pipeline_y,
                vertical_diw_active: in_visible_line,
                interlace_row,
            });
        }
    }
}

pub(crate) fn rgb12_to_argb(c12: u16) -> u32 {
    let r = ((c12 >> 8) & 0xF) as u32;
    let g = ((c12 >> 4) & 0xF) as u32;
    let b = (c12 & 0xF) as u32;
    let r8 = (r << 4) | r;
    let g8 = (g << 4) | g;
    let b8 = (b << 4) | b;
    0xFF00_0000 | (r8 << 16) | (g8 << 8) | b8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture_prior_line_tail(
        denise: &mut Denise<commodore_denise_ocs::DeniseOcs>,
        agnus: &mut commodore_agnus_ocs::Agnus,
        memory: &Memory,
        vpos: u16,
        line_ccks: u16,
        shift_bits: u16,
        interlace_lof: Option<bool>,
    ) {
        agnus.vpos = vpos;
        agnus.hpos = line_ccks - 1;
        agnus.diwstrt = 0x0000;
        agnus.diwstop = 0x00FF;
        agnus.bplcon0 = 0x1000 | if interlace_lof.is_some() { 0x0004 } else { 0 };
        agnus.lof = interlace_lof.unwrap_or(false);

        denise.ocs.set_palette(0, 0x000);
        denise.ocs.set_palette(1, 0xFFF);
        denise.ocs.bpl_shift[0] = shift_bits;
        denise.ocs.shift_count = 3;
        denise.tick(1, None, true, agnus, memory, line_ccks);

        assert_eq!(
            denise.prior_line_raster.map(|context| context.line_ccks),
            Some(line_ccks),
            "the terminal phase must capture the actual physical line length",
        );
    }

    fn render_wrapped_hpos_zero(
        denise: &mut Denise<commodore_denise_ocs::DeniseOcs>,
        agnus: &mut commodore_agnus_ocs::Agnus,
        memory: &Memory,
        vpos: u16,
        line_ccks: u16,
    ) {
        agnus.vpos = vpos;
        agnus.hpos = 0;
        denise.tick(0, None, true, agnus, memory, line_ccks);
        denise.tick(1, None, true, agnus, memory, line_ccks);
    }

    fn observe_ddf_start(agnus: &mut commodore_agnus_ocs::Agnus) {
        if agnus.agnus_id < 0x2000 && !agnus.vertical_diw_active() {
            let diwstrt = agnus.diwstrt;
            let diwstop = agnus.diwstop;
            assert!(agnus.vpos <= 0x00FF);
            agnus.write_diwstop(diwstop);
            agnus.write_diwstrt((agnus.vpos << 8) | (diwstrt & 0x00FF));
            agnus.write_diwstrt(diwstrt);
            for _ in 0..8 {
                agnus.tick_cck();
            }
        }
        let mask = if agnus.agnus_id >= 0x2000 {
            0x00FE
        } else {
            0x00FC
        };
        let start = agnus.ddfstrt & mask;
        assert!(start > 0, "test helper requires a non-zero DDFSTRT");
        agnus.hpos = start - 1;
        agnus.tick_cck();
        assert_eq!(agnus.ddf_start_match(), Some(start));
    }

    #[test]
    fn rgb12_conversion() {
        assert_eq!(rgb12_to_argb(0x0FFF), 0xFFFF_FFFF);
        assert_eq!(rgb12_to_argb(0x0000), 0xFF00_0000);
        assert_eq!(rgb12_to_argb(0x0F00), 0xFFFF_0000);
        assert_eq!(rgb12_to_argb(0x0444), 0xFF44_4444);
    }

    #[test]
    fn diw_vertical_window_pal_nominal() {
        let (vstart, vstop) = diw_vertical_window(0x2C81, 0x2CC1);
        assert_eq!(vstart, 0x2C);
        assert_eq!(vstop, 0x12C);
    }

    #[test]
    fn diw_vertical_window_ntsc_nominal() {
        let (vstart, vstop) = diw_vertical_window(0x2C81, 0xF4C1);
        assert_eq!(vstart, 0x2C);
        assert_eq!(vstop, 0xF4);
    }

    #[test]
    fn ddf_window_masks_to_fetch_alignment() {
        assert_eq!(ddf_window(0x0038, 0x00D0), (0x38, 0xD0));
        assert_eq!(ddf_window(0x003B, 0x00D2), (0x38, 0xD0));
        assert_eq!(ddf_window(0x003C, 0x00D3), (0x3C, 0xD0));
    }

    #[test]
    fn horizontal_diw_gate_obeys_the_variant_comparator_phase() {
        let diwstrt = 0x2C81;
        let diwstop = 0x2CC1;

        let before = |beam_x| {
            horizontal_diw_active(
                beam_x,
                diwstrt,
                diwstop,
                HorizontalDiwComparatorPhase::BeforeOutput,
            )
        };
        assert!(!before(0x080));
        assert!(before(0x081));
        assert!(before(0x1C0));
        assert!(!before(0x1C1));

        let after = |beam_x| {
            horizontal_diw_active(
                beam_x,
                diwstrt,
                diwstop,
                HorizontalDiwComparatorPhase::AfterOutput,
            )
        };
        assert!(!after(0x081));
        assert!(after(0x082));
        assert!(after(0x1C1));
        assert!(!after(0x1C2));
    }

    #[test]
    fn horizontal_blanking_carries_composed_output_levels_only() {
        let disabled = HorizontalBlanking::disabled();
        assert!(!disabled.contains_output_sample(0));
        assert!(!disabled.contains_output_sample(1));

        let enabled = HorizontalBlanking::from_level(true);
        assert!(enabled.contains_output_sample(0));
        assert!(enabled.contains_output_sample(1));

        let split = HorizontalBlanking::from_output_samples([true, false]);
        assert!(split.contains_output_sample(0));
        assert!(!split.contains_output_sample(1));
    }

    #[test]
    fn aga_fine_hblank_can_split_one_output_pair() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let memory = Memory::new(vec![0; 2]);
        let mut agnus = Agnus::new();
        agnus.vpos = 50;
        agnus.hpos = 0x00A0;
        agnus.diwstrt = 0x0000;
        agnus.diwstop = 0x00FF;

        let mut denise = Denise::<DeniseOcs>::new();
        denise.ocs.set_palette(0, 0x0F0);
        let line_ccks = agnus.current_line_ccks();
        denise.tick_with_output_signals(
            1,
            None,
            DeniseOutputSignals::new(true, HorizontalBlanking::from_output_samples([true, false])),
            &mut agnus,
            &memory,
            line_ccks,
        );

        let y = u32::from(agnus.vpos - 0x19) * 2;
        let x = u32::from(agnus.hpos - 0x2C) * 4 + 2;
        assert_eq!(denise.framebuffer[(y * FB_WIDTH + x) as usize], 0xFF00_0000);
        assert_eq!(
            denise.framebuffer[(y * FB_WIDTH + x + 1) as usize],
            0xFF00_FF00,
        );
    }

    #[test]
    fn external_hblank_masks_output_without_stalling_the_pixel_pipeline() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let memory = Memory::new(vec![0; 2]);
        let mut reference_agnus = Agnus::new();
        reference_agnus.vpos = 50;
        reference_agnus.hpos = 0x0080;
        reference_agnus.diwstrt = 0x0000;
        reference_agnus.diwstop = 0x00FF;
        reference_agnus.bplcon0 = 0x1000;

        let mut blanked_agnus = reference_agnus.clone();
        let mut reference = Denise::<DeniseOcs>::new();
        reference.ocs.set_palette(0, 0x0F0);
        reference.ocs.set_palette(1, 0xFFF);
        reference.ocs.bpl_shift[0] = 0xFFFF;
        reference.ocs.shift_count = 16;
        let mut blanked = reference.clone();

        let line_ccks = reference_agnus.current_line_ccks();
        reference.tick(0, None, true, &mut reference_agnus, &memory, line_ccks);
        blanked.tick_with_output_signals(
            0,
            None,
            DeniseOutputSignals::new(true, HorizontalBlanking::from_level(true)),
            &mut blanked_agnus,
            &memory,
            line_ccks,
        );

        assert_eq!(blanked.ocs.bpl_shift, reference.ocs.bpl_shift);
        assert_eq!(blanked.ocs.shift_count, reference.ocs.shift_count);
        let y = u32::from(reference_agnus.vpos - 0x19) * 2;
        let x = u32::from(reference_agnus.hpos - 0x2C) * 4;
        assert_ne!(
            reference.framebuffer[(y * FB_WIDTH + x) as usize],
            0xFF00_0000,
            "the unblanked fixture must compose a visible pixel",
        );
        for dx in 0..2 {
            assert_eq!(
                blanked.framebuffer[(y * FB_WIDTH + x + dx) as usize],
                0xFF00_0000,
            );
        }
    }

    #[test]
    fn noninterlaced_host_row_duplication_does_not_advance_ham_twice() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let memory = Memory::new(vec![0; 2]);
        let mut agnus = Agnus::new();
        agnus.vpos = 50;
        agnus.hpos = 0x0080;
        agnus.diwstrt = 0x0000;
        agnus.diwstop = 0x00FF;
        agnus.bplcon0 = 0xE800; // HIRES, six planes, HAM

        let mut denise = Denise::<DeniseOcs>::new();
        denise.ocs.set_palette(0, 0x000);
        // Emit HAM indices $2F (modify red) then $3F (modify green).
        // Re-resolving the pair for the doubled host row would retain the
        // first pass's green component in that row's first sample.
        for plane in 0..6 {
            let first = u16::from((0x2Fu8 >> plane) & 1) << 15;
            let second = u16::from((0x3Fu8 >> plane) & 1) << 14;
            denise.ocs.bpl_shift[plane] = first | second;
        }
        denise.ocs.shift_count = 16;

        let line_ccks = agnus.current_line_ccks();
        denise.tick(0, None, true, &mut agnus, &memory, line_ccks);

        let y = u32::from(agnus.vpos - 0x19) * 2;
        let x = u32::from(agnus.hpos - 0x2C) * 4;
        let framebuffer = denise.framebuffer();
        let top = &framebuffer[(y * FB_WIDTH + x) as usize..(y * FB_WIDTH + x + 2) as usize];
        let bottom =
            &framebuffer[((y + 1) * FB_WIDTH + x) as usize..((y + 1) * FB_WIDTH + x + 2) as usize];
        assert_eq!(top, &[0xFFFF_0000, 0xFFFF_FF00]);
        assert_eq!(
            bottom, top,
            "host row doubling must copy composed HAM pixels"
        );
    }

    #[test]
    fn wrapped_hpos_zero_uses_the_previous_lines_actual_length() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let memory = Memory::new(vec![0; 2]);
        let prior_vpos = 50;
        let target_y = u32::from(prior_vpos - 0x19) * 2;

        let mut short_agnus = Agnus::new();
        let mut short = Denise::<DeniseOcs>::new();
        capture_prior_line_tail(
            &mut short,
            &mut short_agnus,
            &memory,
            prior_vpos,
            227,
            0x6000,
            None,
        );
        render_wrapped_hpos_zero(&mut short, &mut short_agnus, &memory, prior_vpos + 1, 228);

        for x in 732..736 {
            assert_eq!(
                short.framebuffer[(target_y * FB_WIDTH + x) as usize],
                0xFFFF_FFFF,
                "a 227-CCK line must project physical hpos 0 onto x 732..735",
            );
        }
        assert_eq!(
            short.framebuffer[(target_y * FB_WIDTH + 736) as usize],
            0xFF00_0000,
        );

        let mut long_agnus = Agnus::new();
        let mut long = Denise::<DeniseOcs>::new();
        capture_prior_line_tail(
            &mut long,
            &mut long_agnus,
            &memory,
            prior_vpos,
            228,
            0x6000,
            None,
        );
        render_wrapped_hpos_zero(&mut long, &mut long_agnus, &memory, prior_vpos + 1, 227);

        assert_eq!(
            long.framebuffer[(target_y * FB_WIDTH + 735) as usize],
            0xFF00_0000,
            "a 228-CCK line already owns x 732..735 before counter wrap",
        );
        for x in 736..740 {
            assert_eq!(
                long.framebuffer[(target_y * FB_WIDTH + x) as usize],
                0xFFFF_FFFF,
                "a 228-CCK line must project physical hpos 0 onto x 736..739",
            );
        }
    }

    #[test]
    fn line_local_denise_state_resets_at_hblank_start_not_counter_wrap() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let memory = Memory::new(vec![0; 2]);
        let mut agnus = Agnus::new();
        let mut denise = Denise::<DeniseOcs>::new();
        denise.ocs.queue_shift_load_from_bpl1dat();
        capture_prior_line_tail(&mut denise, &mut agnus, &memory, 50, 227, 0x0000, None);

        agnus.vpos = 51;
        agnus.hpos = 0;
        denise.tick(0, None, true, &mut agnus, &memory, 227);
        assert!(
            denise.ocs.sprite_bpl1dat_enabled(),
            "counter wrap must retain the prior line's BPL1DAT visibility state",
        );

        agnus.hpos = 0x11;
        denise.tick(0, None, true, &mut agnus, &memory, 227);
        assert!(denise.ocs.sprite_bpl1dat_enabled());

        agnus.hpos = 0x12;
        denise.tick(0, None, true, &mut agnus, &memory, 227);
        assert!(
            !denise.ocs.sprite_bpl1dat_enabled(),
            "fixed HBLANK start must begin the new Denise display line",
        );
        assert!(denise.prior_line_raster.is_none());
    }

    #[test]
    fn board_pipeline_snapshot_tracks_fetch_and_prior_line_context() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let memory = Memory::new(vec![0; 2]);
        let mut agnus = Agnus::new();
        let mut denise = Denise::<DeniseOcs>::new();
        assert_eq!(
            denise.board_pipeline_diagnostic_snapshot(),
            DeniseBoardPipelineDiagnosticSnapshot {
                bytes_this_line: 0,
                last_begin_line: None,
                prior_line_raster: None,
                pending_early_writes: Vec::new(),
            },
        );

        agnus.vpos = 0x002C;
        agnus.hpos = 0x0040;
        agnus.vbl_count = 7;
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;
        let line_ccks = agnus.current_line_ccks();
        denise.tick(
            0,
            Some(BitplaneDmaFetch {
                plane: 0,
                width_words: 1,
            }),
            true,
            &mut agnus,
            &memory,
            line_ccks,
        );
        assert_eq!(
            denise.board_pipeline_diagnostic_snapshot(),
            DeniseBoardPipelineDiagnosticSnapshot {
                bytes_this_line: 2,
                last_begin_line: Some(0x002C),
                prior_line_raster: None,
                pending_early_writes: Vec::new(),
            },
        );

        agnus.hpos = line_ccks - 1;
        denise.tick(1, None, true, &mut agnus, &memory, line_ccks);
        assert_eq!(
            denise.board_pipeline_diagnostic_snapshot(),
            DeniseBoardPipelineDiagnosticSnapshot {
                bytes_this_line: 2,
                last_begin_line: Some(0x002C),
                prior_line_raster: Some(DenisePriorLineRasterDiagnosticSnapshot {
                    vpos: 0x002C,
                    line_ccks,
                    vbl_count: 7,
                    ddf_start: None,
                    pipeline_y: 0,
                    vertical_diw_active: true,
                    interlace_row: None,
                }),
                pending_early_writes: Vec::new(),
            },
        );

        agnus.vpos += 1;
        agnus.hpos = 0x0012;
        denise.tick(0, None, true, &mut agnus, &memory, line_ccks);
        let retired = denise.board_pipeline_diagnostic_snapshot();
        assert_eq!(retired.last_begin_line, Some(0x002D));
        assert_eq!(retired.prior_line_raster, None);
    }

    #[test]
    fn color_write_crosses_the_early_output_stage() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let memory = Memory::new(vec![0; 2]);
        let mut agnus = Agnus::new();
        let mut denise = Denise::<DeniseOcs>::new();
        denise.ocs.set_palette(0, 0x0123);

        denise.write_word_before_output_tick(0x0180, 0x0ABC);

        assert_eq!(denise.color(0), 0x0123);
        assert_eq!(
            denise
                .board_pipeline_diagnostic_snapshot()
                .pending_early_writes,
            vec![DenisePendingRegisterWrite {
                register: 0x0180,
                value: 0x0ABC,
            }],
        );

        let line_ccks = agnus.current_line_ccks();
        denise.tick(0, None, false, &mut agnus, &memory, line_ccks);

        assert_eq!(denise.color(0), 0x0ABC);
        assert!(
            denise
                .board_pipeline_diagnostic_snapshot()
                .pending_early_writes
                .is_empty(),
        );
    }

    #[test]
    fn color_write_after_output_is_ready_for_the_next_tick() {
        use commodore_denise_ocs::DeniseOcs;

        let mut denise = Denise::<DeniseOcs>::new();
        denise.ocs.set_palette(0, 0x0123);

        denise.write_word(0x0180, 0x0ABC);

        assert_eq!(denise.color(0), 0x0ABC);
        assert!(
            denise
                .board_pipeline_diagnostic_snapshot()
                .pending_early_writes
                .is_empty(),
        );
    }

    #[test]
    fn pending_early_color_write_survives_serialization() {
        use commodore_denise_ocs::DeniseOcs;

        let mut denise = Denise::<DeniseOcs>::new();
        denise.write_word_before_output_tick(0x0180, 0x0ABC);

        let encoded = postcard::to_allocvec(&denise).expect("serialize Denise pipeline");
        let restored: Denise<DeniseOcs> =
            postcard::from_bytes(&encoded).expect("deserialize Denise pipeline");

        assert_eq!(
            restored.board_pipeline_diagnostic_snapshot(),
            denise.board_pipeline_diagnostic_snapshot(),
        );
        assert_eq!(restored.color(0), 0);
    }

    #[test]
    fn completed_display_field_waits_for_wrapped_raster_carry() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let memory = Memory::new(vec![0; 2]);
        let mut agnus = Agnus::new();
        let mut denise = Denise::<DeniseOcs>::new();
        agnus.vbl_count = 7;
        capture_prior_line_tail(&mut denise, &mut agnus, &memory, 311, 227, 0x0000, None);

        assert_eq!(
            denise.completed_display_field_count(agnus.vbl_count),
            7,
            "an ordinary captured line must not change the completed field count",
        );

        agnus.vbl_count = 8;
        agnus.vpos = 0;
        agnus.hpos = 0;
        denise.tick(0, None, false, &mut agnus, &memory, 227);
        denise.tick(1, None, false, &mut agnus, &memory, 227);
        assert_eq!(
            denise.completed_display_field_count(agnus.vbl_count),
            7,
            "raw field wrap must remain unpublished while prior output is pending",
        );
        assert_eq!(
            denise.completed_display_field_count(agnus.vbl_count + 1),
            9,
            "a context older than one raw transition must not suppress a later field",
        );

        agnus.hpos = 0x11;
        denise.tick(0, None, false, &mut agnus, &memory, 227);
        denise.tick(1, None, false, &mut agnus, &memory, 227);
        assert_eq!(denise.completed_display_field_count(agnus.vbl_count), 7);

        agnus.hpos = 0x12;
        denise.tick(0, None, false, &mut agnus, &memory, 227);
        assert_eq!(
            denise.completed_display_field_count(agnus.vbl_count),
            8,
            "HBLANK-start retirement makes the wrapped field publishable",
        );
    }

    #[test]
    fn wrapped_pixels_keep_the_previous_fields_interlace_row() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let memory = Memory::new(vec![0; 2]);
        let mut agnus = Agnus::new();
        let mut denise = Denise::<DeniseOcs>::new();
        let prior_vpos = 50;
        let top_y = u32::from(prior_vpos - 0x19) * 2;
        capture_prior_line_tail(
            &mut denise,
            &mut agnus,
            &memory,
            prior_vpos,
            227,
            0x6000,
            Some(true),
        );

        agnus.lof = false;
        render_wrapped_hpos_zero(&mut denise, &mut agnus, &memory, prior_vpos + 1, 227);

        assert_eq!(
            denise.framebuffer[(top_y * FB_WIDTH + 732) as usize],
            0xFFFF_FFFF,
            "carry must retain the prior field's selected row",
        );
        assert_eq!(
            denise.framebuffer[((top_y + 1) * FB_WIDTH + 732) as usize],
            0xFF00_0000,
            "the current field selection must not move a prior-line carry",
        );
    }

    #[test]
    fn startup_hpos_zero_without_prior_context_does_not_consume_video_state() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let memory = Memory::new(vec![0; 2]);
        let mut agnus = Agnus::new();
        agnus.vpos = 51;
        agnus.hpos = 0;
        agnus.diwstrt = 0x0000;
        agnus.diwstop = 0x00FF;
        agnus.bplcon0 = 0x1000;

        let mut denise = Denise::<DeniseOcs>::new();
        denise.ocs.bpl_shift[0] = 0xFFFF;
        denise.ocs.shift_count = 16;
        denise.tick(0, None, true, &mut agnus, &memory, 227);
        denise.tick(1, None, true, &mut agnus, &memory, 227);

        assert_eq!(
            denise.ocs.shift_count, 16,
            "no previous-line context means there is no carried pixel to consume",
        );
        assert!(denise.framebuffer.iter().all(|pixel| *pixel == 0xFF00_0000));
    }

    #[test]
    fn sprite_first_pixel_follows_hstart_load_in_board_framebuffer() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let mut agnus = Agnus::new();
        agnus.vpos = 50;
        agnus.hpos = 100;
        agnus.diwstrt = 0x2C00;
        agnus.diwstop = 0xF4FF;

        let mut denise = Denise::<DeniseOcs>::new();
        denise.ocs.set_palette(0, 0x000);
        denise.ocs.set_palette(17, 0xF00);
        denise.ocs.write_sprite_pos(0, 0x3264); // HSTART=200
        denise.ocs.write_sprite_ctl(0, 0x3C00);
        denise.ocs.write_sprite_datb(0, 0x0000);
        denise.ocs.write_sprite_data(0, 0x8000);

        let memory = Memory::new(vec![0; 2]);
        let line_ccks = agnus.current_line_ccks();
        denise.tick(0, None, true, &mut agnus, &memory, line_ccks);
        // BPL1DAT enables sprite contribution for the remainder of the line.
        // Inject it after the phase-0 line reset so this focused board test
        // can exercise the following sprite output step without constructing
        // a complete bitplane-DMA fixture.
        denise.ocs.queue_shift_load_from_bpl1dat();
        denise.tick(1, None, true, &mut agnus, &memory, line_ccks);

        let y = u32::from(agnus.vpos - 0x19) * 2;
        let hstart_x = u32::from(agnus.hpos - 0x2C) * 4;
        let first_sprite_x = hstart_x + 2;
        let framebuffer = denise.framebuffer();
        for x in hstart_x..first_sprite_x {
            assert_eq!(
                framebuffer[(y * FB_WIDTH + x) as usize],
                0xFF00_0000,
                "the HSTART comparison/load step remains background",
            );
        }
        for x in first_sprite_x..first_sprite_x + 2 {
            assert_eq!(
                framebuffer[(y * FB_WIDTH + x) as usize],
                0xFFFF_0000,
                "the first sprite MSB appears on the following lores step",
            );
        }
    }

    #[test]
    fn sprite_index_bypasses_ham_while_the_playfield_stream_advances() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let mut agnus = Agnus::new();
        agnus.vpos = 50;
        agnus.hpos = 100;
        agnus.diwstrt = 0x2C00;
        agnus.diwstop = 0xF4FF;
        agnus.bplcon0 = 0x6800; // six planes, HAM

        let mut denise = Denise::<DeniseOcs>::new();
        denise.ocs.set_palette(0, 0x123);
        denise.ocs.set_palette(17, 0xF00);
        denise.ocs.write_sprite_pos(0, 0x3264); // HSTART=200
        denise.ocs.write_sprite_ctl(0, 0x3C00);
        denise.ocs.write_sprite_datb(0, 0x0000);
        denise.ocs.write_sprite_data(0, 0x8000);

        let memory = Memory::new(vec![0; 2]);
        let line_ccks = agnus.current_line_ccks();
        denise.tick(0, None, true, &mut agnus, &memory, line_ccks);

        // Put HAM command $2f (modify red to $f) underneath the sprite and
        // allow sprite group 0 in front of PF1.  The command must still
        // advance HAM's hidden hold even though COLOR17 supplies the visible
        // pixel.
        for plane in 0..6 {
            denise
                .ocs
                .load_bitplane(plane, if 0x2f & (1 << plane) != 0 { 0x8000 } else { 0 });
        }
        denise.ocs.bplcon2 = 0x0001;
        denise.ocs.queue_shift_load_from_bpl1dat();
        denise.ocs.trigger_shift_load();
        let hold_before = denise.ocs.diagnostic_snapshot().ham_previous_rgb12;
        denise.tick(1, None, true, &mut agnus, &memory, line_ccks);

        let y = u32::from(agnus.vpos - 0x19) * 2;
        let first_sprite_x = u32::from(agnus.hpos - 0x2C) * 4 + 2;
        let framebuffer = denise.framebuffer();
        assert_eq!(
            framebuffer[(y * FB_WIDTH + first_sprite_x) as usize],
            0xFFFF_0000,
            "a winning sprite selects its palette colour instead of becoming a HAM command",
        );
        assert_eq!(
            denise.ocs.diagnostic_snapshot().ham_previous_rgb12,
            0xF23,
            "the underlying playfield HAM command must advance the hidden hold",
        );
        assert_eq!(hold_before, 0x123);
    }

    #[test]
    fn wide_fetch_advances_pointer_by_width_words_per_line() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        // Workbench 3.1 AGA: hires, 2 planes, FMODE=$0003 (64-bit). The
        // DMA scheduler grants 11 plane accesses per line; the fetch
        // loop turns each into 4 words, so each plane pointer must
        // advance by 11*4*2 = 88 bytes (44 words) per line, before the
        // end-of-line modulo. (Driven by Agnus FMODE, so the OCS chip
        // exercises the same fetch-loop path the AGA chip uses.)
        let mut agnus = Agnus::new();
        agnus.agnus_id = 0x2300;
        agnus.max_bitplanes = 8;
        agnus.dmacon = 0x0300; // DMAEN | BPLEN
        agnus.bplcon0 = 0xA302; // HIRES + 2 planes
        agnus.ddfstrt = 0x38;
        agnus.ddfstop = 0xD8;
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;
        agnus.fmode = 0x0003;
        agnus.vpos = 100;
        agnus.bpl_pt[0] = 0x2000;
        agnus.bpl_pt[1] = 0x3000;
        let (bpl0, bpl1) = (agnus.bpl_pt[0], agnus.bpl_pt[1]);

        let mem = Memory::new(vec![0u8; 0x4_0000]);
        let mut denise = Denise::<DeniseOcs>::new();
        observe_ddf_start(&mut agnus);

        // Sweep from the observed start through the rest of the line.
        // Advancing Agnus normally lets the DDFSTOP comparator freeze
        // the terminal fetch unit. Stop before wrapping to hpos 0 again
        // so no modulo is applied.
        loop {
            let plan = agnus.cck_bus_plan();
            let width = agnus.bpl_fetch_width();
            let vertical_diw_active = agnus.vertical_diw_active();
            let line_ccks = agnus.current_line_ccks();
            denise.tick(
                0,
                plan.bitplane_dma_fetch_plane.map(|plane| BitplaneDmaFetch {
                    plane,
                    width_words: width,
                }),
                vertical_diw_active,
                &mut agnus,
                &mem,
                line_ccks,
            );
            if agnus.hpos == 0xE2 {
                break;
            }
            agnus.tick_cck();
        }

        assert_eq!(agnus.bpl_pt[0] - bpl0, 88, "BPL1 bytes/line (44 words)");
        assert_eq!(agnus.bpl_pt[1] - bpl1, 88, "BPL2 bytes/line (44 words)");
    }

    #[test]
    fn narrow_fetch_advances_pointer_by_one_word_per_access() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        // OCS / ECS regression: FMODE=0 keeps 16-bit fetch. DDFSTRT=$40
        // DDFSTOP=$D0 hires = 38 accesses/plane → 38 words = 76 bytes.
        let mut agnus = Agnus::new();
        agnus.dmacon = 0x0300;
        agnus.bplcon0 = 0xA000; // HIRES + 2 planes
        agnus.ddfstrt = 0x40;
        agnus.ddfstop = 0xD0;
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;
        agnus.vpos = 100;
        agnus.bpl_pt[0] = 0x2000;
        agnus.bpl_pt[1] = 0x3000;
        let (bpl0, bpl1) = (agnus.bpl_pt[0], agnus.bpl_pt[1]);

        let mem = Memory::new(vec![0u8; 0x4_0000]);
        let mut denise = Denise::<DeniseOcs>::new();
        observe_ddf_start(&mut agnus);

        loop {
            let plan = agnus.cck_bus_plan();
            let width = agnus.bpl_fetch_width();
            let vertical_diw_active = agnus.vertical_diw_active();
            let line_ccks = agnus.current_line_ccks();
            denise.tick(
                0,
                plan.bitplane_dma_fetch_plane.map(|plane| BitplaneDmaFetch {
                    plane,
                    width_words: width,
                }),
                vertical_diw_active,
                &mut agnus,
                &mem,
                line_ccks,
            );
            if agnus.hpos == 0xE2 {
                break;
            }
            agnus.tick_cck();
        }

        assert_eq!(agnus.bpl_pt[0] - bpl0, 76, "BPL1 bytes/line (38 words)");
        assert_eq!(agnus.bpl_pt[1] - bpl1, 76, "BPL2 bytes/line (38 words)");
    }

    #[test]
    fn ddfstrt_rewrite_after_match_does_not_rephase_pixels() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let mut matched = Agnus::new();
        matched.agnus_id = 0x2000;
        matched.dmacon = 0x0300; // DMAEN | BPLEN
        matched.bplcon0 = 0x1000; // lowres, one bitplane
        matched.ddfstrt = 0x0038;
        matched.ddfstop = 0x0060;
        matched.diwstrt = 0x2C81;
        matched.diwstop = 0x2CC1;
        matched.vpos = 0x0030;
        matched.bpl_pt[0] = 0x0000_1000;

        let mut memory = Memory::new(vec![0u8; 256 * 1024]);
        for word in 0..32u32 {
            memory.write_word(
                0x0000_1000 + word * 2,
                (word as u16).wrapping_mul(0x9E37) ^ 0xA55A,
            );
        }

        observe_ddf_start(&mut matched);
        let mut rewritten = matched.clone();
        rewritten.write_ddfstrt(0x0080);

        let mut reference = Denise::<DeniseOcs>::new();
        let mut after_write = Denise::<DeniseOcs>::new();
        reference.ocs.set_palette(1, 0xFFF);
        after_write.ocs.set_palette(1, 0xFFF);

        loop {
            let reference_fetch = matched
                .cck_bus_plan()
                .bitplane_dma_fetch_plane
                .map(|plane| BitplaneDmaFetch {
                    plane,
                    width_words: 1,
                });
            let rewritten_fetch = rewritten
                .cck_bus_plan()
                .bitplane_dma_fetch_plane
                .map(|plane| BitplaneDmaFetch {
                    plane,
                    width_words: 1,
                });

            let matched_line_ccks = matched.current_line_ccks();
            let rewritten_line_ccks = rewritten.current_line_ccks();
            reference.tick(
                0,
                reference_fetch,
                true,
                &mut matched,
                &memory,
                matched_line_ccks,
            );
            after_write.tick(
                0,
                rewritten_fetch,
                true,
                &mut rewritten,
                &memory,
                rewritten_line_ccks,
            );
            reference.tick(1, None, true, &mut matched, &memory, matched_line_ccks);
            after_write.tick(1, None, true, &mut rewritten, &memory, rewritten_line_ccks);
            if matched.hpos == 0x0070 {
                break;
            }
            matched.tick_cck();
            rewritten.tick_cck();
        }

        assert_eq!(matched.ddf_start_match(), Some(0x0038));
        assert_eq!(rewritten.ddf_start_match(), Some(0x0038));
        assert_eq!(matched.bpl_pt[0], rewritten.bpl_pt[0]);
        assert!(
            reference.framebuffer().contains(&0xFFFF_FFFF),
            "the fixture must render foreground pixels",
        );
        assert_eq!(
            reference.framebuffer(),
            after_write.framebuffer(),
            "the live DDFSTRT register must not move an active pixel pipeline",
        );
    }

    #[test]
    fn early_ddf_primes_bitplane_shifter_before_framebuffer_viewport() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let mut agnus = Agnus::new();
        agnus.agnus_id = 0x2300;
        agnus.max_bitplanes = 8;
        agnus.dmacon = 0x0300; // DMAEN | BPLEN
        agnus.bplcon0 = 0x1200; // one lores bitplane, COLOR enabled
        agnus.ddfstrt = 0x0020;
        agnus.ddfstop = 0x00D8;
        agnus.diwstrt = 0x1B51;
        agnus.diwstop = 0x37D1;
        agnus.vpos = 0x001B;
        agnus.bpl_pt[0] = 0x0000_1000;

        let mut memory = Memory::new(vec![0u8; 256 * 1024]);
        memory.write_word(0x0000_1000, 0xFFFF);
        memory.write_word(0x0000_1002, 0x0000);

        let mut denise = Denise::<DeniseOcs>::new();
        denise.ocs.write_word(0x102, 0); // BPLCON1
        denise.ocs.set_palette(0, 0x000);
        denise.ocs.set_palette(1, 0xFFF);
        observe_ddf_start(&mut agnus);

        loop {
            let plan = agnus.cck_bus_plan();
            let line_ccks = agnus.current_line_ccks();
            denise.tick(
                0,
                plan.bitplane_dma_fetch_plane.map(|plane| BitplaneDmaFetch {
                    plane,
                    width_words: 1,
                }),
                true,
                &mut agnus,
                &memory,
                line_ccks,
            );
            denise.tick(1, None, true, &mut agnus, &memory, line_ccks);
            if agnus.hpos == 0x0031 {
                break;
            }
            agnus.tick_cck();
        }

        let row = usize::from(agnus.vpos - 0x0019) * 2;
        let retained = &denise.framebuffer[row * FB_WIDTH as usize..][..24];
        assert!(
            retained[..18].iter().all(|pixel| *pixel == 0xFFFF_FFFF),
            "the first fetched word must already be shifting when retained output begins",
        );
        assert!(
            retained[18..].iter().all(|pixel| *pixel == 0xFF00_0000),
            "the second fetched word must begin at the next serial-load boundary",
        );
    }

    #[test]
    fn hires_overfetch_keeps_identical_plane_streams_pixel_aligned() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        let mut agnus = Agnus::new();
        agnus.agnus_id = 0x2000;
        agnus.dmacon = 0x0300;
        agnus.bplcon0 = 0xA302; // HIRES + 2 planes + COLOR
        agnus.ddfstrt = 0x0038;
        agnus.ddfstop = 0x00D8;
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;
        agnus.bpl1mod = -4;
        agnus.bpl2mod = -4;
        agnus.bpl_pt[0] = 0x0000_1000;
        agnus.bpl_pt[1] = 0x0000_2000;

        let mut mem = Memory::new(vec![0u8; 256 * 1024]);
        for word in 0..176u32 {
            let value = (word as u16).wrapping_mul(0x9E37) ^ 0xA55A;
            mem.write_word(0x0000_1000 + word * 2, value);
            mem.write_word(0x0000_2000 + word * 2, value);
        }

        let mut denise = Denise::<DeniseOcs>::new();
        denise.ocs.set_palette(0, 0x000);
        denise.ocs.set_palette(1, 0xF00);
        denise.ocs.set_palette(2, 0x0F0);
        denise.ocs.set_palette(3, 0x00F);
        agnus.vpos = 0x002C;
        agnus.hpos = 0;
        for _ in 0..4 {
            loop {
                let plan = agnus.cck_bus_plan();
                let vertical_diw_active = agnus.vertical_diw_active();
                let line_ccks = agnus.current_line_ccks();
                denise.tick(
                    0,
                    plan.bitplane_dma_fetch_plane.map(|plane| BitplaneDmaFetch {
                        plane,
                        width_words: 1,
                    }),
                    vertical_diw_active,
                    &mut agnus,
                    &mem,
                    line_ccks,
                );
                denise.tick(1, None, vertical_diw_active, &mut agnus, &mem, line_ccks);
                let line_end = line_ccks - 1;
                let was_line_end = agnus.hpos == line_end;
                agnus.tick_cck();
                if was_line_end {
                    break;
                }
            }
        }

        for vpos in 0x002Cu16..0x0030 {
            let row = usize::from(vpos - 0x19) * 2;
            let first_visible = row * FB_WIDTH as usize + 82;
            let visible = &denise.framebuffer[first_visible..first_visible + 640];
            assert!(
                visible
                    .iter()
                    .all(|pixel| matches!(*pixel, 0xFF00_0000 | 0xFF00_00FF)),
                "identical BPL1/BPL2 data must remain aligned on line {vpos}",
            );
        }
    }
}
