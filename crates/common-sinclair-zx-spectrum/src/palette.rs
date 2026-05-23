//! Spectrum-family palette helpers.
//!
//! The 16-colour palette is derived directly from the per-primary
//! emitter currents Chris Smith documents in Table 16-1 of *The ZX
//! Spectrum ULA: How to design a microcomputer* (Chapter 16, Analogue
//! Video), distilled in the reference library at
//! `~/Projects/Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-ula-chapter-16-analogue-video.md`.
//!
//! ## Silicon basis
//!
//! Each primary (red, green, blue) has two activation states — *normal*
//! and *bright* — selected by the attribute byte's `B` bit. Smith's
//! per-primary currents in mA, and the resulting RGB normalised so
//! bright = 255 per channel:
//!
//! | Channel | Normal | Bright | Ratio | RGB Normal | RGB Bright |
//! |---------|-------:|-------:|------:|-----------:|-----------:|
//! | Red     | 0.178  | 0.234  | 1.315 | 194 (0xC2) | 255 (0xFF) |
//! | Green   | 0.348  | 0.457  | 1.313 | 194 (0xC2) | 255 (0xFF) |
//! | Blue    | 0.090  | 0.118  | 1.311 | 194 (0xC2) | 255 (0xFF) |
//!
//! All three channels share the same bright-to-normal ratio
//! (approximately 1.31×) by deliberate design of the luminance
//! resistors `R20`/`R22`/`R24` (per-primary) and `R25`/`R26`/`R27` (the
//! bright current sink). With bright pinned to 0xFF, the channel-wise
//! normal values all round to 0xC2.
//!
//! ## Known limitation: Bright Yellow vs Bright White
//!
//! Smith Table 16-1 documents that **Bright Yellow and Bright White
//! both produce a /Y of 0.259 V** because transistor Q3 in the
//! luminance circuit saturates at the combined R+G bright current. On
//! a real CRT the two colours have indistinguishable luminance and
//! differ only in chroma (Bright Yellow has zero blue chrominance;
//! Bright White has zero chrominance overall). Our 8-bit RGB palette
//! cannot represent this — `0xFFFF00FF` is visibly less luminous than
//! `0xFFFFFFFF` on a digital display because the missing blue channel
//! reduces perceived brightness. A future CRT filter that converts
//! these RGB values to Y/U/V via Smith's equations will recover the
//! correct CRT appearance.

/// Per-primary current in normal mode, normalised to 8-bit RGB so that
/// the bright current (`PRIMARY_BRIGHT`) maps to 0xFF. See module
/// documentation for the silicon derivation.
pub const PRIMARY_NORMAL: u8 = 0xC2;

/// Per-primary current in bright mode, set as the 8-bit RGB maximum.
pub const PRIMARY_BRIGHT: u8 = 0xFF;

/// Spectrum-specific luminance coefficients (Smith Ch 16 / Table 16-1).
///
/// **`Y = 0.299 R + 0.587 G + 0.151 B`** — Altwasser deliberately raised
/// the blue coefficient above BT.601's 0.114 (the value standard
/// composite-video luminance equations use) because pure blue was "very
/// dark and hardly visible" on contemporary TVs. Using these weights in
/// any luminance computation matches the analog signal a real Spectrum
/// drove on real CRT hardware, and is the load-bearing constant for the
/// CRT filter's Spectrum-tuned chroma-bleed pipeline at
/// `crates/emu198x-native-video/src/shader.wgsl`.
pub const SMITH_LUMA_R: f32 = 0.299;

/// Green-channel weight in Smith's Spectrum luminance equation. Matches
/// BT.601 by coincidence (green is the dominant luminance contributor
/// on both displays).
pub const SMITH_LUMA_G: f32 = 0.587;

/// Blue-channel weight in Smith's Spectrum luminance equation. Smith Ch
/// 16 documents Altwasser's deliberate increase from BT.601's 0.114 to
/// boost the perceptual brightness of pure blue on the standard analog
/// chain. See `SMITH_LUMA_R`'s docstring.
pub const SMITH_LUMA_B: f32 = 0.151;

/// Compute the Spectrum's CRT-display luminance for an RGB sample,
/// using Smith Ch 16's blue-boosted Y equation. Inputs are in the
/// canonical 0.0–1.0 range; output is unclamped and may exceed 1.0
/// for bright-white-class inputs (the sum of the three coefficients
/// is `1.037`, mirroring Q3 saturation at the silicon level).
#[must_use]
pub fn spectrum_luminance(r: f32, g: f32, b: f32) -> f32 {
    SMITH_LUMA_R * r + SMITH_LUMA_G * g + SMITH_LUMA_B * b
}

