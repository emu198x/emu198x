//! CPU-to-VRAM access windows, checked against the chip's own data manual.
//!
//! TMS9918A/9928A/9929A data manual (November 1982), §2.1.5 and Table 2-2,
//! held at `reference/by-topic/vdp-tms9918/`:
//!
//! > The worst case time between windows occurs during the Graphics I or
//! > Graphics II mode when sprites are being used. During the active display,
//! > CPU windows occur once every 16 memory cycles [...] In the Text mode the
//! > CPU windows occur at least once out of every three memory cycles [...]
//! > The first situation occurs when the blank bit of register 1 is 0. [...]
//! > the VDP does not have to wait for a CPU access window at any time.
//!
//! The chip has no wait output, so nothing slows the CPU down: a program that
//! writes faster than the windows come round loses bytes. These tests drive
//! the data port the way a Z80 at 3.58 MHz would — one write every `N` CPU
//! cycles, 1.5 dots each — and read back what reached VRAM.
//!
//! The thresholds are the manual's worst-case gaps plus the access delay. The
//! Graphics II one is a cycle short of openMSX's measurement on a real MSX
//! (writes `N` Z80 cycles apart corrupt for `N <= 26`, clean for `N >= 27`;
//! see the comment in its `VDP::scheduleCpuVramAccess`), exactly as openMSX's
//! own model is — `CPU_ACCESS_DELAY_DOTS` in the crate says why.

use ti_tms9918::{Tms9918, VdpRegion};

/// Dots in a scan line.
const DOTS_PER_LINE: usize = 342;
/// Lines of active display.
const ACTIVE_LINES: usize = 192;

fn reg(vdp: &mut Tms9918, index: u8, value: u8) {
    vdp.write_control(value);
    vdp.write_control(0x80 | index);
}

#[derive(Debug, Clone, Copy)]
enum Setup {
    GraphicsI,
    GraphicsII,
    Multicolor,
    Text,
    Blanked,
}

impl Setup {
    /// (R0, R1) for this setup, 16K VRAM.
    fn registers(self) -> (u8, u8) {
        match self {
            Self::GraphicsI => (0x00, 0xC0),
            Self::GraphicsII => (0x02, 0xC0),
            Self::Multicolor => (0x00, 0xC8),
            Self::Text => (0x00, 0xD0),
            Self::Blanked => (0x02, 0x80),
        }
    }
}

/// The byte the `i`th write stores. Consecutive values differ, so a lost write
/// shows up as the next one's byte in its place.
fn pattern(i: usize) -> u8 {
    #[allow(clippy::cast_possible_truncation)]
    let byte = (i % 251) as u8;
    byte.wrapping_add(1)
}

/// Write `count` bytes from `$0000`, one every `spacing` Z80 cycles, starting
/// at dot 0 of `start_line`. Returns how many of them reached VRAM intact
/// before the first one that did not.
fn burst(setup: Setup, start_line: usize, spacing: usize, count: usize) -> usize {
    let mut vdp = Tms9918::new(VdpRegion::Ntsc);
    let (r0, r1) = setup.registers();
    reg(&mut vdp, 0, r0);
    reg(&mut vdp, 1, r1);
    for _ in 0..start_line * DOTS_PER_LINE {
        vdp.tick();
    }
    assert_eq!(usize::from(vdp.scanline()), start_line);

    vdp.write_control(0x00);
    vdp.write_control(0x40);

    // Half-dots owed: a Z80 cycle is 1.5 dots (5.37 MHz / 3.58 MHz).
    let mut half_dots = 0;
    for i in 0..count {
        vdp.write_data(pattern(i));
        half_dots += 3 * spacing;
        while half_dots >= 2 {
            vdp.tick();
            half_dots -= 2;
        }
    }
    // Let the last write reach its window.
    for _ in 0..DOTS_PER_LINE {
        vdp.tick();
    }

    (0..count)
        .take_while(|&i| vdp.vram()[i] == pattern(i))
        .count()
}

/// As many writes `spacing` cycles apart as fit in the active display.
fn whole_active_area(spacing: usize) -> usize {
    (ACTIVE_LINES - 1) * DOTS_PER_LINE * 2 / (3 * spacing)
}

