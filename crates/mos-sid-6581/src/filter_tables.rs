//! Lookup-table construction for the SID op-amp filter model (issues #19/#20).
//!
//! Ported from reSID's `filter8580new.cc` (Dag Lem) as vendored in VICE 3.10
//! (`emulators/c64/vice-3.10/src/resid/`) — the unified op-amp filter model
//! that covers **both** the 6581 and the 8580. (VICE builds it behind
//! `--enable-new8580filter`; the default `filter.cc` still carries a linear
//! 8580 approximation marked `FIXME`.)
//!
//! The model rests on op-amp voltage transfer functions **measured on real
//! chips** (a MOS 6581R4AR and a CSG 8580R5). Everything audible falls out of
//! those curves plus transistor physics:
//!
//! - The measured curve is spline-interpolated into a 16-bit `vo - vx → vx`
//!   table (`opamp_rev`) used by the filter integrators.
//! - Every inverting gain / summer stage in the chip (the filter summer, the
//!   audio mixer, the resonance ladder, the master-volume ladder) is solved
//!   ahead of time with Newton-Raphson over the op-amp curve and the triode
//!   transistor model, giving one 64K-entry table per input configuration.
//! - The 6581's cutoff comes from its (nonlinear, un-terminated) 11-bit DAC
//!   driving NMOS voltage-controlled resistors, modelled with the EKV
//!   transistor model (`vcr_kvg` / `vcr_n_ids_term`); the 8580's cutoff comes
//!   from parallel NMOS resistances proportional to the DAC bits.
//!
//! Tables are built once per model on first use and shared (like reSID's
//! static `model_filter`), so nothing here lands in serialized SID state.
//! A full model costs ~5M Newton-Raphson solves and ~10 MB — hence per-model
//! laziness rather than build-both-up-front.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use std::sync::OnceLock;

use crate::SidModel;
use crate::dac::build_dac_table_f64;

/// One point of a measured op-amp voltage transfer function (vi, vo).
type Point = (f64, f64);

/// SID 6581 op-amp voltage transfer, measured on CAP1B/CAP1A of a chip marked
/// MOS 6581R4AR 0687 14 (reSID). Repeated end points pin the spline.
#[rustfmt::skip]
const OPAMP_VOLTAGE_6581: [Point; 35] = [
    (0.81, 10.31), // Approximate start of actual range
    (0.81, 10.31), // Repeated point
    (2.40, 10.31),
    (2.60, 10.30),
    (2.70, 10.29),
    (2.80, 10.26),
    (2.90, 10.17),
    (3.00, 10.04),
    (3.10,  9.83),
    (3.20,  9.58),
    (3.30,  9.32),
    (3.50,  8.69),
    (3.70,  8.00),
    (4.00,  6.89),
    (4.40,  5.21),
    (4.54,  4.54), // Working point (vi = vo)
    (4.60,  4.19),
    (4.80,  3.00),
    (4.90,  2.30), // Change of curvature
    (4.95,  2.03),
    (5.00,  1.88),
    (5.05,  1.77),
    (5.10,  1.69),
    (5.20,  1.58),
    (5.40,  1.44),
    (5.60,  1.33),
    (5.80,  1.26),
    (6.00,  1.21),
    (6.40,  1.12),
    (7.00,  1.02),
    (7.50,  0.97),
    (8.50,  0.89),
    (10.00, 0.81),
    (10.31, 0.81), // Approximate end of actual range
    (10.31, 0.81), // Repeated end point
];

/// SID 8580 op-amp voltage transfer, measured on CAP1B/CAP1A of a chip marked
/// CSG 8580R5 1690 25 (reSID).
#[rustfmt::skip]
const OPAMP_VOLTAGE_8580: [Point; 23] = [
    (1.30,  8.91), // Approximate start of actual range
    (1.30,  8.91), // Repeated end point
    (4.76,  8.91),
    (4.77,  8.90),
    (4.78,  8.88),
    (4.785, 8.86),
    (4.79,  8.80),
    (4.795, 8.60),
    (4.80,  8.25),
    (4.805, 7.50),
    (4.81,  6.10),
    (4.815, 4.05), // Change of curvature
    (4.82,  2.27),
    (4.825, 1.65),
    (4.83,  1.55),
    (4.84,  1.47),
    (4.85,  1.43),
    (4.87,  1.37),
    (4.90,  1.34),
    (5.00,  1.30),
    (5.10,  1.30),
    (8.91,  1.30), // Approximate end of actual range
    (8.91,  1.30), // Repeated end point
];

