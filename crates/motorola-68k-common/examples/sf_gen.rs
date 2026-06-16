// Generates random floatx80 test vectors, runs our SoftFloat port, and emits
// `a_high a_low b_high b_low mode op rust_high rust_low rust_flags` (hex) for
// `validation/winuae_check.cpp` to diff against WinUAE's silicon-validated
// SOFTFLOAT_68K. Deterministic LCG so runs are reproducible; op set on argv[1].
use motorola_68k_common::registers::FpReg;
use motorola_68k_common::softfloat::{self as sf, RoundingMode};
use motorola_68k_common::softfloat_fpsp as fpsp;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

fn rand_fx80(r: &mut Lcg) -> (u16, u64) {
    let sign = (r.next() & 1) as u16;
    let class = r.next() % 100;
    let (exp, low): (u16, u64) = if class < 45 {
        // moderate normal
        let e = 0x3FFFu16
            .wrapping_add((r.next() % 129) as u16)
            .wrapping_sub(64);
        (e & 0x7FFF, r.next() | 0x8000_0000_0000_0000)
    } else if class < 65 {
        // wide normal
        let e = (1 + (r.next() % 0x7FFD)) as u16;
        (e, r.next() | 0x8000_0000_0000_0000)
    } else if class < 75 {
        (0, 0) // zero
    } else if class < 83 {
        (0x7FFF, 0x8000_0000_0000_0000) // inf
    } else if class < 92 {
        // nan (quiet/signalling)
        let frac = (r.next() & 0x3FFF_FFFF_FFFF_FFFF).max(1);
        let quiet = if r.next() & 1 == 0 {
            0x4000_0000_0000_0000
        } else {
            0
        };
        (0x7FFF, 0x8000_0000_0000_0000 | quiet | frac)
    } else {
        // subnormal
        (0, (r.next() & 0x7FFF_FFFF_FFFF_FFFF).max(1))
    };
    ((sign << 15) | exp, low)
}

fn mode_of(m: u64) -> RoundingMode {
    RoundingMode::from_fpcr_bits((m & 3) as u8)
}

