//! Commodore 390537 SDMAC — SCSI DMA controller for the Amiga 3000.
//!
//! The SDMAC sits at `$DD0000–$DDFFFF` and provides a WD33C93(A) SCSI
//! bus interface controller plus DMA transfer logic. Fat Gary generates
//! the chip select.
//!
//! This is a stub implementation: enough for KS 3.x `scsi.device` to
//! initialise, scan all 7 SCSI IDs, time out on each, and proceed to
//! floppy boot. No actual SCSI transfers are supported.

// ---------------------------------------------------------------------------
// WD33C93 registers (indirect access via SASR/SCMD)
// ---------------------------------------------------------------------------

/// WD33C93 register addresses (selected by writing to SASR).
#[allow(dead_code)]
mod wd_reg {
    pub const OWN_ID: u8 = 0x00;
    pub const CONTROL: u8 = 0x01;
    pub const TIMEOUT_PERIOD: u8 = 0x02;
    pub const DESTINATION_ID: u8 = 0x15;
    pub const SOURCE_ID: u8 = 0x16;
    pub const SCSI_STATUS: u8 = 0x17;
    pub const COMMAND: u8 = 0x18;
    pub const DATA: u8 = 0x19;
    pub const AUXILIARY_STATUS: u8 = 0x1F;
}

/// WD33C93 command codes (written to the COMMAND register).
mod wd_cmd {
    pub const RESET: u8 = 0x00;
    pub const ABORT: u8 = 0x01;
    pub const SEL_ATN: u8 = 0x06;
    pub const SEL: u8 = 0x07;
    pub const SEL_ATN_XFER: u8 = 0x08;
    pub const SEL_XFER: u8 = 0x09;
}

/// WD33C93 Command Status Register (CSR) values.
mod wd_csr {
    /// Reset completed (no advanced features).
    pub const RESET: u8 = 0x00;
    /// Reset completed (advanced features enabled).
    pub const RESET_AF: u8 = 0x01;
    /// Selection timed out — no target responded.
    pub const TIMEOUT: u8 = 0x42;
}

/// WD33C93 Auxiliary Status Register (ASR) bits.
#[allow(dead_code)]
mod wd_asr {
    /// Interrupt pending.
    pub const INT: u8 = 0x80;
    /// Busy (Level II command executing).
    pub const BSY: u8 = 0x20;
    /// Command in progress.
    pub const CIP: u8 = 0x10;
}

// ---------------------------------------------------------------------------
// SDMAC (Super DMAC) registers
// ---------------------------------------------------------------------------

/// SDMAC CNTR (control register) bits.
mod cntr_bits {
    /// Peripheral reset — drives WD33C93 /IOW and /IOR low.
    pub const PREST: u8 = 0x10;
    /// Interrupt enable.
    pub const INTEN: u8 = 0x04;
}

/// SDMAC ISTR (interrupt status register) bits.
mod istr_bits {
    /// Any interrupt source active (follow bit).
    pub const INT_F: u8 = 0x80;
    /// SCSI peripheral interrupt (from WD33C93 INT pin).
    pub const INTS: u8 = 0x40;
    /// Interrupt pending (only set when CNTR.INTEN = 1).
    pub const INT_P: u8 = 0x10;
    /// FIFO empty flag.
    pub const FE_FLG: u8 = 0x01;
}

// ---------------------------------------------------------------------------
// SDMAC register byte offsets within the $DD0000–$DD00FF block.
//
// The 68030 accesses these as word or longword cycles. The bus wrapper
// presents individual byte addresses, so we match on `addr & 0xFF`.
// ---------------------------------------------------------------------------

