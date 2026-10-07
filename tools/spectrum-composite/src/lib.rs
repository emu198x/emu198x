//! Offline 48K PAL experiment. This is an ideal-clock decoder, not a TV PLL.
//!
//! ULA pin voltages: Smith, Chapter 16, Tables 16-1/2/3, as held in
//! reference/by-system/sinclair-zx-spectrum/zx-spectrum-ula-chapter-16-analogue-video.md.
//! Horizontal schedule: Chapter 11, Table 11-1 (6C ULA). Receiver FIRs and
//! gain calibration are explicit experimental assumptions, not measured hardware.

use std::f64::consts::TAU;

pub const PIXEL_HZ: f64 = 7_000_000.0;
pub const CARRIER_HZ: f64 = 4_433_618.75;
pub const LINE_PIXELS: usize = 448;
pub const FRAME_LINES: usize = 312;
pub const WIDTH: usize = 352;
pub const HEIGHT: usize = 296;

const Y_PIN: [f64; 16] = [
    2.449, 2.169, 1.896, 1.617, 1.370, 1.091, 0.818, 0.539, 2.449, 2.082, 1.724, 1.356, 1.033,
    0.666, 0.259, 0.259,
];
const U_PIN: [f64; 8] = [2.016, 1.013, 2.358, 1.347, 2.692, 1.681, 3.027, 2.016];
const V_PIN: [f64; 8] = [1.925, 2.153, 0.495, 0.737, 3.099, 3.341, 1.684, 1.925];

/// Picture-oriented, black-referenced Y/U/V, with white Y normalised to 1.
/// Chroma gains match the documented blue U and red V excursions to nominal
/// PAL coordinates. Absolute PCB/receiver gain has not been measured.
#[derive(Clone, Copy, Debug, Default)]
pub struct Yuv {
    pub y: f64,
    pub u: f64,
    pub v: f64,
}

pub fn colour(index: u8) -> Yuv {
    let i = usize::from(index & 15);
    Yuv {
        y: (Y_PIN[0] - Y_PIN[i]) / (Y_PIN[0] - Y_PIN[15]),
        u: (U_PIN[0] - U_PIN[i & 7]) * (0.437 / 1.003),
        v: (V_PIN[0] - V_PIN[i & 7]) * (0.615 / 1.430),
    }
}

/// TV decoder matrix, rather than the Spectrum encoder's blue-lift equation.
/// Inputs and output are video-coded amplitudes; no phosphor transfer is added.
pub fn rgb(c: Yuv) -> [f64; 3] {
    [
        c.y + c.v / 0.877,
        c.y - 0.114 / (0.587 * 0.493) * c.u - 0.299 / (0.587 * 0.877) * c.v,
        c.y + c.u / 0.493,
    ]
}

