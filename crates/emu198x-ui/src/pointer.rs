//! Turning the host cursor into mouse counts for the guest.
//!
//! A real mouse reports how far it moved, not where it is, so the guest owns
//! its pointer's position. The harness has two ways to keep that pointer under
//! the host cursor:
//!
//! - [`RelativeMotion`] sends the cursor's own movement, in the guest's mouse
//!   counts. It carries the fraction of a count each event leaves over, so
//!   slow movement is not lost to rounding. It cannot correct for guest
//!   acceleration or for the guest pointer stopping at a screen edge.
//! - [`PointerSync`] closes the loop for a machine that can report where its
//!   pointer is. Each frame it compares the guest pointer with the cursor and
//!   sends the counts that close part of the gap, so the two meet whatever the
//!   guest's units, acceleration or edge clamping.

/// Fraction of the remaining gap each correction asks the guest to cover.
///
/// Below one so a pointer that accelerates settles instead of overshooting
/// back and forth. AROS on the Amiga settles a 300-pixel jump in about a
/// second at this gain; at 0.75 it takes a third longer.
const GAIN: f64 = 0.9;

/// Most counts one correction sends on an axis. A guest that reads its mouse
/// as an 8-bit counter difference, as the Amiga does, sees a jump larger than
/// 127 as a move the other way, so this leaves headroom below that.
const MAX_STEP: i32 = 100;

/// Frames to wait for the guest to act on a correction before sending another.
/// A guest that ignores the mouse for a while — a game that is not reading it,
/// say — must not have corrections pile up and land at once.
const MAX_WAIT_FRAMES: u32 = 4;

/// Relative cursor motion with the part-count remainder carried forward.
#[derive(Debug, Default)]
pub(crate) struct RelativeMotion {
    last: Option<(f64, f64)>,
    remainder: (f64, f64),
}

impl RelativeMotion {
    /// The whole mouse counts for the movement from the last position to
    /// `to`, both in framebuffer pixels, or `None` for the first sample and
    /// for movement that rounds to nothing yet. `counts_per_pixel` converts
    /// pixels to the counts the guest moves its pointer by.
    pub(crate) fn motion_to(
        &mut self,
        to: (f64, f64),
        counts_per_pixel: (f64, f64),
    ) -> Option<(i32, i32)> {
        let from = self.last.replace(to)?;
        let x = (to.0 - from.0) * counts_per_pixel.0 + self.remainder.0;
        let y = (to.1 - from.1) * counts_per_pixel.1 + self.remainder.1;
        let (dx, dy) = (x.round(), y.round());
        self.remainder = (x - dx, y - dy);
        let delta = (dx as i32, dy as i32);
        (delta != (0, 0)).then_some(delta)
    }

    /// Forget the baseline, as when the cursor leaves the window.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Closed-loop control of a guest pointer the machine can locate.
#[derive(Debug, Default)]
pub(crate) struct PointerSync {
    target: Option<(f64, f64)>,
    awaiting: Option<Awaiting>,
}

#[derive(Debug)]
struct Awaiting {
    from: (f64, f64),
    frames: u32,
}

impl PointerSync {
    /// Where the host cursor is, in framebuffer pixels, or `None` once it has
    /// left the window and the guest pointer should stay where it is.
    pub(crate) fn set_target(&mut self, target: Option<(f64, f64)>) {
        self.target = target;
    }

    /// Whether the cursor is over the window, so the guest pointer stands in
    /// for it.
    pub(crate) fn is_active(&self) -> bool {
        self.target.is_some()
    }

