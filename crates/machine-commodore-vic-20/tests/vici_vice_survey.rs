//! The VIC-I raster survey against VICE xvic (#362).
//!
//! Each case boots the machine from power-on, injects a program at the first
//! execution of `$E5EA` exactly as the capture script makes VICE do, runs to
//! an exact cycle count and compares the whole framebuffer with VICE's
//! full-raster screenshot of the same cycle, pixel for pixel, by VIC-I colour
//! number. `knowledge/processes/vic20-vici-vice-survey.md` explains the
//! method, the fixtures and how to re-run it; the cases and the SHA-256 of
//! every input are pinned in
//! `test-data/commodore/vic-20/vici-vice-survey/cases-v1.json`.
//!
//! The survey asserts the exact number of matching pixels for every case,
//! not a threshold, so any change — better or worse — fails until the table
//! below is updated with it and explained.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use machine_commodore_vic_20::{Vic20, Vic20Model, Vic20RamExpansion};
use mos_vic_i::{VIC_PALETTE, window_first_line, window_first_pixel};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Matching pixels per case, out of the framebuffer's 230 x 288 = 66,240 on
/// PAL and 214 x 240 = 51,360 on NTSC. See the process note for what each
/// shortfall is.
const EXPECTED_MATCHES: &[(&str, usize)] = &[
    ("basic-boot", 66_240),
    ("basic-boot-cursor", 66_240),
    ("vic6561-test36864", 66_211),
    ("vic6561-test36865-1", 66_166),
    ("vic6561-test36865-2", 66_159),
    ("vic6561-test36865-3", 66_158),
    ("vic6561-test36866-1", 66_240),
    ("vic6561-test36866-2", 63_255),
    ("vic6561-test36867-1", 66_240),
    ("vic6561-test36867-2", 66_240),
    ("vic6561-testback", 64_483),
    ("vic6561-testcharheigh-1", 66_228),
    ("vic6561-testcharheigh-2", 63_477),
    ("vic6561-testmemfetch-1", 66_088),
    ("vic6561-testmemfetch-2", 65_856),
    ("vic-9000test", 66_090),
    ("vic-vert0", 66_240),
    ("split-timing", 28_506),
    ("raster-border", 63_109),
    ("raster-background", 63_837),
    ("raster-reverse", 64_193),
    ("raster-auxiliary", 66_006),
    ("ntsc-basic-boot", 51_360),
    ("ntsc-vic-vert0", 51_360),
    ("ntsc-vic-line0", 51_328),
    ("ntsc-raster-border", 48_445),
    ("ntsc-raster-background", 48_957),
    ("ntsc-raster-reverse", 49_371),
    ("ntsc-raster-auxiliary", 51_048),
];

#[derive(Deserialize)]
struct Manifest {
    models: BTreeMap<String, Standard>,
    firmware: Firmware,
    injection: Injection,
    palette_calibration: BTreeMap<String, Calibration>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Standard {
    kernal: String,
    cycles_per_frame: u64,
    screenshot: Screenshot,
}

#[derive(Deserialize, Clone, Copy)]
struct Screenshot {
    width: u32,
    height: u32,
    pixel_width: u32,
    first_line: u32,
}

#[derive(Deserialize)]
struct Firmware {
    files: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Injection {
    pc: String,
    keys: String,
}

#[derive(Deserialize)]
struct Calibration {
    background_sample: [u32; 2],
    border_sample: [u32; 2],
    sha256: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    model: String,
    memory: String,
    program: Option<Program>,
    capture_frame: u64,
    reference_sha256: String,
    /// Keys queued in place of the manifest's default `RUN` + RETURN.
    keys: Option<String>,
}

#[derive(Deserialize)]
struct Program {
    root: String,
    path: String,
    sha256: String,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest() -> Manifest {
    let path = repo_root().join("test-data/commodore/vic-20/vici-vice-survey/cases-v1.json");
    let text = std::fs::read_to_string(&path).expect("the survey manifest is tracked");
    serde_json::from_str(&text).expect("the survey manifest parses")
}

/// The staged fixtures: VICE's test programs and the reference captures.
fn fixture_dir() -> Option<PathBuf> {
    let path = std::env::var("EMU198X_VIC20_VICE_SURVEY_DIR")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME").map(|home| PathBuf::from(home).join(".emu198x/test-suites/vic20"))
        })
        .ok()?;
    path.join("references").is_dir().then_some(path)
}

fn rom_dir() -> Option<PathBuf> {
    let path = std::env::var("EMU198X_VIC20_ROM_DIR")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".emu198x/roms/commodore-vic-20"))
        })
        .ok()?;
    path.is_dir().then_some(path)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// Read a pinned input, refusing it if its bytes are not the pinned ones.
