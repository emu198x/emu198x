//! Commodore Gary address decoder for Amiga systems.
//!
//! Gary is the address decoder present in every Amiga model. It takes a
//! 24-bit address and determines which chip or memory region should handle
//! the bus cycle. On A500/A1000/A2000 this is a discrete PAL or custom
//! gate array; on later models the decode logic moves into Gayle or
//! Fat Gary, but the fundamental address map is the same.
//!
//! This crate centralises the address decode that was previously an
//! inline if-chain in `machine-amiga`'s `poll_cycle()`.

// ---------------------------------------------------------------------------
// Chip select output
// ---------------------------------------------------------------------------

/// Which chip or memory region a 24-bit address maps to.
///
/// The variants are ordered by decode priority — higher-priority chip
/// selects shadow lower-priority ranges when both would match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipSelect {
    /// CIA-A: $BFExxx (accent on low byte, D0-D7).
    CiaA,
    /// CIA-B: $BFDxxx (accent on high byte, D8-D15).
    CiaB,
    /// Custom chip registers: $DFFxxx (Agnus, Denise, Paula).
    Custom,
    /// SDMAC 390537 SCSI controller: $DD0000-$DDFFFF (A3000).
    Dmac,
    /// Motherboard resource registers: $DE0000-$DEFFFF (A3000/A4000).
    ResourceRegisters,
    /// Gayle gate array: $D80000-$DFFFFF (A600/A1200).
    Gayle,
    /// PCMCIA common memory: $600000-$9FFFFF (A600/A1200, when card present).
    PcmciaCommon,
    /// PCMCIA attribute, I/O, and reset: $A00000-$A5FFFF (A600/A1200, when card present).
    PcmciaAttr,
    /// Battery-backed clock (RTC): $DC0000-$DC003F (A2000/A3000/A4000, and
    /// via Gayle on A500+/A600/A1200).
    Rtc,
    /// Chip RAM: $000000-$1FFFFF (up to 2 MB, DMA-accessible).
    ChipRam,
    /// Slow RAM (ranger/trapdoor): $C00000-$D7FFFF.
    SlowRam,
    /// Autoconfig / Zorro expansion: $E80000-$EFFFFF.
    Autoconfig,
    /// Kickstart ROM: $F80000-$FFFFFF.
    Rom,
    /// Address not claimed by any chip select.
    Unmapped,
}

// ---------------------------------------------------------------------------
// Gary state
// ---------------------------------------------------------------------------

/// Gary address decoder state.
///
/// Configuration flags control which model-specific chip selects are
/// active. Every Amiga has CIA-A, CIA-B, custom registers, chip RAM,
/// and ROM. Model-dependent peripherals (DMAC, resource registers,
/// Gayle, slow RAM) are enabled at construction time.
#[derive(Debug, Clone)]
pub struct Gary {
    slow_ram_present: bool,
    gayle_present: bool,
    pcmcia_present: bool,
    dmac_present: bool,
    resource_regs_present: bool,
    rtc_present: bool,
}

impl Gary {
    /// Create a Gary with no optional peripherals (minimal A1000 config).
    #[must_use]
    pub fn new() -> Self {
        Self {
            slow_ram_present: false,
            gayle_present: false,
            pcmcia_present: false,
            dmac_present: false,
            resource_regs_present: false,
            rtc_present: false,
        }
    }

    /// Enable or disable the slow RAM chip select ($C00000-$D7FFFF).
    pub fn set_slow_ram_present(&mut self, present: bool) {
        self.slow_ram_present = present;
    }

    /// Enable or disable the Gayle chip select ($D80000-$DFFFFF).
    pub fn set_gayle_present(&mut self, present: bool) {
        self.gayle_present = present;
    }

    /// Enable or disable the PCMCIA chip selects ($600000-$9FFFFF, $A00000-$A5FFFF).
    pub fn set_pcmcia_present(&mut self, present: bool) {
        self.pcmcia_present = present;
    }

    /// Enable or disable the DMAC chip select ($DD0000-$DDFFFF).
    pub fn set_dmac_present(&mut self, present: bool) {
        self.dmac_present = present;
    }

    /// Enable or disable the resource register chip select ($DE0000-$DEFFFF).
    pub fn set_resource_regs_present(&mut self, present: bool) {
        self.resource_regs_present = present;
    }

