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
}