fn read_pinned(path: &Path, sha256: &str) -> Vec<u8> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        sha256_hex(&bytes),
        sha256,
        "{} is not the file the manifest pins",
        path.display()
    );
    bytes
}

/// An RGB image, as VICE wrote it.
struct Image {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

impl Image {
    fn decode(bytes: &[u8]) -> Self {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().expect("PNG header");
        let mut buf = vec![0; reader.output_buffer_size().expect("PNG buffer size")];
        let info = reader.next_frame(&mut buf).expect("PNG frame");
        buf.truncate(info.buffer_size());
        let rgb = match (info.color_type, info.bit_depth) {
            (png::ColorType::Rgb, png::BitDepth::Eight) => buf,
            (png::ColorType::Rgba, png::BitDepth::Eight) => buf
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect(),
            other => panic!("unexpected PNG format {other:?}"),
        };
        Self {
            width: info.width,
            height: info.height,
            rgb,
        }
    }

    fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.width + x) * 3) as usize;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }
}

/// VICE's rendering of each of the sixteen VIC-I colours, read off its own
/// calibration frames.
struct VicePalette([[u8; 3]; 16]);

impl VicePalette {
    /// `frames[c]` is VICE's frame of background `c` and border `c & 7`.
    fn calibrate(
        frames: &[Image],
        shot: Screenshot,
        calibration: &Calibration,
    ) -> Result<Self, String> {
        let at = |image: &Image, [x, y]: [u32; 2]| {
            image.pixel(x * shot.pixel_width, y - shot.first_line)
        };
        let mut colours = [[0; 3]; 16];
        for (index, frame) in frames.iter().enumerate() {
            colours[index] = at(frame, calibration.background_sample);
        }
        for (index, frame) in frames.iter().enumerate() {
            let border = at(frame, calibration.border_sample);
            if border != colours[index & 7] {
                return Err(format!(
                    "palette frame {index}: border {border:?} is not background colour {}'s {:?}",
                    index & 7,
                    colours[index & 7]
                ));
            }
        }
        for a in 0..16 {
            for b in a + 1..16 {
                if colours[a] == colours[b] {
                    return Err(format!("VICE renders colours {a} and {b} alike"));
                }
            }
        }
        Ok(Self(colours))
    }

    fn classify(&self, rgb: [u8; 3]) -> Option<u8> {
        self.0.iter().position(|c| *c == rgb).map(|i| i as u8)
    }
}

fn emu198x_colour(argb: u32) -> Option<u8> {
    VIC_PALETTE.iter().position(|c| *c == argb).map(|i| i as u8)
}

/// The outcome of comparing one frame.
#[derive(Debug, PartialEq, Eq)]
struct Comparison {
    matched: usize,
    total: usize,
    /// Raster lines with at least one disagreement, and how many.
    mismatched_lines: BTreeMap<u32, usize>,
}

