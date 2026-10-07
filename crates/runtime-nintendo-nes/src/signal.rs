//! 2C02 NTSC waveform source. Measurement provenance and calibration limits:
//! umbrella reference/by-system/nintendo-nes/2c02-video-levels.md.
use std::sync::LazyLock;
pub(crate) static LEVELS: LazyLock<Vec<[f32; 4]>> = LazyLock::new(|| {
    let low = [0.350, 0.518, 0.962, 1.550];
    let high = [1.094, 1.506, 1.962, 1.962];
    (0..512)
        .flat_map(|code| {
            (0..12).map(move |phase| {
                let hue = code & 15;
                let level = if hue >= 14 { 1 } else { (code >> 4) & 3 };
                let on = hue == 0 || (hue < 13 && (hue + phase) % 12 < 6);
                let mut voltage = if on { high[level] } else { low[level] };
                let emphasis = code >> 6;
                let attenuate = ((emphasis & 1 != 0) && phase % 12 < 6)
                    || ((emphasis & 2 != 0) && (phase + 4) % 12 < 6)
                    || ((emphasis & 4 != 0) && (phase + 8) % 12 < 6);
                if attenuate && hue < 14 {
                    voltage *= 0.746;
                }
                [(voltage - 0.518) / (1.962 - 0.518), 0.0, 0.0, 0.0]
            })
        })
        .collect()
});
