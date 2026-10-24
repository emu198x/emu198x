//! The model against ares's accurate PPU.
//!
//! Each case in `tests/oracle/cases/` scripts memory contents and register
//! accesses at given dots; `tests/oracle/golden/` holds what ares printed
//! for it (see `tests/oracle/ppu_harness.cpp` and `regenerate.sh`). This
//! runs the same script through the model and compares every line: each
//! scripted read, a hash of the picture at the end of each frame, and a hash
//! of each non-blank row of the last frame.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use emu198x_nintendo_s_ppu::{FRAME_HEIGHT, FRAME_WIDTH, Region, SPpu};

const BASIS: u64 = 0xcbf2_9ce4_8422_2325;

fn fnv(mut hash: u64, value: u32) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The case PRNG: xorshift64*, top 16 bits.
struct Prng(u64);

impl Prng {
    fn next(&mut self) -> u16 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 48) as u16
    }
}

enum Event {
    Write(u8, u8),
    Read(u8, u8),
    Latch(bool),
}

fn hex(field: Option<&str>) -> u64 {
    u64::from_str_radix(field.expect("a field"), 16).expect("hex")
}

fn dec(field: Option<&str>) -> u32 {
    field.expect("a field").parse().expect("decimal")
}

fn row_hash(ppu: &SPpu, row: usize) -> u64 {
    let raw = ppu.framebuffer_raw();
    raw[row * FRAME_WIDTH..(row + 1) * FRAME_WIDTH]
        .iter()
        .fold(BASIS, |h, &v| fnv(h, v))
}

fn run(script: &str) -> String {
    let mut region = Region::Ntsc;
    let mut frames_wanted = 0;
    let mut pokes = Vec::new();
    let mut events = Vec::new();
    for line in script.lines() {
        let mut fields = line.split_whitespace();
        let Some(kind) = fields.next() else { continue };
        match kind {
            "region" => {
                region = if fields.next() == Some("pal") {
                    Region::Pal
                } else {
                    Region::Ntsc
                }
            }
            "frames" => frames_wanted = dec(fields.next()),
            "w" | "r" | "latch" => {
                let at = (dec(fields.next()), dec(fields.next()), dec(fields.next()));
                let event = match kind {
                    "latch" => Event::Latch(dec(fields.next()) != 0),
                    "w" => Event::Write(hex(fields.next()) as u8, hex(fields.next()) as u8),
                    _ => Event::Read(hex(fields.next()) as u8, hex(fields.next()) as u8),
                };
                events.push((at, event));
            }
            _ => pokes.push(line.to_owned()),
        }
    }

    let mut ppu = SPpu::new(region);
    for line in &pokes {
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("vram") => ppu.poke_vram(hex(fields.next()) as u16, hex(fields.next()) as u16),
            Some("cgram") => ppu.poke_cgram(hex(fields.next()) as u8, hex(fields.next()) as u16),
            Some("oam") => ppu.poke_oam(hex(fields.next()) as u16, hex(fields.next()) as u8),
            Some("vrand") => {
                let mut rng = Prng(hex(fields.next()) | 1);
                let address = hex(fields.next());
                for i in 0..hex(fields.next()) {
                    ppu.poke_vram((address + i) as u16, rng.next());
                }
            }
            Some("crand") => {
                let mut rng = Prng(hex(fields.next()) | 1);
                let index = hex(fields.next());
                for i in 0..hex(fields.next()) {
                    ppu.poke_cgram((index + i) as u8, rng.next());
                }
            }
            _ => {}
        }
    }

    let mut out = String::new();
    let mut next = 0;
    let mut frames = 0u32;
    let apply = |ppu: &mut SPpu, frames: u32, out: &mut String, next: &mut usize| {
        while let Some(((f, v, h), event)) = events.get(*next) {
            if *f != frames || *v != ppu.vcounter() || *h != ppu.hcounter() {
                return;
            }
            *next += 1;
            match *event {
                Event::Write(reg, value) => ppu.write(reg, value),
                Event::Read(reg, mdr) => {
                    let value = ppu.read(reg, mdr);
                    writeln!(out, "r {f} {v} {h} {reg:02x} -> {value:02x}").expect("write");
                }
                Event::Latch(level) => ppu.set_ext_latch(level),
            }
        }
    };
    apply(&mut ppu, frames, &mut out, &mut next);
    while frames < frames_wanted {
        ppu.tick();
        if ppu.take_frame_ready() {
            let hash = (0..FRAME_HEIGHT).fold(BASIS, |h, row| {
                ppu.framebuffer_raw()[row * FRAME_WIDTH..(row + 1) * FRAME_WIDTH]
                    .iter()
                    .fold(h, |h, &v| fnv(h, v))
            });
            writeln!(out, "frame {frames} {hash:016x}").expect("write");
            frames += 1;
            if frames == frames_wanted {
                let blank = (0..FRAME_WIDTH).fold(BASIS, |h, _| fnv(h, 0));
                for row in 0..FRAME_HEIGHT {
                    let hash = row_hash(&ppu, row);
                    if hash != blank {
                        writeln!(out, "row {row} {hash:016x}").expect("write");
                    }
                }
                break;
            }
        }
        apply(&mut ppu, frames, &mut out, &mut next);
    }
    if next < events.len() {
        writeln!(out, "unconsumed {}", events.len() - next).expect("write");
    }
    out
}

#[test]
fn every_case_matches_ares() {
    let oracle = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle");
    let mut names: Vec<_> = fs::read_dir(oracle.join("cases"))
        .expect("cases")
        .map(|entry| entry.expect("entry").path())
        .collect();
    names.sort();
    assert!(names.len() >= 60, "the cases are present");
    let mut failures = Vec::new();
    for path in &names {
        let name = path
            .file_stem()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        let script = fs::read_to_string(path).expect("case");
        let golden =
            fs::read_to_string(oracle.join("golden").join(format!("{name}.txt"))).expect("golden");
        let ours = run(&script);
        // ORACLE_DUMP=dir writes the model's output beside ares's, for
        // diffing a failure.
        if let Some(dir) = std::env::var_os("ORACLE_DUMP") {
            fs::write(Path::new(&dir).join(format!("{name}.txt")), &ours).expect("dump");
        }
        if ours != golden {
            let first = ours
                .lines()
                .zip(golden.lines())
                .find(|(a, b)| a != b)
                .map_or_else(
                    || {
                        format!(
                            "{} lines, expected {}",
                            ours.lines().count(),
                            golden.lines().count()
                        )
                    },
                    |(a, b)| format!("got `{a}`, ares `{b}`"),
                );
            failures.push(format!("{name}: {first}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        names.len(),
        failures.join("\n")
    );
}