/// Compare a framebuffer whose top-left pixel is raster position `origin`
/// with VICE's full-raster screenshot, by colour number.
///
/// Refuses a screenshot of the wrong size, one whose pixels are not doubled
/// the way VICE draws them, and any colour outside either palette: a frame
/// that cannot be classified is an error, never a quiet partial match.
fn compare(
    framebuffer: &[u32],
    fb_width: u32,
    origin: (u32, u32),
    reference: &Image,
    shot: Screenshot,
    palette: &VicePalette,
) -> Result<Comparison, String> {
    if (reference.width, reference.height) != (shot.width, shot.height) {
        return Err(format!(
            "reference is {}x{}, expected {}x{}",
            reference.width, reference.height, shot.width, shot.height
        ));
    }
    let fb_height = framebuffer.len() as u32 / fb_width;
    let mut comparison = Comparison {
        matched: 0,
        total: framebuffer.len(),
        mismatched_lines: BTreeMap::new(),
    };
    for y in 0..fb_height {
        let line = origin.1 + y;
        let ry = line
            .checked_sub(shot.first_line)
            .filter(|ry| *ry < shot.height)
            .ok_or_else(|| format!("raster line {line} is outside the screenshot"))?;
        for x in 0..fb_width {
            let rx = (origin.0 + x) * shot.pixel_width;
            if rx + shot.pixel_width > shot.width {
                return Err(format!(
                    "raster pixel {} is outside the screenshot",
                    origin.0 + x
                ));
            }
            let theirs_rgb = reference.pixel(rx, ry);
            for dx in 1..shot.pixel_width {
                if reference.pixel(rx + dx, ry) != theirs_rgb {
                    return Err(format!("screenshot pixel ({rx}, {ry}) is not doubled"));
                }
            }
            let theirs = palette.classify(theirs_rgb).ok_or_else(|| {
                format!("VICE colour {theirs_rgb:?} at ({rx}, {ry}) is not in its palette")
            })?;
            let argb = framebuffer[(y * fb_width + x) as usize];
            let ours = emu198x_colour(argb).ok_or_else(|| {
                format!("framebuffer colour {argb:08x} at ({x}, {y}) is not in the palette")
            })?;
            if ours == theirs {
                comparison.matched += 1;
            } else {
                *comparison.mismatched_lines.entry(line).or_default() += 1;
            }
        }
    }
    Ok(comparison)
}

struct Roms {
    kernal: BTreeMap<String, Vec<u8>>,
    basic: Vec<u8>,
    chars: Vec<u8>,
}

fn load_roms(dir: &Path, firmware: &Firmware) -> Roms {
    let read = |name: &str| read_pinned(&dir.join(name), &firmware.files[name]);
    Roms {
        kernal: firmware
            .files
            .keys()
            .filter(|name| name.starts_with("kernal"))
            .map(|name| (name.clone(), read(name)))
            .collect(),
        basic: read("basic.rom"),
        chars: read("char.rom"),
    }
}

fn expansion(memory: &str) -> Vic20RamExpansion {
    match memory {
        "none" => Vic20RamExpansion::NONE,
        "3k" => Vic20RamExpansion::EXP_3K,
        "8k" => Vic20RamExpansion::EXP_8K,
        other => panic!("memory configuration {other} is not mapped"),
    }
}

/// Power on, inject `program` (if any) at the injection point, and run to
/// the end of frame `capture_frame - 1` — cycle `capture_frame` x frame
/// length from power-on, where VICE's `-limitcycles` stops. `before_capture`
/// sees the machine at the start of that last frame.
fn run_case(
    manifest: &Manifest,
    roms: &Roms,
    case: &Case,
    program: Option<&[u8]>,
    before_capture: impl FnOnce(&mut Vic20),
) -> Vic20 {
    let standard = &manifest.models[&case.model];
    let vic_model = match case.model.as_str() {
        "pal" => Vic20Model::Pal,
        "ntsc" => Vic20Model::Ntsc,
        other => panic!("model {other} is not mapped"),
    };
    let mut machine = Vic20::new(
        roms.kernal[&standard.kernal].clone(),
        roms.basic.clone(),
        roms.chars.clone(),
        vic_model,
        expansion(&case.memory),
    );
    let target = case.capture_frame * standard.cycles_per_frame;

    if let Some(program) = program {
        let pc = u16::from_str_radix(&manifest.injection.pc, 16).expect("injection pc");
        while machine.cpu().regs.pc != pc {
            machine.step_instruction();
            assert!(machine.master_clock() < target, "never reached ${pc:04X}");
        }
        let load = u16::from_le_bytes([program[0], program[1]]);
        for (offset, &byte) in program[2..].iter().enumerate() {
            machine.poke(load.wrapping_add(offset as u16), byte);
        }
        let [lo, hi] = load.wrapping_add((program.len() - 2) as u16).to_le_bytes();
        for pointer in [0x2D_u16, 0x2F, 0x31] {
            machine.poke(pointer, lo);
            machine.poke(pointer + 1, hi);
        }
        let keys = case
            .keys
            .as_deref()
            .unwrap_or(&manifest.injection.keys)
            .as_bytes();
        assert!(keys.len() <= 10, "the keyboard buffer holds ten keys");
        for (offset, &key) in keys.iter().enumerate() {
            machine.poke(0x0277 + offset as u16, key);
        }
        machine.poke(0x00C6, keys.len() as u8);
    }

    while machine.master_clock() + standard.cycles_per_frame < target {
        machine.run_frame();
    }
    before_capture(&mut machine);
    machine.run_frame();
    assert_eq!(
        machine.master_clock(),
        target,
        "frames are not aligned to the cycle count"
    );
    machine
}