    /// The counts to send after a frame in which the guest pointer was at
    /// `guest`, or `None` when it is close enough or still acting on the last
    /// correction. `counts_per_pixel` converts framebuffer pixels to the mouse
    /// counts the guest moves its pointer by.
    pub(crate) fn correction(
        &mut self,
        guest: (f64, f64),
        counts_per_pixel: (f64, f64),
    ) -> Option<(i32, i32)> {
        let target = self.target?;
        if let Some(awaiting) = &mut self.awaiting {
            if awaiting.from == guest && awaiting.frames < MAX_WAIT_FRAMES {
                awaiting.frames += 1;
                return None;
            }
            self.awaiting = None;
        }
        let step = |gap: f64, scale: f64| {
            let counts = gap * scale;
            // Half a count or less is as close as whole counts can get: one
            // more would overshoot by as much, and the next correction would
            // send it back.
            if counts.abs() <= 0.5 {
                return 0;
            }
            // At least one count, or a small gap would never close.
            let scaled = (counts * GAIN).round();
            let scaled = if scaled == 0.0 {
                counts.signum()
            } else {
                scaled
            };
            (scaled as i32).clamp(-MAX_STEP, MAX_STEP)
        };
        let delta = (
            step(target.0 - guest.0, counts_per_pixel.0),
            step(target.1 - guest.1, counts_per_pixel.1),
        );
        if delta == (0, 0) {
            return None;
        }
        self.awaiting = Some(Awaiting {
            from: guest,
            frames: 0,
        });
        Some(delta)
    }

    /// Stop driving the guest pointer.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: (f64, f64) = (1.0, 1.0);

    #[test]
    fn the_first_sample_only_sets_the_baseline() {
        let mut motion = RelativeMotion::default();
        assert_eq!(motion.motion_to((10.0, 10.0), ONE), None);
        assert_eq!(motion.motion_to((13.0, 8.0), ONE), Some((3, -2)));
    }

    #[test]
    fn slow_motion_adds_up_instead_of_rounding_away() {
        let mut motion = RelativeMotion::default();
        motion.motion_to((0.0, 0.0), ONE);
        let mut moved = 0;
        for step in 1..=10 {
            if let Some((dx, _)) = motion.motion_to((f64::from(step) * 0.4, 0.0), ONE) {
                moved += dx;
            }
        }
        assert_eq!(moved, 4, "ten 0.4-pixel steps are four pixels");
    }

    #[test]
    fn motion_is_sent_in_the_guests_counts() {
        // An Atari ST in low resolution: one count is two framebuffer
        // pixels across and one down.
        let scale = (0.5, 1.0);
        let mut motion = RelativeMotion::default();
        motion.motion_to((0.0, 0.0), scale);
        assert_eq!(motion.motion_to((20.0, 20.0), scale), Some((10, 20)));
        let mut moved = 0;
        for step in 1..=10 {
            if let Some((dx, _)) = motion.motion_to((20.0 + f64::from(step), 20.0), scale) {
                moved += dx;
            }
        }
        assert_eq!(moved, 5, "ten pixels one at a time are five counts");
    }

    #[test]
    fn a_reset_drops_the_baseline_and_the_remainder() {
        let mut motion = RelativeMotion::default();
        motion.motion_to((0.0, 0.0), ONE);
        motion.motion_to((0.4, 0.0), ONE);
        motion.reset();
        assert_eq!(motion.motion_to((5.0, 5.0), ONE), None);
        assert_eq!(motion.motion_to((5.4, 5.0), ONE), None);
    }

    /// A guest that moves its pointer by `scale` pixels per count, with an
    /// optional extra multiplier for fast moves, and clamps it to a screen.
    struct Guest {
        at: (f64, f64),
        pixels_per_count: f64,
        accelerate: bool,
    }

    impl Guest {
        fn apply(&mut self, (dx, dy): (i32, i32)) {
            let boost = |d: i32| {
                if self.accelerate && d.abs() > 8 {
                    2.0
                } else {
                    1.0
                }
            };
            let x = self.at.0 + f64::from(dx) * self.pixels_per_count * boost(dx);
            let y = self.at.1 + f64::from(dy) * self.pixels_per_count * boost(dy);
            self.at = (x.clamp(0.0, 640.0), y.clamp(0.0, 512.0));
        }
    }