    /// Enable or disable the battery-backed clock chip select ($DC0000-$DC003F).
    pub fn set_rtc_present(&mut self, present: bool) {
        self.rtc_present = present;
    }

    /// True when slow RAM is enabled.
    #[must_use]
    pub const fn slow_ram_present(&self) -> bool {
        self.slow_ram_present
    }

    /// True when Gayle is enabled.
    #[must_use]
    pub const fn gayle_present(&self) -> bool {
        self.gayle_present
    }

    /// True when DMAC is enabled.
    #[must_use]
    pub const fn dmac_present(&self) -> bool {
        self.dmac_present
    }

    /// True when resource registers are enabled.
    #[must_use]
    pub const fn resource_regs_present(&self) -> bool {
        self.resource_regs_present
    }

    /// Decode a 24-bit address to a chip select.
    ///
    /// The address is masked to 24 bits internally. Pre-24-bit checks
    /// (IACK, fast RAM, Fat Gary 24-bit gate) are the caller's
    /// responsibility.
    ///
    /// Decode priority matches the real hardware and the existing
    /// `poll_cycle()` if-chain:
    ///
    /// 1. CIA-A ($BFExxx)
    /// 2. CIA-B ($BFDxxx)
    /// 3. Custom registers ($DFFxxx)
    /// 4. DMAC ($DD0000-$DDFFFF, when present)
    /// 5. Resource registers ($DE0000-$DEFFFF, when present)
    /// 6. Gayle ($D80000-$DFFFFF, when present)
    /// 7. RTC ($DC0000-$DC003F, when present)
    /// 8. Chip RAM ($000000-$1FFFFF)
    /// 9. Slow RAM ($C00000-$D7FFFF, when present)
    /// 10. Autoconfig ($E80000-$EFFFFF)
    /// 11. ROM ($F80000-$FFFFFF)
    /// 12. Unmapped
    #[must_use]
    pub const fn decode(&self, addr: u32) -> ChipSelect {
        let addr = addr & 0xFF_FFFF;

        // CIA-A: $BFExxx
        if (addr & 0xFFF000) == 0xBFE000 {
            return ChipSelect::CiaA;
        }

        // CIA-B: $BFDxxx
        if (addr & 0xFFF000) == 0xBFD000 {
            return ChipSelect::CiaB;
        }

        // Custom chip registers: $DFFxxx
        if (addr & 0xFFF000) == 0xDFF000 {
            return ChipSelect::Custom;
        }

        // DMAC: $DD0000-$DDFFFF (A3000 only)
        if self.dmac_present && addr >= 0xDD_0000 && addr < 0xDE_0000 {
            return ChipSelect::Dmac;
        }

        // Resource registers: $DE0000-$DEFFFF (A3000/A4000)
        if self.resource_regs_present && addr >= 0xDE_0000 && addr < 0xDF_0000 {
            return ChipSelect::ResourceRegisters;
        }

        // Gayle: $D80000-$DFFFFF (A600/A1200)
        // Gayle covers the full $D80000-$DFFFFF range, overlapping slow RAM
        // and DMAC/resource-register space. On A600/A1200 there is no DMAC
        // or resource register block, so this is safe.
        if self.gayle_present && addr >= 0xD8_0000 && addr < 0xE0_0000 {
            return ChipSelect::Gayle;
        }

        // Battery-backed clock: $DC0000-$DC003F
        if self.rtc_present && addr >= 0xDC_0000 && addr <= 0xDC_003F {
            return ChipSelect::Rtc;
        }

        // Chip RAM: $000000-$1FFFFF
        if addr < 0x20_0000 {
            return ChipSelect::ChipRam;
        }

        // PCMCIA common memory: $600000-$9FFFFF (A600/A1200 with card)
        if self.pcmcia_present && addr >= 0x60_0000 && addr < 0xA0_0000 {
            return ChipSelect::PcmciaCommon;
        }

        // PCMCIA attribute/IO/reset: $A00000-$A5FFFF (A600/A1200 with card)
        if self.pcmcia_present && addr >= 0xA0_0000 && addr < 0xA6_0000 {
            return ChipSelect::PcmciaAttr;
        }

        // Slow RAM: $C00000-$D7FFFF
        // On models without Gayle/DMAC/resource-regs, slow RAM extends
        // further ($C00000-$DFFFFF), but the CIA and custom checks above
        // already handle $BFxxxx and $DFFxxx. The remaining $D80000-$DFFFFF
        // range without Gayle is just more slow-RAM-capable address space.
        if self.slow_ram_present {
            // Without Gayle, DMAC, or resource regs, the entire $C00000-$DFFFFF
            // range (minus CIAs and custom) is slow-RAM-addressable.
            let slow_end = if self.gayle_present || self.dmac_present || self.resource_regs_present
            {
                0xD8_0000u32
            } else {
                0xE0_0000u32
            };
            if addr >= 0xC0_0000 && addr < slow_end {
                return ChipSelect::SlowRam;
            }
        }

        // Autoconfig / Zorro: $E80000-$EFFFFF
        if addr >= 0xE8_0000 && addr < 0xF0_0000 {
            return ChipSelect::Autoconfig;
        }

        // Kickstart ROM: $F80000-$FFFFFF
        if addr >= 0xF8_0000 {
            return ChipSelect::Rom;
        }

        ChipSelect::Unmapped
    }
}