/// The 8580 resonance ladder gains, `(Rf|Rx)/Ry` per `res` value, derived from
/// die-photograph resistor ratios (reSID `resGain`).
fn res_gain_8580(n8: usize) -> f64 {
    const R1: f64 = 15.3;
    const R2: f64 = 7.3;
    const R3: f64 = 4.7;
    const RF: f64 = 1.4;
    let feedback = [
        RF,
        RF * R1 / (RF + R1),
        RF * R2 / (RF + R2),
        RF * R3 / (RF + R3),
    ];
    let input = [1.0, 1.4, 2.0, 2.8]; // Ri, R4, R8, RC
    feedback[n8 & 0x3] / input[(n8 >> 2) & 0x3]
}

/// The 4.75 V virtual-ground reference (PolySi divider), +1% (reSID `Vref`).
const VREF: f64 = 4.7975;

/// Per-model physical parameters (reSID `model_filter_init`).
struct ModelInit {
    opamp_voltage: &'static [Point],
    voice_voltage_range: f64,
    voice_dc_voltage: f64,
    /// Integrator capacitor value.
    c: f64,
    vdd: f64,
    /// Threshold voltage.
    vth: f64,
    /// Thermal voltage Ut = k*T/q ~ 26 mV.
    ut: f64,
    /// Gate coupling coefficient.
    k: f64,
    /// u*Cox.
    ucox: f64,
    /// W/L for the VCR (6581 only).
    wl_vcr: f64,
    /// W/L for the "snake" (6581 only).
    wl_snake: f64,
    dac_zero: f64,
    dac_scale: f64,
    dac_2r_div_r: f64,
    dac_term: bool,
}

const MODEL_INIT: [ModelInit; 2] = [
    // MOS 6581
    ModelInit {
        opamp_voltage: &OPAMP_VOLTAGE_6581,
        // The dynamic analog range of one voice is approximately 1.5 V,
        // riding at a DC level of approximately 5.0 V (+1.5%).
        voice_voltage_range: 1.5,
        voice_dc_voltage: 5.075,
        c: 470e-12,
        vdd: 12.18, // 12 V +1.5%
        vth: 1.31,
        ut: 26.0e-3,
        k: 1.0,
        ucox: 20e-6,
        wl_vcr: 9.0 / 1.0,
        wl_snake: 1.0 / 115.0,
        dac_zero: 6.65,
        dac_scale: 2.63,
        dac_2r_div_r: 2.20,
        dac_term: false,
    },
    // CSG 8580
    ModelInit {
        opamp_voltage: &OPAMP_VOLTAGE_8580,
        voice_voltage_range: 0.24, // FIXME in reSID: measure for the 8580
        voice_dc_voltage: 4.7975,  // 4.75 V +1%
        c: 22e-9,
        vdd: 9.09, // 9 V +1%
        vth: 0.80,
        ut: 26.0e-3,
        k: 1.0, // unused on the 8580
        ucox: 100e-6,
        wl_vcr: 0.0,   // 6581 only
        wl_snake: 0.0, // 6581 only
        dac_zero: 0.0,
        dac_scale: 0.0,
        dac_2r_div_r: 2.00,
        dac_term: true,
    },
];

/// Summer table offsets by input count (reSID `summer_offset<n>`): the filter
/// summer has 2-6 input "resistors"; `SUMMER_OFFSET[n]` locates the table for
/// `n` *selected* inputs (bandpass + lowpass always add 2 more).
pub const SUMMER_OFFSET: [usize; 6] = [
    0,
    2 << 16,
    (2 + 3) << 16,
    (2 + 3 + 4) << 16,
    (2 + 3 + 4 + 5) << 16,
    (2 + 3 + 4 + 5 + 6) << 16,
];
pub const SUMMER_LEN: usize = SUMMER_OFFSET[5];