    fn settle(guest: &mut Guest, sync: &mut PointerSync, frames: usize) {
        let scale = 1.0 / guest.pixels_per_count;
        for _ in 0..frames {
            if let Some(delta) = sync.correction(guest.at, (scale, scale)) {
                guest.apply(delta);
            }
        }
    }

    #[test]
    fn the_guest_pointer_reaches_the_cursor() {
        let mut guest = Guest {
            at: (100.0, 100.0),
            pixels_per_count: 2.0,
            accelerate: false,
        };
        let mut sync = PointerSync::default();
        sync.set_target(Some((420.0, 37.0)));
        settle(&mut guest, &mut sync, 60);
        assert!((guest.at.0 - 420.0).abs() <= 2.0, "x at {}", guest.at.0);
        assert!((guest.at.1 - 37.0).abs() <= 2.0, "y at {}", guest.at.1);
    }

    #[test]
    fn acceleration_settles_rather_than_oscillating() {
        let mut guest = Guest {
            at: (0.0, 0.0),
            pixels_per_count: 1.0,
            accelerate: true,
        };
        let mut sync = PointerSync::default();
        sync.set_target(Some((300.0, 200.0)));
        settle(&mut guest, &mut sync, 120);
        assert!((guest.at.0 - 300.0).abs() <= 1.0, "x at {}", guest.at.0);
        assert!((guest.at.1 - 200.0).abs() <= 1.0, "y at {}", guest.at.1);
    }

    #[test]
    fn a_pointer_pinned_at_an_edge_returns_with_the_cursor() {
        let mut guest = Guest {
            at: (320.0, 256.0),
            pixels_per_count: 1.0,
            accelerate: false,
        };
        let mut sync = PointerSync::default();
        sync.set_target(Some((-50.0, 256.0)));
        settle(&mut guest, &mut sync, 60);
        assert_eq!(guest.at.0, 0.0, "the guest stops at its edge");
        sync.set_target(Some((200.0, 256.0)));
        settle(&mut guest, &mut sync, 60);
        assert!((guest.at.0 - 200.0).abs() <= 1.0, "x at {}", guest.at.0);
    }

    #[test]
    fn it_waits_for_the_guest_before_correcting_again() {
        let mut sync = PointerSync::default();
        sync.set_target(Some((100.0, 0.0)));
        assert!(sync.correction((0.0, 0.0), (1.0, 1.0)).is_some());
        for _ in 0..MAX_WAIT_FRAMES {
            assert_eq!(sync.correction((0.0, 0.0), (1.0, 1.0)), None);
        }
        assert!(
            sync.correction((0.0, 0.0), (1.0, 1.0)).is_some(),
            "a guest that ignored the correction gets another"
        );
    }

    #[test]
    fn a_pointer_half_a_count_away_is_left_alone() {
        // Two framebuffer pixels a count: a cursor one pixel off cannot be
        // matched more closely, and chasing it would hop either side.
        let mut sync = PointerSync::default();
        sync.set_target(Some((101.0, 50.0)));
        assert_eq!(sync.correction((100.0, 50.0), (0.5, 1.0)), None);
        // Over half a count away it moves.
        sync.set_target(Some((101.5, 50.0)));
        assert_eq!(sync.correction((100.0, 50.0), (0.5, 1.0)), Some((1, 0)));
    }

    #[test]
    fn steps_stay_inside_an_eight_bit_counter() {
        let mut sync = PointerSync::default();
        sync.set_target(Some((10_000.0, -10_000.0)));
        assert_eq!(
            sync.correction((0.0, 0.0), (1.0, 1.0)),
            Some((MAX_STEP, -MAX_STEP))
        );
    }

    #[test]
    fn nothing_is_sent_without_a_cursor_or_when_already_there() {
        let mut sync = PointerSync::default();
        assert_eq!(sync.correction((0.0, 0.0), (1.0, 1.0)), None);
        sync.set_target(Some((5.2, 5.0)));
        assert_eq!(sync.correction((5.0, 5.0), (1.0, 1.0)), None);
        assert!(sync.is_active());
        sync.set_target(None);
        assert!(!sync.is_active());
    }
}