/// Word offset for DAWR (DACK width, write-only).
const REG_DAWR: u8 = 0x02;
/// Word offset for WTC high word.
const REG_WTC_HI: u8 = 0x04;
/// Word offset for WTC low word.
const REG_WTC_LO: u8 = 0x06;
/// Word offset for CNTR (control, read/write).
const REG_CNTR: u8 = 0x0A;
/// Word offset for ACR high word.
const REG_ACR_HI: u8 = 0x0C;
/// Word offset for ACR low word.
const REG_ACR_LO: u8 = 0x0E;
/// Word offset for ST_DMA (start DMA, write strobe).
const REG_ST_DMA: u8 = 0x12;
/// Word offset for FLUSH (flush FIFO, write strobe).
const REG_FLUSH: u8 = 0x16;
/// Word offset for CINT (clear interrupts, write strobe).
const REG_CINT: u8 = 0x1A;
/// Word offset for ISTR (interrupt status, read-only).
const REG_ISTR: u8 = 0x1E;
/// Word offset for SP_DMA (stop DMA, write strobe).
const REG_SP_DMA: u8 = 0x3E;
/// Word offset for SASR (WD register select write / ASR read).
const REG_SASR: u8 = 0x40;
/// Word offset for SCMD (WD register data read/write).
const REG_SCMD: u8 = 0x42;
/// Word offset for SASR alternate port.
const REG_SASR_ALT: u8 = 0x48;
/// Word offset for SCMD alternate port.
const REG_SCMD_ALT: u8 = 0x4A;

// ---------------------------------------------------------------------------
// WD33C93 state
// ---------------------------------------------------------------------------

/// Minimal WD33C93 SCSI controller state.
///
/// Only enough registers are tracked for KS init to reset the chip,
/// scan all target IDs, observe timeouts, and move on.
#[derive(Debug, Clone)]
struct Wd33c93 {
    /// Currently selected indirect register address.
    selected_reg: u8,
    /// Register file ($00–$1F). Only a handful are functionally
    /// significant for the stub; the rest are pure storage.
    regs: [u8; 32],
    /// Auxiliary Status Register (directly readable, not in the
    /// indirect register file proper).
    asr: u8,
}

impl Wd33c93 {
    fn new() -> Self {
        Self {
            selected_reg: 0,
            regs: [0; 32],
            asr: 0,
        }
    }

    /// Hardware reset (SDMAC PREST asserted).
    fn hardware_reset(&mut self) {
        self.regs = [0; 32];
        self.asr = 0;
        self.selected_reg = 0;
    }

    /// Read the Auxiliary Status Register (ASR). Does not clear INT.
    fn read_asr(&self) -> u8 {
        self.asr
    }

    /// Read a register through the indirect SCMD port.
    fn read_data(&mut self) -> u8 {
        let reg = self.selected_reg & 0x1F;
        let val = match reg {
            wd_reg::AUXILIARY_STATUS => self.asr,
            wd_reg::SCSI_STATUS => {
                let status = self.regs[wd_reg::SCSI_STATUS as usize];
                // Reading SCSI_STATUS clears ASR.INT.
                self.asr &= !wd_asr::INT;
                status
            }
            _ => self.regs[reg as usize],
        };
        // Auto-increment, except for ASR, DATA, and COMMAND registers.
        if reg != wd_reg::AUXILIARY_STATUS && reg != wd_reg::DATA && reg != wd_reg::COMMAND {
            self.selected_reg = self.selected_reg.wrapping_add(1) & 0x1F;
        }
        val
    }

    /// Write a register through the indirect SCMD port.
    fn write_data(&mut self, val: u8) {
        let reg = self.selected_reg & 0x1F;
        match reg {
            wd_reg::COMMAND => self.execute_command(val),
            _ => self.regs[reg as usize] = val,
        }
        // Auto-increment, except for ASR, DATA, and COMMAND registers.
        if reg != wd_reg::AUXILIARY_STATUS && reg != wd_reg::DATA && reg != wd_reg::COMMAND {
            self.selected_reg = self.selected_reg.wrapping_add(1) & 0x1F;
        }
    }

    /// Execute a WD33C93 command.
    fn execute_command(&mut self, cmd: u8) {
        match cmd {
            wd_cmd::RESET => {
                // Software reset. Check EAF (OWN_ID bit 3) to decide
                // the post-reset status code.
                let eaf = self.regs[wd_reg::OWN_ID as usize] & 0x08 != 0;
                self.regs = [0; 32];
                self.regs[wd_reg::SCSI_STATUS as usize] =
                    if eaf { wd_csr::RESET_AF } else { wd_csr::RESET };
                self.asr = wd_asr::INT;
            }
            wd_cmd::ABORT => {
                self.regs[wd_reg::SCSI_STATUS as usize] = 0x22; // CSR_SEL_ABORT
                self.asr = wd_asr::INT;
            }
            wd_cmd::SEL_ATN | wd_cmd::SEL | wd_cmd::SEL_ATN_XFER | wd_cmd::SEL_XFER => {
                // No SCSI targets exist — immediate timeout.
                self.regs[wd_reg::SCSI_STATUS as usize] = wd_csr::TIMEOUT;
                self.asr = wd_asr::INT;
            }
            _ => {
                // Unknown or unimplemented command — set LCI (Last
                // Command Ignored) in ASR. KS handles this gracefully.
                self.asr |= 0x40; // LCI bit
            }
        }
    }