struct Survey {
    manifest: Manifest,
    fixtures: PathBuf,
    roms: Roms,
    palettes: BTreeMap<String, VicePalette>,
}

impl Survey {
    fn load() -> Option<Self> {
        let manifest = manifest();
        let fixtures = fixture_dir()?;
        let roms = load_roms(&rom_dir()?, &manifest.firmware);
        let palettes = manifest
            .palette_calibration
            .iter()
            .map(|(model, calibration)| {
                let frames: Vec<Image> = (0..16)
                    .map(|colour| {
                        let key = format!("{colour:02}");
                        let path = fixtures.join(format!("references/{model}/palette-{key}.png"));
                        Image::decode(&read_pinned(&path, &calibration.sha256[&key]))
                    })
                    .collect();
                let shot = manifest.models[model].screenshot;
                let palette = VicePalette::calibrate(&frames, shot, calibration)
                    .unwrap_or_else(|e| panic!("{model} palette calibration: {e}"));
                (model.clone(), palette)
            })
            .collect();
        Some(Self {
            manifest,
            fixtures,
            roms,
            palettes,
        })
    }

    fn program(&self, case: &Case) -> Option<Vec<u8>> {
        case.program.as_ref().map(|program| {
            let root = match program.root.as_str() {
                "fixture" => self.fixtures.clone(),
                "repo" => repo_root(),
                other => panic!("program root {other} is not mapped"),
            };
            read_pinned(&root.join(&program.path), &program.sha256)
        })
    }

    fn reference(&self, case: &Case) -> Image {
        let path = self
            .fixtures
            .join(format!("references/{}/{}.png", case.model, case.id));
        Image::decode(&read_pinned(&path, &case.reference_sha256))
    }

    fn machine(&self, case: &Case, before_capture: impl FnOnce(&mut Vic20)) -> Vic20 {
        run_case(
            &self.manifest,
            &self.roms,
            case,
            self.program(case).as_deref(),
            before_capture,
        )
    }

    fn origin(case: &Case) -> (u32, u32) {
        let pal = case.model == "pal";
        (window_first_pixel(pal), window_first_line(pal))
    }

    fn compare(&self, case: &Case, machine: &Vic20) -> Comparison {
        compare(
            machine.framebuffer(),
            machine.framebuffer_width(),
            Self::origin(case),
            &self.reference(case),
            self.manifest.models[&case.model].screenshot,
            &self.palettes[&case.model],
        )
        .unwrap_or_else(|e| panic!("{}: {e}", case.id))
    }

    fn case(&self, id: &str) -> &Case {
        self.manifest
            .cases
            .iter()
            .find(|case| case.id == id)
            .unwrap_or_else(|| panic!("no case {id}"))
    }
}

