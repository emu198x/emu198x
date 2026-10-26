//! Where GEM's mouse pointer is on screen.
//!
//! The VDI keeps the pointer's hot spot in two line-A variables, GCURX and
//! GCURY, words 602 and 600 bytes below the line-A base, with M_HID_CT, how
//! many times the pointer has been hidden, at 598 below it (*The Atari
//! Compendium*, Scott Sanders, 1992, chapter 8, "Line-A", its table of the
//! line-A variables; EmuTOS's `bios/lineavars.S` lays them out the same way).
//! The VDI's mouse handler adds each IKBD packet's movement to them and clips
//! them to the screen, in screen pixels: one mouse count, one pixel. The hot
//! spot of GEM's arrow is the top-left corner of its outline.
//!
//! Those addresses only mean something while GEM's handler owns the mouse.
//! A game may replace the handler, switch the mouse off or take over the
//! memory, and the variables then hold whatever they held last. So the
//! pointer is only reported once the variables have been seen to follow the
//! counts sent, and stops being reported when they stop following them.

use machine_atari_st::AtariSt;

/// Offsets from the line-A base of the variables read here.
const GCURX: u32 = 602;
const GCURY: u32 = 600;
const M_HID_CT: u32 = 598;
/// The line-A exception vector, which changes when a new handler, and so
/// possibly a new set of variables, is installed.
const LINE_A_VECTOR: u32 = 0x28;
/// Frames counts may go unanswered before the variables count as dead. The
/// IKBD reports every 10 ms and the VDI acts in the interrupt, so a live
/// pointer moves within a frame.
const STALE_FRAMES: u32 = 10;
/// Frames between attempts to find the line-A base when one fails.
const RETRY_FRAMES: u32 = 50;

/// What the shifter shows, as the VDI addresses it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Screen {
    /// Pixels across and down.
    size: (i16, i16),
    /// Framebuffer pixels per screen pixel, across and down.
    scale: (f64, f64),
    /// The framebuffer position of the screen's top-left pixel.
    origin: (f64, f64),
}

impl Screen {
    fn of(machine: &AtariSt) -> Option<Self> {
        let (size, scale) = if machine.frame_width() == machine_atari_st::MONO_WIDTH {
            ((640, 400), (1.0, 1.0))
        } else {
            match machine.shifter.read_resolution() {
                0 => ((320, 200), (2.0, 1.0)),
                1 => ((640, 200), (1.0, 1.0)),
                // High resolution on a colour monitor shows nothing usable.
                _ => return None,
            }
        };
        let (x, y) = machine.display_origin();
        Some(Self {
            size,
            scale,
            origin: (x as f64, y as f64),
        })
    }

    fn contains(&self, (x, y): (i16, i16)) -> bool {
        (0..self.size.0).contains(&x) && (0..self.size.1).contains(&y)
    }

    /// The centre of screen pixel `(x, y)` in framebuffer pixels.
    fn to_framebuffer(self, (x, y): (i16, i16)) -> (f64, f64) {
        (
            self.origin.0 + (f64::from(x) + 0.5) * self.scale.0,
            self.origin.1 + (f64::from(y) + 0.5) * self.scale.1,
        )
    }
}

/// The line-A base found for one line-A handler.
#[derive(Clone, Copy, Debug)]
struct LineA {
    vector: u32,
    base: Option<u32>,
    found_at: u64,
}

/// Watches GEM's pointer variables for evidence that they are live.
#[derive(Debug, Default)]
pub(crate) struct GemPointer {
    line_a: Option<LineA>,
    frame: u64,
    last: Option<(i16, i16)>,
    /// Counts sent since the variables last moved.
    pending: (i32, i32),
    /// Frames the variables have stood still with counts pending.
    pending_frames: u32,
    live: bool,
}

impl GemPointer {
    /// Mouse counts went to the IKBD.
    pub(crate) fn sent(&mut self, dx: i32, dy: i32) {
        self.pending.0 = self.pending.0.saturating_add(dx);
        self.pending.1 = self.pending.1.saturating_add(dy);
    }

    /// Look at the variables after a frame.
    pub(crate) fn after_frame(&mut self, machine: &AtariSt) {
        self.frame += 1;
        let Some(screen) = Screen::of(machine) else {
            self.forget_position();
            return;
        };
        let vector = peek_long(machine, LINE_A_VECTOR);
        let stale = match self.line_a {
            None => true,
            Some(line_a) => {
                line_a.vector != vector
                    || (line_a.base.is_none()
                        && self.frame.saturating_sub(line_a.found_at) >= u64::from(RETRY_FRAMES))
            }
        };
        // Finding the base runs TOS's handler, so only do it once there are
        // counts to check the variables against.
        if stale && self.pending != (0, 0) {
            self.line_a = Some(LineA {
                vector,
                base: machine.line_a_base(),
                found_at: self.frame,
            });
            self.forget_position();
        }
        let Some(base) = self.line_a.and_then(|line_a| line_a.base) else {
            return;
        };
        self.observe(cursor(machine, base), screen);
    }