    /// True when the WD33C93 INT pin is asserted (ASR bit 7).
    fn int_active(&self) -> bool {
        self.asr & wd_asr::INT != 0
    }
}

// ---------------------------------------------------------------------------
// SDMAC 390537
// ---------------------------------------------------------------------------

/// Commodore 390537 SDMAC state.
///
/// Provides the WD33C93 SCSI interface and DMA registers at
/// `$DD0000–$DDFFFF`. This stub is sufficient for KS 3.x to
/// complete its SCSI probe (finding no devices) and continue booting.
#[derive(Debug, Clone)]
pub struct Dmac390537 {
    wd: Wd33c93,
    /// CNTR — control register.
    cntr: u8,
    /// DAWR — DACK width register (write-only, 2 bits).
    dawr: u8,
    /// WTC — word transfer count (24-bit, stored as u32).
    wtc: u32,
    /// ACR — address counter register (32-bit).
    acr: u32,
    /// Latched interrupt flags (cleared by CINT strobe).
    istr_latched: u8,
}

impl Dmac390537 {
    /// Create a new SDMAC in power-on state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            wd: Wd33c93::new(),
            cntr: 0,
            dawr: 0,
            wtc: 0,
            acr: 0,
            istr_latched: 0,
        }
    }

    /// Reset all state to power-on defaults.
    pub fn reset(&mut self) {
        self.wd.hardware_reset();
        self.cntr = 0;
        self.dawr = 0;
        self.wtc = 0;
        self.acr = 0;
        self.istr_latched = 0;
    }

    /// Current CNTR register value.
    #[must_use]
    pub const fn cntr(&self) -> u8 {
        self.cntr
    }

    /// Current DAWR register value.
    #[must_use]
    pub const fn dawr(&self) -> u8 {
        self.dawr
    }

    /// Current WTC register value.
    #[must_use]
    pub const fn wtc(&self) -> u32 {
        self.wtc
    }

    /// Current ACR register value.
    #[must_use]
    pub const fn acr(&self) -> u32 {
        self.acr
    }

    /// Currently selected WD33C93 register.
    #[must_use]
    pub const fn wd_selected_reg(&self) -> u8 {
        self.wd.selected_reg
    }

    /// Current WD33C93 auxiliary-status register.
    #[must_use]
    pub fn wd_asr(&self) -> u8 {
        self.wd.read_asr()
    }

    /// Current WD33C93 SCSI-status register.
    #[must_use]
    pub fn wd_scsi_status(&self) -> u8 {
        self.wd.regs[wd_reg::SCSI_STATUS as usize]
    }

    /// Compute the current ISTR value.
    ///
    /// ISTR is read-only and reflects live state plus latched flags.
    fn istr(&self) -> u8 {
        let mut val = self.istr_latched;

        // FE_FLG: FIFO is always empty (no DMA in stub).
        val |= istr_bits::FE_FLG;

        // INTS: WD33C93 interrupt pin.
        if self.wd.int_active() {
            val |= istr_bits::INTS;
        }

        // INT_F: any interrupt source active.
        if val & (istr_bits::INTS | 0x20 | 0x08 | 0x04) != 0 {
            val |= istr_bits::INT_F;
        }

        // INT_P: only set when CNTR.INTEN is enabled.
        if val & istr_bits::INT_F != 0 && self.cntr & cntr_bits::INTEN != 0 {
            val |= istr_bits::INT_P;
        }

        val
    }

    /// Current ISTR value.
    #[must_use]
    pub fn current_istr(&self) -> u8 {
        self.istr()
    }

    /// Read a word from an SDMAC register.
    ///
    /// `addr` is the full byte address in `$DD0000–$DDFFFF`. The bus
    /// wrapper calls this for word reads; byte reads extract the
    /// relevant byte from the returned word.
    #[must_use]
    pub fn read_word(&mut self, addr: u32) -> u16 {
        let offset = ((addr & 0xFFFF) >> 1) as u8;
        match offset {
            REG_CNTR => u16::from(self.cntr),
            REG_ISTR => u16::from(self.istr()),
            REG_WTC_HI => (self.wtc >> 16) as u16,
            REG_WTC_LO => self.wtc as u16,
            REG_ACR_HI => (self.acr >> 16) as u16,
            REG_ACR_LO => self.acr as u16,
            REG_SASR | REG_SASR_ALT => {
                // Word read: high byte is 0, low byte is ASR.
                u16::from(self.wd.read_asr())
            }
            REG_SCMD | REG_SCMD_ALT => {
                // Word read: high byte is 0, low byte is register data.
                u16::from(self.wd.read_data())
            }
            _ => 0,
        }
    }

    /// Write a word to an SDMAC register.
    ///
    /// `addr` is the full byte address. `val` is the 16-bit data word.
    pub fn write_word(&mut self, addr: u32, val: u16) {
        let offset = ((addr & 0xFFFF) >> 1) as u8;
        match offset {
            REG_DAWR => self.dawr = val as u8 & 0x03,
            REG_CNTR => {
                self.cntr = val as u8;
                // PREST: assert peripheral reset while set.
                if self.cntr & cntr_bits::PREST != 0 {
                    self.wd.hardware_reset();
                }
            }
            REG_WTC_HI => {
                self.wtc = (self.wtc & 0x0000_FFFF) | (u32::from(val) << 16);
            }
            REG_WTC_LO => {
                self.wtc = (self.wtc & 0xFFFF_0000) | u32::from(val);
            }
            REG_ACR_HI => {
                self.acr = (self.acr & 0x0000_FFFF) | (u32::from(val) << 16);
            }
            REG_ACR_LO => {
                self.acr = (self.acr & 0xFFFF_0000) | u32::from(val);
            }
            REG_ST_DMA | REG_FLUSH | REG_SP_DMA => {
                // Strobe registers: no-op in stub.
            }
            REG_CINT => {
                // Clear all latched interrupt flags.
                self.istr_latched = 0;
            }
            REG_SASR | REG_SASR_ALT => {
                // Write selects WD33C93 indirect register address.
                self.wd.selected_reg = val as u8 & 0x1F;
            }
            REG_SCMD | REG_SCMD_ALT => {
                // Write to WD33C93 register data port.
                self.wd.write_data(val as u8);
            }
            _ => {} // Unknown register — ignore.
        }
    }

    /// Read a single byte from an SDMAC address.
    ///
    /// Extracts the correct byte from a word read based on address
    /// alignment (even = high byte, odd = low byte).
    #[must_use]
    pub fn read_byte(&mut self, addr: u32) -> u8 {
        let word = self.read_word(addr & !1);
        if addr & 1 == 0 {
            (word >> 8) as u8
        } else {
            word as u8
        }
    }

    /// Write a single byte to an SDMAC address.
    pub fn write_byte(&mut self, addr: u32, val: u8) {
        // Most registers are in the low byte of the word. Route the
        // byte to a word write. For simplicity, put it in both halves.
        self.write_word(addr & !1, u16::from(val));
    }
}