impl Default for Gary {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn gary_a500_with_slow_ram() -> Gary {
        let mut gary = Gary::new();
        gary.set_slow_ram_present(true);
        gary
    }

    fn gary_a600() -> Gary {
        let mut gary = Gary::new();
        gary.set_gayle_present(true);
        gary.set_slow_ram_present(true);
        gary
    }

    fn gary_a3000() -> Gary {
        let mut gary = Gary::new();
        gary.set_dmac_present(true);
        gary.set_resource_regs_present(true);
        gary
    }

    // -- Basic chip selects (every model) -----------------------------------

    #[test]
    fn cia_a_decode() {
        let gary = Gary::new();
        assert_eq!(gary.decode(0xBFE001), ChipSelect::CiaA);
        assert_eq!(gary.decode(0xBFEF01), ChipSelect::CiaA);
    }

    #[test]
    fn cia_b_decode() {
        let gary = Gary::new();
        assert_eq!(gary.decode(0xBFD000), ChipSelect::CiaB);
        assert_eq!(gary.decode(0xBFDF00), ChipSelect::CiaB);
    }

    #[test]
    fn custom_registers_decode() {
        let gary = Gary::new();
        assert_eq!(gary.decode(0xDFF000), ChipSelect::Custom);
        assert_eq!(gary.decode(0xDFF1FE), ChipSelect::Custom);
    }

    #[test]
    fn chip_ram_decode() {
        let gary = Gary::new();
        assert_eq!(gary.decode(0x000000), ChipSelect::ChipRam);
        assert_eq!(gary.decode(0x080000), ChipSelect::ChipRam);
        assert_eq!(gary.decode(0x1FFFFF), ChipSelect::ChipRam);
    }

    #[test]
    fn rom_decode() {
        let gary = Gary::new();
        assert_eq!(gary.decode(0xF80000), ChipSelect::Rom);
        assert_eq!(gary.decode(0xFC0000), ChipSelect::Rom);
        assert_eq!(gary.decode(0xFFFFFF), ChipSelect::Rom);
    }

    #[test]
    fn autoconfig_decode() {
        let gary = Gary::new();
        assert_eq!(gary.decode(0xE80000), ChipSelect::Autoconfig);
        assert_eq!(gary.decode(0xEFFFFF), ChipSelect::Autoconfig);
    }

    // -- Slow RAM -----------------------------------------------------------

    #[test]
    fn slow_ram_decode_a500() {
        let gary = gary_a500_with_slow_ram();
        assert_eq!(gary.decode(0xC00000), ChipSelect::SlowRam);
        assert_eq!(gary.decode(0xD7FFFF), ChipSelect::SlowRam);
        // Without Gayle/DMAC/resource-regs, $D80000-$DEFFFF is also
        // slow-RAM-addressable (minus CIAs and custom which are caught above).
        assert_eq!(gary.decode(0xD80000), ChipSelect::SlowRam);
        assert_eq!(gary.decode(0xDEFFFF), ChipSelect::SlowRam);
    }

    #[test]
    fn slow_ram_not_present_returns_unmapped() {
        let gary = Gary::new();
        assert_eq!(gary.decode(0xC00000), ChipSelect::Unmapped);
        assert_eq!(gary.decode(0xD7FFFF), ChipSelect::Unmapped);
    }

    #[test]
    fn slow_ram_truncated_when_gayle_present() {
        let gary = gary_a600();
        assert_eq!(gary.decode(0xC00000), ChipSelect::SlowRam);
        assert_eq!(gary.decode(0xD7FFFF), ChipSelect::SlowRam);
        // $D80000+ is Gayle, not slow RAM
        assert_eq!(gary.decode(0xD80000), ChipSelect::Gayle);
    }

