use format_commodore_c64_tap::TapImage;
use serde::{Deserialize, Serialize};

/// Datasette state owned by the C64 board.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Datasette {
    tape: Option<TapImage>,
    playing: bool,
    motor_on: bool,
    next_pulse_index: usize,
    cycles_until_flux: Option<u32>,
}

impl Datasette {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            tape: None,
            playing: false,
            motor_on: false,
            next_pulse_index: 0,
            cycles_until_flux: None,
        }
    }

    pub fn load_tap(&mut self, tape: TapImage) {
        self.tape = Some(tape);
        self.playing = false;
        self.next_pulse_index = 0;
        self.cycles_until_flux = None;
    }

    #[must_use]
    pub const fn is_loaded(&self) -> bool {
        self.tape.is_some()
    }

    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn play(&mut self) {
        if self.tape.is_some() {
            self.playing = true;
        }
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }

    pub fn set_motor_on(&mut self, motor_on: bool) {
        self.motor_on = motor_on;
    }

    #[must_use]
    pub const fn sense_active(&self) -> bool {
        self.playing
    }

    #[must_use]
    pub const fn motor_input_active(&self) -> bool {
        self.playing && self.motor_on
    }

    #[must_use]
    pub const fn write_input_active(&self) -> bool {
        false
    }

    #[must_use]
    pub fn advance_phi2_cycle(&mut self) -> bool {
        if !self.playing || !self.motor_on {
            return false;
        }

        let Some(tape) = &self.tape else {
            self.playing = false;
            return false;
        };

        if self.next_pulse_index >= tape.pulses.len() {
            self.playing = false;
            self.cycles_until_flux = None;
            return false;
        }

        let remaining = self
            .cycles_until_flux
            .get_or_insert(tape.pulses[self.next_pulse_index]);

        if *remaining > 1 {
            *remaining -= 1;
            return false;
        }

        self.next_pulse_index += 1;
        self.cycles_until_flux = None;
        if self.next_pulse_index >= tape.pulses.len() {
            self.playing = false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use format_commodore_c64_tap::{TapSystem, TapVideo};

    fn stub_tape(pulses: &[u32]) -> TapImage {
        TapImage {
            version: 1,
            system: TapSystem::C64,
            video: TapVideo::Pal,
            pulses: pulses.to_vec(),
        }
    }

    #[test]
    fn stays_idle_without_loaded_tape() {
        let mut datasette = Datasette::new();
        datasette.play();
        datasette.set_motor_on(true);

        assert!(!datasette.advance_phi2_cycle());
        assert!(!datasette.is_playing());
    }

    #[test]
    fn only_advances_when_playing_and_motor_on() {
        let mut datasette = Datasette::new();
        datasette.load_tap(stub_tape(&[8]));
        datasette.play();

        assert!(!datasette.advance_phi2_cycle());
        datasette.set_motor_on(true);
        for _ in 0..7 {
            assert!(!datasette.advance_phi2_cycle());
        }
        assert!(datasette.advance_phi2_cycle());
        assert!(!datasette.is_playing());
    }
}