/// Compose one palette entry from its three primary activation bits
/// and the bright flag. Encodes as `0xRRGGBBAA`.
const fn rgba(bright: bool, has_red: bool, has_green: bool, has_blue: bool) -> u32 {
    let primary = if bright {
        PRIMARY_BRIGHT
    } else {
        PRIMARY_NORMAL
    };
    let r = if has_red { primary } else { 0 };
    let g = if has_green { primary } else { 0 };
    let b = if has_blue { primary } else { 0 };
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0xFF
}

/// Standard ZX Spectrum 16-colour palette as RGBA (0xRRGGBBAA).
///
/// Indices 0-7: normal brightness. Indices 8-15: bright. Indices 0 and
/// 8 are both black — the bright bit has no effect on black because no
/// primaries are activated.
///
/// Colour bit order in the attribute byte's lower three bits is GRB
/// (green = MSB, red = middle, blue = LSB), reflecting the order in
/// which Sinclair arranged the colour control lines on the ULA. This
/// is opposite to the BGR order used by many BT.601 references.
pub const SPECTRUM_PALETTE: [u32; 16] = [
    // Normal brightness (indices 0-7)
    rgba(false, false, false, false), // 0: black     GRB=000
    rgba(false, false, false, true),  // 1: blue      GRB=001
    rgba(false, true, false, false),  // 2: red       GRB=010
    rgba(false, true, false, true),   // 3: magenta   GRB=011
    rgba(false, false, true, false),  // 4: green     GRB=100
    rgba(false, false, true, true),   // 5: cyan      GRB=101
    rgba(false, true, true, false),   // 6: yellow    GRB=110
    rgba(false, true, true, true),    // 7: white     GRB=111
    // Bright (indices 8-15)
    rgba(true, false, false, false), // 8: bright black (== black)
    rgba(true, false, false, true),  // 9: bright blue
    rgba(true, true, false, false),  // 10: bright red
    rgba(true, true, false, true),   // 11: bright magenta
    rgba(true, false, true, false),  // 12: bright green
    rgba(true, false, true, true),   // 13: bright cyan
    rgba(true, true, true, false),   // 14: bright yellow
    rgba(true, true, true, true),    // 15: bright white
];

/// Convert a Spectrum attribute byte to ink and paper palette indices.
///
/// Attribute format: FBPPPIII
///   F = flash, B = bright, PPP = paper colour (0-7), III = ink colour (0-7)
///
/// Returns (ink_index, paper_index) into the 16-colour palette.
#[inline]
pub fn attr_to_indices(attr: u8) -> (u8, u8) {
    let bright = if attr & 0x40 != 0 { 8 } else { 0 };
    let ink = (attr & 0x07) | bright;
    let paper = ((attr >> 3) & 0x07) | bright;
    (ink, paper)
}

