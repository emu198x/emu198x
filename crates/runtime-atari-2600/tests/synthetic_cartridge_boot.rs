//! Fixture-free proof that the Atari 2600 starts and draws.
//!
//! Nothing pokes a framebuffer. The 2600 has neither one nor a display
//! processor: the TIA holds three playfield registers and the picture exists
//! only for as long as the program keeps rewriting them ahead of the beam. So
//! a pass here says the reset vector was read, cartridge reads landed across
//! `$F000`-`$FFFF`, and — the part no other machine in this set can prove —
//! that stores reached the TIA at the right point of a scanline.
//!
//! Built by `test-data/synthetic-cartridges/build-synthetic-cartridges.py`.

use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_atari_2600::{Atari2600Runtime, Model};

const MODEL: Model = Model::Vcs2600Ntsc;
/// Colour clocks in one NTSC frame, near enough for a sixty-frame settle.
const FRAME_TICKS: u64 = 228 * 262;

fn cartridge() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/synthetic-cartridges/atari-2600-logo.bin");
    std::fs::read(path).expect("synthetic cartridge — run build-synthetic-cartridges.py")
}

/// The displayable picture, one row a scanline.
///
/// `framebuffer` is the whole 228-clock raster, so its rows begin with 68
/// clocks of horizontal blank that are black on every line. Splitting a raster
/// row in half would put that margin entirely on the left and make any two
/// halves differ, which would pass the timing test below for no reason at all.
fn picture() -> (Vec<Vec<u32>>, usize) {
    let mut runtime =
        Atari2600Runtime::new(MODEL, cartridge()).expect("the cartridge should construct");

    let (mut frames, mut audio, mut trace) = (NullFrameSink, NullAudioSink, NullTraceSink);
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frames,
        audio_sink: &mut audio,
        trace_sink: &mut trace,
    };
    runtime
        .run_until(MachineTime::new(FRAME_TICKS * 60), &mut host)
        .expect("sixty frames should run");

    let machine = runtime.machine().expect("a machine was constructed");
    let raster = machine.framebuffer();
    let stride = machine.framebuffer_width() as usize;
    let left = machine.hblank_clocks() as usize;
    let width = machine.visible_framebuffer_width() as usize;

    let rows = (0..machine.framebuffer_height() as usize)
        .map(|y| {
            raster[y * stride + left..y * stride + left + width]
                .iter()
                .map(|pixel| pixel & 0x00FF_FFFF)
                .collect()
        })
        .collect();
    (rows, width)
}

/// The plate is ink on paper and everything outside it is blanked to black, so
/// the lightest colour on screen is the paper.
fn paper(rows: &[Vec<u32>]) -> u32 {
    *rows
        .iter()
        .flatten()
        .max_by_key(|pixel| {
            let (r, g, b) = ((*pixel >> 16) & 0xFF, (*pixel >> 8) & 0xFF, *pixel & 0xFF);
            2 * r + 5 * g + b
        })
        .expect("a frame was drawn")
}

#[test]
fn the_synthetic_cartridge_boots_and_draws_the_plate() {
    let (rows, _) = picture();
    let paper = paper(&rows);
    assert_ne!(
        paper, 0,
        "nothing but black on screen — no picture was drawn"
    );

    // The plate's frame is two solid bands of ink inside the picture. A band is
    // a run of rows carrying no paper at all, which off the plate only happens
    // in the blanked margins — so bound the search to the rows that have paper.
    let lit: Vec<usize> = (0..rows.len())
        .filter(|y| rows[*y].contains(&paper))
        .collect();
    let (first, last) = (lit[0], lit[lit.len() - 1]);
    let banded = (first..=last)
        .filter(|y| !rows[*y].contains(&paper))
        .count();

    assert_eq!(
        banded, 16,
        "the plate's frame is two eight-line bands of ink, so sixteen rows inside the picture \
         should carry no paper"
    );
}

#[test]
fn the_right_half_of_a_line_differs_from_the_left() {
    let (rows, width) = picture();
    let half = width / 2;

    // With CTRLPF's reflect and priority bits clear, the TIA repeats the left
    // twenty playfield blocks across the right half of every line. The only
    // way the halves can differ is a program that rewrote PF0, PF1 and PF2
    // after the beam passed the middle and before it reached their blocks
    // again — a window of about twenty CPU cycles. Nothing about fetching
    // memory or reaching a register proves that; only the timing does.
    let asymmetric = rows.iter().filter(|row| row[..half] != row[half..]).count();

    assert!(
        asymmetric > 0,
        "every line's halves match, so the mid-line playfield rewrite never landed in its window"
    );
}
