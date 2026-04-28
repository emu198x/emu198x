//! Runtime wrapper for the Dragon 32.

use emu198x_shell::{
    CapabilitySet, FirmwareSet, FramePacket, HostIo, InputEvent, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, QueryError, QueryResult,
    ResetKind, RunResult, SessionQueryProvider, StopReason,
};
use format_dragon_cas::{CasFileType, CasImage, LEADER_BYTE, SYNC_BYTE, parse_cas_tolerant};
use machine_dragon_32::{Dragon32, DragonKey, MatrixKey, ROM_SIZE};
use motorola_vdg_6847::{TEXT_VISIBLE_FRAMEBUFFER_HEIGHT, TEXT_VISIBLE_FRAMEBUFFER_WIDTH};
use serde_json::json;

use crate::{Model, profile_for};

const DRAGON_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "screen.text.lines",
    "dragon.cpu.cycles",
    "dragon.cpu.instructions",
    "dragon.cpu.pc",
    "dragon.machine.halted",
    "dragon.pia0.control_a",
    "dragon.pia0.control_b",
    "dragon.pia0.ddr_a",
    "dragon.pia0.ddr_b",
    "dragon.pia1.ca2",
    "dragon.pia1.cb2",
    "dragon.pia1.control_a",
    "dragon.pia1.control_b",
    "dragon.pia1.ddr_b",
    "dragon.pia1.output_b",
    "dragon.sam.display_offset",
    "dragon.sam.video_mode",
    "dragon.tape.blocks",
    "dragon.tape.checksums_valid",
    "dragon.tape.finished",
    "dragon.tape.header.file_type",
    "dragon.tape.header.name",
    "dragon.tape.ignored_bytes",
    "dragon.tape.ignored_segments",
    "dragon.tape.length_bits",
    "dragon.tape.loaded",
    "dragon.tape.motor_on",
    "dragon.tape.position_bits",
    "dragon.text.base",
    "dragon.video.display_base",
];

const MIN_INITIAL_LEADER_BYTES: usize = 128;

/// Summary of the currently mounted Dragon cassette image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragonTapeSummary {
    /// Number of parsed CAS blocks.
    pub blocks: usize,
    /// `true` when every block checksum matches.
    pub checksums_valid: bool,
    /// Number of non-CAS byte ranges skipped by tolerant parsing.
    pub ignored_segments: usize,
    /// Total number of non-CAS bytes skipped by tolerant parsing.
    pub ignored_bytes: usize,
    /// First standard header filename, if present.
    pub header_name: Option<String>,
    /// First standard header file type, if present.
    pub header_file_type: Option<&'static str>,
}

/// Dragon-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DragonSessionQueryProvider;

/// Dragon 32 runtime.
pub struct DragonRuntime {
    profile: MachineProfile,
    firmware_rom: [u8; ROM_SIZE],
    machine: Dragon32,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    tape: Option<CasImage>,
    tape_bytes: Vec<u8>,
}

