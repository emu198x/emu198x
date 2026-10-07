//! Denise-local display-window registers on the normal RGA path.
//!
//! Register delivery and horizontal equality are distinct from Agnus's vertical
//! window. Lisa compares four fractional positions before composition. See
//! `2026-ecs-output-phase-observations.md` in the primary reference.

use crate::denise_chip::HorizontalDiwComparatorPhase;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct WindowWrite {
    register: u16,
    value: u16,
    ticks: u8,
}

/// Saved display-register delivery and horizontal output stages.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeniseWindow {
    diwstrt: u16,
    diwstop: u16,
    diwhigh: u16,
    high_written: bool,
    pending: Vec<WindowWrite>,
    output_history: [bool; 4],
}

impl DeniseWindow {
    /// Capture a write relative to the board's current output tick. Normal
    /// delivery follows one CCK; ECS DIWHIGH has an additional half CCK.
    pub fn queue_write(
        &mut self,
        register: u16,
        value: u16,
        before_output: bool,
        enhanced: bool,
        lisa: bool,
    ) -> bool {
        if !matches!(register, 0x08E | 0x090 | 0x1E4) {
            return false;
        }
        if register != 0x1E4 || enhanced {
            self.pending.push(WindowWrite {
                register,
                value,
                ticks: 1 + u8::from(before_output) + u8::from(register == 0x1E4 && !lisa),
            });
        }
        true
    }

    /// Retire register writes before comparison at this existing lores tick.
    pub fn begin_output_tick(&mut self) {
        for write in &mut self.pending {
            if write.ticks == 0 {
                match write.register {
                    0x08E => {
                        self.diwstrt = write.value;
                        self.high_written = false;
                    }
                    0x090 => {
                        self.diwstop = write.value;
                        self.high_written = false;
                    }
                    0x1E4 => {
                        self.diwhigh = write.value;
                        self.high_written = true;
                    }
                    _ => unreachable!("validated display-window register"),
                }
            }
        }
        self.pending.retain(|write| write.ticks != 0);
        for write in &mut self.pending {
            write.ticks -= 1;
        }
    }

    /// Compare all four 35 ns positions. The retained latch changes only on
    /// equality, including across strobes, line wraps and register rewrites.
    pub fn output_gates(
        &mut self,
        active: &mut bool,
        position: u16,
        lisa: bool,
        phase: HorizontalDiwComparatorPhase,
    ) -> [bool; 4] {
        let mut start = (self.diwstrt & 0xFF) * 4;
        let mut stop = (self.diwstop & 0xFF) * 4;
        if self.high_written {
            start |= ((self.diwhigh >> 5) & 1) << 10;
            stop |= ((self.diwhigh >> 13) & 1) << 10;
            if lisa {
                start |= (self.diwhigh >> 3) & 3;
                stop |= (self.diwhigh >> 11) & 3;
            }
        } else {
            stop |= 0x400;
        }
        let mut gates = [false; 4];
        for (sample, gate) in gates.iter_mut().enumerate() {
            let counter = position * 4 + sample as u16;
            if counter == start {
                *active = true;
            }
            if counter == stop {
                *active = false;
            }
            *gate = *active;
        }
        let output = match phase {
            HorizontalDiwComparatorPhase::BeforeOutput => gates,
            HorizontalDiwComparatorPhase::AfterOutput => self.output_history,
        };
        self.output_history = gates;
        output
    }

    /// Reject impossible saved register deliveries before installing a state.
    pub fn validate(&self, enhanced: bool, lisa: bool) -> Result<(), String> {
        if (!enhanced && self.high_written)
            || self.pending.iter().any(|write| {
                !matches!(write.register, 0x08E | 0x090 | 0x1E4)
                    || (!enhanced && write.register == 0x1E4)
                    || write.ticks > 2 + u8::from(write.register == 0x1E4 && !lisa)
            })
        {
            return Err("invalid pending Denise display-window stage".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(high: Option<u16>, lisa: bool) -> DeniseWindow {
        let mut window = DeniseWindow::default();
        window.queue_write(0x08E, 0x81, true, true, lisa);
        window.queue_write(0x090, 0xA1, true, true, lisa);
        if let Some(high) = high {
            window.queue_write(0x1E4, high, true, true, lisa);
        }
        for _ in 0..4 {
            window.begin_output_tick();
        }
        window
    }

    #[test]
    fn fractional_edges_retain_a_whole_lores_output_stage() {
        for fine in 0..4 {
            let mut window = settled(Some(0x2000 | (fine << 3) | (fine << 11)), true);
            let mut active = false;
            assert_eq!(
                window.output_gates(
                    &mut active,
                    0x81,
                    true,
                    HorizontalDiwComparatorPhase::AfterOutput
                ),
                [false; 4]
            );
            let start = window.output_gates(
                &mut active,
                0x82,
                true,
                HorizontalDiwComparatorPhase::AfterOutput,
            );
            assert_eq!(
                start,
                std::array::from_fn(|sample| sample >= usize::from(fine))
            );
            window.output_gates(
                &mut active,
                0x1A0,
                true,
                HorizontalDiwComparatorPhase::AfterOutput,
            );
            assert_eq!(
                window.output_gates(
                    &mut active,
                    0x1A1,
                    true,
                    HorizontalDiwComparatorPhase::AfterOutput
                ),
                [true; 4]
            );
            let stop = window.output_gates(
                &mut active,
                0x1A2,
                true,
                HorizontalDiwComparatorPhase::AfterOutput,
            );
            assert_eq!(
                stop,
                std::array::from_fn(|sample| sample < usize::from(fine))
            );
        }
    }

    #[test]
    fn normal_stage_preserves_match_before_a_start_rewrite_arrives() {
        let mut window = settled(None, true);
        let mut active = false;
        window.queue_write(0x08E, 0x41, true, true, true);
        window.begin_output_tick();
        window.output_gates(
            &mut active,
            0x80,
            true,
            HorizontalDiwComparatorPhase::AfterOutput,
        );
        window.begin_output_tick();
        window.output_gates(
            &mut active,
            0x81,
            true,
            HorizontalDiwComparatorPhase::AfterOutput,
        );
        assert!(
            active,
            "old start still matches before normal register delivery"
        );
        window.begin_output_tick();
        assert_eq!(window.diwstrt, 0x41);
        assert!(active, "a rewrite cannot undo an equality event");
    }

    #[test]
    fn explicit_high_changes_both_halves_and_legacy_write_clears_it() {
        let mut window = settled(Some(0x20), false);
        let mut active = false;
        let phase = HorizontalDiwComparatorPhase::BeforeOutput;
        assert_eq!(
            window.output_gates(&mut active, 0x81, false, phase),
            [false; 4]
        );
        assert_eq!(
            window.output_gates(&mut active, 0x181, false, phase),
            [true; 4]
        );
        assert_eq!(
            window.output_gates(&mut active, 0xA1, false, phase),
            [false; 4]
        );
        window.queue_write(0x090, 0xA1, false, true, false);
        window.begin_output_tick();
        window.begin_output_tick();
        assert_eq!(
            window.output_gates(&mut active, 0x81, false, phase),
            [true; 4]
        );
        assert_eq!(
            window.output_gates(&mut active, 0xA1, false, phase),
            [true; 4]
        );
        assert_eq!(
            window.output_gates(&mut active, 0x1A1, false, phase),
            [false; 4]
        );
    }
}
