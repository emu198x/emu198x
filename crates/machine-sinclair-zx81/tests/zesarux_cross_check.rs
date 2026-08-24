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
//! `the_wrx_bitmap_matches_zesarux` covers a non-standard display: a WRX
//! program, where the pattern address is the raw `I:R` pair and neither the
//! opcode nor the line counter takes part. It needs a `.p` and a capture of
//! the same program, so it skips by default.
//!
//! Two things about that test are deliberate and neither is a fudge.
//!
//! It compares a **range** of scan lines rather than all 192, because a WRX
//! program's lower rows often display whatever RAM sits above its bitmap —
//! live data that changes as the program runs and depends on how it was
//! reached. Measured on the demo below, lines 118-119 differ between two of
//! *our own* frames 1200 apart, and 115-117 differ from ZEsarUX because the
//! two machines reached the program by different routes. That is the image,
//! not the ULA, and the range is a property of the image supplied with it.
//!
//! And it **searches for the horizontal alignment** instead of reusing
//! `ZESARUX_X`. The WRX picture aligns one byte further right than the
//! character path does, which is the open question in #301 — whether the
//! refresh address the ULA latches is `R` before or after that M1's
//! increment. Searching keeps this test measuring the bitmap while #301
//! settles the placement.
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

#[test]
#[ignore = "needs a ZX81 ROM, a WRX .p and a ZEsarUX capture of the same program"]
fn the_wrx_bitmap_matches_zesarux() {
    use format_sinclair_zx81_p::Zx81Image;
    use machine_sinclair_zx81::{Zx81, Zx81Key};

    let Ok(rom_path) = env::var("EMU198X_ZX81_ROM")
        .or_else(|_| env::var("HOME").map(|h| format!("{h}/.emu198x/roms/sinclair-zx81/zx81.rom")))
    else {
        emu198x_test_skip::skip!("no ZX81 ROM");
    };
    let Ok(rom) = fs::read(&rom_path) else {
        emu198x_test_skip::skip!("ZX81 ROM not staged at {rom_path}");
    };
    let Ok(image) = env::var("EMU198X_ZX81_WRX_P") else {
        emu198x_test_skip::skip!("no WRX image — set EMU198X_ZX81_WRX_P");
    };
    let Some(theirs) = capture("EMU198X_ZX81_WRX_ZESARUX_BMP") else {
        emu198x_test_skip::skip!("no WRX capture — set EMU198X_ZX81_WRX_ZESARUX_BMP");
    };

    // Where the program's fixed bitmap ends. See the note at the top: below it
    // a WRX program commonly displays live RAM, which is the image's business
    // and not the ULA's.
    let last_line: u32 = env::var("EMU198X_ZX81_WRX_LAST_LINE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(TEXT_H);

    let raw = fs::read(&image).expect("read the WRX .p");
    let pulses = Zx81Image::parse(&raw)
        .expect("a valid .p")
        .to_pulses(&[0x26]);
    let mut machine = Zx81::new(rom, 16384).expect("machine");
    for _ in 0..400 {
        machine.run_frame();
    }
    // LOAD "" — J is the LOAD keyword, shift+P a quote. The tape is threaded
    // only once the loader is listening; see `tape_load.rs`.
    for key in [Zx81Key::J] {
        machine.press_key(key);
        for _ in 0..25 {
            machine.run_frame();
        }
        machine.release_key(key);
        for _ in 0..120 {
            machine.run_frame();
        }
    }
    for _ in 0..2 {
        machine.press_key(Zx81Key::Shift);
        machine.press_key(Zx81Key::P);
        for _ in 0..25 {
            machine.run_frame();
        }
        machine.release_key(Zx81Key::P);
        machine.release_key(Zx81Key::Shift);
        for _ in 0..120 {
            machine.run_frame();
        }
    }
    machine.press_key(Zx81Key::Newline);
    for _ in 0..25 {
        machine.run_frame();
    }
    machine.release_key(Zx81Key::Newline);
    for _ in 0..40 {
        machine.run_frame();
    }
    machine.insert_tape(&pulses);
    let mut frames = 0;
    while machine.tape_remaining() > 0 && frames < 30_000 {
        machine.run_frame();
        frames += 1;
    }
    assert_eq!(machine.tape_remaining(), 0, "the tape should run out");

    // Most hi-res demos are a `1 REM <machine code>` the user starts with
    // RAND USR; the REM's leading HALT pad bytes are not the entry point.
    if let Ok(entry) = env::var("EMU198X_ZX81_WRX_ENTRY") {
        let entry = u16::from_str_radix(entry.trim_start_matches("0x"), 16).expect("hex entry");
        machine.cpu_mut().regs.pc = entry;
    }
    for _ in 0..1500 {
        machine.run_frame();
    }
    assert!(
        machine.cpu().regs.i > 0x1F,
        "I is {:#04x}, which is still a character-set page — the program never \
         entered WRX, so this would be comparing the character path",
        machine.cpu().regs.i
    );

    let width = machine.framebuffer_width() as usize;
    let top = machine_sinclair_zx81::TEXT_TOP as usize;
    let fb = machine.framebuffer().to_vec();

    // What this comparison can and cannot show.
    //
    // `I` above `$1F` is the ULA's own selecting condition, so the assertion
    // above is a direct observation that the bitmap path is the one running.
    // Whether the *pixels* then prove the addressing correct depends on the
    // image: a WRX picture made of long uniform runs reads the same under
    // either formula, so comparing it agrees whatever the emulator does.
    //
    // Measured, not assumed. The demo this was developed against draws a
    // solid block, and the whole comparison passed with the WRX path compiled
    // out. What is wanted is a static image with fine detail, and none of the
    // seven candidates to hand is both -- the detailed ones animate (Starfight
    // moves 186 of 192 scan lines between two frames) and the static ones are
    // uniform. Hence `#[ignore]`: the harness is ready and the image is the
    // outstanding half.

    let ours = move |x: u32, y: u32| fb[(top + y as usize) * width + x as usize] == 0xFF00_0000;

    // Find the alignment rather than assuming it; #301.
    let lines = 0..last_line;
    let (best_x, diffs) = (ZESARUX_X.saturating_sub(16)..ZESARUX_X + 16)
        .map(|zx| {
            let d = scanline_diff(&ours, &theirs, (zx, ZESARUX_Y), lines.clone());
            (zx, d)
        })
        .min_by_key(|(_, d)| d.iter().map(|&(_, n)| n).sum::<u32>())
        .expect("a search window");

    assert!(
        diffs.is_empty(),
        "the WRX bitmap should be pixel-identical to ZEsarUX's over lines \
         {lines:?} at x={best_x}. {}",
        report(&diffs)
    );
    assert_eq!(
        best_x,
        ZESARUX_X + 8,
        "the WRX picture has always aligned one byte right of the character \
         path's x={ZESARUX_X}, which is the open question in #301. It moved, \
         so either #301 was settled or something else did."
    );
}
