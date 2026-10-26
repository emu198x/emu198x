//! Where the guest's mouse pointer is on screen.
//!
//! Intuition, and most software with a mouse pointer, draws it with hardware
//! sprite 0. Its SPR0POS and SPR0CTL words give the pointer's top-left corner
//! in beam coordinates, which map onto the framebuffer the same way the
//! display does. A host front end uses the position to keep the guest's
//! pointer under its own cursor.

use crate::AmigaRuntimeKind;
use crate::live_access::AmigaLiveAccess;

/// First beam colour clock the framebuffer shows, as Denise's viewport sets it.
const FIRST_VISIBLE_CCK: f64 = 0x2C as f64;
/// First beam line the framebuffer shows.
const FIRST_VISIBLE_LINE: f64 = 0x19 as f64;
/// Lores pixels per colour clock.
const LORES_PER_CCK: f64 = 2.0;
/// Denise draws a sprite one lores pixel after its HSTART matches the beam.
const SPRITE_PIPELINE_LORES: f64 = 1.0;
/// Lores pixels across the framebuffer, whatever its sample rate.
const FRAMEBUFFER_LORES_WIDTH: f64 = 384.0;
/// Framebuffer rows per beam line: both interlaced fields are kept.
const ROWS_PER_LINE: f64 = 2.0;
/// Frames a pointer may go without being re-armed before it counts as gone.
/// Sprite DMA re-arms a displayed sprite every frame.
const POINTER_GRACE_FRAMES: u64 = 2;

/// The top-left corner of a sprite with these position and control words, in
/// pixels of a framebuffer `framebuffer_width` wide.
#[must_use]
pub(crate) fn sprite_framebuffer_position(
    pos: u16,
    ctl: u16,
    framebuffer_width: u32,
) -> (f64, f64) {
    let hstart = ((pos & 0x00FF) << 1) | (ctl & 0x0001);
    let vstart = (pos >> 8) | ((ctl & 0x0004) << 6);
    let samples_per_lores = f64::from(framebuffer_width) / FRAMEBUFFER_LORES_WIDTH;
    let lores_x = f64::from(hstart) - FIRST_VISIBLE_CCK * LORES_PER_CCK + SPRITE_PIPELINE_LORES;
    let y = (f64::from(vstart) - FIRST_VISIBLE_LINE) * ROWS_PER_LINE;
    (lores_x * samples_per_lores, y)
}

impl AmigaRuntimeKind {
    /// Where the guest's mouse pointer — hardware sprite 0 — was last drawn,
    /// in framebuffer pixels, or `None` when no pointer is on screen.
    ///
    /// The pointer counts as gone once sprite 0 has not been armed for a
    /// couple of frames, as when a game turns sprites off or takes sprite 0
    /// for itself.
    #[must_use]
    pub fn mouse_pointer(&self) -> Option<(f64, f64)> {
        let (snapshot, frame, seen) = match self {
            Self::Ocs(rt) => (
                rt.machine().denise_diagnostic_snapshot(),
                rt.frame_count(),
                &rt.pointer_seen,
            ),
            Self::Ecs(rt) => (
                rt.machine().denise_diagnostic_snapshot(),
                rt.frame_count(),
                &rt.pointer_seen,
            ),
            Self::Aga(rt) => (
                rt.machine().denise_diagnostic_snapshot(),
                rt.frame_count(),
                &rt.pointer_seen,
            ),
        };
        let sprite = snapshot.sprites[0];
        if sprite.shown_count == 0 {
            return None;
        }
        let (count, changed_at) = seen.get();
        if sprite.shown_count != count {
            seen.set((sprite.shown_count, frame));
        } else if frame.saturating_sub(changed_at) > POINTER_GRACE_FRAMES {
            return None;
        }
        Some(sprite_framebuffer_position(
            sprite.last_shown_position,
            sprite.last_shown_control,
            snapshot.raster_width,
        ))
    }
}

impl AmigaRuntimeKind {
    /// Mouse counts that move Intuition's pointer one framebuffer pixel
    /// across and down. One count moves it about one lores pixel across and
    /// one line down, and the framebuffer has two pixels and two rows for
    /// each. Speed and acceleration preferences change this; a host's closed
    /// loop corrects for them.
    #[must_use]
    pub fn mouse_counts_per_pixel(&self) -> (f64, f64) {
        (0.5, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aros_pointer_lands_where_its_pixels_were_drawn() {
        // AROS's pointer at HSTART 156, VSTART 99 drew its first pixel at
        // (138, 148) of the 768 × 576 OCS framebuffer.
        let pos = (99 << 8) | (156 >> 1);
        let ctl = 115 << 8; // VSTOP 115, HSTART bit 0 clear
        assert_eq!(sprite_framebuffer_position(pos, ctl, 768), (138.0, 148.0));
    }

    #[test]
    fn the_low_hstart_bit_and_the_high_vstart_bit_are_read() {
        let (x, y) = sprite_framebuffer_position(0x0040, 0x0005, 768);
        // HSTART = 0x81, VSTART = 0x100.
        assert_eq!(x, (f64::from(0x81) - 88.0 + 1.0) * 2.0);
        assert_eq!(y, (256.0 - 25.0) * 2.0);
    }

    #[test]
    fn a_superhires_framebuffer_doubles_the_horizontal_scale() {
        let pos = (99 << 8) | (156 >> 1);
        assert_eq!(sprite_framebuffer_position(pos, 0, 1536).0, 276.0);
    }
}
