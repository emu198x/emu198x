//! Where a line interrupt's work lands on the screen.
//!
//! #212 asked whether the line interrupt is timed well enough for raster
//! splits — the Sonic parallax sky, the Out Run road, a status bar — and left
//! it open because the answer wanted a reference emulator or a known-good
//! ROM. This is neither. It is a cartridge that makes the timing *visible*,
//! and a prediction of where the split should land computed from the Z80's
//! own numbers, so what is compared is the machine against arithmetic rather
//! than against another emulator's opinion.
//!
//! The cartridge arms the line counter and has its interrupt handler change
//! the backdrop, so a frame comes out green above a line and red below it.
//! The position of that edge — which line, and how far across it — is the
//! whole measurement.

use std::path::PathBuf;

use machine_sega_master_system::{Sms, SmsVariant};

const GREEN: u32 = 0xFF00_FF00;
const RED: u32 = 0xFFFF_0000;

/// NTSC border, from `VdpRegion`.
const BORDER_LEFT: u32 = 12;
const BORDER_TOP: u32 = 25;

/// What the handler costs before the backdrop changes, and what the Z80
/// spends accepting an interrupt in mode 1. Both are stated in the
/// cartridge's build script beside the instructions they count.
const HANDLER_T_STATES: u32 = 58;
const ACCEPTANCE_T_STATES: u32 = 13;
/// The main loop is `jr $`, so an interrupt arriving mid-instruction waits
/// out the rest of its twelve T-states before it can be taken.
const WORST_CASE_WAIT_T_STATES: u32 = 12;
/// The backdrop changes partway through the final `out`, not at the end of
/// it: the bus write happens while the instruction is still running. So the
/// handler's cost is an upper bound and the write can land up to one `out`
/// earlier than counting whole instructions suggests.
const OUT_T_STATES: u32 = 11;
/// The VDP runs three dots for every two T-states.
const DOTS_PER_T_STATE: f64 = 1.5;

fn cart_with_r10(reload: u8) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/sega/synthetic-cart/master-system-raster.sms");
    let mut rom = std::fs::read(&path).expect("raster cartridge should be committed");
    // The setup writes the counter reload as `ld a,N / out ($BF),a / ld a,$8A
    // / out ($BF),a`. Patching N in place lets one cartridge answer for
    // several split positions, rather than committing a binary per line.
    let marker = [0xD3u8, 0xBF, 0x3E, 0x8A];
    let at = rom
        .windows(4)
        .position(|w| w == marker)
        .expect("the register-10 write should be in the image");
    assert!(
        at >= 2 && rom[at - 2] == 0x3E,
        "expected `ld a,N` before it"
    );
    rom[at - 1] = reload;
    rom
}

/// Run past the first frame, which is still executing the setup, and read a
/// settled one.
fn frame_with_r10(reload: u8) -> Sms {
    let mut machine = Sms::new(cart_with_r10(reload), SmsVariant::SmsNtsc);
    for _ in 0..3 {
        machine.run_frame();
    }
    machine
}

/// The split: the first line holding any red, and how far across it the
/// change lands.
fn split_of(machine: &Sms) -> (u32, Option<u32>) {
    let width = machine.framebuffer_width();
    let fb = machine.framebuffer();
    let at = |line: u32, x: u32| fb[((BORDER_TOP + line) * width + BORDER_LEFT + x) as usize];
    for line in 0..192 {
        if let Some(x) = (0..256).find(|&x| at(line, x) == RED) {
            return (line, if x == 0 { None } else { Some(x) });
        }
    }
    panic!("the handler never changed the backdrop");
}

/// The screen is green above the split and red below it, so the cartridge is
/// doing what it claims before anything is measured from it.
#[test]
fn the_cartridge_paints_a_split_screen() {
    let machine = frame_with_r10(63);
    let width = machine.framebuffer_width();
    let fb = machine.framebuffer();
    let at = |line: u32, x: u32| fb[((BORDER_TOP + line) * width + BORDER_LEFT + x) as usize];

    assert_eq!(at(0, 0), GREEN, "the top of the picture");
    assert_eq!(at(191, 255), RED, "the bottom of it");
    // One edge and one only: every line is uniform bar the one it lands on.
    let ragged = (0..192)
        .filter(|&line| (0..256).any(|x| at(line, x) != at(line, 0)))
        .collect::<Vec<_>>();
    assert_eq!(
        ragged.len(),
        1,
        "expected a single split line, got {ragged:?}"
    );
}

