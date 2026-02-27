//! IEC serial bus connecting the C64 to the 1541 drive.
//!
//! Three open-collector lines: ATN, CLK, DATA. Each participant (C64 and
//! drive) can independently pull a line low. A line reads high only when
//! nobody pulls it low. This matches real hardware where each line has a
//! pull-up resistor and any device can ground it.
//!
//! Signal polarity (from C64 CIA2 perspective):
//!   Output: PA bit = 1 means pull line LOW (bit 3=ATN, 4=CLK, 5=DATA)
//!   Input:  PA bit = 0 means line is LOW; bit = 1 means HIGH
//!           (bit 6=CLK IN, bit 7=DATA IN)

/// IEC serial bus with two participants: C64 and drive.
pub struct IecBus {
    /// ATN pull-down: [c64, drive]. true = pulling low.
    atn_pulls: [bool; 2],
    /// CLK pull-down: [c64, drive].
    clk_pulls: [bool; 2],
    /// DATA pull-down: [c64, drive].
    data_pulls: [bool; 2],
}

impl IecBus {
    /// Create a new IEC bus with all lines released (high).
    #[must_use]
    pub fn new() -> Self {
        Self {
            atn_pulls: [false; 2],
            clk_pulls: [false; 2],
            data_pulls: [false; 2],
        }
    }

    // --- C64 side ---

    /// Set whether the C64 pulls ATN low.
    pub fn set_c64_atn(&mut self, pull_low: bool) {
        self.atn_pulls[0] = pull_low;
    }

    /// Set whether the C64 pulls CLK low.
    pub fn set_c64_clk(&mut self, pull_low: bool) {
        self.clk_pulls[0] = pull_low;
    }

    /// Set whether the C64 pulls DATA low.
    pub fn set_c64_data(&mut self, pull_low: bool) {
        self.data_pulls[0] = pull_low;
    }

    // --- Drive side ---

    /// Set whether the drive pulls CLK low.
    pub fn set_drive_clk(&mut self, pull_low: bool) {
        self.clk_pulls[1] = pull_low;
    }

    /// Set whether the drive pulls DATA low.
    pub fn set_drive_data(&mut self, pull_low: bool) {
        self.data_pulls[1] = pull_low;
    }

    /// Set whether the drive pulls ATN low (rarely used, but available).
    pub fn set_drive_atn(&mut self, pull_low: bool) {
        self.atn_pulls[1] = pull_low;
    }

    // --- Line state (true = high, false = low) ---

    /// ATN line state. High when nobody pulls it low.
    #[must_use]
    pub fn atn(&self) -> bool {
        !self.atn_pulls[0] && !self.atn_pulls[1]
    }

    /// CLK line state. High when nobody pulls it low.
    #[must_use]
    pub fn clk(&self) -> bool {
        !self.clk_pulls[0] && !self.clk_pulls[1]
    }

    /// DATA line state. High when nobody pulls it low.
    #[must_use]
    pub fn data(&self) -> bool {
        !self.data_pulls[0] && !self.data_pulls[1]
    }
}

impl Default for IecBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_start_high() {
        let bus = IecBus::new();
        assert!(bus.atn());
        assert!(bus.clk());
        assert!(bus.data());
    }

    #[test]
    fn c64_pulls_low() {
        let mut bus = IecBus::new();
        bus.set_c64_atn(true);
        assert!(!bus.atn());
        assert!(bus.clk()); // Others unaffected
        assert!(bus.data());
    }

    #[test]
    fn drive_pulls_low() {
        let mut bus = IecBus::new();
        bus.set_drive_data(true);
        assert!(!bus.data());
        assert!(bus.clk());
    }

    #[test]
    fn both_pull_low_still_low() {
        let mut bus = IecBus::new();
        bus.set_c64_clk(true);
        bus.set_drive_clk(true);
        assert!(!bus.clk());
        // Release C64 side — drive still holds it low
        bus.set_c64_clk(false);
        assert!(!bus.clk());
        // Release drive side — now high
        bus.set_drive_clk(false);
        assert!(bus.clk());
    }

    #[test]
    fn open_collector_independence() {
        let mut bus = IecBus::new();
        // Each line is independent
        bus.set_c64_atn(true);
        bus.set_drive_data(true);
        assert!(!bus.atn());
        assert!(bus.clk()); // CLK untouched
        assert!(!bus.data());
    }
}