/// Mixer table offsets by input count (reSID `mixer_offset<n>`): the audio
/// mixer has 0-7 input "resistors".
pub const MIXER_OFFSET: [usize; 9] = [
    0,
    1,
    1 + (1 << 16),
    1 + ((1 + 2) << 16),
    1 + ((1 + 2 + 3) << 16),
    1 + ((1 + 2 + 3 + 4) << 16),
    1 + ((1 + 2 + 3 + 4 + 5) << 16),
    1 + ((1 + 2 + 3 + 4 + 5 + 6) << 16),
    1 + ((1 + 2 + 3 + 4 + 5 + 6 + 7) << 16),
];
pub const MIXER_LEN: usize = MIXER_OFFSET[8];

/// All precomputed tables and fixed-point scaling constants for one SID model.
pub struct ModelTables {
    /// K*(Vdd - Vth), normalized/translated to 16-bit table units.
    pub kvddt: i32,
    /// Multiplier taking a 20-bit voice value to table units (fits 11 bits).
    pub voice_scale_s14: i32,
    /// Voice DC operating point in table units.
    pub voice_dc: i32,
    /// Mixer gain trim for filter components: the 6581's mixer input
    /// "resistors" for the filter lines are slightly bigger than the voice
    /// ones (×0.93); identity (×1.0) on the 8580. Scaled by 2^12.
    pub filter_gain: i32,
    /// Reverse op-amp transfer: capacitor-voltage index → op-amp input vx.
    pub opamp_rev: Box<[u16]>,
    /// Filter summer op-amp solutions, five input configurations.
    pub summer: Box<[u16]>,
    /// Master-volume gain ladder: `gain[vol << 16 | vi]`.
    pub gain: Box<[u16]>,
    /// Resonance ladder: `resonance[res << 16 | vbp]`.
    pub resonance: Box<[u16]>,
    /// Audio mixer op-amp solutions, eight input configurations.
    pub mixer: Box<[u16]>,
    /// Cutoff DAC output per 11-bit `fc` value, in table units (6581) or
    /// parallel W/L unit weight (8580).
    pub f0_dac: Box<[u16]>,
    /// 6581 only: normalized "snake" current factor (1 cycle at 1 MHz).
    pub n_snake: i32,
    /// 6581 only: VCR gate voltage `kVg` by `(Vddt-Vw)²/2 + (Vddt-vi)²/2`.
    pub vcr_kvg: Box<[u16]>,
    /// 6581 only: EKV-model `ln²(1+e^…)` current terms by `kVg - Vx + 2^15`.
    pub vcr_n_ids_term: Box<[u16]>,
    /// 8580 only: normalized current parameter (scaled by 2^5).
    pub n_param: i32,
    /// 8580 only: default DAC gate voltage `nVgt` for zero filter bias.
    pub nvgt_default: i32,
}

/// Tables for `model`, built on first use.
pub fn tables(model: SidModel) -> &'static ModelTables {
    static TABLES: [OnceLock<ModelTables>; 2] = [OnceLock::new(), OnceLock::new()];
    TABLES[model.index()].get_or_init(|| build_model(model.index()))
}

/// Working entry of the op-amp transfer table: `vx` and its derivative `dvx`
/// (reSID `opamp_t`).
#[derive(Clone, Copy, Default)]
struct OpAmp {
    vx: u16,
    dvx: i16,
}

// ---------------------------------------------------------------------------
// Spline interpolation (reSID spline.h): approximates Catmull-Rom properties
// with piecewise cubics y = f(x), evaluated by forward differencing.
// ---------------------------------------------------------------------------

#[allow(clippy::many_single_char_names)]
fn cubic_coefficients(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    k1: f64,
    k2: f64,
) -> (f64, f64, f64, f64) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let a = ((k1 + k2) - 2.0 * dy / dx) / (dx * dx);
    let b = ((k2 - k1) / dx - 3.0 * (x1 + x2) * a) / 2.0;
    let c = k1 - (3.0 * x1 * a + 2.0 * b) * x1;
    let d = y1 - ((x1 * a + b) * x1 + c) * x1;
    (a, b, c, d)
}