/// Random 96-bit packed-decimal operand (three big-endian longwords), with a
/// realistic digit mix plus occasional ±0 / ±∞ / NaN encodings so the
/// special-case paths of `pack_decimal_to_floatx80` are exercised.
fn rand_bcd(r: &mut Lcg) -> (u32, u32, u32) {
    let sign = (r.next() & 1) as u32;
    let class = r.next() % 100;
    if class < 6 {
        // ±0 (zero significand — exponent ignored).
        return (sign << 31, 0, 0);
    }
    if class < 12 {
        // ±∞ (exponent field 0x7FFF, zero packed fraction).
        return ((sign << 31) | 0x7FFF_0000, 0, 0);
    }
    if class < 18 {
        // NaN (exponent field 0x7FFF, non-zero fraction; copied bit for bit).
        let hi = (r.next() & 0xFFFF) as u32;
        return (
            (sign << 31) | 0x7FFF_0000 | hi,
            r.next() as u32,
            r.next() as u32 | 1,
        );
    }
    // Finite: random 3-digit exponent + sign, 1 integer + 16 fraction digits.
    let exp_sign = (r.next() & 1) as u32;
    let d = |r: &mut Lcg| (r.next() % 10) as u32; // one decimal digit
    let pack_exp = (d(r) << 8) | (d(r) << 4) | d(r);
    let pack_int = d(r);
    let mut pack_frac: u64 = 0;
    for i in 0..16 {
        pack_frac |= u64::from(d(r)) << ((15 - i) * 4);
    }
    let wrd0 = (sign << 31) | (exp_sign << 30) | (pack_exp << 16) | pack_int;
    (wrd0, (pack_frac >> 32) as u32, pack_frac as u32)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let op: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let n: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(200_000);
    let mut r = Lcg(0x1234_5678_9ABC_DEF1 ^ ((op as u64) << 40));
    let mut out = String::with_capacity(1 << 20);
    for _ in 0..n {
        // 23/24: packed-decimal FMOVE.P — bit-exact vs WinUAE's
        // softfloat_decimal. These repurpose the columns: op 23 (store,
        // floatx80 → 96-bit BCD) takes the value in `a` and the k-factor in
        // `bl`, and emits wrd1:wrd2 in `rl` and wrd0 in the flags column; op 24
        // (load, BCD → floatx80) takes the 96-bit BCD as wrd0 in `ah` and
        // wrd1:wrd2 in `al`, and emits the floatx80 result in `rh:rl`.
        if op == 23 {
            let (ah, al) = rand_fx80(&mut r);
            let m = r.next() % 4;
            let mode = mode_of(m);
            let kf = (r.next() % 128) as i32 - 64; // decoded k-factor, −64..63
            sf::clear_exception_flags();
            let wrd = sf::floatx80_to_pack_decimal(FpReg::new(ah, al), kf, mode);
            let rl = (u64::from(wrd[1]) << 32) | u64::from(wrd[2]);
            let bl = kf as i64 as u64;
            out.push_str(&format!(
                "{ah:x} {al:x} 0 {bl:x} {m:x} {op:x} 0 {rl:x} {w0:x}\n",
                w0 = wrd[0]
            ));
            continue;
        }
        if op == 24 {
            let (w0, w1, w2) = rand_bcd(&mut r);
            let m = r.next() % 4;
            let mode = mode_of(m);
            let al = (u64::from(w1) << 32) | u64::from(w2);
            sf::clear_exception_flags();
            let z = sf::pack_decimal_to_floatx80([w0, w1, w2], mode);
            let (rh, rl) = (z.high, z.low);
            out.push_str(&format!("{w0:x} {al:x} 0 0 {m:x} {op:x} {rh:x} {rl:x} 0\n"));
            continue;
        }
        let (ah, al) = rand_fx80(&mut r);
        let (bh, bl) = rand_fx80(&mut r);
        let m = r.next() % 4;
        let mode = mode_of(m);
        let a = FpReg::new(ah, al);
        let b = FpReg::new(bh, bl);
        sf::clear_exception_flags();
        // FREM/FMOD emit the FPSR quotient byte (sign<<7 | low 7 bits) in the
        // flags column instead of the exception flags, so the harness validates
        // the quotient too.
        let mut q_byte: Option<u64> = None;
        let (rh, rl): (u16, u64) = match op {
            0 => {
                let z = sf::floatx80_add(80, mode, a, b);
                (z.high, z.low)
            }
            1 => {
                let z = sf::floatx80_sub(80, mode, a, b);
                (z.high, z.low)
            }
            2 => {
                let z = sf::floatx80_mul(80, mode, a, b);
                (z.high, z.low)
            }
            3 => {
                let z = sf::floatx80_div(80, mode, a, b);
                (z.high, z.low)
            }
            4 => {
                let z = sf::floatx80_sqrt(80, mode, a);
                (z.high, z.low)
            }
            5 => {
                let i = sf::floatx80_to_int32(mode, a);
                (0, i as u32 as u64)
            }
            6 => {
                let f = sf::floatx80_to_float32(mode, a);
                (0, f as u64)
            }
            7 => {
                let f = sf::floatx80_to_float64(mode, a);
                (0, f)
            }
            8 => {
                let z = sf::int32_to_floatx80(al as u32 as i32);
                (z.high, z.low)
            }
            9 => {
                let z = sf::float32_to_floatx80(al as u32);
                (z.high, z.low)
            }
            10 => {
                let z = sf::float64_to_floatx80(al);
                (z.high, z.low)
            }
            // 11/12: FSGLMUL / FSGLDIV — dedicated single-precision paths.
            11 => {
                let z = sf::floatx80_sglmul(mode, a, b);
                (z.high, z.low)
            }
            12 => {
                let z = sf::floatx80_sgldiv(mode, a, b);
                (z.high, z.low)
            }
            // 20/21/22: FGETEXP / FGETMAN / FSCALE — WinUAE softfloat oracle.
            20 => {
                let z = sf::floatx80_getexp(a);
                (z.high, z.low)
            }
            21 => {
                let z = sf::floatx80_getman(a);
                (z.high, z.low)
            }
            22 => {
                let z = sf::floatx80_scale(80, mode, a, b);
                (z.high, z.low)
            }
            // 15-19: precision-rounded ops (FPCR/prefix single & double).
            15 => {
                let z = sf::floatx80_add(32, mode, a, b);
                (z.high, z.low)
            }
            16 => {
                let z = sf::floatx80_add(64, mode, a, b);
                (z.high, z.low)
            }
            17 => {
                let z = sf::floatx80_move(32, mode, a);
                (z.high, z.low)
            }
            18 => {
                let z = sf::floatx80_move(64, mode, a);
                (z.high, z.low)
            }
            19 => {
                let z = sf::floatx80_abs(32, mode, a);
                (z.high, z.low)
            }
            // 13/14: FREM / FMOD — value + quotient byte in the flags column.
            13 => {
                let r = sf::floatx80_rem(80, mode, a, b);
                q_byte = Some((r.quotient & 0x7F) | (u64::from(r.sign) << 7));
                (r.value.high, r.value.low)
            }
            14 => {
                let r = sf::floatx80_mod(80, mode, a, b);
                q_byte = Some((r.quotient & 0x7F) | (u64::from(r.sign) << 7));
                (r.value.high, r.value.low)
            }
            // 30-47: the FPSP transcendentals (unary; operand in `a`), each
            // computed at extended precision with the vector's rounding mode.
            30 => {
                let z = fpsp::floatx80_etox(80, mode, a);
                (z.high, z.low)
            }
            31 => {
                let z = fpsp::floatx80_etoxm1(80, mode, a);
                (z.high, z.low)
            }
            32 => {
                let z = fpsp::floatx80_twotox(80, mode, a);
                (z.high, z.low)
            }
            33 => {
                let z = fpsp::floatx80_tentox(80, mode, a);
                (z.high, z.low)
            }
            34 => {
                let z = fpsp::floatx80_logn(80, mode, a);
                (z.high, z.low)
            }
            35 => {
                let z = fpsp::floatx80_lognp1(80, mode, a);
                (z.high, z.low)
            }
            36 => {
                let z = fpsp::floatx80_log10(80, mode, a);
                (z.high, z.low)
            }
            37 => {
                let z = fpsp::floatx80_log2(80, mode, a);
                (z.high, z.low)
            }
            38 => {
                let z = fpsp::floatx80_sin(80, mode, a);
                (z.high, z.low)
            }
            39 => {
                let z = fpsp::floatx80_cos(80, mode, a);
                (z.high, z.low)
            }
            40 => {
                let z = fpsp::floatx80_tan(80, mode, a);
                (z.high, z.low)
            }
            // 41/42: FSINCOS — sine output (41) and cosine output (42).
            41 => {
                let (s, _c) = fpsp::floatx80_sincos(80, mode, a);
                (s.high, s.low)
            }
            42 => {
                let (_s, c) = fpsp::floatx80_sincos(80, mode, a);
                (c.high, c.low)
            }
            43 => {
                let z = fpsp::floatx80_atan(80, mode, a);
                (z.high, z.low)
            }
            44 => {
                let z = fpsp::floatx80_asin(80, mode, a);
                (z.high, z.low)
            }
            45 => {
                let z = fpsp::floatx80_acos(80, mode, a);
                (z.high, z.low)
            }
            46 => {
                let z = fpsp::floatx80_atanh(80, mode, a);
                (z.high, z.low)
            }
            47 => {
                let z = fpsp::floatx80_sinh(80, mode, a);
                (z.high, z.low)
            }
            48 => {
                let z = fpsp::floatx80_cosh(80, mode, a);
                (z.high, z.low)
            }
            49 => {
                let z = fpsp::floatx80_tanh(80, mode, a);
                (z.high, z.low)
            }
            _ => (0, 0),
        };
        let flags = q_byte.unwrap_or_else(|| u64::from(sf::take_exception_flags()));
        out.push_str(&format!(
            "{ah:x} {al:x} {bh:x} {bl:x} {m:x} {op:x} {rh:x} {rl:x} {flags:x}\n"
        ));
    }
    print!("{out}");
}
