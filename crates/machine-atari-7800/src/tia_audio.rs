//! Minimal TIA register file for the Atari 7800.
//!
//! Adapted from `Emu198x-Oldest/crates/machine-atari-7800/src/tia_audio.rs`
//! (RULES.md rule 27).
//!
//! In 7800 mode the TIA does sound and the controller **fire-button** reads —
//! MARIA handles all video. This stores the six audio registers (synthesis is
//! wired in later, tracked in `docs/status/outstanding-work.md`) and the
//! controller buttons surfaced through the TIA `INPT0`-`INPT5` registers.

/// Controller button state, packed exactly as MAME's `a7800` `BUTTONS` port so
/// the `INPT` read logic ports across directly: bit 0 = P2 button 2, bit 1 =
/// P1 button 2, bit 2 = P2 button 1, bit 3 = P1 button 1 (active-high).
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct TiaAudio {
    pub audc0: u8,
    pub audc1: u8,
    pub audf0: u8,
    pub audf1: u8,
    pub audv0: u8,
    pub audv1: u8,
    buttons: u8,
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

    /// Set a controller button. `player` is 1 or 2; `button` is 1 (the standard
    /// fire) or 2. Maps to the MAME `BUTTONS` bit for that (player, button).
    pub fn set_button(&mut self, player: u8, button: u8, pressed: bool) {
        let bit = match (player, button) {
            (1, 1) => 0x08,
            (1, 2) => 0x02,
            (2, 1) => 0x04,
            (2, 2) => 0x01,
            _ => return,
        };
        if pressed {
            self.buttons |= bit;
        } else {
            self.buttons &= !bit;
        }
    }

    /// Read a TIA register. `INPT0`-`INPT3` (`$08`-`$0B`) expose the two
    /// proline buttons per controller (active-high in bit 7); `INPT4`/`INPT5`
    /// (`$0C`/`$0D`) expose the one-button reading (active-low: `0x00` pressed,
    /// `0x80` released). Ported from MAME `a7800` `tia_r`. Audio and collision
    /// registers read back 0.
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        match addr & 0x0F {
            // INPT0: P1 button 2; INPT1: P1 button 1.
            0x08 => (self.buttons & 0x02) << 6,
            0x09 => (self.buttons & 0x08) << 4,
            // INPT2: P2 button 2; INPT3: P2 button 1.
            0x0A => (self.buttons & 0x01) << 7,
            0x0B => (self.buttons & 0x04) << 5,
            // INPT4: P1 either button (one-button mode, active-low bit 7).
            0x0C => {
                if self.buttons & 0x0A != 0 {
                    0x00
                } else {
                    0x80
                }
            }
            // INPT5: P2 either button.
            0x0D => {
                if self.buttons & 0x05 != 0 {
                    0x00
                } else {
                    0x80
                }
            }
            _ => 0,
        }
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
    fn audio_and_collision_reads_return_zero() {
        let tia = TiaAudio::new();
        assert_eq!(tia.read(0x00), 0); // collision
        assert_eq!(tia.read(0x15), 0); // audio
    }

    #[test]
    fn buttons_drive_the_inpt_registers() {
        let mut tia = TiaAudio::new();
        // Idle: INPT4/5 read high (active-low), INPT0-3 read 0.
        assert_eq!(tia.read(0x0C), 0x80, "INPT4 idles high");
        assert_eq!(tia.read(0x0D), 0x80, "INPT5 idles high");
        assert_eq!(tia.read(0x09), 0x00, "INPT1 idles low");

        // P1 button 1 → INPT1 bit 7 (active-high) and INPT4 → 0 (active-low).
        tia.set_button(1, 1, true);
        assert_eq!(tia.read(0x09), 0x80, "INPT1 = P1 button 1");
        assert_eq!(tia.read(0x0C), 0x00, "INPT4 = P1 fire (pressed)");
        // P2 untouched.
        assert_eq!(tia.read(0x0D), 0x80, "INPT5 still idle");

        // P1 button 2 → INPT0 bit 7; releasing button 1 keeps INPT4 low.
        tia.set_button(1, 2, true);
        assert_eq!(tia.read(0x08), 0x80, "INPT0 = P1 button 2");
        tia.set_button(1, 1, false);
        assert_eq!(tia.read(0x0C), 0x00, "INPT4 still 0 via button 2");

        // P2 button 1 → INPT3 bit 7 and INPT5 → 0.
        tia.set_button(2, 1, true);
        assert_eq!(tia.read(0x0B), 0x80, "INPT3 = P2 button 1");
        assert_eq!(tia.read(0x0D), 0x00, "INPT5 = P2 fire");
    }
}