fn interpolate_segment(
    (x1, y1): Point,
    (x2, y2): Point,
    k1: f64,
    k2: f64,
    plot: &mut impl FnMut(f64, f64),
    res: f64,
) {
    let (a, b, c, d) = cubic_coefficients(x1, y1, x2, y2, k1, k2);

    let mut y = ((a * x1 + b) * x1 + c) * x1 + d;
    let mut dy = (3.0 * a * (x1 + res) + 2.0 * b) * x1 * res + ((a * res + b) * res + c) * res;
    let mut d2y = (6.0 * a * (x1 + res) + 2.0 * b) * res * res;
    let d3y = 6.0 * a * res * res * res;

    let mut x = x1;
    while x <= x2 {
        plot(x, y);
        y += dy;
        dy += d2y;
        d2y += d3y;
        x += res;
    }
}

/// Interpolate the full point set; repeated end points pin the ends, repeated
/// interior points introduce discontinuities (reSID `interpolate`).
fn interpolate(points: &[Point], plot: &mut impl FnMut(f64, f64), res: f64) {
    for i2 in 2..points.len() - 1 {
        let (p0, p1, p2, p3) = (points[i2 - 2], points[i2 - 1], points[i2], points[i2 + 1]);
        // p1 and p2 equal; single point.
        if p1.0 == p2.0 {
            continue;
        }
        let (k1, k2);
        if p0.0 == p1.0 && p2.0 == p3.0 {
            // Both end points repeated; straight line.
            k1 = (p2.1 - p1.1) / (p2.0 - p1.0);
            k2 = k1;
        } else if p0.0 == p1.0 {
            // p0 and p1 equal; use f''(x1) = 0.
            k2 = (p3.1 - p1.1) / (p3.0 - p1.0);
            k1 = (3.0 * (p2.1 - p1.1) / (p2.0 - p1.0) - k2) / 2.0;
        } else if p2.0 == p3.0 {
            // p2 and p3 equal; use f''(x2) = 0.
            k1 = (p2.1 - p0.1) / (p2.0 - p0.0);
            k2 = (3.0 * (p2.1 - p1.1) / (p2.0 - p1.0) - k1) / 2.0;
        } else {
            // Normal curve.
            k1 = (p2.1 - p0.1) / (p2.0 - p0.0);
            k2 = (p3.1 - p1.1) / (p3.0 - p1.0);
        }
        interpolate_segment(p1, p2, k1, k2, plot, res);
    }
}

// ---------------------------------------------------------------------------
// Newton-Raphson gain solver (reSID solve_gain_d): output voltage of an
// inverting gain / summer op-amp stage with "resistor" ratio n, input vi.
// `x` threads the previous solution as a warm start across a table sweep.
// ---------------------------------------------------------------------------

/// The per-model constants the gain solver needs: the op-amp table with its
/// root bracket `[ak, bk]`, and the normalized `k*(Vdd - Vth)`.
struct GainSolver<'a> {
    opamp: &'a [OpAmp],
    ak: i32,
    bk: i32,
    kvddt: i32,
}

