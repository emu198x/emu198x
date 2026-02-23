//! Agnus - Beam counter and DMA slot allocation.

pub const PAL_CCKS_PER_LINE: u16 = 227;
pub const PAL_LINES_PER_FRAME: u16 = 312;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotOwner {
    Cpu,
    Refresh,
    Disk,
    Audio(u8),
    Sprite(u8),
    Bitplane(u8),
    Copper,
}

/// How Paula audio DMA return-latency timing should behave for this CCK slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaulaReturnProgressPolicy {
    /// Return latency advances normally this CCK.
    Advance,
    /// Return latency is stalled by an Agnus-reserved DMA slot.
    Stall,
    /// Return latency advances unless copper actually performs a chip fetch.
    ///
    /// Agnus grants the slot to copper, but the machine must observe whether
    /// copper is in a fetch state or waiting.
    CopperFetchConditional,
}

/// Agnus-owned summary of one CCK bus decision.
///
/// This is the machine-facing API for consumers that need to react to Agnus DMA
/// arbitration (e.g. Paula DMA service/return progress) without duplicating the
/// slot decoding rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CckBusPlan {
    /// Raw slot owner for debugging/inspection. Prefer the explicit grant fields
    /// below for machine behavior.
    pub slot_owner: SlotOwner,
    /// Paula audio DMA slot service grant for this CCK.
    pub audio_dma_service_channel: Option<u8>,
    /// Bitplane DMA fetch grant for this CCK.
    pub bitplane_dma_fetch_plane: Option<u8>,
    /// Copper is granted this slot (it may still be in WAIT and not fetch).
    pub copper_dma_slot_granted: bool,
    /// CPU chip-bus grant for this CCK in the current arbitration model.
    ///
    /// This is true on CPU/free slots unless another modeled chip-bus client
    /// (currently blitter nasty mode) takes the grant.
    pub cpu_chip_bus_granted: bool,
    /// Blitter chip-bus grant for this CCK.
    ///
    /// Minimal model: a busy blitter in nasty mode (BLTPRI) takes CPU/free
    /// slots when blitter DMA is enabled. The blitter operation itself is still
    /// executed synchronously elsewhere, so this only models bus arbitration.
    pub blitter_chip_bus_granted: bool,
    /// Blitter work-progress grant for this CCK.
    ///
    /// This is the coarse scheduler's "blitter may make progress now" signal.
    /// In the current model, progress is granted on Agnus CPU/free slots while
    /// blitter DMA is enabled and the blitter is busy.
    pub blitter_dma_progress_granted: bool,
    /// Paula audio DMA return-latency policy for this slot.
    pub paula_return_progress_policy: PaulaReturnProgressPolicy,
}

impl CckBusPlan {
    /// Resolve Paula return-latency progress for this CCK.
    ///
    /// `copper_used_chip_bus` is only relevant when
    /// [`PaulaReturnProgressPolicy::CopperFetchConditional`] is selected.
    #[must_use]
    pub fn paula_return_progress(self, copper_used_chip_bus: bool) -> bool {
        match self.paula_return_progress_policy {
            PaulaReturnProgressPolicy::Advance => true,
            PaulaReturnProgressPolicy::Stall => false,
            PaulaReturnProgressPolicy::CopperFetchConditional => !copper_used_chip_bus,
        }
    }
}

/// Maps ddfseq position (0-7) within an 8-CCK group to bitplane index.
/// From Minimig Verilog: plane = {~ddfseq[0], ~ddfseq[1], ~ddfseq[2]}.
/// None = free slot (available for copper/CPU).
pub const LOWRES_DDF_TO_PLANE: [Option<u8>; 8] = [
    None,    // 0: free
    Some(3), // 1: BPL4
    Some(5), // 2: BPL6
    Some(1), // 3: BPL2
    None,    // 4: free
    Some(2), // 5: BPL3
    Some(4), // 6: BPL5
    Some(0), // 7: BPL1 (triggers shift register load)
];

pub struct Agnus {
    pub vpos: u16,
    pub hpos: u16, // in CCKs

    // DMA Registers
    pub dmacon: u16,
    pub bplcon0: u16,
    pub bpl_pt: [u32; 6],
    pub ddfstrt: u16,
    pub ddfstop: u16,

    // Blitter Registers
    pub bltcon0: u16,
    pub bltcon1: u16,
    pub bltsize: u16,
    pub blitter_busy: bool,
    pub blitter_exec_pending: bool,
    pub blitter_ccks_remaining: u32,
    pub blt_apt: u32,
    pub blt_bpt: u32,
    pub blt_cpt: u32,
    pub blt_dpt: u32,
    pub blt_amod: i16,
    pub blt_bmod: i16,
    pub blt_cmod: i16,
    pub blt_dmod: i16,
    pub blt_adat: u16,
    pub blt_bdat: u16,
    pub blt_cdat: u16,
    pub blt_afwm: u16,
    pub blt_alwm: u16,

