//! Cross-check the ZX81 boot screen against ZEsarUX.
//!
//! A second independent implementation beside the MAME check next door, and
//! the one that matters for what comes next: MAME's ZX81 driver implements
//! only the character path, so it can never be the reference for WRX (#301).
//! ZEsarUX implements both.
//!
//! # Alignment
//!
//! ZEsarUX renders a 352x296 raster closing with seven blank lines of vertical
//! sync at rows 289-295 — frame lines 304-310, which is where our own field
//! arithmetic puts the sync. Its row 0 is therefore frame line 15, against our
//! 8, so **our row is its row plus seven**.
//!
//! Horizontally its text area starts at column 50 where ours starts at 32.
//! That offset is not asserted as agreement, for the same reason the MAME
//! check does not assert one: `FIRST_CHAR_TSTATE` is fitted per machine, which
//! is #1123.
//!
//! Both origins were found by exhaustive search rather than assumed, and the
//! match at (50, 41) is exact — zero differing pixels of 49,152 — while every
//! neighbouring origin differs by 24 or more. That sharpness is what makes it
//! an alignment rather than a coincidence.
//!
//! # Producing a capture
//!
//! ```text
//! tools/zx8x-zesarux-capture/build.sh /tmp/zesarux
//! tools/zx8x-zesarux-capture/capture.sh /tmp/zesarux/zesarux /tmp/zx81.bmp
//! EMU198X_ZX81_ZESARUX_BMP=/tmp/zx81.bmp cargo test -p machine-sinclair-zx81 --test zesarux_cross_check
//! ```
//!
//! # Per scan line
//!
//! Divergence is reported per scan line rather than as one total, because the
//! ULA composes the picture a line at a time: a fault in the line counter, the
//! sync, or the pattern address shows up as a *band* of wrong lines, and a
//! single number cannot tell that from noise spread over the frame. The report
//! names the first line that differs and how many of its 256 pixels do, which
//! is what the bus-stolen-video rewrite this scaffolds will need.
//!
//! # Two images
//!
//! The boot screen is the stock character path, and it agrees exactly.
//!
//! `the_wrx_fixture_matches_zesarux` covers a non-standard display: WRX, where
//! the pattern address is the raw `I:R` pair and neither the opcode nor the
//! line counter takes part.
//!
//! That fixture is built here rather than found. `I` above `$1F` is the whole
//! of what selects WRX, so a pattern page and an `I` pointing at it are the
//! entire thing, and ZEsarUX is handed the same two through `write-memory` and
//! `set-register` — no `.p`, no BASIC, no entry point. See the test for why a
//! *generated* pattern is what makes the comparison mean anything, and
//! `tools/zx8x-zesarux-capture/wrx-fixture.sh` for the capture.
//!
//! Skips when no capture is supplied.

use std::{env, fs, path::PathBuf};

/// Where our text area begins.
const OUR_X: u32 = 32;
/// And ZEsarUX's, in its own raster.
const ZESARUX_X: u32 = 50;
const ZESARUX_Y: u32 = 41;
const TEXT_W: u32 = 256;
const TEXT_H: u32 = 192;

/// A 24-bit bottom-up BMP, which is what ZEsarUX's `save-screen` writes.
///
/// Hand-rolled because the alternative is a dependency for one test, and
/// because ZEsarUX writes exactly one shape of file.
struct Bmp {
    data: Vec<u8>,
    offset: usize,
    width: u32,
    height: u32,
    row: usize,
    bytes_per_pixel: usize,
    bottom_up: bool,
}

impl Bmp {
    fn read(path: &PathBuf) -> Option<Self> {
        let data = fs::read(path).ok()?;
        if data.len() < 54 || &data[..2] != b"BM" {
            return None;
        }
        let u32_at = |o: usize| -> Option<u32> {
            Some(u32::from_le_bytes(data.get(o..o + 4)?.try_into().ok()?))
        };
        let offset = u32_at(10)? as usize;
        let width = u32_at(18)?;
        let signed_height = i32::from_le_bytes(data[22..26].try_into().ok()?);
        let bpp = u16::from_le_bytes(data[28..30].try_into().ok()?) as usize;
        if !bpp.is_multiple_of(8) || width == 0 {
            return None;
        }
        let bytes_per_pixel = bpp / 8;
        Some(Self {
            offset,
            width,
            height: signed_height.unsigned_abs(),
            row: (width as usize * bpp).div_ceil(32) * 4,
            bytes_per_pixel,
            bottom_up: signed_height > 0,
            data,
        })
    }