/// Does a burst through the whole active display, one write every `spacing`
/// cycles, lose anything?
fn loses_writes(setup: Setup, spacing: usize) -> bool {
    let count = whole_active_area(spacing);
    burst(setup, 0, spacing, count) != count
}

/// The widest spacing, from 40 cycles down to 11, at which writes are lost —
/// `None` if every spacing is clean. 11 is the fastest a Z80 can write the
/// data port twice (`OUT (n),A`).
fn widest_failing_spacing(setup: Setup) -> Option<usize> {
    (11..=40)
        .rev()
        .find(|&spacing| loses_writes(setup, spacing))
}

#[test]
fn graphics_ii_loses_writes_paced_faster_than_its_windows() {
    // 18 cycles is `LD A,n` + `OUT (n),A`, the obvious unrolled copy loop.
    // On the real chip it outruns the one-in-16 windows of Graphics II.
    let count = 64;
    let intact = burst(Setup::GraphicsII, 10, 18, count);
    assert!(
        intact < count,
        "all {count} writes landed at 18 cycles apart during Graphics II active display"
    );
}

#[test]
fn the_same_burst_lands_intact_with_the_display_blanked() {
    let count = 64;
    assert_eq!(burst(Setup::Blanked, 10, 18, count), count);
}

#[test]
fn the_same_burst_lands_intact_in_the_vertical_border() {
    // Table 2-2: no window wait for 4300 µs after the vertical interrupt.
    let count = 64;
    assert_eq!(burst(Setup::GraphicsII, 200, 18, count), count);
}

#[test]
fn graphics_i_and_ii_need_26_cycles_between_writes() {
    // A window closer than the 7-dot delay is missed, so the longest wait is
    // 6 + 32 = 38 dots, 25.3 Z80 cycles: 25 cycles apart can be overtaken,
    // 26 cannot. (openMSX measured 26 failing on a real MSX; its model and
    // this one put the boundary a cycle lower.)
    for setup in [Setup::GraphicsI, Setup::GraphicsII] {
        assert_eq!(widest_failing_spacing(setup), Some(25), "{setup:?}");
    }
}

#[test]
fn multicolor_needs_24_cycles_between_writes() {
    // One window in four memory cycles across the pixels, but the sprite
    // fetches in the horizontal border leave a 30-dot gap: 6 + 30 = 36 dots,
    // 24 cycles exactly, so 23 apart can be overtaken and 24 cannot.
    assert_eq!(widest_failing_spacing(Setup::Multicolor), Some(23));
}

#[test]
fn text_mode_and_a_blanked_display_take_writes_as_fast_as_a_z80_can_send_them() {
    // Text: a window in every three memory cycles, at most 6 + 6 = 12 dots,
    // under the 16.5 of the fastest `OUT`. Blanked: refresh only, 6 + 4 = 10.
    for setup in [Setup::Text, Setup::Blanked] {
        assert_eq!(widest_failing_spacing(setup), None, "{setup:?}");
    }
}

#[test]
fn a_read_straight_after_address_setup_returns_the_old_latch() {
    // §2.1.5: "The VDP requires approximately [...] 2 microseconds following
    // address setup" to fetch the byte. A read sooner than that gets whatever
    // the latch held.
    let mut vdp = Tms9918::new(VdpRegion::Ntsc);
    reg(&mut vdp, 0, 0x02);
    reg(&mut vdp, 1, 0xC0);
    vdp.write_vram(0x0100, 0xA5);
    for _ in 0..10 * DOTS_PER_LINE {
        vdp.tick();
    }

    vdp.write_control(0x00);
    vdp.write_control(0x01); // read from $0100
    assert_ne!(vdp.read_data(), 0xA5, "the byte cannot be there yet");

    vdp.write_control(0x00);
    vdp.write_control(0x01);
    for _ in 0..40 {
        vdp.tick();
    }
    assert_eq!(
        vdp.read_data(),
        0xA5,
        "40 dots covers the worst-case window"
    );
}