    // Display window
    pub diwstrt: u16,
    pub diwstop: u16,
    pub bpl1mod: i16,
    pub bpl2mod: i16,

    // Sprite pointers
    pub spr_pt: [u32; 8],

    // Disk pointer
    pub dsk_pt: u32,
}

impl Agnus {
    pub fn new() -> Self {
        Self {
            vpos: 0,
            hpos: 0,
            dmacon: 0,
            bplcon0: 0,
            bpl_pt: [0; 6],
            ddfstrt: 0,
            ddfstop: 0,
            bltcon0: 0,
            bltcon1: 0,
            bltsize: 0,
            blitter_busy: false,
            blitter_exec_pending: false,
            blitter_ccks_remaining: 0,
            blt_apt: 0,
            blt_bpt: 0,
            blt_cpt: 0,
            blt_dpt: 0,
            blt_amod: 0,
            blt_bmod: 0,
            blt_cmod: 0,
            blt_dmod: 0,
            blt_adat: 0,
            blt_bdat: 0,
            blt_cdat: 0,
            blt_afwm: 0xFFFF,
            blt_alwm: 0xFFFF,
            diwstrt: 0,
            diwstop: 0,
            bpl1mod: 0,
            bpl2mod: 0,
            spr_pt: [0; 8],
            dsk_pt: 0,
        }
    }

    pub fn num_bitplanes(&self) -> u8 {
        let bpl_bits = (self.bplcon0 >> 12) & 0x07;
        if bpl_bits > 6 { 6 } else { bpl_bits as u8 }
    }

    pub fn dma_enabled(&self, bit: u16) -> bool {
        (self.dmacon & 0x0200) != 0 && (self.dmacon & bit) != 0
    }

    /// `true` when a busy blitter is in nasty mode and may steal CPU/free slots.
    #[must_use]
    pub fn blitter_nasty_active(&self) -> bool {
        const DMACON_BLTEN: u16 = 0x0040;
        const DMACON_BLTPRI: u16 = 0x0400;

        self.blitter_busy && self.dma_enabled(DMACON_BLTEN) && (self.dmacon & DMACON_BLTPRI) != 0
    }

    /// Start a coarse per-CCK blitter completion timer.
    ///
    /// This preserves `blitter_busy` across CCKs so bus arbitration can react
    /// to the blitter before the existing synchronous blit implementation runs.
    pub fn start_blit(&mut self) {
        self.blitter_busy = true;
        self.blitter_exec_pending = true;
        self.blitter_ccks_remaining = self.coarse_blit_cck_budget();
    }

    /// Advance the coarse blitter scheduler by one CCK.
    ///
    /// Returns `true` when the pending blit should execute now.
    pub fn tick_blitter_scheduler(&mut self, progress_this_cck: bool) -> bool {
        if !self.blitter_exec_pending || !self.blitter_busy || !progress_this_cck {
            return false;
        }

        if self.blitter_ccks_remaining > 0 {
            self.blitter_ccks_remaining -= 1;
        }
        if self.blitter_ccks_remaining == 0 {
            self.blitter_exec_pending = false;
            return true;
        }
        false
    }

    fn coarse_blit_cck_budget(&self) -> u32 {
        // Coarse placeholder until per-slot blitter DMA is modeled.
        // Keep delays non-zero (to expose `blitter_busy` timing) but capped so
        // boot/test runtime does not explode on large blits.
        let height = u32::from((self.bltsize >> 6) & 0x03FF);
        let width_words = u32::from(self.bltsize & 0x003F);
        let height = if height == 0 { 1024 } else { height };
        let width_words = if width_words == 0 { 64 } else { width_words };
        let work_units = if (self.bltcon1 & 0x0001) != 0 {
            height // line mode: one plotted step per BLTSIZE row
        } else {
            height.saturating_mul(width_words)
        };
        work_units.clamp(1, 512)
    }

    /// Tick one CCK (8 crystal ticks).
    pub fn tick_cck(&mut self) {
        self.hpos += 1;
        if self.hpos >= PAL_CCKS_PER_LINE {
            self.hpos = 0;
            self.vpos += 1;
            if self.vpos >= PAL_LINES_PER_FRAME {
                self.vpos = 0;
            }
        }
    }

