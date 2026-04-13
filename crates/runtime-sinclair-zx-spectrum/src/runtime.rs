//! Runtime wrapper for the fresh-workspace ZX Spectrum 48K.
//!
//! This layer is deliberately thin. It does not own timing logic or media
//! emulation itself; it owns the translation between the shared shell boundary
//! and the concrete 48K machine crate.

use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_48K};
use common_sinclair_zx_spectrum::{MemoryBus, RomImageError, SPECTRUM_PALETTE, TapeBlock};
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, MediaTransportAction,
    QueryError, QueryResult, ResetKind, RunResult, SessionQueryProvider, StopReason, SupportTier,
    known_capability,
};
use machine_sinclair_zx_spectrum_48k::{BoardIssue, Spectrum48k, Spectrum48kSnapshot};
use serde_json::json;

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

/// Runtime wrapper over the concrete 48K Spectrum machine.
pub struct Spectrum48kRuntime {
    profile: MachineProfile,
    machine: Spectrum48k,
    time: MachineTime,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SnapshotEnvelopeV1 {
    version: u32,
    profile_id: String,
    time: MachineTime,
    machine: Spectrum48kSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SpectrumBootStatus {
    detected: bool,
    reason: String,
    row: Option<usize>,
}

/// Spectrum-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpectrumSessionQueryProvider;

impl Spectrum48kRuntime {
    /// Creates an Issue 3 runtime from one validated 16 KiB ROM image.
    #[must_use]
    pub fn new(rom: [u8; 16 * 1024]) -> Self {
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

        Self {
            profile,
            machine: Spectrum48k::with_rom(BoardIssue::Issue3, rom),
            time: MachineTime::default(),
        }
    }

    /// Creates an Issue 3 runtime from a borrowed 16 KiB ROM byte slice.
    ///
    /// # Errors
    ///
    /// Returns an error if the slice is not exactly 16 KiB.
    pub fn from_rom_bytes(rom: &[u8]) -> Result<Self, RomImageError> {
        let rom: [u8; 16 * 1024] = rom
            .try_into()
            .map_err(|_| RomImageError::WrongSize { actual: rom.len() })?;
        Ok(Self::new(rom))
    }

    /// Creates an Issue 3 runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing, if unknown firmware is
    /// supplied, or if the ROM image fails validation.
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

    /// Creates a runtime backed by a zero-filled ROM image.
    #[must_use]
    pub fn blank() -> Self {
        Self::new([0; 16 * 1024])
    }

    /// Returns the wrapped 48K machine.
    #[must_use]
    pub fn machine(&self) -> &Spectrum48k {
        &self.machine
    }

    /// Returns mutable access to the wrapped 48K machine.
    #[must_use]
    pub fn machine_mut(&mut self) -> &mut Spectrum48k {
        &mut self.machine
    }

    /// Returns the current runtime time in authoritative half-cycles.
    #[must_use]
    pub const fn time(&self) -> MachineTime {
        self.time
    }

    fn load_tape_bytes(&mut self, slot: &str, bytes: &[u8]) -> Result<(), MachineError> {
        if is_tzx(bytes) {
            let pulses =
                format_sinclair_zx_spectrum_tzx::tzx_to_pulses(bytes).map_err(|reason| {
                    MachineError::InvalidMedia {
                        slot: slot.to_owned(),
                        reason,
                    }
                })?;
            self.machine.load_tape_pulses(pulses);
        } else {
            let blocks = format_sinclair_zx_spectrum_tap::parse_tap(bytes).map_err(|reason| {
                MachineError::InvalidMedia {
                    slot: slot.to_owned(),
                    reason,
                }
            })?;
            self.machine
                .load_tape_blocks(tap_blocks_to_tape_blocks(blocks));
        }

        Ok(())
    }
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
        machine: &Spectrum48kRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "boot.detected" => {
                json!(spectrum_boot_status(&spectrum_screen_text_lines(machine.machine())).detected)
            }
            "boot.reason" => {
                json!(spectrum_boot_status(&spectrum_screen_text_lines(machine.machine())).reason)
            }
            "boot.row" => {
                json!(spectrum_boot_status(&spectrum_screen_text_lines(machine.machine())).row)
            }
            "screen.text.cols" => json!(SCREEN_TEXT_COLS),
            "screen.text.lines" => json!(spectrum_screen_text_lines(machine.machine())),
            "screen.text.rows" => json!(SCREEN_TEXT_ROWS),
            "spectrum.keyboard.rows" => json!(machine.machine().keyboard().rows()),
            "spectrum.machine.half_cycle_in_frame" => json!(machine.machine().hc()),
            "spectrum.machine.tstate_in_frame" => json!(machine.machine().tstate_in_frame()),
            "spectrum.machine.issue" => json!(board_issue_name(machine.machine().issue())),
            "spectrum.tape.loaded" => json!(machine.machine().tape_is_loaded()),
            "spectrum.tape.playing" => json!(machine.machine().tape_is_playing()),
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

impl MachineCore for Spectrum48kRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.machine.reset();
        self.time = MachineTime::default();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            if image.slot.as_ref() != "tape-1" {
                return Err(MachineError::UnknownMediaSlot {
                    slot: image.slot.as_ref().to_owned(),
                });
            }

            if image.kind != MediaKind::Tape {
                return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
            }

            self.load_tape_bytes(image.slot.as_ref(), image.bytes)?;
        }

        Ok(())
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        for event in host.input_events {
            self.machine.apply_input_event(event);
        }

        while self.time < target {
            self.machine.run_frame();
            self.time = self
                .time
                .saturating_add(u64::from(TIMING_48K.halfcycles_per_frame));

            host.frame_sink.push_frame(FramePacket {
                timestamp: self.time,
                format: emu198x_shell::PixelFormat::Indexed8,
                width: SCREEN_WIDTH as u32,
                height: SCREEN_HEIGHT as u32,
                palette: Some(&SPECTRUM_PALETTE),
                pixels: self.machine.framebuffer(),
            })?;

            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: self.machine.audio_sample_rate(),
                channels: 1,
                samples: self.machine.audio_frame(),
            })?;
        }

        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        postcard::to_allocvec(&SnapshotEnvelopeV1 {
            version: 1,
            profile_id: self.profile.profile_id.as_str().to_owned(),
            time: self.time,
            machine: self.machine.snapshot_state(),
        })
        .map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("encode failed: {reason}"),
        })
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        let snapshot: SnapshotEnvelopeV1 =
            postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
                reason: format!("decode failed: {reason}"),
            })?;

        if snapshot.version != 1 {
            return Err(MachineError::InvalidSnapshot {
                reason: format!("unsupported snapshot version {}", snapshot.version),
            });
        }

        if snapshot.profile_id != self.profile.profile_id.as_str() {
            return Err(MachineError::InvalidSnapshot {
                reason: format!(
                    "snapshot profile {} does not match runtime profile {}",
                    snapshot.profile_id,
                    self.profile.profile_id.as_str()
                ),
            });
        }

        self.machine
            .restore_snapshot_state(snapshot.machine)
            .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
        self.time = snapshot.time;
        Ok(())
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        match command {
            ControlCommand::MediaTransport(command) => {
                if command.slot.as_ref() != "tape-1" {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: command.slot.as_ref().to_owned(),
                    });
                }

                match command.action {
                    MediaTransportAction::Start => self.machine.play_tape(),
                    MediaTransportAction::Stop => self.machine.stop_tape(),
                    _ => {
                        return Err(MachineError::UnsupportedOperation {
                            operation: "media-transport",
                        });
                    }
                }

                Ok(())
            }
            _ => Err(MachineError::UnsupportedOperation {
                operation: command.operation_name(),
            }),
        }
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
}

fn is_tzx(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && &bytes[..7] == b"ZXTape!" && bytes[7] == 0x1A
}

fn tap_blocks_to_tape_blocks(
    blocks: Vec<format_sinclair_zx_spectrum_tap::TapBlock>,
) -> Vec<TapeBlock> {
    blocks
        .into_iter()
        .map(|block| {
            let mut full = Vec::with_capacity(block.data.len() + 2);
            full.push(block.flag);
            full.extend_from_slice(&block.data);
            let checksum = full.iter().fold(0u8, |acc, &byte| acc ^ byte);
            full.push(checksum);

            TapeBlock {
                flag: block.flag,
                data: full,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