    fn ink(&self, x: u32, y: u32) -> bool {
        let row = if self.bottom_up {
            self.height - 1 - y
        } else {
            y
        };
        let at = self.offset + row as usize * self.row + x as usize * self.bytes_per_pixel;
        self.data.get(at).is_some_and(|&v| v < 128)
    }
}

/// Divergence per scan line: `(row, pixels differing)`, rows that agree
/// omitted.
fn scanline_diff(
    ours: &dyn Fn(u32, u32) -> bool,
    theirs: &Bmp,
    origin: (u32, u32),
    lines: std::ops::Range<u32>,
) -> Vec<(u32, u32)> {
    let (zx, zy) = origin;
    lines
        .filter_map(|row| {
            let n = (0..TEXT_W)
                .filter(|col| ours(OUR_X + col, row) != theirs.ink(zx + col, zy + row))
                .count();
            #[allow(clippy::cast_possible_truncation)]
            (n > 0).then_some((row, n as u32))
        })
        .collect()
}

/// A report an operator can act on: how many lines, which came first, and how
/// badly.
fn report(diffs: &[(u32, u32)]) -> String {
    let total: u32 = diffs.iter().map(|&(_, n)| n).sum();
    let worst = diffs.iter().max_by_key(|&&(_, n)| n).copied();
    format!(
        "{} scan lines differ ({total} pixels). First: line {} with {} of {TEXT_W}. \
         Worst: line {} with {} of {TEXT_W}.",
        diffs.len(),
        diffs[0].0,
        diffs[0].1,
        worst.map_or(0, |(r, _)| r),
        worst.map_or(0, |(_, n)| n),
    )
}

/// Our committed boot golden, as a pixel lookup in framebuffer coordinates.
fn golden(name: &str) -> impl Fn(u32, u32) -> bool {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(name);
    let decoder = png::Decoder::new(std::io::BufReader::new(
        fs::File::open(&path).expect("golden"),
    ));
    let mut reader = decoder.read_info().expect("golden header");
    let mut buf = vec![0u8; reader.output_buffer_size().expect("golden size")];
    let info = reader.next_frame(&mut buf).expect("golden pixels");
    let bpp = info.buffer_size() / (info.width as usize * info.height as usize);
    let width = info.width as usize;
    move |x: u32, y: u32| buf[(y as usize * width + x as usize) * bpp] < 128
}

fn capture(var: &str) -> Option<Bmp> {
    let path = PathBuf::from(env::var(var).ok()?);
    Some(Bmp::read(&path).unwrap_or_else(|| panic!("could not read a BMP at {}", path.display())))
}

#[test]
fn the_text_area_matches_zesarux() {
    let Some(theirs) = capture("EMU198X_ZX81_ZESARUX_BMP") else {
        emu198x_test_skip::skip!(
            "no ZEsarUX capture — see tools/zx8x-zesarux-capture/ and set EMU198X_ZX81_ZESARUX_BMP"
        );
    };
    assert!(
        theirs.width >= ZESARUX_X + TEXT_W && theirs.height >= ZESARUX_Y + TEXT_H,
        "the capture is {}x{}, too small to hold the text area at ({ZESARUX_X}, {ZESARUX_Y}) — \
         ZEsarUX's raster has moved and the offsets here need re-deriving",
        theirs.width,
        theirs.height
    );

    let ours = golden("zx81-boot.png");
    let our_y = machine_sinclair_zx81::TEXT_TOP;
    let shifted = |x: u32, y: u32| ours(x, our_y + y);
    let diffs = scanline_diff(&shifted, &theirs, (ZESARUX_X, ZESARUX_Y), 0..TEXT_H);

    assert!(
        diffs.is_empty(),
        "the boot screen should be pixel-identical to ZEsarUX's. {}",
        report(&diffs)
    );
}

/// The WRX fixture: a pattern page and an `I` that points at it.
///
/// `PATTERN_PAGE` is clear of the program, the display file and the stack.
/// `PATTERN_STEP` is odd, so 256 additions walk all 256 byte values and the
/// page holds no repeats -- which is what makes the picture detailed enough to
/// tell the two address paths apart.
///
/// Duplicated in `tools/zx8x-zesarux-capture/wrx-fixture.sh`, which must
/// match. Three constants are few enough to read at a glance, which beats
/// generating one side from the other and hiding the fixture in a build step.
const PATTERN_PAGE: u8 = 0x60;
const PATTERN_SEED: u8 = 0x01;
const PATTERN_STEP: u8 = 0x4D;