pub fn raw_rgb(index: u8) -> [f64; 3] {
    let level = if index & 8 != 0 { 1.0 } else { 194.0 / 255.0 };
    [
        if index & 2 != 0 { level } else { 0.0 },
        if index & 4 != 0 { level } else { 0.0 },
        if index & 1 != 0 { level } else { 0.0 },
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Separated,
    Composite,
}

#[derive(Clone, Copy, Debug)]
pub struct Receiver {
    pub samples_per_pixel: usize,
    pub luma_hz: f64,
    pub chroma_hz: f64,
    pub phase_cycles: f64,
    pub field: u64,
    pub delay_line: bool,
}

impl Default for Receiver {
    fn default() -> Self {
        Self {
            samples_per_pixel: 4,
            luma_hz: 3_000_000.0,
            chroma_hz: 1_300_000.0,
            phase_cycles: 0.0,
            field: 0,
            delay_line: true,
        }
    }
}

/// Zero-phase, odd-length windowed-sinc low-pass with unit DC gain.
/// Tap count scales with sample rate so 4x and 8x have the same time support.
pub fn low_pass(sample_hz: f64, cutoff_hz: f64, radius: usize) -> Vec<f64> {
    let mut taps = Vec::with_capacity(radius * 2 + 1);
    for n in 0..=radius * 2 {
        let x = n as f64 - radius as f64;
        let ideal = if n == radius {
            2.0 * cutoff_hz / sample_hz
        } else {
            (TAU * cutoff_hz * x / sample_hz).sin() / (std::f64::consts::PI * x)
        };
        let window = 0.54 + 0.46 * (std::f64::consts::PI * x / radius as f64).cos();
        taps.push(ideal * window);
    }
    let sum: f64 = taps.iter().sum();
    for tap in &mut taps {
        *tap /= sum;
    }
    taps
}

fn filter(input: &[f64], taps: &[f64]) -> Vec<f64> {
    if input.is_empty() {
        return Vec::new();
    }
    let radius = taps.len() / 2;
    // Edge extension is confined to the off-picture ends of a physical line.
    // No wraparound of sync/burst or opposite picture edges.
    let mut padded = Vec::with_capacity(input.len() + radius * 2);
    padded.resize(radius, input[0]);
    padded.extend_from_slice(input);
    padded.resize(input.len() + radius * 2, input[input.len() - 1]);
    padded
        .windows(taps.len())
        .map(|window| {
            window
                .iter()
                .zip(taps)
                .map(|(sample, tap)| sample * tap)
                .sum()
        })
        .collect()
}

/// Translate the existing 48K framebuffer crop to engine pixel positions.
/// CONFIG_48K uses p+36 before blanking and p-412 after blanking. This is a
/// static-frame adapter, not an observation of analogue pins or beam position.
pub fn engine_pixel(x: usize) -> usize {
    if x < 36 { x + 412 } else { x - 36 }
}
pub fn physical_line(y: usize) -> usize {
    if y < 48 { y + 264 } else { y - 48 }
}

pub struct Experiment {
    receiver: Receiver,
    luma_taps: Vec<f64>,
    chroma_taps: Vec<f64>,
    oscillators: Vec<Vec<(f64, f64)>>,
}

impl Experiment {
    /// Precompute filters and colour reference. Processing timings exclude setup.
    pub fn new(receiver: Receiver) -> Result<Self, &'static str> {
        if !matches!(receiver.samples_per_pixel, 4 | 8) {
            return Err("sampling must be 4 or 8");
        }
        if !(receiver.luma_hz > 0.0
            && receiver.luma_hz < CARRIER_HZ
            && receiver.chroma_hz > 0.0
            && receiver.chroma_hz < CARRIER_HZ / 2.0
            && receiver.phase_cycles.is_finite())
        {
            return Err("invalid receiver parameters");
        }
        let fs = PIXEL_HZ * receiver.samples_per_pixel as f64;
        let luma_taps = low_pass(fs, receiver.luma_hz, 8 * receiver.samples_per_pixel);
        let chroma_taps = low_pass(fs, receiver.chroma_hz, 12 * receiver.samples_per_pixel);
        let oscillators = (0..HEIGHT)
            .map(|row| {
                // The crop starts at line 264 and crosses the counter wrap.
                // An independent oscillator continues through that wrap.
                let line = row + 264;
                (0..LINE_PIXELS * receiver.samples_per_pixel)
                    .map(|sample| {
                        let tick = receiver.field as f64 * (FRAME_LINES * LINE_PIXELS) as f64
                            + line as f64 * LINE_PIXELS as f64
                            + (sample as f64 + 0.5) / receiver.samples_per_pixel as f64;
                        let phase =
                            TAU * (tick * CARRIER_HZ / PIXEL_HZ + receiver.phase_cycles).fract();
                        (phase.cos(), phase.sin())
                    })
                    .collect()
            })
            .collect();
        Ok(Self {
            receiver,
            luma_taps,
            chroma_taps,
            oscillators,
        })
    }

    /// Decode a static indexed frame through a full horizontal line, including
    /// idealised blank/sync/burst. Vertical sync and receiver lock are absent.
    /// Values remain unclipped YUV until RGB export, allowing numerical checks.
    pub fn decode(&self, indices: &[u8], connection: Connection) -> Result<Vec<Yuv>, &'static str> {
        if indices.len() != WIDTH * HEIGHT || indices.iter().any(|i| *i > 15) {
            return Err("expected a 352 by 296 frame of indices 0 through 15");
        }
        let spp = self.receiver.samples_per_pixel;
        let n = LINE_PIXELS * spp;
        let mut output = vec![Yuv::default(); indices.len()];
        let mut previous: Option<Vec<Yuv>> = None;
        for (row, input) in indices.as_chunks::<WIDTH>().0.iter().enumerate() {
            let line = physical_line(row);
            let sign = if line.is_multiple_of(2) { 1.0 } else { -1.0 };
            let oscillator = &self.oscillators[row];
            let mut source = vec![Yuv::default(); n];
            for (x, index) in input.iter().enumerate() {
                let p = engine_pixel(x);
                source[p * spp..(p + 1) * spp].fill(colour(*index));
            }
            // Canonical 6C counter is engine p+4: sync 344..375,
            // burst 384..399, blank 320..415. Amplitudes are normalised
            // experimental values, not measured composite volts.
            source[340 * spp..372 * spp].fill(Yuv {
                y: -0.3 / 0.7,
                u: 0.0,
                v: 0.0,
            });
            source[380 * spp..396 * spp].fill(Yuv {
                y: 0.0,
                u: -0.2,
                v: 0.2,
            });
            let decoded = match connection {
                Connection::Separated => {
                    let y = filter(
                        &source.iter().map(|c| c.y).collect::<Vec<_>>(),
                        &self.luma_taps,
                    );
                    let u = filter(
                        &source.iter().map(|c| c.u).collect::<Vec<_>>(),
                        &self.chroma_taps,
                    );
                    let v = filter(
                        &source.iter().map(|c| c.v).collect::<Vec<_>>(),
                        &self.chroma_taps,
                    );
                    y.into_iter()
                        .zip(u)
                        .zip(v)
                        .map(|((y, u), v)| Yuv { y, u, v })
                        .collect::<Vec<_>>()
                }
                Connection::Composite => {
                    let composite: Vec<_> = source
                        .iter()
                        .zip(oscillator)
                        .map(|(c, (cos, sin))| c.y + c.u * cos + sign * c.v * sin)
                        .collect();
                    let y = filter(&composite, &self.luma_taps);
                    let mixed_u: Vec<_> = composite
                        .iter()
                        .zip(&y)
                        .zip(oscillator)
                        .map(|((c, y), (cos, _))| 2.0 * (c - y) * cos)
                        .collect();
                    let mixed_v: Vec<_> = composite
                        .iter()
                        .zip(&y)
                        .zip(oscillator)
                        .map(|((c, y), (_, sin))| 2.0 * (c - y) * sin * sign)
                        .collect();
                    let u = filter(&mixed_u, &self.chroma_taps);
                    let v = filter(&mixed_v, &self.chroma_taps);
                    y.into_iter()
                        .zip(u)
                        .zip(v)
                        .map(|((y, u), v)| Yuv { y, u, v })
                        .collect::<Vec<_>>()
                }
            };
            for x in 0..WIDTH {
                let sample = engine_pixel(x) * spp + spp / 2;
                // Samples are taken at (s+0.5)/spp. Interpolate the two central
                // samples to the same pixel centre at either sampling rate.
                let a = decoded[sample - 1];
                let b = decoded[sample];
                let mut c = Yuv {
                    y: (a.y + b.y) * 0.5,
                    u: (a.u + b.u) * 0.5,
                    v: (a.v + b.v) * 0.5,
                };
                if self.receiver.delay_line
                    && let Some(prev) = &previous
                {
                    c.u = (c.u + (prev[sample - 1].u + prev[sample].u) * 0.5) * 0.5;
                    c.v = (c.v + (prev[sample - 1].v + prev[sample].v) * 0.5) * 0.5;
                }
                output[row * WIDTH + x] = c;
            }
            previous = Some(decoded);
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carrier_continues_across_the_ula_frame_counter_wrap() {
        let e = Experiment::new(Receiver::default()).expect("receiver");
        let (cos, sin) = e.oscillators[47][LINE_PIXELS * 4 - 1];
        let (next_cos, next_sin) = e.oscillators[48][0];
        let step = TAU * CARRIER_HZ / (PIXEL_HZ * 4.0);
        assert!((next_cos - (cos * step.cos() - sin * step.sin())).abs() < 1e-9);
        assert!((next_sin - (sin * step.cos() + cos * step.sin())).abs() < 1e-9);
    }

    #[test]
    fn analogue_levels_preserve_bright_chroma_and_black_identity() {
        for i in 0..8 {
            assert_eq!(colour(i).u, colour(i + 8).u);
            assert_eq!(colour(i).v, colour(i + 8).v);
        }
        assert_eq!(colour(0).y, colour(8).y);
        assert_eq!(colour(7).u, 0.0);
        assert_eq!(colour(7).v, 0.0);
        assert_eq!(colour(14).y, colour(15).y); // provisional shadowed table cell
        assert!((colour(1).y - 0.12785).abs() < 0.00001);
    }

    #[test]
    fn receiver_passes_dc_and_rejects_subcarrier() {
        let fs = PIXEL_HZ * 4.0;
        let taps = low_pass(fs, 1_300_000.0, 48);
        assert!((taps.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        let gain = |hz: f64| {
            taps.iter()
                .enumerate()
                .map(|(i, t)| t * (TAU * hz / fs * (i as f64 - 48.0)).cos())
                .sum::<f64>()
                .abs()
        };
        assert!(gain(500_000.0) > 0.95);
        assert!(gain(CARRIER_HZ) < 0.005);
    }

    #[test]
    fn solid_colour_decodes_on_both_pal_line_polarities() {
        let experiment = Experiment::new(Receiver::default()).expect("receiver");
        let decoded = experiment
            .decode(&vec![2; WIDTH * HEIGHT], Connection::Composite)
            .expect("frame");
        let expected = colour(2);
        for row in [100, 101] {
            let c = decoded[row * WIDTH + 160];
            assert!((c.y - expected.y).abs() < 0.01);
            assert!((c.u - expected.u).abs() < 0.01);
            assert!((c.v - expected.v).abs() < 0.01);
        }
    }

    #[test]
    fn monochrome_detail_generates_chroma_only_when_combined() {
        let experiment = Experiment::new(Receiver::default()).expect("receiver");
        let frame: Vec<_> = (0..WIDTH * HEIGHT)
            .map(|i| if (i % WIDTH).is_multiple_of(2) { 15 } else { 0 })
            .collect();
        let composite = experiment
            .decode(&frame, Connection::Composite)
            .expect("frame");
        let separate = experiment
            .decode(&frame, Connection::Separated)
            .expect("frame");
        let energy = |v: &[Yuv]| {
            v[100 * WIDTH + 60..100 * WIDTH + 290]
                .iter()
                .map(|c| c.u * c.u + c.v * c.v)
                .sum::<f64>()
        };
        assert!(energy(&composite) > 0.01);
        assert_eq!(energy(&separate), 0.0);
    }

    #[test]
    fn malformed_frames_and_sampling_are_rejected() {
        assert!(
            Experiment::new(Receiver {
                samples_per_pixel: 1,
                ..Receiver::default()
            })
            .is_err()
        );
        let e = Experiment::new(Receiver::default()).expect("receiver");
        assert!(e.decode(&[], Connection::Composite).is_err());
        assert!(
            e.decode(&vec![16; WIDTH * HEIGHT], Connection::Composite)
                .is_err()
        );
    }
}
