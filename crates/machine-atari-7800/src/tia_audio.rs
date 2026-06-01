//! Minimal TIA audio register stub for the Atari 7800.
//!
//! Adapted from `Emu198x-Oldest/crates/machine-atari-7800/src/tia_audio.rs`
//! (RULES.md rule 27).
//!
//! In 7800 mode the TIA is responsible for sound only — MARIA handles all
//! video. This stub stores the six audio registers; synthesis is wired in
//! later (tracked in `docs/status/outstanding-work.md`).

#[derive(Default)]
pub struct TiaAudio {
    pub audc0: u8,
    pub audc1: u8,
    pub audf0: u8,
    pub audf1: u8,
    pub audv0: u8,
    pub audv1: u8,
}

impl TiaAudio {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write(&mut self, addr: u8, value: u8) {
        match addr & 0x1F {
            0x15 => self.audc0 = value,
            0x16 => self.audc1 = value,
            0x17 => self.audf0 = value,
            0x18 => self.audf1 = value,
            0x19 => self.audv0 = value,
            0x1A => self.audv1 = value,
            _ => {}
        }
    }

    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn read(&self, _addr: u8) -> u8 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_registers_roundtrip() {
        let mut tia = TiaAudio::new();
        tia.write(0x15, 0x0A);
        tia.write(0x16, 0x0B);
        tia.write(0x17, 0x1F);
        tia.write(0x18, 0x1E);
        tia.write(0x19, 0x0F);
        tia.write(0x1A, 0x0E);
        assert_eq!(tia.audc0, 0x0A);
        assert_eq!(tia.audc1, 0x0B);
        assert_eq!(tia.audf0, 0x1F);
        assert_eq!(tia.audf1, 0x1E);
        assert_eq!(tia.audv0, 0x0F);
        assert_eq!(tia.audv1, 0x0E);
    }

    #[test]
    fn video_writes_are_ignored() {
        let mut tia = TiaAudio::new();
        tia.write(0x00, 0xFF);
        tia.write(0x0D, 0xFF);
        assert_eq!(tia.audc0, 0);
    }

    #[test]
    fn reads_return_zero() {
        let tia = TiaAudio::new();
        assert_eq!(tia.read(0x00), 0);
        assert_eq!(tia.read(0x0D), 0);
    }
}
