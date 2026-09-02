//! The browser build is the same emulator, checked pixel for pixel.
//!
//! ```text
//! cargo test -p emu198x-web --test parity
//! wasm-pack test --headless --chrome crates/emu198x-web
//! ```
//!
//! The accuracy bar does not move for the web. A browser build that renders
//! differently from the native one is a defect, not a trade-off — but "we
//! compiled it for another target and it still ran" is not evidence of that.
//! This hashes the framebuffer and pins the value, so both targets have to
//! agree with the same constant.
//!
//! ## Why no real ROM
//!
//! Every other real-machine test here loads the 48K ROM, and CI has no rights
//! to one. That is fine, because the claim under test is *native equals wasm*,
//! not *emulator equals hardware*: what it needs is determinism, not firmware.
//! A zeroed ROM leaves the CPU running NOPs, so the test paints the display
//! file itself — which exercises the ULA's fetch, the attribute decode, the
//! border and the palette walk, all the places a target-dependent difference
//! (float rounding, endianness, a `usize` width assumption) would surface.

use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet};
use emu198x_web::WebMachine;
use runtime_sinclair_zx_spectrum::{Model, SpectrumLiveAccess, SpectrumRuntimeKind};
use twox_hash::XxHash64;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

#[cfg(target_arch = "wasm32")]
wasm_bindgen_test_configure!(run_in_browser);

/// Frames run before painting, to settle the machine out of reset.
const SETTLE_FRAMES: u32 = 4;

/// Frames run after painting, so the ULA has read the pattern back out.
const RENDER_FRAMES: u32 = 3;

/// Spectrum display file and attribute area.
const DISPLAY_FILE: u16 = 0x4000;
const DISPLAY_FILE_LEN: u16 = 0x1800;
const ATTRIBUTES: u16 = 0x5800;
const ATTRIBUTES_LEN: u16 = 0x0300;

/// Hash of the framebuffer after the fixed sequence below.
///
/// Generated from a native run and pinned. If this changes, either the
/// renderer changed — in which case regenerate it deliberately and say so in
/// the commit — or the two targets have diverged, which is the defect this
/// test exists to catch.
const GOLDEN_FRAME_HASH: u64 = 0x2e2e_2dd4_770f_7a7c;

/// Builds a 48K on a zeroed ROM. Deterministic and carries no rights.
fn machine() -> WebMachine<SpectrumRuntimeKind> {
    let rom = [0u8; 16 * 1024];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("sinclair-zx-spectrum-48k-rom", &rom));
    let runtime = SpectrumRuntimeKind::from_firmware(Model::Spectrum48KPal, &firmware)
        .expect("a zeroed ROM is a valid 16K image");
    WebMachine::new(runtime)
}

/// Paints a fixed pattern into the display file and attributes.
///
/// Deliberately not uniform: a solid fill would hash the same whether the ULA
/// read the display file correctly or not.
fn paint(machine: &mut WebMachine<SpectrumRuntimeKind>) {
    let runtime = machine.runtime_mut();
    for offset in 0..DISPLAY_FILE_LEN {
        // A walking pattern, so adjacent bytes and adjacent rows differ.
        let value = (offset.wrapping_mul(31) ^ offset.rotate_left(3)) as u8;
        runtime.write_byte(DISPLAY_FILE + offset, value);
    }
    for offset in 0..ATTRIBUTES_LEN {
        // Cycle ink, paper, bright and flash across the attribute area so the
        // palette walk is exercised rather than one colour pair.
        let value = (offset % 128) as u8 | if offset % 5 == 0 { 0x40 } else { 0 };
        runtime.write_byte(ATTRIBUTES + offset, value);
    }
}

/// Runs the fixed sequence and hashes the resulting framebuffer.
fn frame_hash() -> u64 {
    use std::hash::Hasher as _;

    let mut machine = machine();
    for _ in 0..SETTLE_FRAMES {
        machine.run_one_frame().expect("the machine runs");
    }
    paint(&mut machine);
    for _ in 0..RENDER_FRAMES {
        machine.run_one_frame().expect("the machine runs");
    }

    let (width, height) = machine.frame_size();
    let pixels = machine.frame_rgba();
    assert_eq!(
        pixels.len(),
        (width as usize) * (height as usize) * 4,
        "the framebuffer is not four bytes a pixel"
    );

    let mut hasher = XxHash64::with_seed(0);
    hasher.write_u32(width);
    hasher.write_u32(height);
    hasher.write(pixels);
    hasher.finish()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_browser_build_draws_the_same_pixels_as_the_native_one() {
    let hash = frame_hash();
    assert_eq!(
        hash, GOLDEN_FRAME_HASH,
        "framebuffer hash is {hash:#018x}, expected {GOLDEN_FRAME_HASH:#018x}. \
         Native and wasm must agree pixel for pixel; if the renderer changed \
         on purpose, regenerate this constant and say so in the commit."
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_sequence_is_deterministic_within_a_single_target() {
    // If this fails, the hash above is meaningless: it would be comparing two
    // runs that do not even agree with themselves.
    assert_eq!(
        frame_hash(),
        frame_hash(),
        "two identical runs produced different pixels"
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_painted_pattern_actually_reaches_the_picture() {
    // Guards the guard: if painting stopped affecting the framebuffer, the
    // parity hash would still match on both targets and prove nothing.
    let mut plain = machine();
    for _ in 0..(SETTLE_FRAMES + RENDER_FRAMES) {
        plain.run_one_frame().expect("the machine runs");
    }
    let unpainted = plain.frame_rgba().to_vec();

    let mut painted = machine();
    for _ in 0..SETTLE_FRAMES {
        painted.run_one_frame().expect("the machine runs");
    }
    paint(&mut painted);
    for _ in 0..RENDER_FRAMES {
        painted.run_one_frame().expect("the machine runs");
    }

    assert_ne!(
        painted.frame_rgba(),
        unpainted.as_slice(),
        "painting the display file changed nothing on screen"
    );
}
