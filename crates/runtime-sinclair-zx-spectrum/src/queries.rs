//! Generic `SessionQueryProvider` for every Spectrum-family variant.
//!
//! This module owns the path catalogue that every Spectrum runtime
//! exposes through `SpectrumSessionQueryProvider`. Anything that's the
//! same shape across the family (screen geometry, ROM-glyph text
//! decode, keyboard rows, tape state, per-frame timing) is resolved
//! here against the [`SpectrumMachine`] trait. Variant-specific paths
//! (boot-banner detection, AY register snapshots, SCLD high-res mode,
//! etc.) come from [`SpectrumMachine::variant_query_paths`] and
//! [`SpectrumMachine::resolve_variant_query`].
//!
//! The screen-text scanner here is the same routine the 48K provider
//! used before generalisation — it walks the ULA screen at `$4000`
//! and decodes each cell against the ROM glyph table at `$3D00`.
//! That layout is a Spectrum-family convention, not a 48K-only
//! quirk; every variant ships with the same character ROM in its
//! lowest 16 KiB ROM bank.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use serde_json::{Value, json};

use crate::runtime::{SpectrumMachine, SpectrumRuntime};

/// Query paths that every `SpectrumRuntime<M>` exposes regardless of
/// variant. Variant-specific paths (boot detection, AY state, SCLD
/// high-res, etc.) are appended at runtime via
/// [`SpectrumMachine::variant_query_paths`].
pub(crate) const SHARED_QUERY_PATHS: &[&str] = &[
    // The CPU. The data was already reachable — `SpectrumMachine` exposes
    // `z80_registers`, `z80_halted` and `instructions_retired` for the
    // `query_cpu` MCP tool — but none of it was in the query catalogue, so
    // a script could not ask "where is the guest?".
    //
    // That gap is why diagnosing #872 needed a bespoke Rust probe: the
    // question "is the Z80 halted or spinning, and where?" is three lines
    // of script and was instead a test binary. `runtime-atari-800xl` and
    // `runtime-jupiter-ace` both expose `cpu.pc`; this family did not.
    "cpu",
    "cpu.af",
    "cpu.bc",
    "cpu.de",
    "cpu.halted",
    "cpu.hl",
    "cpu.i",
    "cpu.iff1",
    "cpu.iff2",
    "cpu.im",
    "cpu.instruction_complete",
    "cpu.instructions_retired",
    "cpu.ix",
    "cpu.iy",
    "cpu.pc",
    "cpu.r",
    "cpu.sp",
    "screen.text.cols",
    "screen.text.lines",
    "basic.prog",
    "basic.vars",
    "basic.e_line",
    "basic.worksp",
    "basic.e_ppc",
    "basic.newppc",
    "basic.flags",
    "basic.mode",
    "screen.text.rows",
    "keyboard.rows",
    "machine.half_cycle_in_frame",
    "machine.tstate_in_frame",
    "tape.loaded",
    "tape.playing",
    // Position, so "did the tape drain" is observable rather than inferred.
    // Names come from `common_tape::POSITION_QUERY_PATHS` so every machine
    // with a deck answers the same set.
    "tape.span_index",
    "tape.span_count",
    "tape.span_countdown",
    "tape.progress",
];

/// Every CPU leaf in one object, so a single query answers "where is the
/// guest and what is it doing?" — the question a hang needs.
fn cpu_object<M: SpectrumMachine>(machine: &M) -> Value {
    let r = machine.z80_registers();
    json!({
        "af": r.af,
        "bc": r.bc,
        "de": r.de,
        "halted": machine.z80_halted(),
        "hl": r.hl,
        "i": r.i,
        "iff1": r.iff1,
        "iff2": r.iff2,
        "im": r.im,
        "instruction_complete": machine.z80_instruction_complete(),
        "instructions_retired": machine.z80_instructions_retired(),
        "ix": r.ix,
        "iy": r.iy,
        "pc": r.pc,
        "r": r.r,
        "sp": r.sp,
    })
}

pub(crate) const SCREEN_TEXT_COLS: usize = 32;
pub(crate) const SCREEN_TEXT_ROWS: usize = 24;
#[cfg(test)]
pub(crate) const ROM_TEXT_GLYPH_BASE: u16 = 0x3d00;
pub(crate) const ROM_TEXT_GLYPH_FIRST: u8 = 0x20;
pub(crate) const ROM_TEXT_GLYPH_COUNT: usize = 96;
const ROM_TEXT_GLYPH_COPYRIGHT: u8 = 0x7f;