impl Default for Dmac390537 {
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

    #[test]
    fn power_on_istr_fifo_empty() {
        let mut d = Dmac390537::new();
        let istr = d.read_word(0xDD_0000 | (u32::from(REG_ISTR) << 1));
        assert_eq!(istr as u8 & istr_bits::FE_FLG, istr_bits::FE_FLG);
    }

    #[test]
    fn cntr_roundtrip() {
        let mut d = Dmac390537::new();
        let cntr_addr = 0xDD_0000 | (u32::from(REG_CNTR) << 1);
        d.write_word(cntr_addr, 0x04); // INTEN
        assert_eq!(d.read_word(cntr_addr) as u8, 0x04);
    }

    #[test]
    fn wd_reset_sets_int_and_status() {
        let mut d = Dmac390537::new();
        let sasr_addr = 0xDD_0000 | (u32::from(REG_SASR) << 1);
        let scmd_addr = 0xDD_0000 | (u32::from(REG_SCMD) << 1);

        // Write OWN_ID with EAF bit set.
        d.write_word(sasr_addr, wd_reg::OWN_ID as u16);
        d.write_word(scmd_addr, 0x0F); // SCSI ID 7 + EAF

        // Write COMMAND = RESET.
        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::RESET as u16);

        // ASR should have INT set.
        let asr = d.read_word(sasr_addr) as u8;
        assert_ne!(asr & wd_asr::INT, 0, "ASR.INT should be set after reset");

