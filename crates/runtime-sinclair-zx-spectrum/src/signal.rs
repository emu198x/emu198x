//! 48K Ferranti source model. Primary source: umbrella reference/by-system/
//! sinclair-zx-spectrum/zx-spectrum-ula-chapter-16-analogue-video.md.
//! Gains and bright-yellow's shadowed table cell remain calibration assumptions.
use std::sync::LazyLock;
pub(crate) static LEVELS: LazyLock<[[f32; 4]; 16]> = LazyLock::new(|| {
    let y = [
        2.449, 2.169, 1.896, 1.617, 1.370, 1.091, 0.818, 0.539, 2.449, 2.082, 1.724, 1.356, 1.033,
        0.666, 0.259, 0.259,
    ];
    let u = [2.016, 1.013, 2.358, 1.347, 2.692, 1.681, 3.027, 2.016];
    let v = [1.925, 2.153, 0.495, 0.737, 3.099, 3.341, 1.684, 1.925];
    std::array::from_fn(|i| {
        [
            (2.449 - y[i]) / (2.449 - 0.259),
            (2.016 - u[i & 7]) * 0.437 / 1.003,
            (1.925 - v[i & 7]) * 0.615 / 1.430,
            0.0,
        ]
    })
});