    // -- DMAC and resource registers (A3000) --------------------------------

    #[test]
    fn dmac_decode() {
        let gary = gary_a3000();
        assert_eq!(gary.decode(0xDD0000), ChipSelect::Dmac);
        assert_eq!(gary.decode(0xDDFFFF), ChipSelect::Dmac);
    }

    #[test]
    fn resource_registers_decode() {
        let gary = gary_a3000();
        assert_eq!(gary.decode(0xDE0000), ChipSelect::ResourceRegisters);
        assert_eq!(gary.decode(0xDEFFFF), ChipSelect::ResourceRegisters);
    }

    #[test]
    fn dmac_not_present_returns_unmapped() {
        let gary = Gary::new();
        assert_eq!(gary.decode(0xDD0000), ChipSelect::Unmapped);
    }

    #[test]
    fn resource_regs_not_present_returns_unmapped() {
        let gary = Gary::new();
        assert_eq!(gary.decode(0xDE0000), ChipSelect::Unmapped);
    }

    // -- Gayle (A600/A1200) -------------------------------------------------

    #[test]
    fn gayle_decode() {
        let gary = gary_a600();
        assert_eq!(gary.decode(0xD80000), ChipSelect::Gayle);
        assert_eq!(gary.decode(0xDEFFFF), ChipSelect::Gayle);
        // $DFFxxx is shadowed by Custom, not Gayle
        assert_eq!(gary.decode(0xDFF000), ChipSelect::Custom);
    }

    #[test]
    fn gayle_not_present_returns_unmapped_or_slow_ram() {
        let gary = Gary::new();
        assert_eq!(gary.decode(0xD80000), ChipSelect::Unmapped);

        let gary = gary_a500_with_slow_ram();
        assert_eq!(gary.decode(0xD80000), ChipSelect::SlowRam);
    }

    // -- Priority / shadowing -----------------------------------------------

    #[test]
    fn cia_shadows_slow_ram_range() {
        // CIA-A is at $BFExxx which is within the $B00000-$DFFFFF "slow RAM"
        // area. CIAs must win.
        let gary = gary_a500_with_slow_ram();
        assert_eq!(gary.decode(0xBFE001), ChipSelect::CiaA);
        assert_eq!(gary.decode(0xBFD000), ChipSelect::CiaB);
    }

    #[test]
    fn custom_shadows_gayle_range() {
        // Custom registers at $DFFxxx are within Gayle's $D80000-$DFFFFF.
        // Custom must win.
        let gary = gary_a600();
        assert_eq!(gary.decode(0xDFF000), ChipSelect::Custom);
        assert_eq!(gary.decode(0xDFF1FE), ChipSelect::Custom);
    }

    #[test]
    fn dmac_shadows_gayle_range() {
        // Hypothetical config with both DMAC and Gayle (shouldn't happen
        // in practice, but tests priority).
        let mut gary = Gary::new();
        gary.set_gayle_present(true);
        gary.set_dmac_present(true);
        assert_eq!(gary.decode(0xDD0000), ChipSelect::Dmac);
    }

    // -- Unmapped gaps ------------------------------------------------------

    #[test]
    fn expansion_gap_is_unmapped() {
        let gary = Gary::new();
        // $200000-$9FFFFF is expansion (unmapped on stock boards)
        assert_eq!(gary.decode(0x200000), ChipSelect::Unmapped);
        assert_eq!(gary.decode(0x500000), ChipSelect::Unmapped);
        assert_eq!(gary.decode(0x9FFFFF), ChipSelect::Unmapped);
    }

    #[test]
    fn ranger_gap_is_unmapped() {
        let gary = Gary::new();
        // $A00000-$BEFFFF without slow RAM
        assert_eq!(gary.decode(0xA00000), ChipSelect::Unmapped);
        assert_eq!(gary.decode(0xB00000), ChipSelect::Unmapped);
    }

    #[test]
    fn diagnostics_gap_is_unmapped() {
        let gary = Gary::new();
        // $E00000-$E7FFFF and $F00000-$F7FFFF
        assert_eq!(gary.decode(0xE00000), ChipSelect::Unmapped);
        assert_eq!(gary.decode(0xE7FFFF), ChipSelect::Unmapped);
        assert_eq!(gary.decode(0xF00000), ChipSelect::Unmapped);
        assert_eq!(gary.decode(0xF7FFFF), ChipSelect::Unmapped);
    }

