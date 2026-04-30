//! 48K-specific runtime extras: firmware constructors, the rich
//! `SpectrumSessionQueryProvider` (boot detection + ROM-glyph screen
//! text extraction), and a couple of typed accessors that the wider
//! Spectrum tooling depends on.
//!
//! The runtime itself is the generic `SpectrumRuntime<Spectrum48k>` —
//! everything below builds on that, not around it.

use common_sinclair_zx_spectrum::MemoryBus;
use common_sinclair_zx_spectrum::audio::{AudioControls, SpeakerChannel};
use common_sinclair_zx_spectrum::error::RomImageError;
use emu198x_shell::{
    CapabilitySet, FirmwareSet, MachineError, MachineProfile, QueryError, QueryResult,
    SessionQueryProvider, SupportTier, known_capability,
};
use machine_sinclair_zx_spectrum_48k::{BoardIssue, Spectrum48k};
use serde_json::json;

use crate::runtime::SpectrumRuntime;
use crate::variants::Spectrum48kRuntime;
use crate::{Model, profile_for};

const SPECTRUM_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "screen.text.cols",
    "screen.text.lines",
    "screen.text.rows",
    "spectrum.keyboard.rows",
    "spectrum.machine.half_cycle_in_frame",
    "spectrum.machine.tstate_in_frame",
    "spectrum.machine.issue",
    "spectrum.tape.loaded",
    "spectrum.tape.playing",
];

const SCREEN_TEXT_COLS: usize = 32;
const SCREEN_TEXT_ROWS: usize = 24;
const ROM_TEXT_GLYPH_BASE: u16 = 0x3d00;
const ROM_TEXT_GLYPH_FIRST: u8 = 0x20;
const ROM_TEXT_GLYPH_COUNT: usize = 96;
const ROM_TEXT_GLYPH_COPYRIGHT: u8 = 0x7f;
const BOOT_BANNER: &str = "(C) 1982 Sinclair Research Ltd";
const BOOT_BANNER_FALLBACK: &str = "1982 Sinclair Research Ltd";
const BOOT_BANNER_UNICODE: &str = "© 1982 Sinclair Research Ltd";

#[derive(Clone, Debug, PartialEq, Eq)]
struct SpectrumBootStatus {
    detected: bool,
    reason: String,
    row: Option<usize>,
}

/// Spectrum 48K query provider — owns boot detection + screen text
/// extraction by reading the ROM glyph table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpectrumSessionQueryProvider;

impl SpectrumRuntime<Spectrum48k> {
    /// Builds a 48K runtime around the given Issue 3 ROM image.
    #[must_use]
    pub fn new_48k(rom: [u8; 16 * 1024]) -> Self {
        let mut runtime = SpectrumRuntime::new(
            Model::Spectrum48KPal,
            Spectrum48k::with_rom(BoardIssue::Issue3, rom),
        );
        *runtime.profile_mut() = boots_profile_with_export();
        runtime
    }

    /// Builds an Issue 3 runtime from a borrowed 16 KiB ROM byte slice.
    ///
    /// # Errors
    ///
    /// Returns an error if the slice is not exactly 16 KiB.
    pub fn from_rom_bytes(rom: &[u8]) -> Result<Self, RomImageError> {
        let rom: [u8; 16 * 1024] = rom
            .try_into()
            .map_err(|_| RomImageError::WrongSize { actual: rom.len() })?;
        Ok(Self::new_48k(rom))
    }

    /// Builds an Issue 3 runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing, if unknown
    /// firmware is supplied, or if the ROM image fails validation.
    pub fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(Model::Spectrum48KPal);
        let rom_id = "sinclair-zx-spectrum-48k-rom";
        firmware.validate_for_profile(&profile)?;
        let rom = firmware
            .bytes(rom_id)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: rom_id.to_owned(),
            })?;

        Self::from_rom_bytes(rom).map_err(|reason| MachineError::InvalidFirmware {
            id: rom_id.to_owned(),
            reason: reason.to_string(),
        })
    }

    /// Builds a runtime backed by a zero-filled ROM image.
    #[must_use]
    pub fn blank() -> Self {
        Self::new_48k([0; 16 * 1024])
    }

    /// Current host-side speaker audio controls.
    #[must_use]
    pub fn audio_controls(&self) -> AudioControls {
        self.machine().audio_controls()
    }

    /// Replace all host-side speaker audio controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.machine_mut().set_audio_controls(controls);
    }

    /// Enable or disable the speaker in host output.
    pub fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        self.machine_mut()
            .set_audio_channel_enabled(channel, enabled);
    }

    /// Set speaker host-side gain.
    pub fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        self.machine_mut().set_audio_channel_gain(channel, gain);
    }
}

/// 48K-only profile that bumps `support_tier` to `Boots` and advertises
/// the snapshot-export capability the bespoke runtime used to declare.
fn boots_profile_with_export() -> MachineProfile {
    let mut profile = profile_for(Model::Spectrum48KPal);
    profile.support_tier = SupportTier::Boots;
    profile.capabilities = CapabilitySet::with_all([
        known_capability("beeper-audio"),
        known_capability("keyboard-matrix"),
        known_capability("snapshot-export"),
        known_capability("snapshot-import"),
        known_capability("tape-input"),
        known_capability("tape-transport-control"),
        known_capability("scripted-input"),
    ]);
    profile
}