impl DragonRuntime {
    /// Build a Dragon runtime from profile-declared firmware.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing or the Dragon BASIC ROM
    /// is not exactly 16 KiB.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        let rom_id = "dragon32-basic-rom";
        firmware.validate_for_profile(&profile)?;
        let rom = firmware
            .bytes(rom_id)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: rom_id.to_owned(),
            })?;
        Self::new(model, rom).map_err(|reason| MachineError::InvalidFirmware {
            id: rom_id.to_owned(),
            reason,
        })
    }

    /// Build a Dragon runtime from raw ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the supplied ROM is not exactly 16 KiB.
    pub fn new(model: Model, rom: &[u8]) -> Result<Self, String> {
        let firmware_rom: [u8; ROM_SIZE] = rom
            .try_into()
            .map_err(|_| format!("Dragon 32 BASIC ROM must be exactly {ROM_SIZE} bytes"))?;
        Ok(Self {
            profile: profile_for(model),
            firmware_rom,
            machine: Dragon32::new(&firmware_rom),
            time: MachineTime::default(),
            rgba_framebuffer: Vec::with_capacity(
                TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT * 4,
            ),
            tape: None,
            tape_bytes: Vec::new(),
        })
    }

    /// Build a runtime backed by a zero-filled ROM image.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        let firmware_rom = [0; ROM_SIZE];
        Self {
            profile: profile_for(model),
            firmware_rom,
            machine: Dragon32::new(&firmware_rom),
            time: MachineTime::default(),
            rgba_framebuffer: Vec::with_capacity(
                TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT * 4,
            ),
            tape: None,
            tape_bytes: Vec::new(),
        }
    }

    /// Returns the current machine.
    #[must_use]
    pub fn machine(&self) -> &Dragon32 {
        &self.machine
    }

    /// Returns a summary of the currently mounted cassette image.
    #[must_use]
    pub fn tape_summary(&self) -> Option<DragonTapeSummary> {
        self.tape.as_ref().map(|tape| {
            let header = tape.first_header();
            DragonTapeSummary {
                blocks: tape.blocks.len(),
                checksums_valid: tape.checksums_valid(),
                ignored_segments: tape.ignored_ranges.len(),
                ignored_bytes: tape.ignored_byte_count(),
                header_name: header.map(|header| header.name.clone()),
                header_file_type: header.map(|header| cas_file_type_label(header.file_type)),
            }
        })
    }

    fn rebuild_machine(&mut self) {
        self.machine = Dragon32::new(&self.firmware_rom);
        if !self.tape_bytes.is_empty() {
            self.machine.load_cassette_bytes(self.tape_bytes.clone());
        }
        self.time = MachineTime::default();
        self.rgba_framebuffer.clear();
    }

    fn apply_input_event(&mut self, event: &InputEvent) -> Result<(), MachineError> {
        let (name, pressed) = match event {
            InputEvent::Key { name, pressed } => (name.as_ref(), *pressed),
            InputEvent::Button { name, pressed, .. } => (name.as_ref(), *pressed),
            _ => return Ok(()),
        };
        let Some(key) = DragonKey::from_label(name) else {
            return Ok(());
        };
        let key = MatrixKey::from_dragon_key(key);
        let result = if pressed {
            self.machine.keyboard_mut().press(key)
        } else {
            self.machine.keyboard_mut().release(key)
        };
        result.map_err(|reason| MachineError::InvalidRequest {
            reason: reason.to_string(),
        })
    }

    fn update_framebuffer(&mut self) {
        let argb = self.machine.beam_visible_argb();
        self.rgba_framebuffer.clear();
        self.rgba_framebuffer.reserve(argb.len() * 4);
        for pixel in argb.iter().copied() {
            self.rgba_framebuffer.push(((pixel >> 16) & 0xFF) as u8);
            self.rgba_framebuffer.push(((pixel >> 8) & 0xFF) as u8);
            self.rgba_framebuffer.push((pixel & 0xFF) as u8);
            self.rgba_framebuffer.push(((pixel >> 24) & 0xFF) as u8);
        }
    }
}

impl MachineCore for DragonRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_machine();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            let slot = image.slot.as_ref();
            match image.kind {
                MediaKind::Tape if slot == "tape-1" => {
                    let tape = parse_cas_tolerant(image.bytes).map_err(|reason| {
                        MachineError::InvalidMedia {
                            slot: slot.to_owned(),
                            reason: reason.to_string(),
                        }
                    })?;
                    let tape_bytes = cassette_bytes_from_cas(&tape);
                    self.machine.load_cassette_bytes(tape_bytes.clone());
                    self.tape = Some(tape);
                    self.tape_bytes = tape_bytes;
                }
                MediaKind::Tape => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                _ => return Err(MachineError::UnsupportedMediaKind { kind: image.kind }),
            }
        }
        Ok(())
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        for event in host.input_events {
            self.apply_input_event(event)?;
        }

        if target <= self.time {
            return Ok(RunResult::new(self.time, StopReason::ReachedTarget));
        }

        let cycles_to_run = target.0.saturating_sub(self.time.0);
        let report = self.machine.run_cycles(cycles_to_run, 0);
        self.time = self.time.saturating_add(report.cycles);
        self.update_framebuffer();
        host.frame_sink.push_frame(FramePacket {
            timestamp: self.time,
            format: PixelFormat::Rgba8888,
            width: TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
            height: TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32,
            palette: None,
            pixels: &self.rgba_framebuffer,
        })?;

        let stop_reason = if report.stop_reason == machine_dragon_32::StopReason::CpuHalted {
            StopReason::Halted
        } else {
            StopReason::ReachedTarget
        };
        Ok(RunResult::new(self.time, stop_reason))
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: "snapshot",
        })
    }

    fn restore(&mut self, _bytes: &[u8]) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: "restore",
        })
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
}