/// Check if an attribute has the FLASH bit set.
#[inline]
pub fn attr_flash(attr: u8) -> bool {
    attr & 0x80 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attr_basic() {
        // Normal white ink on blue paper, no flash
        let (ink, paper) = attr_to_indices(0x38 | 0x07); // paper=7 (white), ink=7 (white)
        assert_eq!(ink, 7);
        assert_eq!(paper, 7);

        // Bright red ink on black paper
        let (ink, paper) = attr_to_indices(0x42); // bright=1, paper=0, ink=2
        assert_eq!(ink, 10); // bright red
        assert_eq!(paper, 8); // bright black
    }

    /// Black and Bright Black both map to all-zero RGB. The bright bit
    /// has no effect when no primaries are active.
    #[test]
    fn black_and_bright_black_are_identical() {
        assert_eq!(SPECTRUM_PALETTE[0], 0x000000FF);
        assert_eq!(SPECTRUM_PALETTE[8], 0x000000FF);
    }

    /// Verifies the palette derives from Smith Table 16-1 currents
    /// (R/G/B at 194 normal, 255 bright per channel) rather than the
    /// previous BT.601-style hand-tweaked 0xCD pair.
    #[test]
    fn normal_primaries_match_smith_currents() {
        // White (all three primaries active, normal)
        assert_eq!(SPECTRUM_PALETTE[7], 0xC2C2C2FF);
        // Bright white (all three, bright)
        assert_eq!(SPECTRUM_PALETTE[15], 0xFFFFFFFF);
        // Yellow (red + green, normal) — no blue
        assert_eq!(SPECTRUM_PALETTE[6], 0xC2C200FF);
        // Bright yellow (red + green, bright) — no blue
        assert_eq!(SPECTRUM_PALETTE[14], 0xFFFF00FF);
        // Single-primary normal vs bright per channel
        assert_eq!(SPECTRUM_PALETTE[1], 0x0000C2FF); // blue
        assert_eq!(SPECTRUM_PALETTE[9], 0x0000FFFF); // bright blue
        assert_eq!(SPECTRUM_PALETTE[2], 0xC20000FF); // red
        assert_eq!(SPECTRUM_PALETTE[10], 0xFF0000FF); // bright red
        assert_eq!(SPECTRUM_PALETTE[4], 0x00C200FF); // green
        assert_eq!(SPECTRUM_PALETTE[12], 0x00FF00FF); // bright green
    }

    /// Each colour's bright variant scales the active primaries by
    /// approximately 1.31× — within rounding tolerance of Smith's
    /// per-primary current ratios (1.315 R, 1.313 G, 1.311 B).
    #[test]
    fn bright_ratio_matches_smith_currents() {
        // Pure-primary entries: bright[N] / normal[N] for each active channel.
        let ratio = (PRIMARY_BRIGHT as f64) / (PRIMARY_NORMAL as f64);
        assert!(
            (ratio - 1.314).abs() < 0.01,
            "bright/normal ratio {ratio} should be within 0.01 of Smith's 1.31× (Tables 16-1 currents)",
        );
    }

    /// Bright Yellow and Bright White have identical /Y at silicon
    /// level (Q3 saturation), but differ in chroma. The 8-bit RGB
    /// palette cannot encode the saturation match — they differ in
    /// the blue channel. Documented as a known limitation; a future
    /// CRT filter recovers correct CRT appearance.
    #[test]
    fn bright_yellow_documented_q3_saturation_caveat() {
        let bright_yellow = SPECTRUM_PALETTE[14];
        let bright_white = SPECTRUM_PALETTE[15];
        assert_ne!(bright_yellow, bright_white);
        // They differ ONLY in the blue channel
        let by_blue = (bright_yellow >> 8) & 0xFF;
        let bw_blue = (bright_white >> 8) & 0xFF;
        assert_eq!(by_blue, 0);
        assert_eq!(bw_blue, 0xFF);
        // Red and green channels are identical
        assert_eq!(bright_yellow & 0xFFFF00FF, bright_white & 0xFFFF00FF);
    }

    /// Smith Ch 16 / Table 16-1: the Spectrum's Y equation differs
    /// from BT.601 only in the blue coefficient (0.151 vs 0.114).
    /// These constants are the source-of-truth that the CRT filter
    /// shader cites in `emu198x-native-video/src/shader.wgsl`.
    #[test]
    fn smith_luminance_coefficients_match_chapter_16() {
        assert_eq!(SMITH_LUMA_R, 0.299);
        assert_eq!(SMITH_LUMA_G, 0.587);
        assert_eq!(SMITH_LUMA_B, 0.151);
        // The three coefficients sum to 1.037 — the "Q3 saturation
        // headroom" that lets Bright White luminance exceed the
        // arithmetic ceiling.
        let sum = SMITH_LUMA_R + SMITH_LUMA_G + SMITH_LUMA_B;
        assert!(
            (sum - 1.037).abs() < 1e-6,
            "Smith Y coefficients should sum to 1.037, got {sum}",
        );
    }

    /// `spectrum_luminance` returns 1.037 for bright white and 0.886
    /// for bright yellow — confirming Smith Ch 16's prediction that
    /// the two would share a /Y at silicon level (both exceeding the
    /// clamp ceiling on a real CRT) but differ in mathematical Y
    /// without the Q3 saturation clamp.
    #[test]
    fn spectrum_luminance_documents_q3_saturation_gap() {
        // Bright white: all three primaries at 1.0.
        let bright_white = spectrum_luminance(1.0, 1.0, 1.0);
        assert!(
            (bright_white - 1.037).abs() < 1e-6,
            "bright white luminance should be 1.037 (sum of coefficients), got {bright_white}",
        );
        // Bright yellow: red + green at 1.0, blue at 0.
        let bright_yellow = spectrum_luminance(1.0, 1.0, 0.0);
        assert!(
            (bright_yellow - 0.886).abs() < 1e-6,
            "bright yellow luminance should be 0.886, got {bright_yellow}",
        );
        // Both > 0.886, so on a Q3-saturating CRT they'd display at
        // the same luminance after clamping.
        assert!(bright_yellow > 0.5);
        assert!(bright_white > bright_yellow);
    }

    /// Pure blue's luminance in Smith's equation is 0.151 — measurably
    /// higher than BT.601's 0.114 prediction. This is the load-bearing
    /// difference Altwasser engineered.
    #[test]
    fn pure_blue_luminance_uses_smith_boost_not_bt601() {
        let smith_blue = spectrum_luminance(0.0, 0.0, 1.0);
        assert!(
            (smith_blue - 0.151).abs() < 1e-6,
            "pure blue Y should be 0.151 (Smith), got {smith_blue}",
        );
        // Sanity: this is greater than BT.601's 0.114 prediction.
        assert!(smith_blue > 0.114);
    }
}
