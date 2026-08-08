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
    /// The mounted tape can be recorded (a writable SAVE work image).
    writable: bool,
    /// Last-seen cassette WRITE line level (6510 port `$01` bit 3).
    write_line: bool,
    /// Cycles accumulated since the last recorded write-line edge.
    record_cycles: u32,
    /// Set once the first falling edge has been seen, so the leading gap before
    /// the first pulse is not recorded.
    recording_started: bool,
    /// One-shot extra spin-up delay applied to the next motor start,
    /// modelling *when* the user pressed PLAY within a frame. Some
    /// loaders gate a game's whole IRQ timeline off the tape-start
    /// phase (Novaload "Monty on the Run" has an IRQ-window race that
    /// a handful of phases lose); catalogue entries can nudge this.
    #[serde(default)]
    play_phase_cycles: u32,
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
            writable: false,
            write_line: true,
            record_cycles: 0,
            recording_started: false,
            play_phase_cycles: 0,
        }
    }

    pub fn load_tap(&mut self, tape: TapImage) {
        self.tape = Some(tape);
        self.play_pressed = false;
        self.next_pulse_index = 0;
        self.cycles_until_flux = None;
        self.writable = false;
        self.recording_started = false;
        self.record_cycles = 0;
    }

    /// Mounts a blank, writable tape for recording a SAVE. The recorded pulse
    /// stream is retrieved with [`Self::recorded_tap_image`].
    pub fn insert_blank_writable_tape(&mut self, video: format_commodore_c64_tap::TapVideo) {
        self.tape = Some(TapImage {
            version: 1,
            system: format_commodore_c64_tap::TapSystem::C64,
            video,
            pulses: Vec::new(),
        });
        self.play_pressed = false;
        self.next_pulse_index = 0;
        self.cycles_until_flux = None;
        self.writable = true;
        self.recording_started = false;
        self.record_cycles = 0;
    }

    /// Returns the current tape image (including any recorded SAVE pulses) when a
    /// writable tape is mounted, for flushing to a `.tap` sidecar file.
    #[must_use]
    pub fn recorded_tap_image(&self) -> Option<&TapImage> {
        if self.writable {
            self.tape.as_ref()
        } else {
            None
        }
    }

    /// Feeds the current cassette WRITE line level (6510 port `$01` bit 3). While
    /// recording, a falling edge closes the current pulse and appends its cycle
    /// length to the tape.
    pub fn set_write_line(&mut self, high: bool) {
        let recording = self.writable && self.motor_running && self.play_pressed;
        if recording && self.write_line && !high {
            if self.recording_started {
                if let Some(tape) = self.tape.as_mut() {
                    tape.pulses.push(self.record_cycles.max(1));
                }
            } else {
                self.recording_started = true;
            }
            self.record_cycles = 0;
        }
        self.write_line = high;
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

    /// Sets a one-shot extra spin-up delay for the next motor start,
    /// shifting the whole tape timeline (and any loader whose IRQ
    /// timing derives from it) by that many cycles. Models pressing
    /// PLAY at a different moment within the frame.
    pub fn set_play_phase_cycles(&mut self, cycles: u32) {
        self.play_phase_cycles = cycles;
    }

    pub fn set_motor_on(&mut self, motor_on: bool) {
        // This method reflects a physical line level. Re-observing the same
        // level must not restart an in-flight spin-up/spin-down delay; that
        // would make a restore-time port refresh change future tape timing.
        if self.motor_requested == motor_on {
            return;
        }
        self.motor_requested = motor_on;

        if motor_on {
            if self.motor_running {
                self.pending_motor_state = None;
                self.motor_delay_remaining = 0;
            } else {
                self.pending_motor_state = Some(true);
                self.motor_delay_remaining =
                    MOTOR_DELAY_CYCLES + std::mem::take(&mut self.play_phase_cycles);
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

        // Recording a SAVE: accumulate cycles between write-line edges. No read
        // flux is produced from a writable tape.
        if self.writable {
            self.record_cycles = self.record_cycles.saturating_add(1);
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
    fn records_write_line_pulses_into_a_writable_tape() {
        let mut datasette = Datasette::new();
        datasette.insert_blank_writable_tape(TapVideo::Pal);
        datasette.play();
        datasette.set_motor_on(true);
        // Spin the motor up so recording is active.
        for _ in 0..=MOTOR_DELAY_CYCLES {
            let _ = datasette.advance_phi2_cycle();
        }
        assert!(datasette.motor_on());

        // Baseline high, then a train of falling edges spaced by known intervals.
        datasette.set_write_line(true);
        let pulse = |datasette: &mut Datasette, cycles: u32| {
            for _ in 0..cycles {
                let _ = datasette.advance_phi2_cycle();
            }
            datasette.set_write_line(false);
            datasette.set_write_line(true);
        };
        pulse(&mut datasette, 200); // first falling edge — starts recording
        pulse(&mut datasette, 200); // records ~200
        pulse(&mut datasette, 400); // records ~400

        let tap = datasette
            .recorded_tap_image()
            .expect("writable tape should expose its recording");
        assert_eq!(tap.pulses.len(), 2);
        assert!((199..=201).contains(&tap.pulses[0]), "{:?}", tap.pulses);
        assert!((399..=401).contains(&tap.pulses[1]), "{:?}", tap.pulses);
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

    #[test]
    fn repeated_motor_level_does_not_restart_transition_delay() {
        let mut datasette = Datasette::new();
        datasette.load_tap(stub_tape(&[8]));
        datasette.play();
        datasette.set_motor_on(true);

        for _ in 0..1_000 {
            let _ = datasette.advance_phi2_cycle();
        }
        datasette.set_motor_on(true);
        for _ in 1_000..MOTOR_DELAY_CYCLES {
            let _ = datasette.advance_phi2_cycle();
        }

        assert!(
            datasette.motor_on(),
            "reasserting the same line level must not restart spin-up"
        );
    }
}