/// Boot-banner detection result. Each variant supplies its own
/// banner constants and surfaces this through `resolve_variant_query`
/// for `boot.detected` / `boot.reason` / `boot.row`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpectrumBootStatus {
    /// `true` when at least one banner was found in the decoded
    /// screen text.
    pub detected: bool,
    /// Human-readable explanation of the detection result.
    pub reason: String,
    /// Row index (0-23) carrying the matched banner, when known.
    pub row: Option<usize>,
}

impl SpectrumBootStatus {
    /// Constructs the "no banner visible" status.
    #[must_use]
    pub fn not_detected() -> Self {
        Self {
            detected: false,
            reason: "copyright banner not visible".to_owned(),
            row: None,
        }
    }
}

/// Builds the 24-row screen-text rendering of `machine`'s display
/// memory by decoding each 8x8 character cell against the ROM glyph
/// table at $3D00.
#[must_use]
pub fn screen_text_lines<M: SpectrumMachine>(machine: &M) -> Vec<String> {
    let glyphs = rom_glyphs(machine);
    let mut lines = Vec::with_capacity(SCREEN_TEXT_ROWS);

    for row in 0..SCREEN_TEXT_ROWS {
        let mut line = String::with_capacity(SCREEN_TEXT_COLS);
        for col in 0..SCREEN_TEXT_COLS {
            let cell = screen_cell(machine, row, col);
            line.push(decode_screen_char(&glyphs, cell));
        }
        lines.push(line);
    }

    lines
}

/// Walks the rendered screen-text lines for any of the supplied
/// banner strings. Returns the first matching row, if any.
#[must_use]
pub fn boot_status_from_banners(lines: &[String], banners: &[&str]) -> SpectrumBootStatus {
    for (row, line) in lines.iter().enumerate() {
        if banners.iter().any(|banner| line.contains(banner)) {
            return SpectrumBootStatus {
                detected: true,
                reason: format!("found copyright banner on row {row}"),
                row: Some(row),
            };
        }
    }
    SpectrumBootStatus::not_detected()
}

fn rom_glyphs<M: SpectrumMachine>(machine: &M) -> Vec<[u8; 8]> {
    let mut glyphs = Vec::with_capacity(ROM_TEXT_GLYPH_COUNT);

    // Calls the variant's `glyph_byte` rather than a fixed
    // `read_byte($3D00 + ..)` so paged-ROM variants (128K family) can
    // reach the 48 BASIC sub-ROM regardless of which ROM is currently
    // mapped at $0000-$3FFF.
    for glyph_index in 0..ROM_TEXT_GLYPH_COUNT {
        let glyph_offset_base = (glyph_index as u16) * 8;
        let mut glyph = [0u8; 8];
        for (row, byte) in glyph.iter_mut().enumerate() {
            *byte = machine.glyph_byte(glyph_offset_base + row as u16);
        }
        glyphs.push(glyph);
    }

    glyphs
}

fn screen_cell<M: SpectrumMachine>(machine: &M, text_row: usize, text_col: usize) -> [u8; 8] {
    let mut cell = [0u8; 8];

    for (pixel_row, byte) in cell.iter_mut().enumerate() {
        let y = text_row * 8 + pixel_row;
        let addr = 0x4000
            + (((y & 0b1100_0000) as u16) << 5)
            + (((y & 0b0011_1000) as u16) << 2)
            + (((y & 0b0000_0111) as u16) << 8)
            + text_col as u16;
        *byte = machine.read_byte(addr);
    }

    cell
}

fn read_word_le<M: SpectrumMachine>(machine: &M, addr: u16) -> u16 {
    u16::from(machine.read_byte(addr)) | (u16::from(machine.read_byte(addr.wrapping_add(1))) << 8)
}

fn decode_screen_char(glyphs: &[[u8; 8]], cell: [u8; 8]) -> char {
    for (glyph_index, glyph) in glyphs.iter().enumerate() {
        if *glyph == cell {
            let code = ROM_TEXT_GLYPH_FIRST + glyph_index as u8;
            return match code {
                0x20..=0x7e => code as char,
                ROM_TEXT_GLYPH_COPYRIGHT => '©',
                _ => '?',
            };
        }
    }

    '?'
}

/// Spectrum-family query provider — generic over every variant of
/// `SpectrumMachine`. Owns the shared screen-text / keyboard / tape /
/// timing paths and delegates variant-specific paths (boot banner,
/// AY state, board issue, SCLD high-res flag, …) into the trait.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpectrumSessionQueryProvider;

impl<M: SpectrumMachine> SessionQueryProvider<SpectrumRuntime<M>> for SpectrumSessionQueryProvider {
    fn query_paths(&self, _runtime: &SpectrumRuntime<M>, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = SHARED_QUERY_PATHS
            .iter()
            .copied()
            .chain(M::variant_query_paths().iter().copied())
            .chain(M::ay_query_paths().iter().copied())
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths.dedup();
        paths
    }

    fn query(
        &self,
        runtime: &SpectrumRuntime<M>,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let machine = runtime.machine();
        let value = match path {
            "cpu" => cpu_object(machine),
            "cpu.af" => json!(machine.z80_registers().af),
            "cpu.bc" => json!(machine.z80_registers().bc),
            "cpu.de" => json!(machine.z80_registers().de),
            "cpu.halted" => json!(machine.z80_halted()),
            "cpu.hl" => json!(machine.z80_registers().hl),
            "cpu.i" => json!(machine.z80_registers().i),
            "cpu.iff1" => json!(machine.z80_registers().iff1),
            "cpu.iff2" => json!(machine.z80_registers().iff2),
            "cpu.im" => json!(machine.z80_registers().im),
            "cpu.instruction_complete" => json!(machine.z80_instruction_complete()),
            "cpu.instructions_retired" => json!(machine.z80_instructions_retired()),
            "cpu.ix" => json!(machine.z80_registers().ix),
            "cpu.iy" => json!(machine.z80_registers().iy),
            "cpu.pc" => json!(machine.z80_registers().pc),
            "cpu.r" => json!(machine.z80_registers().r),
            "cpu.sp" => json!(machine.z80_registers().sp),
            "screen.text.cols" => json!(SCREEN_TEXT_COLS),
            "screen.text.rows" => json!(SCREEN_TEXT_ROWS),
            "screen.text.lines" => json!(screen_text_lines(machine)),
            "keyboard.rows" => json!(machine.keyboard_rows()),
            "machine.half_cycle_in_frame" => json!(machine.half_cycle_in_frame()),
            "machine.tstate_in_frame" => json!(machine.tstate_in_frame()),
            "tape.loaded" => json!(machine.tape_is_loaded()),
            "tape.playing" => json!(machine.tape_is_playing()),
            "tape.span_index" => json!(machine.tape_player().span_index()),
            "tape.span_count" => json!(machine.tape_player().span_count()),
            "tape.span_countdown" => json!(machine.tape_player().span_countdown()),
            "tape.progress" => json!(machine.tape_player().progress()),
            "basic.prog" => json!(read_word_le(machine, 0x5C53)),
            "basic.vars" => json!(read_word_le(machine, 0x5C4B)),
            "basic.e_line" => json!(read_word_le(machine, 0x5C59)),
            "basic.worksp" => json!(read_word_le(machine, 0x5C61)),
            "basic.e_ppc" => json!(read_word_le(machine, 0x5C49)),
            "basic.newppc" => json!(read_word_le(machine, 0x5C42)),
            "basic.flags" => json!(machine.read_byte(0x5C3B)),
            "basic.mode" => json!(machine.read_byte(0x5C41)),
            _ => return machine.resolve_variant_query(path),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

/// Does `path` address chip `chip` — either the bare group name or a
/// dotted leaf beneath it (`ay`, `ay.mixer`)? Shared by the variant
/// resolvers that fold a chip snapshot into grouped + leaf query paths.
pub(crate) fn is_chip(path: &str, chip: &str) -> bool {
    path == chip
        || path
            .strip_prefix(chip)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Resolve a chip path against a built snapshot: the bare group name
/// returns the whole object, a `chip.field` leaf returns that field, and
/// an unknown sub-field returns `None` (an unknown path, not a null).
pub(crate) fn chip_field(path: &str, chip: &str, snapshot: Value) -> Option<QueryResult> {
    let value = if path == chip {
        snapshot
    } else {
        let field = path.strip_prefix(chip)?.strip_prefix('.')?;
        snapshot.get(field)?.clone()
    };
    Some(QueryResult {
        path: path.to_owned(),
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variants::Spectrum48kRuntime;
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use machine_sinclair_zx_spectrum_48k::Spectrum48k;

    fn write_screen_cell(
        machine: &mut Spectrum48k,
        text_row: usize,
        text_col: usize,
        glyph: [u8; 8],
    ) {
        for (pixel_row, byte) in glyph.into_iter().enumerate() {
            let y = text_row * 8 + pixel_row;
            let addr = 0x4000
                + (((y & 0b1100_0000) as u16) << 5)
                + (((y & 0b0011_1000) as u16) << 2)
                + (((y & 0b0000_0111) as u16) << 8)
                + text_col as u16;
            machine.write(addr, byte);
        }
    }

    #[test]
    fn screen_text_lines_decode_rom_glyph_cells() {
        let mut rom = [0u8; 16 * 1024];
        let glyph_a = [0x18, 0x24, 0x42, 0x7e, 0x42, 0x42, 0x42, 0x00];
        let glyph_base =
            (ROM_TEXT_GLYPH_BASE as usize) + (usize::from(b'A' - ROM_TEXT_GLYPH_FIRST) * 8);
        rom[glyph_base..glyph_base + 8].copy_from_slice(&glyph_a);

        let mut machine = Spectrum48k::new();
        machine
            .load_rom_bytes(&rom)
            .expect("synthetic ROM should load into the 48K machine");
        write_screen_cell(&mut machine, 2, 3, glyph_a);

        let lines = screen_text_lines(&machine);

        assert_eq!(lines.len(), SCREEN_TEXT_ROWS);
        assert_eq!(lines[2].len(), SCREEN_TEXT_COLS);
        assert_eq!(lines[2].chars().nth(3), Some('A'));
    }

    #[test]
    fn boot_status_reports_absence_when_banner_is_missing() {
        let lines = vec![" ".repeat(SCREEN_TEXT_COLS); SCREEN_TEXT_ROWS];

        let status = boot_status_from_banners(&lines, &["(C) 1982 Sinclair Research Ltd"]);

        assert!(!status.detected);
        assert_eq!(status.row, None);
        assert_eq!(status.reason, "copyright banner not visible");
    }

    #[test]
    fn provider_returns_none_for_unrecognised_path() {
        // The unknown-path arm must surface as Ok(None) so the
        // session layer can report `UnknownPath`.
        let runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])
            .expect("dummy ROM should construct");
        let provider = SpectrumSessionQueryProvider;

        let result = provider
            .query(&runtime, "no.such.path")
            .expect("unknown paths should resolve cleanly");
        assert!(result.is_none(), "unknown paths must surface as Ok(None)");
    }

    #[test]
    fn provider_resolves_variant_specific_issue_path_via_trait() {
        let runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])
            .expect("dummy ROM should construct");
        let provider = SpectrumSessionQueryProvider;

        // `spectrum.machine.issue` is a 48K-only path. The shared
        // dispatcher must hand it off to `resolve_variant_query`.
        let issue = provider
            .query(&runtime, "machine.issue")
            .expect("issue query should resolve")
            .expect("provider should own issue path");
        assert_eq!(issue.value, serde_json::json!("issue3"));
    }

    #[test]
    fn decode_screen_char_returns_copyright_for_glyph_7f() {
        let mut glyphs = vec![[0u8; 8]; ROM_TEXT_GLYPH_COUNT];
        let copyright_index = (ROM_TEXT_GLYPH_COPYRIGHT - ROM_TEXT_GLYPH_FIRST) as usize;
        glyphs[copyright_index] = [0x3c, 0x42, 0x99, 0xa1, 0xa1, 0x99, 0x42, 0x3c];
        assert_eq!(
            decode_screen_char(&glyphs, glyphs[copyright_index]),
            '©',
            "the 0x7f arm should map to the unicode copyright sign"
        );
    }

    #[test]
    fn decode_screen_char_returns_question_mark_for_unknown_cell() {
        let glyphs: Vec<[u8; 8]> = Vec::new();
        let unique = [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55];
        assert_eq!(decode_screen_char(&glyphs, unique), '?');
    }

    #[test]
    fn boot_status_detects_first_matching_banner() {
        let mut lines = vec![" ".repeat(SCREEN_TEXT_COLS); SCREEN_TEXT_ROWS];
        lines[12] = format!("{:<32}", "© 1986 Amstrad");

        let status = boot_status_from_banners(&lines, &["1986 Amstrad", "© 1986 Amstrad"]);

        assert!(status.detected);
        assert_eq!(status.row, Some(12));
        assert_eq!(status.reason, "found copyright banner on row 12");
    }

    #[test]
    fn boot_status_reports_absence_when_no_banner_matches() {
        let lines = vec![" ".repeat(SCREEN_TEXT_COLS); SCREEN_TEXT_ROWS];
        let status = boot_status_from_banners(&lines, &["any banner"]);
        assert!(!status.detected);
        assert_eq!(status.row, None);
        assert_eq!(status.reason, "copyright banner not visible");
    }
}