    /// Determine who owns the current CCK slot.
    pub fn current_slot(&self) -> SlotOwner {
        match self.hpos {
            // Fixed slots
            0x01..=0x03 | 0x1B => SlotOwner::Refresh,
            0x04..=0x06 => {
                if self.dma_enabled(0x0010) {
                    SlotOwner::Disk
                } else {
                    SlotOwner::Cpu
                }
            }
            0x07 => {
                if self.dma_enabled(0x0001) {
                    SlotOwner::Audio(0)
                } else {
                    SlotOwner::Cpu
                }
            }
            0x08 => {
                if self.dma_enabled(0x0002) {
                    SlotOwner::Audio(1)
                } else {
                    SlotOwner::Cpu
                }
            }
            0x09 => {
                if self.dma_enabled(0x0004) {
                    SlotOwner::Audio(2)
                } else {
                    SlotOwner::Cpu
                }
            }
            0x0A => {
                if self.dma_enabled(0x0008) {
                    SlotOwner::Audio(3)
                } else {
                    SlotOwner::Cpu
                }
            }
            0x0B..=0x1A => {
                if self.dma_enabled(0x0020) {
                    SlotOwner::Sprite(((self.hpos - 0x0B) / 2) as u8)
                } else {
                    SlotOwner::Cpu
                }
            }

            // Variable slots (Bitplane, Copper, CPU)
            0x1C..=0xE2 => {
                // Bitplane DMA: fetch window runs from DDFSTRT to DDFSTOP+7.
                // Within each 8-CCK group, planes are fetched in the Minimig
                // interleaved order (LOWRES_DDF_TO_PLANE), not sequentially.
                let num_bpl = self.num_bitplanes();
                if self.dma_enabled(0x0100)
                    && num_bpl > 0
                    && self.hpos >= self.ddfstrt
                    && self.hpos <= self.ddfstop + 7
                {
                    let pos_in_group = ((self.hpos - self.ddfstrt) % 8) as usize;
                    if let Some(plane) = LOWRES_DDF_TO_PLANE[pos_in_group] {
                        if plane < num_bpl {
                            return SlotOwner::Bitplane(plane);
                        }
                    }
                }

                // Copper
                if self.dma_enabled(0x0080) && (self.hpos % 2 == 0) {
                    return SlotOwner::Copper;
                }

                SlotOwner::Cpu
            }

            _ => SlotOwner::Cpu,
        }
    }

    /// Compute the machine-facing Agnus bus-arbitration plan for this CCK.
    pub fn cck_bus_plan(&self) -> CckBusPlan {
        let slot_owner = self.current_slot();
        let audio_dma_service_channel = match slot_owner {
            SlotOwner::Audio(channel) => Some(channel),
            _ => None,
        };
        let bitplane_dma_fetch_plane = match slot_owner {
            SlotOwner::Bitplane(plane) => Some(plane),
            _ => None,
        };
        let copper_dma_slot_granted = matches!(slot_owner, SlotOwner::Copper);
        let blitter_dma_progress_granted =
            matches!(slot_owner, SlotOwner::Cpu) && self.blitter_busy && self.dma_enabled(0x0040);
        let blitter_nasty_active = self.blitter_nasty_active();
        let blitter_chip_bus_granted = blitter_dma_progress_granted && blitter_nasty_active;
        let cpu_chip_bus_granted =
            matches!(slot_owner, SlotOwner::Cpu) && !blitter_chip_bus_granted;
        let paula_return_progress_policy = match slot_owner {
            SlotOwner::Refresh
            | SlotOwner::Disk
            | SlotOwner::Sprite(_)
            | SlotOwner::Bitplane(_) => PaulaReturnProgressPolicy::Stall,
            SlotOwner::Copper => PaulaReturnProgressPolicy::CopperFetchConditional,
            SlotOwner::Cpu | SlotOwner::Audio(_) => PaulaReturnProgressPolicy::Advance,
        };
        CckBusPlan {
            slot_owner,
            audio_dma_service_channel,
            bitplane_dma_fetch_plane,
            copper_dma_slot_granted,
            cpu_chip_bus_granted,
            blitter_chip_bus_granted,
            blitter_dma_progress_granted,
            paula_return_progress_policy,
        }
    }
}