impl SessionQueryProvider<DragonRuntime> for DragonSessionQueryProvider {
    fn query_paths(&self, _machine: &DragonRuntime, prefix: Option<&str>) -> Vec<String> {
        DRAGON_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect()
    }

    fn query(
        &self,
        machine: &DragonRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "boot.detected" => json!(machine.boot_status().detected),
            "boot.reason" => json!(machine.boot_status().reason),
            "screen.text.lines" => json!(machine.screen_text_lines()),
            "dragon.cpu.cycles" => json!(machine.machine.cycles()),
            "dragon.cpu.instructions" => json!(machine.machine.instructions()),
            "dragon.cpu.pc" => json!(machine.machine.pc()),
            "dragon.machine.halted" => json!(machine.machine.is_halted()),
            "dragon.pia0.control_a" => json!(machine.machine.pia0_control_a()),
            "dragon.pia0.control_b" => json!(machine.machine.pia0_control_b()),
            "dragon.pia0.ddr_a" => json!(machine.machine.pia0_ddr_a()),
            "dragon.pia0.ddr_b" => json!(machine.machine.pia0_ddr_b()),
            "dragon.pia1.ca2" => json!(machine.machine.pia1_ca2()),
            "dragon.pia1.cb2" => json!(machine.machine.pia1_cb2()),
            "dragon.pia1.control_a" => json!(machine.machine.pia1_control_a()),
            "dragon.pia1.control_b" => json!(machine.machine.pia1_control_b()),
            "dragon.pia1.ddr_b" => json!(machine.machine.pia1_ddr_b()),
            "dragon.pia1.output_b" => json!(machine.machine.pia1_output_b()),
            "dragon.pia1.pins_b" => json!(machine.machine.pia1_pins_b()),
            "dragon.sam.display_offset" => json!(machine.machine.sam_display_offset()),
            "dragon.sam.video_mode" => json!(machine.machine.sam_video_mode()),
            "dragon.tape.loaded" => json!(machine.tape.is_some()),
            "dragon.tape.blocks" => json!(machine.tape.as_ref().map(|tape| tape.blocks.len())),
            "dragon.tape.checksums_valid" => {
                json!(machine.tape.as_ref().map(CasImage::checksums_valid))
            }
            "dragon.tape.ignored_segments" => {
                json!(machine.tape.as_ref().map(|tape| tape.ignored_ranges.len()))
            }
            "dragon.tape.ignored_bytes" => {
                json!(machine.tape.as_ref().map(CasImage::ignored_byte_count))
            }
            "dragon.tape.finished" => json!(machine.machine.cassette_finished()),
            "dragon.tape.length_bits" => json!(machine.machine.cassette_len_bits()),
            "dragon.tape.motor_on" => json!(machine.machine.cassette_motor_on()),
            "dragon.tape.position_bits" => json!(machine.machine.cassette_position_bits()),
            "dragon.tape.header.name" => {
                json!(
                    machine
                        .tape
                        .as_ref()
                        .and_then(CasImage::first_header)
                        .map(|header| header.name.as_str())
                )
            }
            "dragon.tape.header.file_type" => {
                json!(
                    machine
                        .tape
                        .as_ref()
                        .and_then(CasImage::first_header)
                        .map(|header| cas_file_type_label(header.file_type))
                )
            }
            "dragon.text.base" | "dragon.video.display_base" => {
                json!(machine.machine.text_screen_base())
            }
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn cassette_bytes_from_cas(tape: &CasImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    for block in &tape.blocks {
        let leader_len = block.leader_len.max(MIN_INITIAL_LEADER_BYTES);
        bytes.extend(std::iter::repeat_n(LEADER_BYTE, leader_len));
        bytes.push(SYNC_BYTE);
        bytes.push(block.block_type);
        bytes.push(block.data.len() as u8);
        bytes.extend_from_slice(&block.data);
        bytes.push(block.checksum);
        bytes.push(LEADER_BYTE);
    }
    bytes
}

const fn cas_file_type_label(file_type: CasFileType) -> &'static str {
    match file_type {
        CasFileType::Basic => "basic",
        CasFileType::Data => "data",
        CasFileType::MachineCode => "machine-code",
        CasFileType::Unknown(_) => "unknown",
    }
}

struct BootStatus {
    detected: bool,
    reason: &'static str,
}

impl DragonRuntime {
    fn screen_text_lines(&self) -> Vec<String> {
        self.machine
            .capture_text_screen()
            .to_plain_text()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn boot_status(&self) -> BootStatus {
        if self
            .screen_text_lines()
            .iter()
            .any(|line| line.trim() == "OK")
        {
            BootStatus {
                detected: true,
                reason: "basic-ok-prompt",
            }
        } else {
            BootStatus {
                detected: false,
                reason: "waiting-for-basic-ok-prompt",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use emu198x_shell::{
        FirmwareImage, FirmwareSet, FramePacket, FrameSink, HostIo, MachineCore, MachineTime,
        MediaImage, MediaKind, MediaSet, NullAudioSink, NullTraceSink, PixelFormat,
    };
    use format_dragon_cas::{LEADER_BYTE, SYNC_BYTE, checksum_for};
    use motorola_vdg_6847::TEXT_ROWS;

    use super::*;

    #[derive(Default)]
    struct CaptureFrameSink {
        frames: usize,
        last_size: Option<(u32, u32)>,
        last_format: Option<PixelFormat>,
    }

    impl FrameSink for CaptureFrameSink {
        fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
            self.frames += 1;
            self.last_size = Some((frame.width, frame.height));
            self.last_format = Some(frame.format);
            Ok(())
        }
    }

    fn rom_with_reset_vector(pc: u16) -> [u8; ROM_SIZE] {
        let mut rom = [0; ROM_SIZE];
        let [hi, lo] = pc.to_be_bytes();
        rom[0x3FFE] = hi;
        rom[0x3FFF] = lo;
        rom
    }

    #[test]
    fn runtime_builds_from_declared_firmware() {
        let rom = rom_with_reset_vector(0x8000);
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new("dragon32-basic-rom", &rom));

        let runtime = DragonRuntime::from_firmware(Model::Dragon32Pal, &firmware)
            .expect("declared firmware should build runtime");

        assert_eq!(runtime.profile().profile_id.as_str(), "dragon-32-pal");
    }

    #[test]
    fn runtime_emits_text_framebuffer() {
        let mut runtime = DragonRuntime::blank(Model::Dragon32Pal);
        let mut frame_sink = CaptureFrameSink::default();
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };

        let result = runtime
            .run_until(MachineTime(64), &mut host)
            .expect("runtime should run");

        assert_eq!(result.reached, MachineTime(64));
        assert_eq!(frame_sink.frames, 1);
        assert_eq!(
            frame_sink.last_size,
            Some((
                TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
                TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32
            ))
        );
        assert_eq!(frame_sink.last_format, Some(PixelFormat::Rgba8888));
    }

    #[test]
    fn query_provider_reports_machine_state() {
        let runtime = DragonRuntime::blank(Model::Dragon32Pal);
        let provider = DragonSessionQueryProvider;

        let query = provider
            .query(&runtime, "dragon.video.display_base")
            .expect("query should not fail")
            .expect("query should be owned");
        let legacy_query = provider
            .query(&runtime, "dragon.text.base")
            .expect("query should not fail")
            .expect("query should be owned");

        assert_eq!(query.value, json!(0));
        assert_eq!(legacy_query.value, query.value);
    }

    #[test]
    fn boot_query_reports_pending_without_basic_prompt() {
        let runtime = DragonRuntime::blank(Model::Dragon32Pal);
        let provider = DragonSessionQueryProvider;

        let detected = provider
            .query(&runtime, "boot.detected")
            .expect("query should not fail")
            .expect("query should be owned");
        let reason = provider
            .query(&runtime, "boot.reason")
            .expect("query should not fail")
            .expect("query should be owned");

        assert_eq!(detected.value, json!(false));
        assert_eq!(reason.value, json!("waiting-for-basic-ok-prompt"));
    }

    #[test]
    fn query_provider_reports_screen_text_lines() {
        let runtime = DragonRuntime::blank(Model::Dragon32Pal);
        let provider = DragonSessionQueryProvider;

        let lines = provider
            .query(&runtime, "screen.text.lines")
            .expect("query should not fail")
            .expect("query should be owned");

        let lines = lines
            .value
            .as_array()
            .expect("screen text lines should be an array");
        assert_eq!(lines.len(), TEXT_ROWS);
    }

    fn cas_with_header(name: &[u8; 8], file_type: u8) -> Vec<u8> {
        let payload = [
            name[0], name[1], name[2], name[3], name[4], name[5], name[6], name[7], file_type,
            0x00, 0x00, 0x12, 0x34, 0x56, 0x78,
        ];
        let mut cas = vec![
            LEADER_BYTE,
            LEADER_BYTE,
            SYNC_BYTE,
            0x00,
            payload.len() as u8,
        ];
        cas.extend_from_slice(&payload);
        cas.push(checksum_for(0x00, payload.len() as u8, &payload));
        cas.extend_from_slice(&[
            LEADER_BYTE,
            SYNC_BYTE,
            0x01,
            0x02,
            0xaa,
            0xbb,
            checksum_for(0x01, 0x02, &[0xaa, 0xbb]),
            LEADER_BYTE,
            SYNC_BYTE,
            0xff,
            0x00,
            0xff,
        ]);
        cas
    }

    #[test]
    fn load_media_accepts_dragon_cas_tape() {
        let mut runtime = DragonRuntime::blank(Model::Dragon32Pal);
        let cas = cas_with_header(b"TEST    ", 0x02);
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &cas));

