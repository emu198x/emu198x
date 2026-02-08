//! Video chip trait for Spectrum family.
//!
//! All Spectrum variants have a video chip that generates display output
//! and may inject memory contention. The trait abstracts the differences
//! between ULA (Sinclair), SCLD (Timex), and custom implementations
//! (Pentagon, Next).

use crate::memory::SpectrumMemory;

/// Video chip interface for all Spectrum variants.
///
/// The video chip ticks at 7 MHz (one pixel per tick) and owns the
/// framebuffer. It generates INT signals, tracks beam position, and
/// provides contention delays for ULA-based models.
pub trait SpectrumVideo {
    /// Advance the video chip by one pixel clock tick (7 MHz).
    ///
    /// The memory reference is used to fetch VRAM data during active display.
    fn tick(&mut self, memory: &dyn SpectrumMemory);

    /// Return contention wait states for a memory access at the current beam
    /// position.
    ///
    /// Standard ULA returns 0-6 based on position in the contention pattern;
    /// SCLD, Pentagon, and Next return 0.
    fn contention(&self, addr: u16, memory: &dyn SpectrumMemory) -> u8;

    /// Return contention wait states for an I/O access at the current beam
    /// position.
    fn io_contention(&self, port: u16, memory: &dyn SpectrumMemory) -> u8;

    /// Is the INT signal currently asserted?
    fn int_active(&self) -> bool;

    /// Has the frame completed? Auto-clears on read.
    fn take_frame_complete(&mut self) -> bool;

    /// Total T-states per scanline (224 for all known Sinclair/Timex variants).
    fn tstates_per_line(&self) -> u16;

    /// Total scanlines per frame (312 for Sinclair/Timex/Scorpion, 320 for Pentagon).
    fn lines_per_frame(&self) -> u16;

    /// Current scanline (0-based).
    fn line(&self) -> u16;

    /// Current T-state within the current scanline.
    fn line_tstate(&self) -> u16;

    /// Reference to the framebuffer (ARGB32).
    fn framebuffer(&self) -> &[u32];

    /// Framebuffer width in pixels.
    fn framebuffer_width(&self) -> u32;

    /// Framebuffer height in pixels.
    fn framebuffer_height(&self) -> u32;

    /// Current border colour index (0-7).
    fn border_colour(&self) -> u8;

    /// Set border colour (from port $FE write).
    fn set_border_colour(&mut self, colour: u8);

    /// Return the floating bus value at the current beam position.
    ///
    /// On a real 48K, unattached port reads leak the ULA's data bus through
    /// 470-ohm resistors. The value depends on what the ULA is fetching:
    ///   T+0: bitmap byte, T+1: attribute byte,
    ///   T+2: bitmap+1 byte, T+3: attribute+1 byte,
    ///   T+4..T+7: $FF (idle).
    /// During border/vblank, returns $FF.
    fn floating_bus(&self, memory: &dyn SpectrumMemory) -> u8;
}
