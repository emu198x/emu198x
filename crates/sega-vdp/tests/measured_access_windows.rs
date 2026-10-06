//! CPU-to-VRAM access timing, checked against what has been measured on real
//! Master Systems.
//!
//! The data port does not reach VRAM at once. The VDP performs each access in
//! a memory cycle its scan engine leaves free, and the chip has no wait output,
//! so a program that writes faster than those cycles come round loses bytes.
//!
//! - Sega's *Software Reference Manual for the Sega Mark III* (1986), Hardware
//!   Reference Manual p. 12, at
//!   `reference/by-system/sega-master-system/1986-sega-mark-iii-software-reference-manual.source.json`:
//!   "The VDP chip cannot process data any faster than the following rates:
//!   16 Z80A T-States during VBLANK, 29 Z80A T-States during active video."
//!   It applies this to VRAM and CRAM.
//! - The figure is conservative. Maxim tested an SMS2 and found it "could be
//!   accessed as fast as every 26 CPU cycles without corruption" (SMS Power!
//!   forum topic 10523, 2007-10-30). sverx reported tests from 2014 in which
//!   "anything faster than 26 cycles would anyway lead to corruption", for
//!   VRAM and CRAM, with no problem writing the address registers (topic
//!   16298, 2016-09-11). So 26 cycles is clean and 25 is not.
//! - Outside the active display "there are no timing restrictions": a Z80
//!   cannot write fast enough to outrun the chip (TmEE, topic 14599,
//!   2013-10-20; Maxim, topic 10523).
//!
//! These tests drive the data port as a Z80 at 3.58 MHz would, one access
//! every `N` CPU cycles at 1.5 dots each, and read back what arrived.

use sega_vdp::{SegaVdp, VdpRegion, VdpVariant};

/// Dots in a scan line.
const DOTS_PER_LINE: usize = 342;

fn reg(vdp: &mut SegaVdp, index: u8, value: u8) {
    vdp.write_control(value);
    vdp.write_control(0x80 | index);
}

#[derive(Debug, Clone, Copy)]
enum Setup {
    /// Mode 4, 192 lines.
    Mode4,
    /// Mode 4, 224 lines (315-5246).
    Mode4Lines224,
    /// Mode 4, 240 lines (315-5246).
    Mode4Lines240,
    /// Mode 4 with the display blanked (R1 bit 6 clear).
    Blanked,
}

impl Setup {
    /// (R0, R1, active lines) for this setup.
    fn registers(self) -> (u8, u8, usize) {
        match self {
            Self::Mode4 => (0x04, 0xC0, 192),
            Self::Mode4Lines224 => (0x06, 0xD0, 224),
            Self::Mode4Lines240 => (0x06, 0xC8, 240),
            Self::Blanked => (0x04, 0x80, 192),
        }
    }
}

/// A Mode 4 VDP set up as `setup` and run to dot 0 of `line`.
fn vdp_at(setup: Setup, variant: VdpVariant, line: usize) -> SegaVdp {
    let mut vdp = SegaVdp::new(VdpRegion::Ntsc, variant);
    let (r0, r1, _) = setup.registers();
    reg(&mut vdp, 0, r0);
    reg(&mut vdp, 1, r1);
    for _ in 0..line * DOTS_PER_LINE {
        vdp.tick();
    }
    assert_eq!(usize::from(vdp.scanline()), line);
    vdp
}

/// The byte the `i`th write stores. Consecutive values differ, so a lost write
/// shows up as another write's byte in its place.
fn pattern(i: usize) -> u8 {
    #[allow(clippy::cast_possible_truncation)]
    let byte = (i % 251) as u8;
    byte.wrapping_add(1)
}

/// Make `count` data-port writes, one every `spacing` Z80 cycles, ticking the
/// chip between them, then give the last one time to land.
fn write_burst(vdp: &mut SegaVdp, spacing: usize, count: usize) {
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
    for _ in 0..DOTS_PER_LINE {
        vdp.tick();
    }
}

/// Write `count` bytes to VRAM from `$0000`, one every `spacing` cycles from
/// dot 0 of `start_line`. Returns how many reached VRAM intact before the
/// first that did not.
fn burst(setup: Setup, start_line: usize, spacing: usize, count: usize) -> usize {
    let mut vdp = vdp_at(setup, VdpVariant::Sms2, start_line);
    vdp.write_control(0x00);
    vdp.write_control(0x40);
    write_burst(&mut vdp, spacing, count);
    (0..count)
        .take_while(|&i| vdp.vram()[i] == pattern(i))
        .count()
}