fn spectrum_screen_text_lines(machine: &Spectrum48k) -> Vec<String> {
    let glyphs = spectrum_rom_glyphs(machine);
    let mut lines = Vec::with_capacity(SCREEN_TEXT_ROWS);

    for row in 0..SCREEN_TEXT_ROWS {
        let mut line = String::with_capacity(SCREEN_TEXT_COLS);
        for col in 0..SCREEN_TEXT_COLS {
            let cell = spectrum_screen_cell(machine, row, col);
            line.push(decode_screen_char(&glyphs, cell));
        }
        lines.push(line);
    }

    lines
}

fn spectrum_boot_status(lines: &[String]) -> SpectrumBootStatus {
    if let Some((row, _)) = lines.iter().enumerate().find(|(_, line)| {
        line.contains(BOOT_BANNER)
            || line.contains(BOOT_BANNER_UNICODE)
            || line.contains(BOOT_BANNER_FALLBACK)
    }) {
        return SpectrumBootStatus {
            detected: true,
            reason: format!("found copyright banner on row {row}"),
            row: Some(row),
        };
    }

    SpectrumBootStatus {
        detected: false,
        reason: "copyright banner not visible".to_owned(),
        row: None,
    }
}

fn spectrum_rom_glyphs(machine: &Spectrum48k) -> Vec<[u8; 8]> {
    let mut glyphs = Vec::with_capacity(ROM_TEXT_GLYPH_COUNT);

    for glyph_index in 0..ROM_TEXT_GLYPH_COUNT {
        let glyph_base = ROM_TEXT_GLYPH_BASE + (glyph_index as u16 * 8);
        let mut glyph = [0u8; 8];
        for (row, byte) in glyph.iter_mut().enumerate() {
            *byte = machine.read(glyph_base + row as u16);
        }
        glyphs.push(glyph);
    }

    glyphs
}