    /// Weigh the variables' new value against the counts sent.
    fn observe(&mut self, now: (i16, i16), screen: Screen) {
        let last = self.last.replace(now);
        if !screen.contains(now) {
            self.live = false;
            self.pending = (0, 0);
            return;
        }
        let Some(last) = last else {
            // Counts sent before there was a baseline are already in it.
            self.pending = (0, 0);
            return;
        };
        let moved = (i32::from(now.0 - last.0), i32::from(now.1 - last.1));
        if moved != (0, 0) {
            if self.pending != (0, 0) {
                let follows = |moved: i32, sent: i32| moved == 0 || moved.signum() == sent.signum();
                self.live = follows(moved.0, self.pending.0) && follows(moved.1, self.pending.1);
            }
            self.pending = (0, 0);
            self.pending_frames = 0;
            return;
        }
        if self.pending == (0, 0) {
            return;
        }
        // Counts pushing a pointer into the edge it is already at are no
        // evidence either way.
        let pinned = |at: i16, sent: i32, size: i16| {
            sent == 0 || (sent < 0 && at == 0) || (sent > 0 && at == size - 1)
        };
        if pinned(now.0, self.pending.0, screen.size.0)
            && pinned(now.1, self.pending.1, screen.size.1)
        {
            self.pending = (0, 0);
            return;
        }
        self.pending_frames += 1;
        if self.pending_frames > STALE_FRAMES {
            self.live = false;
            self.pending = (0, 0);
        }
    }

    fn forget_position(&mut self) {
        self.live = false;
        self.last = None;
    }

    /// Where GEM's pointer is, in framebuffer pixels, or `None` when the
    /// variables have not been seen to follow the mouse or the pointer is
    /// hidden.
    pub(crate) fn position(&self, machine: &AtariSt) -> Option<(f64, f64)> {
        if !self.live {
            return None;
        }
        let base = self.line_a?.base?;
        let screen = Screen::of(machine)?;
        let at = cursor(machine, base);
        if peek_word(machine, base - M_HID_CT) != 0 || !screen.contains(at) {
            return None;
        }
        Some(screen.to_framebuffer(at))
    }
}

/// Mouse counts per framebuffer pixel, across and down: one count is one
/// screen pixel, and a low-resolution pixel is two framebuffer pixels wide.
#[must_use]
pub(crate) fn counts_per_pixel(machine: &AtariSt) -> (f64, f64) {
    Screen::of(machine).map_or((1.0, 1.0), |screen| {
        (1.0 / screen.scale.0, 1.0 / screen.scale.1)
    })
}

fn cursor(machine: &AtariSt, base: u32) -> (i16, i16) {
    (
        peek_word(machine, base - GCURX) as i16,
        peek_word(machine, base - GCURY) as i16,
    )
}

fn peek_word(machine: &AtariSt, address: u32) -> u16 {
    u16::from_be_bytes([machine.peek(address), machine.peek(address + 1)])
}

fn peek_long(machine: &AtariSt, address: u32) -> u32 {
    u32::from(peek_word(machine, address)) << 16 | u32::from(peek_word(machine, address + 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOW: Screen = Screen {
        size: (320, 200),
        scale: (2.0, 1.0),
        origin: (64.0, 29.0),
    };

    fn watching() -> GemPointer {
        let mut pointer = GemPointer::default();
        pointer.observe((100, 100), LOW);
        pointer
    }

    #[test]
    fn variables_that_follow_the_counts_are_live() {
        let mut pointer = watching();
        pointer.sent(5, -3);
        pointer.observe((105, 97), LOW);
        assert!(pointer.live);
    }

    #[test]
    fn variables_that_move_the_wrong_way_are_not() {
        let mut pointer = watching();
        pointer.sent(5, 0);
        pointer.observe((95, 100), LOW);
        assert!(!pointer.live);
    }

    #[test]
    fn moving_with_no_counts_sent_proves_nothing() {
        let mut pointer = watching();
        pointer.observe((10, 10), LOW);
        assert!(!pointer.live);
    }

    #[test]
    fn variables_that_stop_following_go_dead() {
        let mut pointer = watching();
        pointer.sent(5, 0);
        pointer.observe((105, 100), LOW);
        assert!(pointer.live);
        pointer.sent(5, 0);
        for _ in 0..=STALE_FRAMES {
            pointer.observe((105, 100), LOW);
        }
        assert!(!pointer.live, "a game took the mouse");
    }

    #[test]
    fn a_pointer_held_at_an_edge_stays_live() {
        let mut pointer = GemPointer::default();
        pointer.observe((1, 100), LOW);
        pointer.sent(-5, 0);
        pointer.observe((0, 100), LOW);
        assert!(pointer.live);
        for _ in 0..3 * STALE_FRAMES {
            pointer.sent(-5, 0);
            pointer.observe((0, 100), LOW);
        }
        assert!(pointer.live);
    }

    #[test]
    fn variables_off_the_screen_are_not_a_pointer() {
        let mut pointer = watching();
        pointer.sent(5, 0);
        pointer.observe((105, 100), LOW);
        pointer.observe((-300, 100), LOW);
        assert!(!pointer.live);
    }

    #[test]
    fn a_low_resolution_pixel_is_two_framebuffer_pixels_wide() {
        assert_eq!(LOW.to_framebuffer((0, 0)), (65.0, 29.5));
        assert_eq!(LOW.to_framebuffer((319, 199)), (64.0 + 639.0, 228.5));
    }
}
