use format_commodore_c64_tap::TapImage;
use serde::{Deserialize, Serialize};

pub(crate) const MOTOR_DELAY_CYCLES: u32 = 32_000;

/// Datasette state owned by the C64 board.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Datasette {
    tape: Option<TapImage>,
    play_pressed: bool,
    motor_requested: bool,
    motor_running: bool,
    pending_motor_state: Option<bool>,
    motor_delay_remaining: u32,
    next_pulse_index: usize,
    cycles_until_flux: Option<u32>,
}

impl Datasette {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            tape: None,
            play_pressed: false,
            motor_requested: false,
            motor_running: false,
            pending_motor_state: None,
            motor_delay_remaining: 0,
            next_pulse_index: 0,
            cycles_until_flux: None,
        }
    }

    pub fn load_tap(&mut self, tape: TapImage) {
        self.tape = Some(tape);
        self.play_pressed = false;
        self.next_pulse_index = 0;
        self.cycles_until_flux = None;
    }

    #[must_use]
    pub const fn is_loaded(&self) -> bool {
        self.tape.is_some()
    }

    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.play_pressed
            && self.motor_running
            && self
                .tape
                .as_ref()
                .is_some_and(|tape| self.next_pulse_index < tape.pulses.len())
    }

    pub fn play(&mut self) {
        if self.tape.is_some() {
            self.play_pressed = true;
            self.cycles_until_flux = None;
        }
    }

    pub fn stop(&mut self) {
        self.play_pressed = false;
        self.cycles_until_flux = None;
    }

    pub fn set_motor_on(&mut self, motor_on: bool) {
        self.motor_requested = motor_on;

        if motor_on {
            if self.motor_running {
                self.pending_motor_state = None;
                self.motor_delay_remaining = 0;
            } else {
                self.pending_motor_state = Some(true);
                self.motor_delay_remaining = MOTOR_DELAY_CYCLES;
            }
        } else if self.motor_running {
            self.pending_motor_state = Some(false);
            self.motor_delay_remaining = MOTOR_DELAY_CYCLES;
        } else {
            self.pending_motor_state = None;
            self.motor_delay_remaining = 0;
        }
    }

    #[must_use]
    pub const fn sense_active(&self) -> bool {
        self.play_pressed
    }

    #[must_use]
    pub const fn motor_on(&self) -> bool {
        self.motor_running
    }

    #[must_use]
    pub const fn pulse_index(&self) -> usize {
        self.next_pulse_index
    }

    #[must_use]
    pub fn pulse_count(&self) -> usize {
        self.tape.as_ref().map_or(0, |tape| tape.pulses.len())
    }

    #[must_use]
    pub const fn write_input_active(&self) -> bool {
        false
    }

    #[must_use]
    pub fn advance_phi2_cycle(&mut self) -> bool {
        self.advance_motor_state();

        if !self.play_pressed || !self.motor_running {
            return false;
        }

        let Some(tape) = &self.tape else {
            self.play_pressed = false;
            return false;
        };

        if self.next_pulse_index >= tape.pulses.len() {
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
        true
    }

    fn advance_motor_state(&mut self) {
        let Some(target_state) = self.pending_motor_state else {
            return;
        };

        if self.motor_delay_remaining > 1 {
            self.motor_delay_remaining -= 1;
            return;
        }

        self.motor_running = target_state;
        self.pending_motor_state = None;
        self.motor_delay_remaining = 0;
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
        for _ in 0..(MOTOR_DELAY_CYCLES - 1) {
            assert!(!datasette.advance_phi2_cycle());
        }
        assert!(!datasette.motor_on());
        assert!(!datasette.advance_phi2_cycle());
        assert!(datasette.motor_on());
        for _ in 0..6 {
            assert!(!datasette.advance_phi2_cycle());
        }
        assert!(datasette.advance_phi2_cycle());
        assert!(!datasette.is_playing());
    }

    #[test]
    fn motor_stop_is_delayed_after_line_drops() {
        let mut datasette = Datasette::new();
        datasette.load_tap(stub_tape(&[1, 1, 1]));
        datasette.play();
        datasette.set_motor_on(true);

        for _ in 0..MOTOR_DELAY_CYCLES {
            let _ = datasette.advance_phi2_cycle();
        }
        assert!(datasette.motor_on());

        datasette.set_motor_on(false);
        for _ in 0..(MOTOR_DELAY_CYCLES - 1) {
            let _ = datasette.advance_phi2_cycle();
            assert!(datasette.motor_on());
        }

        assert!(!datasette.advance_phi2_cycle());
        assert!(!datasette.motor_on());
    }
}