/// The disagreeing raster lines as compact ranges, for the survey table.
fn line_ranges(lines: &BTreeMap<u32, usize>) -> String {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for &line in lines.keys() {
        match ranges.last_mut() {
            Some((_, end)) if *end + 1 == line => *end = line,
            _ => ranges.push((line, line)),
        }
    }
    ranges
        .iter()
        .map(|&(a, b)| {
            if a == b {
                a.to_string()
            } else {
                format!("{a}-{b}")
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
#[ignore = "FIXTURE: needs the VIC-20 VICE survey (EMU198X_VIC20_VICE_SURVEY_DIR) and VIC-20 ROMs (EMU198X_VIC20_ROM_DIR)"]
fn vic_i_matches_the_vice_survey() {
    let Some(survey) = Survey::load() else {
        emu198x_test_skip::skip!(
            "VIC-20 VICE survey or ROMs not staged (EMU198X_VIC20_VICE_SURVEY_DIR, EMU198X_VIC20_ROM_DIR)"
        );
    };

    let results: Vec<(String, Comparison)> = std::thread::scope(|scope| {
        let handles: Vec<_> = survey
            .manifest
            .cases
            .iter()
            .map(|case| {
                let survey = &survey;
                scope.spawn(move || {
                    let machine = survey.machine(case, |_| {});
                    (case.id.clone(), survey.compare(case, &machine))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("survey case panicked"))
            .collect()
    });

    eprintln!(
        "{:<26} {:>7} {:>7} {:>9}  disagreeing raster lines",
        "case", "match", "of", "%"
    );
    for (id, comparison) in &results {
        eprintln!(
            "{id:<26} {:>7} {:>7} {:>8.3}%  {}",
            comparison.matched,
            comparison.total,
            comparison.matched as f64 * 100.0 / comparison.total as f64,
            line_ranges(&comparison.mismatched_lines)
        );
    }

    let measured: Vec<(&str, usize)> = results
        .iter()
        .map(|(id, comparison)| (id.as_str(), comparison.matched))
        .collect();
    assert_eq!(
        measured, EXPECTED_MATCHES,
        "the survey moved; update EXPECTED_MATCHES and the process note together"
    );
}

/// The comparator has to be able to fail. The boot screen matches VICE
/// exactly; the same frame with the border colour changed from 3 to 2 at the
/// start of the capture frame must lose exactly the border — every pixel
/// outside the 176 x 184 display — and nothing else.
#[test]
#[ignore = "FIXTURE: needs the VIC-20 VICE survey (EMU198X_VIC20_VICE_SURVEY_DIR) and VIC-20 ROMs (EMU198X_VIC20_ROM_DIR)"]
fn the_comparator_rejects_a_deliberately_wrong_frame() {
    let Some(survey) = Survey::load() else {
        emu198x_test_skip::skip!(
            "VIC-20 VICE survey or ROMs not staged (EMU198X_VIC20_VICE_SURVEY_DIR, EMU198X_VIC20_ROM_DIR)"
        );
    };
    let case = survey.case("basic-boot");

    let right = survey.compare(case, &survey.machine(case, |_| {}));
    assert_eq!(right.matched, right.total, "the boot screen is the control");

    let wrong = survey.compare(
        case,
        &survey.machine(case, |machine| machine.poke(0x900F, 0x1A)),
    );
    let display = 176 * 184;
    assert_eq!(wrong.total, right.total);
    assert_eq!(
        wrong.matched, display,
        "only the display may still match once the border is wrong"
    );
}

/// Write Emu198x's frame, VICE's (in Emu198x's palette) and a difference
/// image, disagreements in magenta, for one case.
#[test]
#[ignore = "DIAGNOSTIC: set VIC20_SURVEY_CASE=<id> and VIC20_SURVEY_OUT=<dir>"]
fn dump_a_survey_case() {
    let (Ok(id), Ok(out)) = (
        std::env::var("VIC20_SURVEY_CASE"),
        std::env::var("VIC20_SURVEY_OUT"),
    ) else {
        eprintln!("set VIC20_SURVEY_CASE=<case id> and VIC20_SURVEY_OUT=<directory>");
        return;
    };
    let Some(survey) = Survey::load() else {
        emu198x_test_skip::skip!("VIC-20 VICE survey or ROMs not staged");
    };
    let case = survey.case(&id);
    let machine = survey.machine(case, |_| {});
    let (width, height) = (machine.framebuffer_width(), machine.framebuffer_height());
    let origin = Survey::origin(case);
    let shot = survey.manifest.models[&case.model].screenshot;
    let reference = survey.reference(case);
    let palette = &survey.palettes[&case.model];
    let rgb = |argb: u32| [(argb >> 16) as u8, (argb >> 8) as u8, argb as u8];

    let (mut ours, mut theirs, mut diff) = (Vec::new(), Vec::new(), Vec::new());
    for y in 0..height {
        for x in 0..width {
            let argb = machine.framebuffer()[(y * width + x) as usize];
            let vice = reference.pixel(
                (origin.0 + x) * shot.pixel_width,
                origin.1 + y - shot.first_line,
            );
            let vice_index = palette.classify(vice).expect("VICE colour");
            ours.extend_from_slice(&rgb(argb));
            theirs.extend_from_slice(&rgb(VIC_PALETTE[vice_index as usize]));
            if emu198x_colour(argb) == Some(vice_index) {
                diff.extend_from_slice(&rgb(argb));
            } else {
                diff.extend_from_slice(&[0xFF, 0x00, 0xFF]);
            }
        }
    }
    for (name, image) in [("emu198x", ours), ("vice", theirs), ("diff", diff)] {
        let path = PathBuf::from(&out).join(format!("{id}-{name}.png"));
        let file = std::fs::File::create(&path).expect("output path");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .expect("PNG header")
            .write_image_data(&image)
            .expect("PNG data");
        eprintln!("wrote {}", path.display());
    }
}

// ---------------------------------------------------------------------------
// The comparator on synthetic frames: no fixtures, so these run everywhere.
// ---------------------------------------------------------------------------

const SHOT: Screenshot = Screenshot {
    width: 8,
    height: 3,
    pixel_width: 2,
    first_line: 1,
};

fn synthetic_palette() -> VicePalette {
    VicePalette(core::array::from_fn(|i| [i as u8 * 10, 0, 1]))
}

/// A VICE-shaped 8 x 3 screenshot of raster lines 1-3, pixels doubled, raster
/// pixel `(x, line)` in colour `colour(x, line)`.
fn synthetic_shot(colour: impl Fn(u32, u32) -> u8) -> Image {
    let palette = synthetic_palette();
    let mut rgb = Vec::new();
    for ry in 0..SHOT.height {
        for rx in 0..SHOT.width {
            rgb.extend_from_slice(&palette.0[colour(rx / 2, ry + 1) as usize]);
        }
    }
    Image {
        width: SHOT.width,
        height: SHOT.height,
        rgb,
    }
}

#[test]
fn the_comparator_counts_matches_by_colour_number() {
    // A 2 x 2 framebuffer at raster (1, 2): colours 5, 6 / 7, 8.
    let fb = [
        VIC_PALETTE[5],
        VIC_PALETTE[6],
        VIC_PALETTE[7],
        VIC_PALETTE[8],
    ];
    let shot = synthetic_shot(|x, line| match (x, line) {
        (1, 2) => 5,
        (2, 2) => 6,
        (1, 3) => 7,
        (2, 3) => 9, // the one disagreement
        _ => 0,
    });
    let comparison = compare(&fb, 2, (1, 2), &shot, SHOT, &synthetic_palette()).expect("compares");
    assert_eq!(
        comparison,
        Comparison {
            matched: 3,
            total: 4,
            mismatched_lines: BTreeMap::from([(3, 1)]),
        }
    );
}

#[test]
fn the_comparator_refuses_what_it_cannot_classify() {
    let fb = [VIC_PALETTE[0]; 4];
    let good = synthetic_shot(|_, _| 0);
    assert!(compare(&fb, 2, (1, 2), &good, SHOT, &synthetic_palette()).is_ok());

    // The right-hand half of raster pixel 1 on line 2 differs from its left.
    let mut undoubled = synthetic_shot(|_, _| 0);
    undoubled.rgb[(SHOT.width as usize + 3) * 3] = 10;
    assert!(
        compare(&fb, 2, (1, 2), &undoubled, SHOT, &synthetic_palette()).is_err(),
        "an undoubled screenshot pixel"
    );

    let mut stray = synthetic_shot(|_, _| 0);
    stray.rgb[(SHOT.width as usize + 2) * 3..][..6].copy_from_slice(&[1, 2, 3, 1, 2, 3]);
    assert!(
        compare(&fb, 2, (1, 2), &stray, SHOT, &synthetic_palette()).is_err(),
        "a screenshot colour outside VICE's palette"
    );

    assert!(
        compare(
            &[0x1234_5678; 4],
            2,
            (1, 2),
            &good,
            SHOT,
            &synthetic_palette()
        )
        .is_err(),
        "a framebuffer colour outside the palette"
    );
    assert!(
        compare(&fb, 2, (3, 2), &good, SHOT, &synthetic_palette()).is_err(),
        "a window wider than the screenshot"
    );
    assert!(
        compare(&fb, 2, (1, 3), &good, SHOT, &synthetic_palette()).is_err(),
        "a window taller than the screenshot"
    );
}
