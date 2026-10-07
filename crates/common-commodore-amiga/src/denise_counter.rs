//! Denise's normal RGA strobe and horizontal-counter commit stages.
//!
//! The counter advances at the existing lores output clock, independently of
//! Agnus's beam. See `amiga-denise-horizontal-counter.md` for source provenance.

pub use commodore_agnus_ocs::DmaStrobe as DeniseStrobe;

/// Saved state of the normal strobe stage and the two-tick counter commit.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeniseHorizontalCounter {
    current: u16,
    next: u16,
    incoming: Option<DeniseStrobe>,
    pending: Option<DeniseStrobe>,
    second_tick: bool,
}

impl DeniseHorizontalCounter {
    /// Strobe retiring from the normal RGA stage before this CCK's output.
    /// The second lores tick must not deliver it again.
    #[must_use]
    pub const fn output_strobe(&self) -> Option<DeniseStrobe> {
        if self.second_tick { None } else { self.pending }
    }

    /// Capture a strobe actually serviced on the RGA bus, before output.
    pub fn service_strobe(&mut self, strobe: DeniseStrobe) {
        assert!(
            !self.second_tick,
            "Denise strobe serviced between output ticks"
        );
        assert!(
            self.incoming.is_none(),
            "two Denise strobes in one RGA cell"
        );
        self.incoming = Some(strobe);
    }

    /// Start one existing output tick and return its nine-bit comparison position.
    /// A strobe from this CCK enters the normal stage; only the preceding cell
    /// can replace the value committed after the second output tick.
    pub fn begin_output_tick(&mut self, resets_on_equalisation: bool) -> u16 {
        if !self.second_tick {
            self.next = (self.current + 2) & 511;
            if let Some(strobe) = self.pending.take()
                && (resets_on_equalisation || strobe != DeniseStrobe::Equalisation)
            {
                self.next = 2;
            }
            self.pending = self.incoming.take();
        }
        self.current
    }

    /// Retire the output tick without advancing any other chip or the master clock.
    pub fn end_output_tick(&mut self) {
        self.current = if self.second_tick {
            self.next
        } else {
            (self.current + 1) & 511
        };
        self.second_tick = !self.second_tick;
    }

    #[must_use]
    pub const fn position(&self) -> u16 {
        self.current
    }

    /// Counter seen by next-position comparators during the upcoming output.
    /// On the second tick this is the selected commit value, including a
    /// strobe reset, rather than an increment of the old output position.
    #[must_use]
    pub const fn next_comparison_position(&self) -> u16 {
        if self.second_tick {
            self.next
        } else {
            (self.current + 1) & 511
        }
    }