impl GainSolver<'_> {
    fn solve(&self, n: f64, vi: i32, x: &mut i32) -> i32 {
        // All variables are translated and scaled to fit 16 bits; translations
        // cancel in the subtractions below: (a - t) - (b - t) = a - b.
        let mut ak = self.ak;
        let mut bk = self.bk;

        let a = n + 1.0;
        let b = self.kvddt;
        let b_vi = if b > vi { f64::from(b - vi) } else { 0.0 };
        let c = n * (b_vi * b_vi);

        loop {
            let xk = *x;

            let vx = i32::from(self.opamp[*x as usize].vx);
            let dvx = i32::from(self.opamp[*x as usize].dvx);

            // f = a*(b - vx)^2 - c - (b - vo)^2
            // df = 2*((b - vo) - a*(b - vx))*dvx
            let vo = (vx + (*x << 1) - (1 << 16)).clamp(0, (1 << 16) - 1);
            let b_vx = if b > vx { f64::from(b - vx) } else { 0.0 };
            let b_vo = if b > vo { f64::from(b - vo) } else { 0.0 };
            let f = a * (b_vx * b_vx) - c - (b_vo * b_vo);
            let df = 2.0 * (b_vo - a * b_vx) * f64::from(dvx);

            // Newton-Raphson step; if f or df are zero we can't improve further.
            if df != 0.0 {
                *x -= (f64::from(1 << 11) * f / df) as i32;
            }
            if *x == xk {
                return vo;
            }

            // Narrow down the root bracket.
            if f < 0.0 {
                ak = xk;
            } else {
                bk = xk;
            }

            if *x <= ak || *x >= bk {
                // Bisection step (à la Dekker's method).
                *x = (ak + bk) >> 1;
                if *x == ak {
                    return vo;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Model construction (reSID Filter::Filter class-init block).
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)]
fn build_model(m: usize) -> ModelTables {
    let fi = &MODEL_INIT[m];
    let dac_bits = 11u32;

    // Convert op-amp voltage transfer to 16-bit values.
    let vmin = fi.opamp_voltage[0].0;
    let opamp_max = fi.opamp_voltage[0].1;
    let kvddt_v = fi.k * (fi.vdd - fi.vth);
    let vmax = if kvddt_v < opamp_max {
        opamp_max
    } else {
        kvddt_v
    };
    let denorm = vmax - vmin;
    let norm = 1.0 / denorm;

    // Scaling and translation constants. (reSID also derives an N30 here for
    // its commented-out capacitor clamp; the clamp is FIXME-disabled there,
    // so it is not ported.)
    let n16 = norm * f64::from((1u32 << 16) - 1);
    let n31 = norm * f64::from((1u32 << 31) - 1);
    let n14 = norm * f64::from(1u32 << 14);

    // The 6581's mixer input resistors for the filter lines are slightly
    // bigger than the voice ones; scale accordingly.
    let scale_factor = if m == 0 { 0.93 } else { 1.0 };
    let filter_gain = (scale_factor * f64::from(1 << 12)) as i32;

    let voice_scale_s14 = (n14 * fi.voice_voltage_range) as i32;
    let voice_dc = (n16 * (fi.voice_dc_voltage - vmin)) as i32;

    // k*Vddt - x = (k*Vddt - t) - (x - t): normalized for translated subtraction.
    let kvddt = (n16 * (kvddt_v - vmin) + 0.5) as i32;

    let tmp_n_param = denorm * f64::from(1 << 13) * ((fi.ucox / 2.0) * 1.0e-6 / fi.c);

    // Map op-amp voltage across output and input to input voltage:
    // vo - vx -> vx, x axis scaled to 16 bits, y axis to 31 bits for the
    // derivative's accuracy. Reversed so x ascends.
    let size = fi.opamp_voltage.len();
    let mut scaled_voltage = vec![(0.0f64, 0.0f64); size];
    for i in 0..size {
        scaled_voltage[size - 1 - i].0 = n16 * (fi.opamp_voltage[i].1 - fi.opamp_voltage[i].0) / 2.0
            // Translate to the positive axis by adding 2^15; the integrator
            // lookup does the same at run time.
            + f64::from(1 << 15);
        scaled_voltage[size - 1 - i].1 = n31 * (fi.opamp_voltage[i].0 - vmin);
    }
    // Clamp x to the 16-bit range (rounding may cause overflow).
    if scaled_voltage[size - 1].0 > 65535.0 {
        scaled_voltage[size - 1].0 = 65535.0;
        scaled_voltage[size - 2].0 = 65535.0;
    }

    let mut voltages = vec![0u32; 1 << 16];
    interpolate(
        &scaled_voltage,
        &mut |x: f64, y: f64| {
            let y = if y < 0.0 { 0.0 } else { y };
            voltages[x as usize] = (y + 0.5) as u32;
        },
        1.0,
    );

    let ak = (scaled_voltage[0].0 + 0.5) as i32;
    let bk = (scaled_voltage[size - 1].0 + 0.5) as i32;

    // Store both fn and dfn in the same table.
    let mut opamp = vec![OpAmp::default(); 1 << 16];
    let mut f = voltages[ak as usize];
    for j in (ak as usize)..(bk as usize) {
        let fp = f;
        f = voltages[j]; // Scaled by m*2^31
        // m*2^31*dy/1 = (m*2^31*dy)/(m*2^16*dx) = 2^15*dy/dx
        let df = f.wrapping_sub(fp) as i32; // Scaled by 2^15
        // 16 bits unsigned: m*2^16*(fn - xmin)
        opamp[j].vx = if f > (0xffff << 15) {
            0xffff
        } else {
            (f >> 15) as u16
        };
        // 16 bits (15 bits + sign bit): 2^11*dfn
        opamp[j].dvx = (df >> (15 - 11)) as i16;
    }
    // We don't have the differential for the first point, so assume it equals
    // the second point's.
    opamp[ak as usize].dvx = opamp[ak as usize + 1].dvx;

    // The filter summer operates at n ~ 1 and has 5 fundamentally different
    // input configurations (2-6 input "resistors"). All "on" transistors are
    // modeled as one — modeling them separately would be extremely costly.
    let solver = GainSolver {
        opamp: &opamp,
        ak,
        bk,
        kvddt,
    };
    let mut summer = vec![0u16; SUMMER_LEN];
    let mut offset = 0usize;
    for k in 0..5usize {
        let idiv = 2 + k; // 2-6 input "resistors"
        let n_idiv = idiv as f64;
        let seg = idiv << 16;
        let mut x = ak;
        for vi in 0..seg {
            summer[offset + vi] = solver.solve(n_idiv, (vi / idiv) as i32, &mut x) as u16;
        }
        offset += seg;
    }

    // The audio mixer operates at n ~ 8/6 (6581) or 8/5 (8580) and has 8
    // input configurations (0-7 input "resistors").
    let mixer_divider = if m == 0 { 6.0 } else { 5.0 };
    let mut mixer = vec![0u16; MIXER_LEN];
    let mut offset = 0usize;
    let mut seg = 1usize; // one lookup element for 0 input "resistors"
    for l in 0..8usize {
        let n_idiv = ((l << 3) as f64) / mixer_divider;
        let idiv = if l == 0 { 1 } else { l }; // avoid /0; correct since n = 0
        let mut x = ak;
        for vi in 0..seg {
            mixer[offset + vi] = solver.solve(n_idiv, (vi / idiv) as i32, &mut x) as u16;
        }
        offset += seg;
        seg = (l + 1) << 16;
    }

    // 4-bit "resistor" ladders in the audio output gain necessitate 16 gain
    // tables. From die photographs, gain ~ vol/12 (6581) or vol/16 (8580).
    let gain_divider = if m == 0 { 12.0 } else { 16.0 };
    let mut gain = vec![0u16; 16 << 16];
    for n8 in 0..16usize {
        let n = n8 as f64 / gain_divider;
        let mut x = ak;
        for vi in 0..(1usize << 16) {
            gain[(n8 << 16) + vi] = solver.solve(n, vi as i32, &mut x) as u16;
        }
    }

    // Resonance ladder. 6581: 1/Q ~ ~res/8 from the die's linear ladder.
    // 8580: the ladder is split into op-amp input and feedback parts, giving
    // 1/Q ~ 2^((4 - res)/8) — see res_gain_8580.
    let mut resonance = vec![0u16; 16 << 16];
    for n8 in 0..16usize {
        let n = if m == 0 {
            ((!n8) & 0xf) as f64 / 8.0
        } else {
            res_gain_8580(n8)
        };
        let mut x = ak;
        for vi in 0..(1usize << 16) {
            resonance[(n8 << 16) + vi] = solver.solve(n, vi as i32, &mut x) as u16;
        }
    }

    // Capacitor voltage to op-amp input voltage: vc -> vx.
    let opamp_rev: Vec<u16> = opamp.iter().map(|o| o.vx).collect();

    let mut t = ModelTables {
        kvddt,
        voice_scale_s14,
        voice_dc,
        filter_gain,
        opamp_rev: opamp_rev.into_boxed_slice(),
        summer: summer.into_boxed_slice(),
        gain: gain.into_boxed_slice(),
        resonance: resonance.into_boxed_slice(),
        mixer: mixer.into_boxed_slice(),
        f0_dac: vec![0u16; 1 << dac_bits].into_boxed_slice(),
        n_snake: 0,
        vcr_kvg: Box::default(),
        vcr_n_ids_term: Box::default(),
        n_param: 0,
        nvgt_default: 0,
    };

    if m == 0 {
        // 6581 only.

        // Normalized snake current factor, 1 cycle at 1 MHz. Fits 5 bits.
        t.n_snake = (fi.wl_snake * tmp_n_param + 0.5) as i32;

        // Cutoff DAC table: the 6581's un-terminated 11-bit R-2R ladder
        // (dac.rs), mapped to op-amp table units.
        let dac = build_dac_table_f64(dac_bits as usize, fi.dac_2r_div_r, fi.dac_term);
        for (n, value) in dac.iter().enumerate() {
            t.f0_dac[n] = (n16
                * (fi.dac_zero + value * fi.dac_scale / f64::from(1 << dac_bits) - vmin)
                + 0.5) as u16;
        }

        // VCR gate-voltage table: Vg = Vddt - sqrt(((Vddt-Vw)^2 + Vgdt^2)/2),
        // with the argument pre-divided by 2 and right-shifted 16 at lookup.
        let k = fi.k;
        let kvddt_n16 = n16 * (k * (fi.vdd - fi.vth));
        let vmin_n16 = vmin * n16;
        let mut vcr_kvg = vec![0u16; 1 << 16];
        for (i, slot) in vcr_kvg.iter_mut().enumerate() {
            let vg = kvddt_n16 - (i as f64 * f64::from(1 << 16)).sqrt();
            *slot = (k * vg - vmin_n16 + 0.5) as u16;
        }
        t.vcr_kvg = vcr_kvg.into_boxed_slice();

        // EKV model current terms:
        //   Ids = Is*(if - ir)
        //   Is = ((2*u*Cox*Ut^2)/k)*W/L
        //   if = ln^2(1 + e^((k*(Vg - Vt) - Vs)/(2*Ut)))
        //   ir = ln^2(1 + e^((k*(Vg - Vt) - Vd)/(2*Ut)))
        let kvt = fi.k * fi.vth;
        let ut = fi.ut;
        let is = ((2.0 * fi.ucox * ut * ut) / fi.k) * fi.wl_vcr;
        // Normalized current factor for 1 cycle at 1 MHz.
        let n15 = n16 / 2.0;
        let n_is = n15 * 1.0e-6 / fi.c * is;
        let mut vcr_n_ids_term = vec![0u16; 1 << 16];
        for (i, slot) in vcr_n_ids_term.iter_mut().enumerate() {
            // kVg_Vx = k*Vg - Vx, translated by 2^15 into the table index.
            let kvg_vx = i as i32 - (1 << 15);
            let log_term = ((f64::from(kvg_vx) / n16 - kvt) / (2.0 * ut)).exp().ln_1p();
            // Scaled by m*2^15.
            *slot = (n_is * log_term * log_term) as u16;
        }
        t.vcr_n_ids_term = vcr_n_ids_term.into_boxed_slice();
    } else {
        // 8580 only.

        // Normalized current parameter, scaled by 2^5.
        t.n_param = (tmp_n_param * 32.0 + 0.5) as i32;

        // Default DAC gate voltage for zero filter bias: the gate is driven by
        // a switched-capacitor voltage divider, Vg = Vref * 1.6.
        let vgt = VREF * 1.6 - fi.vth;
        t.nvgt_default = (n16 * (vgt - vmin) + 0.5) as i32;

        // Cutoff "DAC": parallel NMOS resistances with W/L proportional to the
        // fc bits. dacWL = 806 ~= 0.003075 * 1024 * 256, scaled by 2^5 after
        // the >>8.
        let dac_wl = 806u32;
        t.f0_dac[0] = (dac_wl >> 8) as u16;
        for n in 1..(1usize << dac_bits) {
            let mut wl = 0u32;
            for i in 0..dac_bits {
                let bitmask = 1u32 << i;
                if n as u32 & bitmask != 0 {
                    wl += dac_wl * (bitmask << 1);
                }
            }
            t.f0_dac[n] = (wl >> 8) as u16;
        }
    }

    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice_of(table: &[u16], start: usize, span: usize, step: usize) -> Vec<u16> {
        (0..span).step_by(step).map(|i| table[start + i]).collect()
    }

    /// Golden slices captured from reSID itself (a `#define protected public`
    /// dump of `Filter::model_filter` in the vendored VICE 3.10 reSID with
    /// `NEW_8580_FILTER=1`, sampled every 8192 entries). Bit-exact agreement
    /// here means the whole Newton-Raphson table build matches; only
    /// `f0_dac` is checked with ±4 slack because reSID rounds its R-2R DAC
    /// table to `u16` before scaling while we keep `f64` precision.
    #[test]
    fn tables_match_resid_golden_slices() {
        let t = tables(SidModel::Mos6581);
        assert_eq!(t.kvddt, 65535);
        assert_eq!(t.voice_scale_s14, 2442);
        assert_eq!(t.voice_dc, 27783);
        assert_eq!(t.filter_gain, 3809);
        assert_eq!(t.n_snake, 15);
        assert_eq!(
            slice_of(&t.opamp_rev, 0, 1 << 16, 8192),
            [0, 49698, 35073, 26571, 24299, 21266, 17764, 12553]
        );
        assert_eq!(
            slice_of(&t.gain, 15 << 16, 1 << 16, 8192),
            [55423, 42310, 31994, 24058, 17786, 13232, 10064, 8297]
        );
        assert_eq!(
            slice_of(&t.resonance, 8 << 16, 1 << 16, 8192),
            [47034, 37446, 29970, 24122, 19467, 16070, 13724, 12324]
        );
        assert_eq!(
            slice_of(&t.mixer, MIXER_OFFSET[3], 3 << 16, 24576),
            [61896, 60249, 41540, 23727, 8849, 3876, 2605, 2056]
        );
        assert_eq!(
            slice_of(&t.summer, SUMMER_OFFSET[3], 5 << 16, 40960),
            [61897, 61421, 43439, 23643, 7055, 3220, 2174, 1659]
        );
        assert_eq!(
            slice_of(&t.vcr_kvg, 0, 1 << 16, 8192),
            [65535, 42365, 32767, 25403, 19194, 13724, 8779, 4232]
        );
        assert_eq!(
            slice_of(&t.vcr_n_ids_term, 0, 1 << 16, 8192),
            [0, 0, 0, 0, 0, 0, 905, 3782]
        );
        let resid_f0_dac = [38170, 40361, 42419, 44603, 46401, 48593, 50643, 52835];
        for (i, want) in (0..2048).step_by(256).zip(resid_f0_dac) {
            let got = i32::from(t.f0_dac[i]);
            assert!((got - want).abs() <= 4, "f0_dac[{i}] = {got}, want ~{want}");
        }

        let t = tables(SidModel::Mos8580);
        assert_eq!(t.kvddt, 60196);
        assert_eq!(t.voice_scale_s14, 516);
        assert_eq!(t.voice_dc, 30119);
        assert_eq!(t.filter_gain, 4096);
        assert_eq!(t.n_param, 4534);
        assert_eq!(t.nvgt_default, 48019);
        assert_eq!(
            slice_of(&t.opamp_rev, 0, 1 << 16, 8192),
            [0, 49151, 32767, 30294, 30254, 30211, 30098, 16398]
        );
        assert_eq!(
            slice_of(&t.gain, 15 << 16, 1 << 16, 8192),
            [65535, 65535, 65535, 36761, 28083, 22938, 19951, 18650]
        );
        assert_eq!(
            slice_of(&t.resonance, 8 << 16, 1 << 16, 8192),
            [65535, 65535, 46601, 34956, 28618, 24650, 22294, 21260]
        );
        assert_eq!(
            slice_of(&t.mixer, MIXER_OFFSET[3], 3 << 16, 24576),
            [65535, 65535, 65535, 65535, 20442, 2122, 0, 0]
        );
        assert_eq!(
            slice_of(&t.summer, SUMMER_OFFSET[3], 5 << 16, 40960),
            [65535, 65535, 65535, 65535, 20089, 1481, 0, 0]
        );
        assert_eq!(
            slice_of(&t.f0_dac, 0, 2048, 256),
            [3, 1612, 3224, 4836, 6448, 8060, 9672, 11284]
        );
    }
}