    // -- 24-bit masking -----------------------------------------------------

    #[test]
    fn decode_masks_to_24_bits() {
        let gary = Gary::new();
        // $01000000 masks to $000000 → ChipRam
        assert_eq!(gary.decode(0x0100_0000), ChipSelect::ChipRam);
        // $01BFE001 masks to $BFE001 → CiaA
        assert_eq!(gary.decode(0x01BF_E001), ChipSelect::CiaA);
    }

    // -- Truth table: comprehensive address matrix --------------------------

    #[test]
    fn truth_table_a500_with_slow_ram() {
        let gary = gary_a500_with_slow_ram();

        let cases: &[(u32, ChipSelect)] = &[
            // Chip RAM
            (0x000000, ChipSelect::ChipRam),
            (0x080000, ChipSelect::ChipRam),
            (0x1FFFFF, ChipSelect::ChipRam),
            // Expansion gap (unmapped on A500)
            (0x200000, ChipSelect::Unmapped),
            (0x500000, ChipSelect::Unmapped),
            (0x9FFFFF, ChipSelect::Unmapped),
            // Ranger gap
            (0xA00000, ChipSelect::Unmapped),
            (0xBF0000, ChipSelect::Unmapped),
            // CIAs
            (0xBFD000, ChipSelect::CiaB),
            (0xBFDF00, ChipSelect::CiaB),
            (0xBFE001, ChipSelect::CiaA),
            (0xBFEF01, ChipSelect::CiaA),
            // Slow RAM
            (0xC00000, ChipSelect::SlowRam),
            (0xD7FFFF, ChipSelect::SlowRam),
            (0xD80000, ChipSelect::SlowRam),
            (0xDCFFFF, ChipSelect::SlowRam),
            // Custom (shadows slow RAM range)
            (0xDFF000, ChipSelect::Custom),
            (0xDFF1FE, ChipSelect::Custom),
            // Diagnostics gap
            (0xE00000, ChipSelect::Unmapped),
            (0xE7FFFF, ChipSelect::Unmapped),
            // Autoconfig
            (0xE80000, ChipSelect::Autoconfig),
            (0xEFFFFF, ChipSelect::Autoconfig),
            // Diagnostics gap
            (0xF00000, ChipSelect::Unmapped),
            (0xF7FFFF, ChipSelect::Unmapped),
            // ROM
            (0xF80000, ChipSelect::Rom),
            (0xFFFFFF, ChipSelect::Rom),
        ];

        for &(addr, expected) in cases {
            assert_eq!(
                gary.decode(addr),
                expected,
                "A500 decode mismatch at ${addr:06X}: expected {expected:?}, got {:?}",
                gary.decode(addr),
            );
        }
    }

    #[test]
    fn truth_table_a3000() {
        let gary = gary_a3000();

        let cases: &[(u32, ChipSelect)] = &[
            (0x000000, ChipSelect::ChipRam),
            (0x1FFFFF, ChipSelect::ChipRam),
            (0x200000, ChipSelect::Unmapped),
            (0xBFD000, ChipSelect::CiaB),
            (0xBFE001, ChipSelect::CiaA),
            // No slow RAM on A3000 by default
            (0xC00000, ChipSelect::Unmapped),
            // DMAC
            (0xDD0000, ChipSelect::Dmac),
            (0xDDFFFF, ChipSelect::Dmac),
            // Resource registers
            (0xDE0000, ChipSelect::ResourceRegisters),
            (0xDEFFFF, ChipSelect::ResourceRegisters),
            // Custom
            (0xDFF000, ChipSelect::Custom),
            // Autoconfig
            (0xE80000, ChipSelect::Autoconfig),
            // ROM
            (0xF80000, ChipSelect::Rom),
        ];

        for &(addr, expected) in cases {
            assert_eq!(
                gary.decode(addr),
                expected,
                "A3000 decode mismatch at ${addr:06X}: expected {expected:?}, got {:?}",
                gary.decode(addr),
            );
        }
    }