        // SCSI_STATUS should be CSR_RESET_AF.
        d.write_word(sasr_addr, wd_reg::SCSI_STATUS as u16);
        let status = d.read_word(scmd_addr) as u8;
        assert_eq!(status, wd_csr::RESET_AF);

        // Reading SCSI_STATUS should clear ASR.INT.
        let asr_after = d.read_word(sasr_addr) as u8;
        assert_eq!(asr_after & wd_asr::INT, 0, "ASR.INT should be cleared");
    }

    #[test]
    fn wd_select_times_out() {
        let mut d = Dmac390537::new();
        let sasr_addr = 0xDD_0000 | (u32::from(REG_SASR) << 1);
        let scmd_addr = 0xDD_0000 | (u32::from(REG_SCMD) << 1);

        // Set DESTINATION_ID to target 0.
        d.write_word(sasr_addr, wd_reg::DESTINATION_ID as u16);
        d.write_word(scmd_addr, 0x00);

        // Issue SEL_ATN_XFER.
        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::SEL_ATN_XFER as u16);

        // ASR.INT should be set.
        let asr = d.read_word(sasr_addr) as u8;
        assert_ne!(asr & wd_asr::INT, 0);

        // SCSI_STATUS should be CSR_TIMEOUT.
        d.write_word(sasr_addr, wd_reg::SCSI_STATUS as u16);
        let status = d.read_word(scmd_addr) as u8;
        assert_eq!(status, wd_csr::TIMEOUT);
    }

    #[test]
    fn cint_clears_latched_flags() {
        let mut d = Dmac390537::new();
        let cint_addr = 0xDD_0000 | (u32::from(REG_CINT) << 1);

        d.istr_latched = 0xFF;
        d.write_word(cint_addr, 0);
        assert_eq!(d.istr_latched, 0);
    }

    #[test]
    fn istr_reflects_wd_int() {
        let mut d = Dmac390537::new();
        let sasr_addr = 0xDD_0000 | (u32::from(REG_SASR) << 1);
        let scmd_addr = 0xDD_0000 | (u32::from(REG_SCMD) << 1);
        let istr_addr = 0xDD_0000 | (u32::from(REG_ISTR) << 1);

        // Trigger a reset to set WD INT.
        d.write_word(sasr_addr, wd_reg::OWN_ID as u16);
        d.write_word(scmd_addr, 0x07); // SCSI ID 7, no EAF
        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::RESET as u16);

        let istr = d.read_word(istr_addr) as u8;
        assert_ne!(istr & istr_bits::INTS, 0, "ISTR.INTS should reflect WD INT");
        assert_ne!(istr & istr_bits::INT_F, 0, "ISTR.INT_F should be set");
    }

    #[test]
    fn istr_int_p_requires_inten() {
        let mut d = Dmac390537::new();
        let sasr_addr = 0xDD_0000 | (u32::from(REG_SASR) << 1);
        let scmd_addr = 0xDD_0000 | (u32::from(REG_SCMD) << 1);
        let istr_addr = 0xDD_0000 | (u32::from(REG_ISTR) << 1);
        let cntr_addr = 0xDD_0000 | (u32::from(REG_CNTR) << 1);

        // Trigger WD INT.
        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::RESET as u16);

        // INT_P should be 0 without INTEN.
        let istr = d.read_word(istr_addr) as u8;
        assert_eq!(istr & istr_bits::INT_P, 0);

        // Enable INTEN.
        d.write_word(cntr_addr, cntr_bits::INTEN as u16);
        let istr = d.read_word(istr_addr) as u8;
        assert_ne!(istr & istr_bits::INT_P, 0);
    }

    #[test]
    fn prest_resets_wd() {
        let mut d = Dmac390537::new();
        let cntr_addr = 0xDD_0000 | (u32::from(REG_CNTR) << 1);
        let sasr_addr = 0xDD_0000 | (u32::from(REG_SASR) << 1);

        // Set some WD state.
        d.wd.asr = 0xFF;
        d.wd.regs[0] = 0xAA;

        // Assert PREST.
        d.write_word(cntr_addr, cntr_bits::PREST as u16);

        // WD should be reset.
        let asr = d.read_word(sasr_addr) as u8;
        assert_eq!(asr, 0);
        assert_eq!(d.wd.regs[0], 0);
    }

    #[test]
    fn byte_read_odd_returns_low_byte() {
        let mut d = Dmac390537::new();
        let cntr_addr = 0xDD_0000 | (u32::from(REG_CNTR) << 1);
        d.write_word(cntr_addr, 0x07);

        // Odd byte address should return the low byte of the word.
        let val = d.read_byte(cntr_addr | 1);
        assert_eq!(val, 0x07);
    }

    #[test]
    fn all_seven_ids_timeout() {
        let mut d = Dmac390537::new();
        let sasr_addr = 0xDD_0000 | (u32::from(REG_SASR) << 1);
        let scmd_addr = 0xDD_0000 | (u32::from(REG_SCMD) << 1);

        for target_id in 0..7u8 {
            // Set DESTINATION_ID.
            d.write_word(sasr_addr, wd_reg::DESTINATION_ID as u16);
            d.write_word(scmd_addr, u16::from(target_id));

            // Issue SELECT.
            d.write_word(sasr_addr, wd_reg::COMMAND as u16);
            d.write_word(scmd_addr, wd_cmd::SEL_ATN as u16);

            // Verify timeout.
            d.write_word(sasr_addr, wd_reg::SCSI_STATUS as u16);
            let status = d.read_word(scmd_addr) as u8;
            assert_eq!(status, wd_csr::TIMEOUT, "target {target_id} should timeout");
        }
    }

    #[test]
    fn reset_restores_dma_register_defaults() {
        let mut d = Dmac390537::new();
        d.cntr = cntr_bits::INTEN;
        d.dawr = 0x03;
        d.wtc = 0x12_3456;
        d.acr = 0x89AB_CDEF;
        d.istr_latched = 0xA0;
        d.wd.selected_reg = 0x1F;
        d.wd.asr = wd_asr::INT;

        d.reset();

        assert_eq!(d.cntr, 0);
        assert_eq!(d.dawr, 0);
        assert_eq!(d.wtc, 0);
        assert_eq!(d.acr, 0);
        assert_eq!(d.istr_latched, 0);
        assert_eq!(d.wd.selected_reg, 0);
        assert_eq!(
            d.read_word(0xDD_0000 | (u32::from(REG_ISTR) << 1)) as u8,
            istr_bits::FE_FLG
        );
    }

    #[test]
    fn dawr_masks_to_low_two_bits() {
        let mut d = Dmac390537::new();
        let dawr_addr = 0xDD_0000 | (u32::from(REG_DAWR) << 1);

        d.write_word(dawr_addr, 0x00FF);

        assert_eq!(d.dawr, 0x03);
    }

    #[test]
    fn wtc_and_acr_roundtrip() {
        let mut d = Dmac390537::new();
        let wtc_hi_addr = 0xDD_0000 | (u32::from(REG_WTC_HI) << 1);
        let wtc_lo_addr = 0xDD_0000 | (u32::from(REG_WTC_LO) << 1);
        let acr_hi_addr = 0xDD_0000 | (u32::from(REG_ACR_HI) << 1);
        let acr_lo_addr = 0xDD_0000 | (u32::from(REG_ACR_LO) << 1);

        d.write_word(wtc_hi_addr, 0x1234);
        d.write_word(wtc_lo_addr, 0x5678);
        d.write_word(acr_hi_addr, 0x89AB);
        d.write_word(acr_lo_addr, 0xCDEF);

        assert_eq!(d.read_word(wtc_hi_addr), 0x1234);
        assert_eq!(d.read_word(wtc_lo_addr), 0x5678);
        assert_eq!(d.read_word(acr_hi_addr), 0x89AB);
        assert_eq!(d.read_word(acr_lo_addr), 0xCDEF);
    }

    #[test]
    fn byte_access_uses_low_register_byte() {
        let mut d = Dmac390537::new();
        let cntr_addr = 0xDD_0000 | (u32::from(REG_CNTR) << 1);

        d.write_byte(cntr_addr + 1, cntr_bits::INTEN);

        assert_eq!(d.read_byte(cntr_addr), 0);
        assert_eq!(d.read_byte(cntr_addr + 1), cntr_bits::INTEN);
    }

    #[test]
    fn sasr_alt_port_mirrors_primary_and_auto_increments() {
        let mut d = Dmac390537::new();
        let sasr_alt_addr = 0xDD_0000 | (u32::from(REG_SASR_ALT) << 1);
        let scmd_alt_addr = 0xDD_0000 | (u32::from(REG_SCMD_ALT) << 1);
        let sasr_addr = 0xDD_0000 | (u32::from(REG_SASR) << 1);
        let scmd_addr = 0xDD_0000 | (u32::from(REG_SCMD) << 1);

        d.write_word(sasr_alt_addr, wd_reg::OWN_ID as u16);
        d.write_word(scmd_alt_addr, 0x12);
        d.write_word(scmd_alt_addr, 0x34);

        d.write_word(sasr_addr, wd_reg::OWN_ID as u16);
        assert_eq!(d.read_word(scmd_addr) as u8, 0x12);
        assert_eq!(d.read_word(scmd_addr) as u8, 0x34);
    }

    #[test]
    fn unknown_command_sets_lci_without_wd_interrupt() {
        let mut d = Dmac390537::new();
        let sasr_addr = 0xDD_0000 | (u32::from(REG_SASR) << 1);
        let scmd_addr = 0xDD_0000 | (u32::from(REG_SCMD) << 1);
        let istr_addr = 0xDD_0000 | (u32::from(REG_ISTR) << 1);

        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, 0xFF);

        let asr = d.read_word(sasr_addr) as u8;
        assert_eq!(asr & 0x40, 0x40);
        assert_eq!(asr & wd_asr::INT, 0);
        assert_eq!(d.read_word(istr_addr) as u8 & istr_bits::INTS, 0);
    }

    #[test]
    fn cint_clears_latched_flags_but_not_wd_interrupt_source() {
        let mut d = Dmac390537::new();
        let sasr_addr = 0xDD_0000 | (u32::from(REG_SASR) << 1);
        let scmd_addr = 0xDD_0000 | (u32::from(REG_SCMD) << 1);
        let cint_addr = 0xDD_0000 | (u32::from(REG_CINT) << 1);
        let istr_addr = 0xDD_0000 | (u32::from(REG_ISTR) << 1);

        d.istr_latched = 0x20;
        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::RESET as u16);

        d.write_word(cint_addr, 0);

        let istr = d.read_word(istr_addr) as u8;
        assert_eq!(d.istr_latched, 0);
        assert_eq!(istr & 0x20, 0);
        assert_ne!(istr & istr_bits::INTS, 0);
        assert_ne!(istr & istr_bits::INT_F, 0);
    }

    #[test]
    fn public_state_accessors_reflect_register_values() {
        let mut d = Dmac390537::new();
        d.write_word(0xDD_0000 | (u32::from(REG_DAWR) << 1), 0x0003);
        d.write_word(0xDD_0000 | (u32::from(REG_WTC_HI) << 1), 0x0012);
        d.write_word(0xDD_0000 | (u32::from(REG_WTC_LO) << 1), 0x3456);
        d.write_word(0xDD_0000 | (u32::from(REG_ACR_HI) << 1), 0x89AB);
        d.write_word(0xDD_0000 | (u32::from(REG_ACR_LO) << 1), 0xCDEF);
        d.write_word(
            0xDD_0000 | (u32::from(REG_SASR) << 1),
            wd_reg::COMMAND as u16,
        );

        assert_eq!(d.dawr(), 0x03);
        assert_eq!(d.wtc(), 0x0012_3456);
        assert_eq!(d.acr(), 0x89AB_CDEF);
        assert_eq!(d.wd_selected_reg(), wd_reg::COMMAND);
        assert_eq!(d.cntr(), 0x00);
        assert_eq!(d.current_istr() & istr_bits::FE_FLG, istr_bits::FE_FLG);
    }
}
