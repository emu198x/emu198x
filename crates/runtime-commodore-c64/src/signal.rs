//! VIC-II electrical palette, calibrated to the 6569R3 measurement table.
//! See umbrella reference/by-system/commodore-c64/vic-ii-video-levels.md.
//! Chroma phase/gain use VICE precedent; these are receiver calibration choices.
use std::sync::LazyLock;
pub(crate) static LEVELS: LazyLock<[[f32; 4]; 16]> = LazyLock::new(|| palette(false));
pub(crate) static NTSC_LEVELS: LazyLock<[[f32; 4]; 16]> = LazyLock::new(|| palette(true));
fn palette(ntsc: bool) -> [[f32; 4]; 16] {
    let y = [
        700.0, 1850.0, 1090.0, 1480.0, 1180.0, 1340.0, 1020.0, 1620.0, 1180.0, 1020.0, 1340.0,
        1090.0, 1300.0, 1620.0, 1300.0, 1480.0,
    ];
    let y = if ntsc {
        [
            590.0, 1825.0, 950.0, 1380.0, 1030.0, 1210.0, 860.0, 1560.0, 1030.0, 860.0, 1210.0,
            950.0, 1160.0, 1560.0, 1160.0, 1380.0,
        ]
    } else {
        y
    };
    let black = y[0];
    let range = y[1] - black;
    let angle: [f32; 16] = [
        0.0, 0.0, 112.5, 292.5, 67.5, 247.5, 0.0, 180.0, 135.0, 157.5, 112.5, 0.0, 0.0, 247.5, 0.0,
        0.0,
    ];
    std::array::from_fn(|i| {
        let phase = (angle[i] - 4.5).to_radians();
        let saturation = if [0, 1, 11, 12, 15].contains(&i) {
            0.0
        } else {
            0.20
        };
        [
            (y[i] - black) / range,
            phase.cos() * saturation,
            phase.sin() * saturation,
            0.0,
        ]
    })
}
