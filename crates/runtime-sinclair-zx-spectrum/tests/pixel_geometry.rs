//! Every Spectrum variant's picture geometry, across the whole family.
//!
//! The framebuffer is 352×296 for all of them, so nothing here can be caught
//! by comparing dimensions. What differs is the clock the pixels come out at,
//! and for the TS2068 the television standard itself — which is exactly why
//! the presentation hook takes the runtime rather than a constant.

use emu198x_shell::MachineCore;
use emu198x_shell::display::{active_lines, pixel_aspect_ratio};
use emu198x_shell::machine::Region;
use runtime_sinclair_zx_spectrum::SpectrumRuntimeKind as Kind;
use runtime_sinclair_zx_spectrum::{
    Model, Pentagon128Runtime, ScorpionZS256Runtime, Spectrum16kRuntime, Spectrum48kRuntime,
    Spectrum128kRuntime, SpectrumPlus2ARuntime, SpectrumPlus2BRuntime, SpectrumPlus2Runtime,
    SpectrumPlus3Runtime, SpectrumPlusRuntime, TimexTC2048Runtime, TimexTS2068Runtime,
};

/// Every variant by name. Built one at a time on purpose: each runtime holds
/// its RAM inline, so thirteen of them alive together overflow a test thread's
/// stack. The loop below drops each before making the next.
const VARIANTS: &[&str] = &[
    "16K", "48K", "+", "128K", "+2", "+2A", "+2B", "+3", "Pentagon", "Scorpion", "TC2048",
    "TC2068", "TS2068",
];

fn variant(name: &str) -> Kind {
    match name {
        "16K" => Kind::Spectrum16K(Spectrum16kRuntime::blank()),
        "48K" => Kind::Spectrum48K(Spectrum48kRuntime::blank()),
        "+" => Kind::SpectrumPlus(SpectrumPlusRuntime::blank()),
        "128K" => Kind::Spectrum128K(Spectrum128kRuntime::blank()),
        "+2" => Kind::SpectrumPlus2(SpectrumPlus2Runtime::blank()),
        "+2A" => Kind::SpectrumPlus2A(SpectrumPlus2ARuntime::blank()),
        "+2B" => Kind::SpectrumPlus2B(SpectrumPlus2BRuntime::blank()),
        "+3" => Kind::SpectrumPlus3(SpectrumPlus3Runtime::blank()),
        "Pentagon" => Kind::Pentagon128(Pentagon128Runtime::blank()),
        "Scorpion" => Kind::ScorpionZS256(ScorpionZS256Runtime::blank()),
        "TC2048" => Kind::TimexTC2048(TimexTC2048Runtime::blank()),
        "TC2068" => Kind::TimexTC2068(TimexTS2068Runtime::blank(Model::TimexTC2068)),
        "TS2068" => Kind::TimexTS2068(TimexTS2068Runtime::blank(Model::TimexTS2068)),
        other => panic!("unknown variant {other}"),
    }
}

/// Two pixels per T-state, so the pixel clock is the CPU clock doubled. The
/// 48K's 14 MHz crystal divides by four; the 128K's is four times the PAL
/// colour subcarrier and divides by five.
#[test]
fn the_48k_and_128k_classes_run_at_different_pixel_clocks() {
    let fortyeight = Kind::Spectrum48K(Spectrum48kRuntime::blank()).pixel_clock_hz();
    let one_two_eight = Kind::Spectrum128K(Spectrum128kRuntime::blank()).pixel_clock_hz();

    assert!(
        (fortyeight - 7_000_000.0).abs() < 1.0,
        "48K should be 14 MHz / 4 doubled; got {fortyeight}"
    );
    assert!(
        (one_two_eight - 7_093_790.0).abs() < 1.0,
        "128K should be 17_734_475 / 5 doubled; got {one_two_eight}"
    );
    assert_ne!(
        fortyeight, one_two_eight,
        "the classes must not collapse onto one clock"
    );
}

/// Nothing in the family is square, so presenting the framebuffer unstretched
/// was wrong for all of them — not only the ones with unusual crystals.
#[test]
fn no_variant_has_square_pixels() {
    for name in VARIANTS {
        let kind = variant(name);
        let region = kind.profile().region;
        let lines = active_lines(region).unwrap_or_else(|| panic!("{name} drove a television"));
        let par = pixel_aspect_ratio(region, kind.pixel_clock_hz(), lines)
            .unwrap_or_else(|| panic!("{name} should have a pixel aspect"));

        assert!(
            (par - 1.0).abs() > 0.005,
            "{name} came out at {par}, which is square to within half a percent \
             — check the clock rather than relaxing this bound"
        );
        assert!(
            (0.8..1.3).contains(&par),
            "{name} at {par} is outside anything a Spectrum-class raster produces"
        );
    }
}

/// The TS2068 is the family's NTSC machine, and NTSC's shorter active height
/// against a longer active line makes its pixels narrower than any PAL
/// sibling's. A regression that lost the region would show up here.
#[test]
fn the_ts2068_is_ntsc_and_narrower_than_its_pal_siblings() {
    let ts = Kind::TimexTS2068(TimexTS2068Runtime::blank(Model::TimexTS2068));
    let tc = Kind::TimexTC2068(TimexTS2068Runtime::blank(Model::TimexTC2068));

    assert_eq!(
        ts.profile().region,
        Region::Ntsc,
        "the TS2068 is the NTSC one"
    );
    assert_eq!(
        tc.profile().region,
        Region::Pal,
        "the TC2068 is the PAL one"
    );

    let par = |k: &Kind| {
        let region = k.profile().region;
        pixel_aspect_ratio(
            region,
            k.pixel_clock_hz(),
            active_lines(region).expect("tv"),
        )
        .expect("a television")
    };
    assert!(
        par(&ts) < par(&tc),
        "NTSC {} should be narrower than PAL {}",
        par(&ts),
        par(&tc)
    );
}