/// The split lands on the line after the counter underflows, and a reload of
/// `n` underflows on line `n`. So the edge is on `n + 1`, and moving the
/// reload moves the edge by exactly as much.
///
/// This is the part that would catch a phase error: a counter that fired a
/// line early or late would still produce a tidy split, just not this one.
#[test]
fn the_split_lands_one_line_after_the_counter_underflows() {
    for reload in [31u8, 63, 95, 127] {
        let machine = frame_with_r10(reload);
        let (line, _) = split_of(&machine);
        assert_eq!(
            line,
            u32::from(reload) + 1,
            "a reload of {reload} should split on line {}",
            u32::from(reload) + 1
        );
    }
}

/// How far across the line the split lands is the interrupt's latency made
/// visible, and it can be bounded without reference to any other emulator.
///
/// From the counter underflowing to the backdrop changing, the Z80 spends up
/// to twelve T-states finishing the `jr $` it was in, thirteen accepting the
/// interrupt, and fifty-eight running the handler — less however much of the
/// final `out` follows its bus write, since the register changes partway
/// through that instruction rather than at the end of it. Three dots to every
/// two T-states turns that into a window of screen positions.
///
/// The window is wide, and deliberately so: it is the honest width of what
/// can be derived, and it is still narrow enough to catch what matters. A
/// split a line out, or a handler charged twice its cost, falls outside it.
#[test]
fn the_split_lands_where_the_z80s_own_timing_puts_it() {
    let dots = |t: u32| (f64::from(t) * DOTS_PER_T_STATE) as u32;
    let earliest = dots(ACCEPTANCE_T_STATES + HANDLER_T_STATES - OUT_T_STATES);
    let latest = dots(ACCEPTANCE_T_STATES + HANDLER_T_STATES + WORST_CASE_WAIT_T_STATES);

    for reload in [31u8, 63, 95, 127] {
        let machine = frame_with_r10(reload);
        let (line, x) = split_of(&machine);
        let x = x.unwrap_or_else(|| {
            panic!("reload {reload}: split at the very start of line {line}, earlier than the handler can possibly run")
        });
        assert!(
            (earliest..=latest).contains(&x),
            "reload {reload}: split at x {x}, outside the {earliest}..={latest} the Z80's timing allows"
        );
    }
}

/// The split moves a little from one reload to the next, and the amount it
/// moves is bounded by the instruction the interrupt interrupts.
///
/// A scan line is exactly 228 T-states and `jr $` is twelve, so the loop's
/// boundaries would sit at the same place on every line — except that each
/// handler run is not a multiple of twelve and shifts the phase behind it.
/// What a game can rely on is therefore not an exact position but a position
/// to within one instruction, and that is what this pins: the spread across
/// four different split lines stays inside a single `jr $`.
///
/// A split that wandered further than that would mean the latency depended on
/// something other than the CPU — and a game drawing a status bar would see
/// its edge crawl.
#[test]
fn the_split_stays_within_one_instruction_of_itself() {
    let positions: Vec<u32> = [31u8, 63, 95, 127]
        .into_iter()
        .map(|reload| split_of(&frame_with_r10(reload)).1.expect("a split"))
        .collect();
    let (lowest, highest) = positions
        .iter()
        .fold((u32::MAX, 0), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    let spread = highest - lowest;
    let one_instruction = (f64::from(WORST_CASE_WAIT_T_STATES) * DOTS_PER_T_STATE) as u32;
    assert!(
        spread <= one_instruction,
        "splits spread {spread} dots across {positions:?}, more than the {one_instruction} one `jr $` accounts for"
    );
}