/// Does a burst through the whole active display, one write every `spacing`
/// cycles, lose anything?
fn loses_writes(setup: Setup, spacing: usize) -> bool {
    let (_, _, lines) = setup.registers();
    let count = (lines - 1) * DOTS_PER_LINE * 2 / (3 * spacing);
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
fn mode_4_loses_writes_from_an_unpaced_copy_loop_during_active_display() {
    // 16 cycles is `OUTI`, the usual unrolled copy. Sega's manual allows it
    // only in vertical blanking.
    let count = 64;
    let intact = burst(Setup::Mode4, 10, 16, count);
    assert!(
        intact < count,
        "all {count} writes landed 16 cycles apart during the active display"
    );
}

#[test]
fn the_same_burst_lands_intact_with_the_display_blanked() {
    let count = 64;
    assert_eq!(burst(Setup::Blanked, 10, 16, count), count);
}

#[test]
fn the_same_burst_lands_intact_in_the_vertical_border() {
    let count = 64;
    assert_eq!(burst(Setup::Mode4, 200, 16, count), count);
}

#[test]
fn every_mode_4_height_takes_writes_26_cycles_apart_and_loses_them_at_25() {
    // Maxim: 26 cycles clean on an SMS2. sverx: anything faster corrupts.
    for setup in [Setup::Mode4, Setup::Mode4Lines224, Setup::Mode4Lines240] {
        assert_eq!(widest_failing_spacing(setup), Some(25), "{setup:?}");
    }
}

#[test]
fn a_blanked_display_takes_writes_as_fast_as_a_z80_can_send_them() {
    assert_eq!(widest_failing_spacing(Setup::Blanked), None);
}

#[test]
fn the_315_5124_has_the_same_limit() {
    // The measurements are from an SMS2. Nothing separates the first chip's
    // CPU interface from the second's, so the same limit holds until
    // something does.
    let count = (191 * DOTS_PER_LINE * 2) / (3 * 25);
    let mut vdp = vdp_at(Setup::Mode4, VdpVariant::Sms1, 0);
    vdp.write_control(0x00);
    vdp.write_control(0x40);
    write_burst(&mut vdp, 25, count);
    assert!((0..count).any(|i| vdp.vram()[i] != pattern(i)));
}

#[test]
fn master_system_cram_writes_are_lost_the_same_way() {
    // MacDonald, "Sega Master System VDP documentation" §15: rapid CRAM
    // writes during the active display on an SMS 2 are sometimes "written to
    // the wrong address or [...] not written altogether". sverx's tests found
    // the same limit for CRAM as for VRAM.
    let mut vdp = vdp_at(Setup::Mode4, VdpVariant::Sms2, 10);
    vdp.write_control(0x00);
    vdp.write_control(0xC0);
    write_burst(&mut vdp, 16, 32);
    assert!(
        (0..32).any(|i| vdp.cram()[i] != pattern(i)),
        "32 CRAM writes 16 cycles apart all landed during the active display"
    );
}

#[test]
fn game_gear_cram_takes_writes_as_fast_as_a_z80_can_send_them() {
    // MacDonald §15: the program that lost CRAM writes on an SMS 2 "runs
    // fine on a Genesis and Game Gear".
    let mut vdp = SegaVdp::new_game_gear();
    reg(&mut vdp, 0, 0x04);
    reg(&mut vdp, 1, 0xC0);
    for _ in 0..10 * DOTS_PER_LINE {
        vdp.tick();
    }
    vdp.write_control(0x00);
    vdp.write_control(0xC0);
    write_burst(&mut vdp, 11, 64);
    let cram = vdp.cram();
    assert!((0..64).all(|i| cram[i] == pattern(i)), "{cram:?}");
}

#[test]
fn every_data_port_access_advances_the_address_however_fast() {
    // MacDonald (SMS Power! topic 13374, 2011-10-19): reads can be mixed in
    // with writes "to advance the VRAM address as much as you need [...] you
    // can read faster than the VDP can provide data", the data read being
    // garbage. So an access that comes too soon still moves the address on.
    let mut vdp = vdp_at(Setup::Mode4, VdpVariant::Sms2, 10);
    vdp.write_control(0x00);
    vdp.write_control(0x40);
    // Twenty accesses 11 cycles apart, alternately writes and reads.
    let mut half_dots = 0;
    for i in 0..20 {
        if i % 2 == 0 {
            vdp.write_data(0x11);
        } else {
            let _ = vdp.read_data();
        }
        half_dots += 3 * 11;
        while half_dots >= 2 {
            vdp.tick();
            half_dots -= 2;
        }
    }
    // A write long after the burst lands at $0014.
    for _ in 0..DOTS_PER_LINE {
        vdp.tick();
    }
    vdp.write_data(0xA5);
    for _ in 0..DOTS_PER_LINE {
        vdp.tick();
    }
    assert_eq!(vdp.vram()[0x0014], 0xA5);
}

#[test]
fn a_read_straight_after_address_setup_returns_the_old_buffer() {
    // MacDonald, topic 13374: "Normally you need a bit of a delay to read
    // valid data from VRAM". Setting a read address starts the fetch; it
    // lands in a free memory cycle, not at once.
    let mut vdp = vdp_at(Setup::Mode4, VdpVariant::Sms2, 10);
    vdp.write_vram(0x0100, 0xA5);
    // Find a moment where the fetch has to wait: try each dot of a window's
    // span, reading straight after the setup.
    let mut stale = false;
    for _ in 0..32 {
        vdp.write_control(0x00);
        vdp.write_control(0x01);
        if vdp.read_data() != 0xA5 {
            stale = true;
        }
        for _ in 0..DOTS_PER_LINE + 1 {
            vdp.tick();
        }
    }
    assert!(stale, "the byte was always there at once");

    vdp.write_control(0x00);
    vdp.write_control(0x01);
    for _ in 0..40 {
        vdp.tick();
    }
    assert_eq!(vdp.read_data(), 0xA5, "40 dots covers the longest wait");
}