    #[test]
    fn truth_table_a1200() {
        let mut gary = Gary::new();
        gary.set_gayle_present(true);
        gary.set_slow_ram_present(true);

        let cases: &[(u32, ChipSelect)] = &[
            (0x000000, ChipSelect::ChipRam),
            (0x1FFFFF, ChipSelect::ChipRam),
            (0x200000, ChipSelect::Unmapped),
            (0xBFD000, ChipSelect::CiaB),
            (0xBFE001, ChipSelect::CiaA),
            // Slow RAM (truncated at $D80000 by Gayle)
            (0xC00000, ChipSelect::SlowRam),
            (0xD7FFFF, ChipSelect::SlowRam),
            // Gayle
            (0xD80000, ChipSelect::Gayle),
            (0xDEFFFF, ChipSelect::Gayle),
            // Custom (shadows Gayle)
            (0xDFF000, ChipSelect::Custom),
            // Autoconfig
            (0xE80000, ChipSelect::Autoconfig),
            // ROM
            (0xF80000, ChipSelect::Rom),
        ];

        for &(addr, expected) in cases {
            assert_eq!(
                gary.decode(addr),
                expected,
                "A1200 decode mismatch at ${addr:06X}: expected {expected:?}, got {:?}",
                gary.decode(addr),
            );
        }
    }

    // -- PCMCIA (A600/A1200 with card) --------------------------------------

    fn gary_a600_with_pcmcia() -> Gary {
        let mut gary = Gary::new();
        gary.set_gayle_present(true);
        gary.set_slow_ram_present(true);
        gary.set_pcmcia_present(true);
        gary
    }

    #[test]
    fn pcmcia_common_decode() {
        let gary = gary_a600_with_pcmcia();
        assert_eq!(gary.decode(0x600000), ChipSelect::PcmciaCommon);
        assert_eq!(gary.decode(0x700000), ChipSelect::PcmciaCommon);
        assert_eq!(gary.decode(0x9FFFFF), ChipSelect::PcmciaCommon);
    }

    #[test]
    fn pcmcia_attr_decode() {
        let gary = gary_a600_with_pcmcia();
        assert_eq!(gary.decode(0xA00000), ChipSelect::PcmciaAttr);
        assert_eq!(gary.decode(0xA20300), ChipSelect::PcmciaAttr);
        assert_eq!(gary.decode(0xA40000), ChipSelect::PcmciaAttr);
        assert_eq!(gary.decode(0xA5FFFF), ChipSelect::PcmciaAttr);
    }

    #[test]
    fn pcmcia_not_present_returns_unmapped() {
        let gary = gary_a600();
        assert_eq!(gary.decode(0x600000), ChipSelect::Unmapped);
        assert_eq!(gary.decode(0xA00000), ChipSelect::Unmapped);
    }

    #[test]
    fn pcmcia_above_attr_is_unmapped() {
        let gary = gary_a600_with_pcmcia();
        // $A60000+ is not PCMCIA
        assert_eq!(gary.decode(0xA60000), ChipSelect::Unmapped);
    }

    #[test]
    fn truth_table_a1200_with_pcmcia() {
        let gary = gary_a600_with_pcmcia();

        let cases: &[(u32, ChipSelect)] = &[
            (0x000000, ChipSelect::ChipRam),
            (0x1FFFFF, ChipSelect::ChipRam),
            (0x200000, ChipSelect::Unmapped),
            (0x5FFFFF, ChipSelect::Unmapped),
            // PCMCIA common
            (0x600000, ChipSelect::PcmciaCommon),
            (0x9FFFFF, ChipSelect::PcmciaCommon),
            // PCMCIA attribute/IO/reset
            (0xA00000, ChipSelect::PcmciaAttr),
            (0xA5FFFF, ChipSelect::PcmciaAttr),
            // Above PCMCIA attr → unmapped
            (0xA60000, ChipSelect::Unmapped),
            // CIAs
            (0xBFD000, ChipSelect::CiaB),
            (0xBFE001, ChipSelect::CiaA),
            // Slow RAM
            (0xC00000, ChipSelect::SlowRam),
            (0xD7FFFF, ChipSelect::SlowRam),
            // Gayle
            (0xD80000, ChipSelect::Gayle),
            // Custom (shadows Gayle)
            (0xDFF000, ChipSelect::Custom),
            // ROM
            (0xF80000, ChipSelect::Rom),
        ];

        for &(addr, expected) in cases {
            assert_eq!(
                gary.decode(addr),
                expected,
                "A1200+PCMCIA decode mismatch at ${addr:06X}: expected {expected:?}, got {:?}",
                gary.decode(addr),
            );
        }
    }
}
