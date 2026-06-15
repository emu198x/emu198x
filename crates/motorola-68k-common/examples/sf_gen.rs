// Generates random floatx80 test vectors, runs our SoftFloat port, and emits
// `a_high a_low b_high b_low mode op rust_high rust_low rust_flags` (hex) for
// `validation/winuae_check.cpp` to diff against WinUAE's silicon-validated
// SOFTFLOAT_68K. Deterministic LCG so runs are reproducible; op set on argv[1].
use motorola_68k_common::registers::FpReg;
use motorola_68k_common::softfloat::{self as sf, RoundingMode};

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let op: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let n: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(200_000);
    let mut r = Lcg(0x1234_5678_9ABC_DEF1 ^ ((op as u64) << 40));
    let mut out = String::with_capacity(1 << 20);
    for _ in 0..n {
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
            _ => (0, 0),
        };
        let flags = q_byte.unwrap_or_else(|| u64::from(sf::take_exception_flags()));
        out.push_str(&format!(
            "{ah:x} {al:x} {bh:x} {bl:x} {m:x} {op:x} {rh:x} {rl:x} {flags:x}\n"
        ));
    }
    print!("{out}");
}
