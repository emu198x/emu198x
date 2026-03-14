//! Minimal TIA audio register stub.
//!
//! The Atari 7800 uses TIA only for sound -- MARIA handles all video.
//! This stub stores the six audio registers. Actual audio synthesis
//! will be added later.

/// TIA audio register file for the 7800.
pub struct TiaAudio {
    /// Audio control channel 0 ($15).
    pub audc0: u8,
    /// Audio control channel 1 ($16).
    pub audc1: u8,
    /// Audio frequency channel 0 ($17).
    pub audf0: u8,
    /// Audio frequency channel 1 ($18).
    pub audf1: u8,
    /// Audio volume channel 0 ($19).
    pub audv0: u8,
    /// Audio volume channel 1 ($1A).
    pub audv1: u8,
}

impl TiaAudio {
    /// Create a new TIA audio stub with all registers zeroed.
    #[must_use]
    pub fn new() -> Self {
        Self {
            audc0: 0,
            audc1: 0,
            audf0: 0,
            audf1: 0,
            audv0: 0,
            audv1: 0,
        }
    }

    /// Write a TIA register. Only audio registers ($15-$1A) are stored;
    /// all other writes (video registers) are silently ignored.
    pub fn write(&mut self, addr: u8, value: u8) {
        match addr & 0x1F {
            0x15 => self.audc0 = value,
            0x16 => self.audc1 = value,
            0x17 => self.audf0 = value,
            0x18 => self.audf1 = value,
            0x19 => self.audv0 = value,
            0x1A => self.audv1 = value,
            _ => {} // Video registers -- ignored in 7800 mode.
        }
    }

    /// Read a TIA register. In 7800 mode, TIA reads return 0.
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn read(&self, _addr: u8) -> u8 {
        0
    }
}

impl Default for TiaAudio {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_registers_roundtrip() {
        let mut tia = TiaAudio::new();
        tia.write(0x15, 0x0A);
        assert_eq!(tia.audc0, 0x0A);
        tia.write(0x16, 0x0B);
        assert_eq!(tia.audc1, 0x0B);
        tia.write(0x17, 0x1F);
        assert_eq!(tia.audf0, 0x1F);
        tia.write(0x18, 0x1E);
        assert_eq!(tia.audf1, 0x1E);
        tia.write(0x19, 0x0F);
        assert_eq!(tia.audv0, 0x0F);
        tia.write(0x1A, 0x0E);
        assert_eq!(tia.audv1, 0x0E);
    }

    #[test]
    fn video_writes_are_ignored() {
        let mut tia = TiaAudio::new();
        tia.write(0x00, 0xFF); // VSYNC -- should be ignored
        tia.write(0x0D, 0xFF); // PF0 -- should be ignored
        // No panic, no state change.
    }

    #[test]
    fn reads_return_zero() {
        let tia = TiaAudio::new();
        assert_eq!(tia.read(0x00), 0);
        assert_eq!(tia.read(0x0D), 0);
    }
}
