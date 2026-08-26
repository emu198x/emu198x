//! TIA frame oracle.
//!
//! A deterministic, pixel-exact comparison harness for the TIA renderer so the
//! cycle-accuracy work (RESP pipeline delay, HMOVE comb/bar, VDELP, NUSIZ, …)
//! is *gated and scored* against a reference instead of eyeballed (#416).
//!
//! Each scenario drives a fresh [`Tia`] through one full NTSC frame with a
//! fixed register program — no CPU or ROM, so the output is reproducible and
//! the goldens carry no copyrighted content. The captured framebuffer is
//! diffed against a committed golden PNG; a mismatch reports how many pixels
//! differ and where (the "score"), and dumps the actual frame for inspection.
//!
//! ## Workflow
//! - Run normally: `cargo test -p atari-tia --test tia_oracle` — fails on any
//!   pixel that drifts from the golden.
//! - Re-bless after an *intentional* change (verify the new frame against
//!   Stella first): `BLESS_TIA_GOLDENS=1 cargo test -p atari-tia --test
//!   tia_oracle`, then commit the regenerated `tests/goldens/*.png`.
//! - On failure the actual frame is written to `target/tia-oracle/<name>.png`
//!   so it can be diffed against a Stella capture by eye.

use std::path::{Path, PathBuf};

use atari_tia::{CLOCKS_PER_LINE, Tia, TiaRegion};

/// Tick one full scanline (228 colour clocks).
fn line(tia: &mut Tia) {
    for _ in 0..CLOCKS_PER_LINE {
        tia.tick();
    }
}

/// Render one NTSC frame under `program` and return the framebuffer.
fn render(program: impl FnOnce(&mut Tia)) -> Vec<u32> {
    let mut tia = Tia::new(TiaRegion::Ntsc);
    program(&mut tia);
    tia.framebuffer().to_vec()
}

// --- Scenarios -------------------------------------------------------------

/// Solid background colour across the visible region; HBLANK stays black.
fn scenario_background(tia: &mut Tia) {
    tia.write(0x01, 0x00); // VBLANK off
    tia.write(0x09, 0x88); // COLUBK
    for _ in 0..262 {
        line(tia);
    }
}

/// A single player positioned mid-line via RESP0, static for the frame. Pins
/// the RESP pipeline-delay column and basic player rendering.
fn scenario_player(tia: &mut Tia) {
    tia.write(0x01, 0x00); // VBLANK off
    tia.write(0x09, 0x00); // COLUBK black
    tia.write(0x06, 0x44); // COLUP0
    tia.write(0x04, 0x00); // NUSIZ0 — one copy
    tia.write(0x1B, 0xFF); // GRP0 solid

    // Line 0: strobe RESP0 at a known beam column, then finish the line.
    for _ in 0..120 {
        tia.tick();
    }
    tia.write(0x10, 0); // RESP0 → column 120 + pipeline delay − HBLANK
    for _ in 0..(CLOCKS_PER_LINE - 120) {
        tia.tick();
    }
    // Remaining lines: the player holds its column.
    for _ in 0..261 {
        line(tia);
    }
}

/// Same player, but HMOVE is strobed (zero motion) every line so the HMOVE
/// comb — the 8 blanked pixels at the left — appears on each visible line.
/// This is the scenario the HMOVE rewrite will move; re-bless against Stella.
fn scenario_hmove_comb(tia: &mut Tia) {
    tia.write(0x01, 0x00);
    tia.write(0x09, 0x88); // COLUBK — coloured so the (black) comb is visible
    tia.write(0x06, 0x44);
    tia.write(0x04, 0x00);
    tia.write(0x1B, 0xFF);

    for _ in 0..120 {
        tia.tick();
    }
    tia.write(0x10, 0); // RESP0
    tia.write(0x20, 0x00); // HMP0 = no motion (isolate the comb)
    for _ in 0..(CLOCKS_PER_LINE - 120) {
        tia.tick();
    }
    for _ in 0..261 {
        tia.write(0x2A, 0); // HMOVE at HBLANK → comb on this line
        line(tia);
    }
}

// --- Harness ---------------------------------------------------------------

fn goldens_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
}

/// Decode an RGBA8 golden PNG into the TIA's ARGB `u32` framebuffer format.
fn load_golden(path: &Path) -> Option<Vec<u32>> {
    let file = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    let mut reader = png::Decoder::new(file).read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let rgba = &buf[..info.buffer_size()];
    Some(
        rgba.as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                u32::from(p[3]) << 24
                    | u32::from(p[0]) << 16
                    | u32::from(p[1]) << 8
                    | u32::from(p[2])
            })
            .collect(),
    )
}

/// Encode an ARGB `u32` framebuffer as an RGBA8 PNG.
fn save_png(path: &Path, frame: &[u32], width: u32, height: u32) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create golden dir");
    }
    let mut rgba = Vec::with_capacity(frame.len() * 4);
    for &p in frame {
        rgba.push(((p >> 16) & 0xFF) as u8); // R
        rgba.push(((p >> 8) & 0xFF) as u8); // G
        rgba.push((p & 0xFF) as u8); // B
        rgba.push(((p >> 24) & 0xFF) as u8); // A
    }
    let file = std::fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&rgba).expect("png data");
}

/// Number of differing pixels and the first differing `(x, y)`.
fn frame_diff(actual: &[u32], golden: &[u32], width: u32) -> (usize, Option<(u32, u32)>) {
    let mut differing = 0;
    let mut first = None;
    for (i, (a, g)) in actual.iter().zip(golden).enumerate() {
        if a != g {
            differing += 1;
            if first.is_none() {
                let i = i as u32;
                first = Some((i % width, i / width));
            }
        }
    }
    (differing, first)
}

/// Render `program`, then either bless or diff its frame against the golden.
fn check(name: &str, program: impl FnOnce(&mut Tia)) {
    let width = Tia::new(TiaRegion::Ntsc).framebuffer_width();
    let height = Tia::new(TiaRegion::Ntsc).framebuffer_height();
    let actual = render(program);
    let golden_path = goldens_dir().join(format!("{name}.png"));

    if std::env::var("BLESS_TIA_GOLDENS").is_ok() {
        save_png(&golden_path, &actual, width, height);
        eprintln!("blessed golden: {}", golden_path.display());
        return;
    }

    let golden = load_golden(&golden_path).unwrap_or_else(|| {
        panic!(
            "missing golden {} — run with BLESS_TIA_GOLDENS=1 to create it",
            golden_path.display()
        )
    });
    assert_eq!(
        actual.len(),
        golden.len(),
        "{name}: frame size changed ({} vs golden {})",
        actual.len(),
        golden.len()
    );

    let (differing, first) = frame_diff(&actual, &golden, width);
    if differing != 0 {
        let dump = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/tia-oracle")
            .join(format!("{name}-actual.png"));
        save_png(&dump, &actual, width, height);
        panic!(
            "{name}: {differing} pixel(s) differ from the golden (first at {first:?}). \
             Actual frame written to {}. If this change is intended, verify it against \
             Stella and re-bless with BLESS_TIA_GOLDENS=1.",
            dump.display()
        );
    }
}

#[test]
fn oracle_background() {
    check("background", scenario_background);
}

#[test]
fn oracle_player() {
    check("player", scenario_player);
}

#[test]
fn oracle_hmove_comb() {
    check("hmove_comb", scenario_hmove_comb);
}