impl Default for Agnus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DMACON_DMAEN: u16 = 0x0200;
    const DMACON_AUD0EN: u16 = 0x0001;
    const DMACON_BLTEN: u16 = 0x0040;
    const DMACON_COPEN: u16 = 0x0080;
    const DMACON_BPLEN: u16 = 0x0100;
    const DMACON_BLTPRI: u16 = 0x0400;

    #[test]
    fn cck_bus_plan_reports_audio_service_grant() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x07;
        agnus.dmacon = DMACON_DMAEN | DMACON_AUD0EN;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Audio(0));
        assert_eq!(plan.audio_dma_service_channel, Some(0));
        assert_eq!(plan.bitplane_dma_fetch_plane, None);
        assert!(!plan.copper_dma_slot_granted);
        assert!(!plan.cpu_chip_bus_granted);
        assert!(!plan.blitter_chip_bus_granted);
        assert!(!plan.blitter_dma_progress_granted);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Advance
        );
    }

    #[test]
    fn cck_bus_plan_reports_copper_grant_and_conditional_return_policy() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x1C; // even, variable-slot region
        agnus.dmacon = DMACON_DMAEN | DMACON_COPEN;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Copper);
        assert_eq!(plan.audio_dma_service_channel, None);
        assert_eq!(plan.bitplane_dma_fetch_plane, None);
        assert!(plan.copper_dma_slot_granted);
        assert!(!plan.cpu_chip_bus_granted);
        assert!(!plan.blitter_chip_bus_granted);
        assert!(!plan.blitter_dma_progress_granted);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::CopperFetchConditional
        );
    }

    #[test]
    fn cck_bus_plan_reports_bitplane_grant_and_stall_policy() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x23; // ddfstrt + 7 => BPL1 slot in lowres fetch group
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN | DMACON_COPEN;
        agnus.bplcon0 = 1 << 12; // 1 bitplane enabled
        agnus.ddfstrt = 0x1C;
        agnus.ddfstop = 0x1C;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Bitplane(0));
        assert_eq!(plan.audio_dma_service_channel, None);
        assert_eq!(plan.bitplane_dma_fetch_plane, Some(0));
        assert!(!plan.copper_dma_slot_granted);
        assert!(!plan.cpu_chip_bus_granted);
        assert!(!plan.blitter_chip_bus_granted);
        assert!(!plan.blitter_dma_progress_granted);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Stall
        );
    }

    #[test]
    fn cck_bus_plan_reports_cpu_chip_bus_grant_on_free_slot() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x00; // free slot outside fixed/variable DMA windows
        agnus.dmacon = DMACON_DMAEN | DMACON_COPEN | DMACON_BPLEN;
        agnus.bplcon0 = 1 << 12;
        agnus.ddfstrt = 0x1C;
        agnus.ddfstop = 0xD8;
        agnus.blitter_busy = false;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Cpu);
        assert_eq!(plan.audio_dma_service_channel, None);
        assert_eq!(plan.bitplane_dma_fetch_plane, None);
        assert!(!plan.copper_dma_slot_granted);
        assert!(plan.cpu_chip_bus_granted);
        assert!(
            !plan.blitter_chip_bus_granted,
            "blitter per-CCK slot grants are not modeled yet"
        );
        assert!(!plan.blitter_dma_progress_granted);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Advance
        );
    }

    #[test]
    fn cck_bus_plan_reports_blitter_nasty_grant_on_cpu_slot() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x00; // free slot
        agnus.blitter_busy = true;
        agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN | DMACON_BLTPRI;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Cpu);
        assert!(
            !plan.cpu_chip_bus_granted,
            "CPU should lose free slot to blitter in nasty mode"
        );
        assert!(
            plan.blitter_chip_bus_granted,
            "blitter should claim free slot in nasty mode"
        );
        assert!(plan.blitter_dma_progress_granted);
    }

    #[test]
    fn cck_bus_plan_blitter_busy_without_nasty_does_not_take_cpu_slot() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x00; // free slot
        agnus.blitter_busy = true;
        agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN; // BLTPRI clear

        let plan = agnus.cck_bus_plan();
        assert!(plan.cpu_chip_bus_granted);
        assert!(!plan.blitter_chip_bus_granted);
        assert!(
            plan.blitter_dma_progress_granted,
            "non-nasty blitter should still progress on free slots"
        );
    }

    #[test]
    fn blitter_scheduler_counts_down_and_requires_progress() {
        let mut agnus = Agnus::new();
        agnus.bltsize = (1 << 6) | 2; // height=1, width=2 => budget=2
        agnus.start_blit();

        assert!(agnus.blitter_busy);
        assert!(agnus.blitter_exec_pending);
        assert_eq!(agnus.blitter_ccks_remaining, 2);

        assert!(
            !agnus.tick_blitter_scheduler(false),
            "no progress when bus grant is withheld"
        );
        assert_eq!(agnus.blitter_ccks_remaining, 2);

        assert!(!agnus.tick_blitter_scheduler(true));
        assert_eq!(agnus.blitter_ccks_remaining, 1);

        assert!(agnus.tick_blitter_scheduler(true));
        assert!(!agnus.blitter_exec_pending);
        assert_eq!(agnus.blitter_ccks_remaining, 0);
    }
}