fn spectrum_screen_cell(machine: &Spectrum48k, text_row: usize, text_col: usize) -> [u8; 8] {
    let mut cell = [0u8; 8];

    for (pixel_row, byte) in cell.iter_mut().enumerate() {
        let y = text_row * 8 + pixel_row;
        let addr = 0x4000
            + (((y & 0b1100_0000) as u16) << 5)
            + (((y & 0b0011_1000) as u16) << 2)
            + (((y & 0b0000_0111) as u16) << 8)
            + text_col as u16;
        *byte = machine.read(addr);
    }

    cell
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

impl SessionQueryProvider<Spectrum48kRuntime> for SpectrumSessionQueryProvider {
    fn query_paths(&self, _machine: &Spectrum48kRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = SPECTRUM_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(
        &self,
        runtime: &Spectrum48kRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let machine = runtime.machine();
        let value = match path {
            "boot.detected" => {
                json!(spectrum_boot_status(&spectrum_screen_text_lines(machine)).detected)
            }
            "boot.reason" => {
                json!(spectrum_boot_status(&spectrum_screen_text_lines(machine)).reason)
            }
            "boot.row" => {
                json!(spectrum_boot_status(&spectrum_screen_text_lines(machine)).row)
            }
            "screen.text.cols" => json!(SCREEN_TEXT_COLS),
            "screen.text.lines" => json!(spectrum_screen_text_lines(machine)),
            "screen.text.rows" => json!(SCREEN_TEXT_ROWS),
            "spectrum.keyboard.rows" => json!(machine.keyboard().rows()),
            "spectrum.machine.half_cycle_in_frame" => json!(machine.hc()),
            "spectrum.machine.tstate_in_frame" => json!(machine.tstate_in_frame()),
            "spectrum.machine.issue" => json!(board_issue_name(machine.issue())),
            "spectrum.tape.loaded" => json!(machine.tape_is_loaded()),
            "spectrum.tape.playing" => json!(machine.tape_is_playing()),
            _ => return Ok(None),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn board_issue_name(issue: BoardIssue) -> &'static str {
    match issue {
        BoardIssue::Issue2 => "issue2",
        BoardIssue::Issue3 => "issue3",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
    use emu198x_shell::FirmwareImage;
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

        let lines = spectrum_screen_text_lines(&machine);

        assert_eq!(lines.len(), SCREEN_TEXT_ROWS);
        assert_eq!(lines[2].len(), SCREEN_TEXT_COLS);
        assert_eq!(lines[2].chars().nth(3), Some('A'));
        // Silence dead-code warnings for the screen geometry constants.
        let _ = (SCREEN_WIDTH, SCREEN_HEIGHT);
    }

    #[test]
    fn boot_status_detects_copyright_banner() {
        let mut lines = vec![" ".repeat(SCREEN_TEXT_COLS); SCREEN_TEXT_ROWS];
        lines[20] = format!("{BOOT_BANNER:<32}");

        let status = spectrum_boot_status(&lines);

        assert!(status.detected);
        assert_eq!(status.row, Some(20));
        assert_eq!(status.reason, "found copyright banner on row 20");
    }

    #[test]
    fn boot_status_reports_absence_when_banner_is_missing() {
        let lines = vec![" ".repeat(SCREEN_TEXT_COLS); SCREEN_TEXT_ROWS];

        let status = spectrum_boot_status(&lines);

        assert!(!status.detected);
        assert_eq!(status.row, None);
        assert_eq!(status.reason, "copyright banner not visible");
    }

    #[test]
    fn from_firmware_rejects_missing_rom() {
        // Empty firmware set — `validate_for_profile` flags the missing
        // 48K ROM, which `from_firmware` then surfaces verbatim.
        let firmware = FirmwareSet::new();
        match SpectrumRuntime::<Spectrum48k>::from_firmware(&firmware) {
            Err(MachineError::MissingFirmware { .. }) => {}
            Err(other) => panic!("expected MissingFirmware, got {other:?}"),
            Ok(_) => panic!("empty firmware set should fail to boot the 48K runtime"),
        }
    }

    #[test]
    fn from_firmware_reports_invalid_rom_when_size_mismatches() {
        // Wrong-size ROM passes `validate_for_profile` (only the id is
        // checked) but fails the `from_rom_bytes` round-trip — that's the
        // InvalidFirmware arm.
        let mut firmware = FirmwareSet::new();
        let too_small = [0u8; 1024];
        firmware.push(FirmwareImage::new(
            "sinclair-zx-spectrum-48k-rom",
            &too_small,
        ));
        match SpectrumRuntime::<Spectrum48k>::from_firmware(&firmware) {
            Err(MachineError::InvalidFirmware { .. }) => {}
            Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
            Ok(_) => panic!("wrong-size 48K ROM must be rejected"),
        }
    }

    #[test]
    fn from_rom_bytes_rejects_wrong_size_slice() {
        match SpectrumRuntime::<Spectrum48k>::from_rom_bytes(&[0u8; 1024]) {
            Err(RomImageError::WrongSize { actual }) => assert_eq!(actual, 1024),
            Ok(_) => panic!("wrong-size slice must be rejected at construction time"),
        }
    }

    #[test]
    fn audio_controls_round_trip_through_runtime() {
        let mut runtime = SpectrumRuntime::<Spectrum48k>::blank();
        let mut controls = runtime.audio_controls();
        controls.set_channel_gain(SpeakerChannel::Speaker, 0.125);
        runtime.set_audio_controls(controls);
        assert!(
            (runtime
                .audio_controls()
                .channel(SpeakerChannel::Speaker)
                .gain()
                - 0.125)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn decode_screen_char_returns_copyright_for_glyph_7f() {
        // Build a glyph table where the glyph at index 0x7f - 0x20 (the
        // last cell) is unique, then decode that exact cell. The arm
        // around `ROM_TEXT_GLYPH_COPYRIGHT` returns '©'.
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
        // Empty glyph table → no match; trailing '?' fall-through.
        let glyphs: Vec<[u8; 8]> = Vec::new();
        let unique = [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55];
        assert_eq!(decode_screen_char(&glyphs, unique), '?');
    }

    #[test]
    fn board_issue_name_round_trips_for_both_revisions() {
        // Issue3 is exercised by the standing 48K runtime; Issue2 was
        // never read by a test before Cov-5b.
        assert_eq!(board_issue_name(BoardIssue::Issue2), "issue2");
        assert_eq!(board_issue_name(BoardIssue::Issue3), "issue3");
    }

    #[test]
    fn query_provider_resolves_boot_reason_and_row_paths() {
        // The `boot.detected` arm is exercised by tests/runtime_48k.rs;
        // the `boot.reason` and `boot.row` arms had no test before.
        let runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])
            .expect("dummy ROM should construct");
        let provider = SpectrumSessionQueryProvider;

        let reason = provider
            .query(&runtime, "boot.reason")
            .expect("boot.reason should resolve")
            .expect("boot.reason should be owned by the provider");
        let row = provider
            .query(&runtime, "boot.row")
            .expect("boot.row should resolve")
            .expect("boot.row should be owned by the provider");

        assert_eq!(reason.value, json!("copyright banner not visible"));
        assert_eq!(row.value, serde_json::Value::Null);
    }

    #[test]
    fn query_provider_returns_none_for_unrecognised_path() {
        // The unknown-path arm in `query` returns `Ok(None)` so the
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
    fn boots_profile_with_export_promotes_support_tier_and_capabilities() {
        use emu198x_shell::MachineCore;
        let runtime = Spectrum48kRuntime::blank();
        let caps = runtime.profile().capabilities.clone();
        // The bespoke 48K profile bumps the support tier and advertises
        // snapshot-export beyond the base profile_for(...) bundle.
        assert_eq!(runtime.profile().support_tier, SupportTier::Boots);
        assert!(caps.contains(&known_capability("snapshot-export")));
    }
}