    /// Reject out-of-range saved comparison/commit state before installing it.
    pub fn validate(&self) -> Result<(), String> {
        if self.current > 511 || self.next > 511 {
            return Err("invalid Denise horizontal counter".into());
        }
        if self.second_tick && self.incoming.is_some() {
            return Err("unconsumed Denise strobe between output ticks".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cck(counter: &mut DeniseHorizontalCounter, enhanced: bool) -> [u16; 2] {
        let first = counter.begin_output_tick(enhanced);
        counter.end_output_tick();
        let second = counter.begin_output_tick(enhanced);
        counter.end_output_tick();
        [first, second]
    }

    fn restore(counter: DeniseHorizontalCounter) -> DeniseHorizontalCounter {
        postcard::from_bytes(&postcard::to_allocvec(&counter).expect("encode")).expect("decode")
    }

    #[test]
    fn serviced_strobe_crosses_normal_rga_then_commits_after_two_output_ticks() {
        for enhanced in [false, true] {
            for strobe in [DeniseStrobe::Horizontal, DeniseStrobe::VerticalBlank] {
                let mut counter = DeniseHorizontalCounter::default();
                for _ in 0..20 {
                    cck(&mut counter, enhanced);
                }
                counter.service_strobe(strobe);
                counter = restore(counter);
                assert_eq!(cck(&mut counter, enhanced), [40, 41]);
                counter = restore(counter);
                assert_eq!(counter.begin_output_tick(enhanced), 42);
                counter.end_output_tick();
                counter = restore(counter);
                assert_eq!(counter.begin_output_tick(enhanced), 43);
                counter.end_output_tick();
                assert_eq!(cck(&mut counter, enhanced), [2, 3]);
                assert_eq!(cck(&mut counter, enhanced), [4, 5]);
            }
        }
    }

    #[test]
    fn ocs_equalisation_free_runs_through_nine_bit_wrap() {
        for enhanced in [false, true] {
            let mut counter = DeniseHorizontalCounter::default();
            for _ in 0..255 {
                cck(&mut counter, enhanced);
            }
            counter.service_strobe(DeniseStrobe::Equalisation);
            assert_eq!(cck(&mut counter, enhanced), [510, 511]);
            assert_eq!(cck(&mut counter, enhanced), [0, 1]);
            assert_eq!(cck(&mut counter, enhanced), [2, 3]);
            // A second strobe away from wrap distinguishes a reset from free-running.
            for _ in 0..30 {
                cck(&mut counter, enhanced);
            }
            counter.service_strobe(DeniseStrobe::Equalisation);
            assert_eq!(cck(&mut counter, enhanced), [64, 65]);
            assert_eq!(cck(&mut counter, enhanced), [66, 67]);
            assert_eq!(
                cck(&mut counter, enhanced),
                if enhanced { [2, 3] } else { [68, 69] }
            );
        }
    }

    #[test]
    fn reference_refresh_service_at_three_reaches_counter_two_at_five() {
        let mut counter = DeniseHorizontalCounter::default();
        for h in 0..3 {
            assert_eq!(cck(&mut counter, true), [h * 2, h * 2 + 1]);
        }
        counter.service_strobe(DeniseStrobe::Horizontal);
        assert_eq!(cck(&mut counter, true), [6, 7]);
        assert_eq!(cck(&mut counter, true), [8, 9]);
        for h in 5..227 {
            assert_eq!(cck(&mut counter, true), [(h - 4) * 2, (h - 4) * 2 + 1]);
        }
        // Agnus wraps at 227; Denise continues until the next actual strobe.
        assert_eq!(cck(&mut counter, true), [446, 447]);
        assert_eq!(cck(&mut counter, true), [448, 449]);
    }

    #[test]
    fn next_comparator_selects_the_value_committed_after_each_output_tick() {
        for enhanced in [false, true] {
            for strobe in [
                DeniseStrobe::Horizontal,
                DeniseStrobe::VerticalBlank,
                DeniseStrobe::Equalisation,
            ] {
                let mut counter = DeniseHorizontalCounter::default();
                for _ in 0..20 {
                    cck(&mut counter, enhanced);
                }
                counter.service_strobe(strobe);
                assert_eq!(cck(&mut counter, enhanced), [40, 41]);
                assert_eq!(counter.next_comparison_position(), 43);
                assert_eq!(counter.begin_output_tick(enhanced), 42);
                counter.end_output_tick();
                counter = restore(counter);
                let resets = enhanced || strobe != DeniseStrobe::Equalisation;
                assert_eq!(counter.position(), 43);
                assert_eq!(
                    counter.next_comparison_position(),
                    if resets { 2 } else { 44 }
                );
                counter.begin_output_tick(enhanced);
                let next = counter.next_comparison_position();
                counter.end_output_tick();
                assert_eq!(counter.position(), next);
                for _ in 0..1024 {
                    let next = counter.next_comparison_position();
                    counter.begin_output_tick(enhanced);
                    counter.end_output_tick();
                    assert_eq!(
                        counter.position(),
                        next,
                        "next comparison must follow nine-bit wrap"
                    );
                }
            }
        }
    }

    #[test]
    fn malformed_saved_counters_are_rejected() {
        for (current, next) in [(512, 2), (2, 512), (u16::MAX, 0)] {
            let counter = restore(DeniseHorizontalCounter {
                current,
                next,
                ..Default::default()
            });
            assert!(counter.validate().is_err());
        }
    }
}
