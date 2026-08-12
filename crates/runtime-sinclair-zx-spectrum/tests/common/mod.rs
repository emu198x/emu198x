//! Machine-agnostic half of the ZXSpectrum4.net timing surveys.
//!
//! The 48K and 128K suites are the same program built for two machines,
//! by the same authors, with the same prompt and the same transcript
//! format. What differs is how you boot one and read its screen; what does
//! not differ is how you scrape `Test N {Mode}` blocks out of a scrolling
//! display, merge partial samples, spell a test number at the prompt, or
//! write the report.
//!
//! That half lives here so the two harnesses cannot drift on the subtle
//! parts. The scraper in particular encodes lessons that were expensive to
//! learn — see `scrape_cases` and `absorb`.
//!
//! Each `tests/*.rs` file is its own crate, so this module is compiled
//! into every harness that declares it and any given harness uses only
//! some of it.
#![allow(dead_code)]

use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use std::collections::BTreeMap;
use std::path::Path;

pub fn sha256_hex(bytes: &[u8]) -> String {
    // Small local SHA-256 so the survey can pin its fixture without the
    // crate taking a dependency for one hash.
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let mut msg = bytes.to_vec();
    let bit_len = (bytes.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d) = (h[0], h[1], h[2], h[3]);
        let (mut e, mut f, mut g, mut hh) = (h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(v);
        }
    }
    h.iter().map(|w| format!("{w:08x}")).collect()
}

pub fn revision() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |s| s.trim().to_owned())
}

pub fn set_key(rows: &mut [u8; 8], key: SpectrumKey, pressed: bool) {
    let (row, bit) = key.row_bit();
    let mask = 1u8 << bit;
    if pressed {
        rows[row] &= !mask;
    } else {
        rows[row] |= mask;
    }
}

/// One `{Contended}` or `{Uncontended}` case within a numbered test.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CaseResult {
    pub test: usize,
    pub mode: String,
    pub description: String,
    pub verdict: String,
    pub measured: BTreeMap<String, i64>,
    pub expected: BTreeMap<String, i64>,
}

/// Pull `R=`, `loop=` and `sp=` style readings out of one decoded line.
pub fn parse_readings(line: &str) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    for token in line.split_whitespace() {
        if let Some((name, value)) = token.split_once('=')
            && let Ok(parsed) = value
                .trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse()
            && !name.is_empty()
        {
            out.insert(name.to_ascii_lowercase(), parsed);
        }
    }
    out
}

/// Scrape every case visible on the current screen.
///
/// The suite prints a running transcript, so a screen can hold several
/// cases at once. Each begins `Test N {Mode}`, carries an opcode
/// description, then a readings line ending `Pass` or `Fail`; a failing
/// case follows with `Expecting:` and a second readings line.
pub fn scrape_cases(lines: &[String]) -> Vec<CaseResult> {
    let mut cases = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        let Some(rest) = line.strip_prefix("Test ") else {
            i += 1;
            continue;
        };
        let Some((num, mode)) = rest.split_once('{') else {
            i += 1;
            continue;
        };
        let Ok(test) = num.trim().parse::<usize>() else {
            i += 1;
            continue;
        };
        let mode = mode.trim_end_matches('}').trim().to_owned();

        // Description and readings follow within the next few lines.
        let mut description = String::new();
        let mut verdict = String::new();
        let mut measured = BTreeMap::new();
        let mut expected = BTreeMap::new();
        let mut j = i + 1;
        while j < lines.len() && j < i + 8 {
            let l = lines[j].trim();
            if l.starts_with("Test ") {
                break;
            }
            if l.contains("Pass") || l.contains("Fail") {
                verdict = if l.contains("Fail") { "fail" } else { "pass" }.to_owned();
                measured = parse_readings(l);
            } else if l.starts_with("Expecting") {
                if let Some(next) = lines.get(j + 1) {
                    expected = parse_readings(next.trim());
                }
            } else if !l.is_empty() && !l.starts_with('-') && description.is_empty() {
                description = l.to_owned();
            }
            j += 1;
        }

        if !verdict.is_empty() {
            cases.push(CaseResult {
                test,
                mode,
                description,
                verdict,
                measured,
                expected,
            });
        }
        i = j;
    }
    cases
}

/// Merge freshly scraped cases into the accumulated set.
///
/// A case can be sampled while it is still printing, so a later sample
/// of the same `(test, mode)` may carry readings the earlier one lacked
/// — most often the `Expecting:` block, which prints after the verdict.
/// Keep whichever version knows more.
/// Keys spelling a test number at the suite's "choose test 1-35" prompt.
pub fn digit_keys(mut n: usize) -> Vec<SpectrumKey> {
    let digits = [
        SpectrumKey::Num0,
        SpectrumKey::Num1,
        SpectrumKey::Num2,
        SpectrumKey::Num3,
        SpectrumKey::Num4,
        SpectrumKey::Num5,
        SpectrumKey::Num6,
        SpectrumKey::Num7,
        SpectrumKey::Num8,
        SpectrumKey::Num9,
    ];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    if n == 0 {
        stack.push(0);
    }
    while n > 0 {
        stack.push(n % 10);
        n /= 10;
    }
    while let Some(d) = stack.pop() {
        out.push(digits[d]);
    }
    out
}

pub fn absorb(into: &mut Vec<CaseResult>, fresh: Vec<CaseResult>) {
    for case in fresh {
        if case.verdict.is_empty() {
            continue;
        }
        match into
            .iter_mut()
            .find(|c| c.test == case.test && c.mode == case.mode)
        {
            Some(existing) => {
                if case.expected.len() > existing.expected.len()
                    || case.measured.len() > existing.measured.len()
                {
                    *existing = case;
                }
            }
            None => into.push(case),
        }
    }
}

pub fn write_report(path: &Path, body: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("report directory");
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(body).expect("serialize report"),
    )
    .expect("write report");
}