/// A non-standard display we make ourselves, rather than one we go looking for.
///
/// # Why there is no program
///
/// `I` above `$1F` is the whole of what selects WRX, so a pattern page and an
/// `I` pointing at it are the entire fixture -- the ROM's own display routine
/// then draws the bitmap on the ROM's own timing. No `.p` to build, no BASIC to
/// type, no entry point to find, and ZEsarUX is handed the same two things
/// through `write-memory` and `set-register`.
///
/// A first attempt did write a program, which set `I` and then looped. It drew
/// nothing: in SLOW the display file is entered from the ROM's display routine,
/// so a foreground loop starves the picture it was meant to produce.
///
/// # Why this one cannot pass vacuously
///
/// The picture has to be one the character path could not have drawn, or the
/// comparison agrees whatever the emulator does. Every WRX image to hand failed
/// that: the detailed ones animate, and the static one is a solid block, where
/// a long run of `$FF` reads as `$FF` under either formula. That demo passed
/// this comparison with the WRX path compiled out (#297).
///
/// A generated pattern settles it by construction. With the display file empty
/// the character path reads `PAGE*256 + 0*8 + COUNT` -- eight bytes, each
/// repeated across all 32 columns -- against 32 distinct bytes a row here.
/// Compiling the WRX path out moves the best alignment *anywhere* from 0
/// differing pixels to 2,732.
///
/// # Alignment
///
/// At `ZESARUX_X`, the character path's own origin, and not the byte further
/// right that a WRX *title* needed (#301). The difference between them is that
/// this fixture leaves `R` to the ROM's display routine while a title reloads
/// it in a routine of its own, so the two are not measuring the same thing.
#[test]
#[ignore = "needs a ZX81 ROM and a capture from tools/zx8x-zesarux-capture/wrx-fixture.sh"]
fn the_wrx_fixture_matches_zesarux() {
    use machine_sinclair_zx81::Zx81;

    let Ok(rom_path) = env::var("EMU198X_ZX81_ROM")
        .or_else(|_| env::var("HOME").map(|h| format!("{h}/.emu198x/roms/sinclair-zx81/zx81.rom")))
    else {
        emu198x_test_skip::skip!("no ZX81 ROM");
    };
    let Ok(rom) = fs::read(&rom_path) else {
        emu198x_test_skip::skip!("ZX81 ROM not staged at {rom_path}");
    };
    let Some(theirs) = capture("EMU198X_ZX81_WRX_FIXTURE_BMP") else {
        emu198x_test_skip::skip!(
            "no fixture capture — run tools/zx8x-zesarux-capture/wrx-fixture.sh and set \
             EMU198X_ZX81_WRX_FIXTURE_BMP"
        );
    };

    let mut machine = Zx81::new(rom, 16384).expect("machine");
    for _ in 0..400 {
        machine.run_frame();
    }
    let base = u16::from(PATTERN_PAGE) << 8;
    let mut value = PATTERN_SEED;
    for offset in 0..=0xFFu16 {
        machine.poke(base + offset, value);
        value = value.wrapping_add(PATTERN_STEP);
    }
    machine.cpu_mut().regs.i = PATTERN_PAGE;
    for _ in 0..200 {
        machine.run_frame();
    }

    let read = |m: &Zx81| -> Vec<bool> {
        let w = m.framebuffer_width() as usize;
        let top = machine_sinclair_zx81::TEXT_TOP as usize;
        let fb = m.framebuffer();
        (0..TEXT_H as usize)
            .flat_map(|y| (0..TEXT_W as usize).map(move |x| (y, x)))
            .map(|(y, x)| fb[(top + y) * w + OUR_X as usize + x] == 0xFF00_0000)
            .collect()
    };
    let first = read(&machine);
    for _ in 0..600 {
        machine.run_frame();
    }
    let second = read(&machine);
    assert_eq!(
        first, second,
        "the fixture has to hold still to be comparable against a single \
         capture, and it did not"
    );

    assert_eq!(
        machine.cpu().regs.i,
        PATTERN_PAGE,
        "the ROM took `I` back, so this is no longer a WRX display"
    );

    let ours = move |x: u32, y: u32| first[y as usize * TEXT_W as usize + (x - OUR_X) as usize];
    let diffs = scanline_diff(&ours, &theirs, (ZESARUX_X, ZESARUX_Y), 0..TEXT_H);
    assert!(
        diffs.is_empty(),
        "the WRX bitmap should be pixel-identical to ZEsarUX's. {}",
        report(&diffs)
    );
}