        runtime.load_media(&media).expect("CAS tape should load");

        let summary = runtime.tape_summary().expect("tape should be mounted");
        assert_eq!(summary.blocks, 3);
        assert!(summary.checksums_valid);
        assert_eq!(summary.header_name.as_deref(), Some("TEST"));
        assert_eq!(summary.header_file_type, Some("machine-code"));

        let provider = DragonSessionQueryProvider;
        assert_eq!(
            provider
                .query(&runtime, "dragon.tape.header.name")
                .expect("query should not fail")
                .expect("query should be owned")
                .value,
            json!("TEST")
        );
        assert_eq!(
            provider
                .query(&runtime, "dragon.tape.loaded")
                .expect("query should not fail")
                .expect("query should be owned")
                .value,
            json!(true)
        );
        assert_eq!(
            provider
                .query(&runtime, "dragon.tape.position_bits")
                .expect("query should not fail")
                .expect("query should be owned")
                .value,
            json!(0)
        );
        assert_eq!(
            provider
                .query(&runtime, "dragon.tape.length_bits")
                .expect("query should not fail")
                .expect("query should be owned")
                .value,
            json!(runtime.machine.cassette_len_bits())
        );
    }

    #[test]
    fn load_media_rejects_unknown_tape_slot() {
        let mut runtime = DragonRuntime::blank(Model::Dragon32Pal);
        let cas = cas_with_header(b"TEST    ", 0x00);
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-2", MediaKind::Tape, &cas));

        let err = runtime.load_media(&media).expect_err("unknown slot");

        match err {
            MachineError::UnknownMediaSlot { slot } => assert_eq!(slot, "tape-2"),
            other => panic!("expected UnknownMediaSlot, got {other:?}"),
        }
    }

    #[test]
    fn load_media_rejects_malformed_cas() {
        let mut runtime = DragonRuntime::blank(Model::Dragon32Pal);
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &[0x00]));

        let err = runtime.load_media(&media).expect_err("malformed CAS");

        match err {
            MachineError::InvalidMedia { slot, reason } => {
                assert_eq!(slot, "tape-1");
                assert!(reason.contains("unexpected byte"));
            }
            other => panic!("expected InvalidMedia, got {other:?}"),
        }
    }
}
