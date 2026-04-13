//! Runtime wrapper for the fresh-workspace Commodore 64.

use emu198x_shell::{
    AudioPacket, CapabilitySet, FirmwareSet, FramePacket, HostIo, InputEvent, MachineCore,
    MachineError, MachineProfile, MachineTime, QueryError, QueryResult, ResetKind, RunResult,
    SessionQueryProvider, StopReason,
};
use machine_commodore_c64::{C64, C64Config, C64Model, C64Snapshot};
use serde_json::json;

use crate::{Model, profile_for};

const C64_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.offset",
    "c64.cia1.irq",
    "c64.cia2.irq",
    "c64.machine.cycle_in_line",
    "c64.machine.raster_line",
    "c64.vic.ba_low",
    "c64.vic.irq",
];

const READY_SCREEN_CODES: [u8; 6] = [18, 5, 1, 4, 25, 46];
const KERNAL_ROM_SIZE: usize = 0x2000;
const BASIC_ROM_SIZE: usize = 0x2000;
const CHARACTER_ROM_SIZE: usize = 0x1000;

/// Firmware-backed Commodore 64 runtime.
pub struct C64Runtime {
    profile: MachineProfile,
    model: Model,
    machine: C64,
    time: MachineTime,
    kernal_rom: Vec<u8>,
    basic_rom: Vec<u8>,
    character_rom: Vec<u8>,
    rgba_framebuffer: Vec<u8>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SnapshotEnvelopeV1 {
    version: u32,
    profile_id: String,
    time: MachineTime,
    machine: C64Snapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct C64BootStatus {
    detected: bool,
    reason: String,
    offset: Option<u16>,
}

/// C64-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct C64SessionQueryProvider;

impl Model {
    const fn to_machine_model(self) -> C64Model {
        match self {
            Self::C64PalBreadbin => C64Model::PalBreadbin,
            Self::C64NtscBreadbin => C64Model::NtscBreadbin,
        }
    }
}

impl C64Runtime {
    /// Creates a C64 runtime from owned ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if any ROM image has the wrong size.
    pub fn new(
        model: Model,
        kernal_rom: Vec<u8>,
        basic_rom: Vec<u8>,
        character_rom: Vec<u8>,
    ) -> Result<Self, MachineError> {
        validate_rom_size("commodore-c64-kernal-rom", &kernal_rom, KERNAL_ROM_SIZE)?;
        validate_rom_size("commodore-c64-basic-rom", &basic_rom, BASIC_ROM_SIZE)?;
        validate_rom_size(
            "commodore-c64-character-rom",
            &character_rom,
            CHARACTER_ROM_SIZE,
        )?;

        let machine = build_machine(model, &kernal_rom, &basic_rom, &character_rom)?;
        let rgba_framebuffer = vec![
            0;
            (machine.vic().framebuffer_width() * machine.vic().framebuffer_height() * 4)
                as usize
        ];

        Ok(Self {
            profile: profile_for(model),
            model,
            machine,
            time: MachineTime::default(),
            kernal_rom,
            basic_rom,
            character_rom,
            rgba_framebuffer,
        })
    }

    /// Creates a runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if firmware is missing or invalid.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;

        let kernal = firmware.bytes("commodore-c64-kernal-rom").ok_or_else(|| {
            MachineError::MissingFirmware {
                id: "commodore-c64-kernal-rom".to_owned(),
            }
        })?;
        let basic = firmware.bytes("commodore-c64-basic-rom").ok_or_else(|| {
            MachineError::MissingFirmware {
                id: "commodore-c64-basic-rom".to_owned(),
            }
        })?;
        let character = firmware
            .bytes("commodore-c64-character-rom")
            .ok_or_else(|| MachineError::MissingFirmware {
                id: "commodore-c64-character-rom".to_owned(),
            })?;

        Self::new(model, kernal.to_vec(), basic.to_vec(), character.to_vec())
    }

    /// Creates a runtime backed by zero-filled ROMs.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        match Self::new(
            model,
            vec![0; KERNAL_ROM_SIZE],
            vec![0; BASIC_ROM_SIZE],
            vec![0; CHARACTER_ROM_SIZE],
        ) {
            Ok(runtime) => runtime,
            Err(reason) => unreachable!(
                "blank C64 runtime should always construct from fixed-size ROM images: {reason}"
            ),
        }
    }

    /// Returns the wrapped C64 machine.
    #[must_use]
    pub fn machine(&self) -> &C64 {
        &self.machine
    }

    /// Returns mutable access to the wrapped C64 machine.
    #[must_use]
    pub fn machine_mut(&mut self) -> &mut C64 {
        &mut self.machine
    }

    /// Returns the current runtime time in `phi2` cycles.
    #[must_use]
    pub const fn time(&self) -> MachineTime {
        self.time
    }

    /// Imports one PRG byte stream into raw RAM and returns its load address.
    ///
    /// This is a host-side convenience path, not emulated media.
    ///
    /// # Errors
    ///
    /// Returns an error if the PRG header is malformed.
    pub fn load_prg_bytes(&mut self, data: &[u8]) -> Result<u16, String> {
        self.machine.load_prg(data)
    }

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        self.machine = build_machine(
            self.model,
            &self.kernal_rom,
            &self.basic_rom,
            &self.character_rom,
        )?;
        self.time = MachineTime::default();
        Ok(())
    }

    fn emit_frame(&mut self, host: &mut HostIo<'_>) -> Result<(), MachineError> {
        repack_rgba8888(self.machine.framebuffer(), &mut self.rgba_framebuffer);
        host.frame_sink.push_frame(FramePacket {
            timestamp: self.time,
            format: emu198x_shell::PixelFormat::Rgba8888,
            width: self.machine.vic().framebuffer_width(),
            height: self.machine.vic().framebuffer_height(),
            palette: None,
            pixels: &self.rgba_framebuffer,
        })?;
        Ok(())
    }
}

impl MachineCore for C64Runtime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_machine()
            .expect("C64 runtime reset should rebuild from already-validated ROMs");
    }

    fn load_media(&mut self, media: &emu198x_shell::MediaSet<'_>) -> Result<(), MachineError> {
        if let Some(image) = media.images.first() {
            let Some(slot) = self
                .profile
                .media_slots
                .iter()
                .find(|slot| slot.id.as_ref() == image.slot.as_ref())
            else {
                return Err(MachineError::UnknownMediaSlot {
                    slot: image.slot.as_ref().to_owned(),
                });
            };

            if slot.kind != image.kind {
                return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
            }

            Err(MachineError::UnsupportedOperation {
                operation: "load_media",
            })
        } else {
            Ok(())
        }
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        for event in host.input_events {
            apply_input_event(&mut self.machine, event);
        }

        while self.time < target {
            let frame_complete = self.machine.tick();
            self.time = MachineTime::new(self.machine.phi2_cycles());

            if frame_complete {
                self.emit_frame(host)?;
                let audio = self.machine.take_audio_buffer();
                if !audio.is_empty() {
                    host.audio_sink.push_audio(AudioPacket {
                        timestamp: self.time,
                        sample_rate: self.machine.audio_sample_rate(),
                        channels: 1,
                        samples: &audio,
                    })?;
                }
            }
        }

        let _ = &host.trace_sink;

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

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
}

impl SessionQueryProvider<C64Runtime> for C64SessionQueryProvider {
    fn query_paths(&self, _machine: &C64Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = C64_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &C64Runtime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let boot = c64_boot_status(machine.machine());

        let value = match path {
            "boot.detected" => json!(boot.detected),
            "boot.reason" => json!(boot.reason),
            "boot.offset" => json!(boot.offset),
            "c64.machine.raster_line" => json!(machine.machine().raster_line()),
            "c64.machine.cycle_in_line" => json!(machine.machine().cycle_in_line()),
            "c64.vic.ba_low" => json!(machine.machine().vic().ba_is_low()),
            "c64.vic.irq" => json!(machine.machine().vic().irq_active()),
            "c64.cia1.irq" => json!(machine.machine().cia1().irq_active()),
            "c64.cia2.irq" => json!(machine.machine().cia2().irq_active()),
            _ => return Ok(None),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn build_machine(
    model: Model,
    kernal_rom: &[u8],
    basic_rom: &[u8],
    character_rom: &[u8],
) -> Result<C64, MachineError> {
    C64::new(C64Config {
        model: model.to_machine_model(),
        kernal_rom,
        basic_rom,
        character_rom,
    })
    .map_err(|reason| MachineError::InvalidRequest {
        reason: reason.to_string(),
    })
}

fn validate_rom_size(id: &'static str, bytes: &[u8], expected: usize) -> Result<(), MachineError> {
    if bytes.len() == expected {
        return Ok(());
    }

    Err(MachineError::InvalidFirmware {
        id: id.to_owned(),
        reason: format!("expected exactly {expected} bytes, got {}", bytes.len()),
    })
}

fn repack_rgba8888(argb_pixels: &[u32], rgba: &mut Vec<u8>) {
    let required_len = argb_pixels.len() * 4;
    if rgba.len() != required_len {
        rgba.resize(required_len, 0);
    }

    for (index, pixel) in argb_pixels.iter().copied().enumerate() {
        let base = index * 4;
        rgba[base] = ((pixel >> 16) & 0xFF) as u8;
        rgba[base + 1] = ((pixel >> 8) & 0xFF) as u8;
        rgba[base + 2] = (pixel & 0xFF) as u8;
        rgba[base + 3] = ((pixel >> 24) & 0xFF) as u8;
    }
}

fn c64_boot_status(machine: &C64) -> C64BootStatus {
    let end = 0x07E8u16 - 0x0400u16 - READY_SCREEN_CODES.len() as u16;
    for offset in 0..=end {
        let mut matched = true;
        for (index, expected) in READY_SCREEN_CODES.iter().copied().enumerate() {
            if machine.memory().ram_read(0x0400 + offset + index as u16) != expected {
                matched = false;
                break;
            }
        }

        if matched {
            return C64BootStatus {
                detected: true,
                reason: format!("found READY. screen codes at offset ${offset:04X}"),
                offset: Some(offset),
            };
        }
    }

    C64BootStatus {
        detected: false,
        reason: "READY. screen codes not visible".to_owned(),
        offset: None,
    }
}

fn apply_input_event(machine: &mut C64, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some((row, col)) = c64_key_position(name.as_ref())
    {
        machine.keyboard_mut().set_key(row, col, *pressed);
    }
}

fn c64_key_position(name: &str) -> Option<(u8, u8)> {
    let upper = name.to_ascii_uppercase();
    match upper.as_str() {
        "RETURN" | "ENTER" => Some((0, 1)),
        "3" => Some((1, 0)),
        "W" => Some((1, 1)),
        "A" => Some((1, 2)),
        "4" => Some((1, 3)),
        "Z" => Some((1, 4)),
        "S" => Some((1, 5)),
        "E" => Some((1, 6)),
        "LSHIFT" => Some((1, 7)),
        "5" => Some((2, 0)),
        "R" => Some((2, 1)),
        "D" => Some((2, 2)),
        "6" => Some((2, 3)),
        "C" => Some((2, 4)),
        "F" => Some((2, 5)),
        "T" => Some((2, 6)),
        "X" => Some((2, 7)),
        "7" => Some((3, 0)),
        "Y" => Some((3, 1)),
        "G" => Some((3, 2)),
        "8" => Some((3, 3)),
        "B" => Some((3, 4)),
        "H" => Some((3, 5)),
        "U" => Some((3, 6)),
        "V" => Some((3, 7)),
        "9" => Some((4, 0)),
        "I" => Some((4, 1)),
        "J" => Some((4, 2)),
        "0" => Some((4, 3)),
        "M" => Some((4, 4)),
        "K" => Some((4, 5)),
        "O" => Some((4, 6)),
        "N" => Some((4, 7)),
        "P" => Some((5, 1)),
        "L" => Some((5, 2)),
        "." | "PERIOD" => Some((5, 4)),
        "," | "COMMA" => Some((5, 7)),
        "RSHIFT" => Some((6, 4)),
        "/" | "SLASH" => Some((6, 7)),
        "1" => Some((7, 0)),
        "2" => Some((7, 3)),
        "SPACE" => Some((7, 4)),
        "Q" => Some((7, 6)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
    use emu198x_shell::{
        AudioPacket, AudioSink, FirmwareImage, FirmwareSet, FrameSink, NullAudioSink,
        NullTraceSink, PixelFormat,
    };
    use std::fs;
    use std::path::PathBuf;

    #[derive(Default)]
    struct FrameCollector {
        count: usize,
        last_timestamp: MachineTime,
        last_width: u32,
        last_height: u32,
        last_format: Option<PixelFormat>,
    }

    #[derive(Default)]
    struct AudioCollector {
        count: usize,
        last_timestamp: MachineTime,
        last_sample_rate: u32,
        last_channels: u8,
        last_samples_len: usize,
    }

    impl FrameSink for FrameCollector {
        fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
            self.count += 1;
            self.last_timestamp = frame.timestamp;
            self.last_width = frame.width;
            self.last_height = frame.height;
            self.last_format = Some(frame.format);
            Ok(())
        }
    }

    impl AudioSink for AudioCollector {
        fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
            self.count += 1;
            self.last_timestamp = packet.timestamp;
            self.last_sample_rate = packet.sample_rate;
            self.last_channels = packet.channels;
            self.last_samples_len = packet.samples.len();
            Ok(())
        }
    }

    fn blank_firmware() -> FirmwareSet<'static> {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(
            "commodore-c64-kernal-rom",
            &[0; KERNAL_ROM_SIZE],
        ));
        firmware.push(FirmwareImage::new(
            "commodore-c64-basic-rom",
            &[0; BASIC_ROM_SIZE],
        ));
        firmware.push(FirmwareImage::new(
            "commodore-c64-character-rom",
            &[0; CHARACTER_ROM_SIZE],
        ));
        firmware
    }

    fn local_rom_firmware() -> FirmwareSet<'static> {
        let rom_dir = PathBuf::from(
            std::env::var("HOME").expect("HOME should be available for local C64 ROM tests"),
        )
        .join(".emu198x/roms/commodore-c64");

        let kernal = Box::leak(
            fs::read(rom_dir.join("kernal.rom"))
                .expect("local C64 KERNAL ROM should exist")
                .into_boxed_slice(),
        );
        let basic = Box::leak(
            fs::read(rom_dir.join("basic.rom"))
                .expect("local C64 BASIC ROM should exist")
                .into_boxed_slice(),
        );
        let chargen = Box::leak(
            fs::read(rom_dir.join("chargen.rom"))
                .expect("local C64 chargen ROM should exist")
                .into_boxed_slice(),
        );

        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new("commodore-c64-kernal-rom", kernal));
        firmware.push(FirmwareImage::new("commodore-c64-basic-rom", basic));
        firmware.push(FirmwareImage::new("commodore-c64-character-rom", chargen));
        firmware
    }

    #[test]
    fn runtime_can_build_from_declared_firmware() {
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware());
        assert!(runtime.is_ok(), "blank C64 firmware set should construct");
    }

    #[test]
    fn runtime_run_until_emits_rgba_frame() {
        let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
            .expect("blank C64 firmware should construct a runtime");
        let mut frame_sink = FrameCollector::default();
        let mut audio_sink = AudioCollector::default();
        let mut trace_sink = NullTraceSink;
        let target = MachineTime::new(u64::from(TIMING_PAL_BREADBIN.cycles_per_frame));

        let result = runtime
            .run_until(
                target,
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frame_sink,
                    audio_sink: &mut audio_sink,
                    trace_sink: &mut trace_sink,
                },
            )
            .expect("blank C64 runtime should run one frame");

        assert_eq!(result.stop_reason, StopReason::ReachedTarget);
        assert_eq!(result.reached, target);
        assert_eq!(frame_sink.count, 1);
        assert_eq!(frame_sink.last_timestamp, target);
        assert_eq!(frame_sink.last_width, 416);
        assert_eq!(frame_sink.last_height, 312);
        assert_eq!(frame_sink.last_format, Some(PixelFormat::Rgba8888));
        assert_eq!(audio_sink.count, 1);
        assert_eq!(audio_sink.last_timestamp, target);
        assert_eq!(audio_sink.last_sample_rate, runtime.machine().audio_sample_rate());
        assert_eq!(audio_sink.last_channels, 1);
        assert!(audio_sink.last_samples_len > 0);
    }

    #[test]
    fn query_provider_reports_blank_runtime_as_not_booted() {
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
            .expect("blank C64 firmware should construct a runtime");
        let provider = C64SessionQueryProvider;

        let paths = provider.query_paths(&runtime, Some("boot."));
        assert_eq!(
            paths,
            vec![
                "boot.detected".to_owned(),
                "boot.offset".to_owned(),
                "boot.reason".to_owned()
            ]
        );

        let detected = provider
            .query(&runtime, "boot.detected")
            .expect("boot.detected query should not fail")
            .expect("boot.detected should resolve");
        assert_eq!(detected.value, json!(false));

        let reason = provider
            .query(&runtime, "boot.reason")
            .expect("boot.reason query should not fail")
            .expect("boot.reason should resolve");
        assert_eq!(reason.value, json!("READY. screen codes not visible"));

        assert!(matches!(provider.query(&runtime, "not-a-path"), Ok(None)));
    }

    #[test]
    fn snapshot_round_trip_preserves_mid_cycle_runtime_state() {
        let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
            .expect("blank C64 firmware should construct a runtime");
        let mut frame_sink = FrameCollector::default();
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let target = MachineTime::new(3);

        runtime
            .run_until(
                target,
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frame_sink,
                    audio_sink: &mut audio_sink,
                    trace_sink: &mut trace_sink,
                },
            )
            .expect("blank C64 runtime should run a few cycles");

        let snapshot = runtime
            .snapshot()
            .expect("blank C64 runtime should snapshot");
        let mut expected_machine = runtime.machine.clone();
        let expected_time = runtime.time();

        let mut restored = C64Runtime::blank(Model::C64PalBreadbin);
        restored
            .restore(&snapshot)
            .expect("snapshot restore should succeed");

        assert_eq!(restored.time(), expected_time);
        assert_eq!(restored.machine().cpu().regs, expected_machine.cpu().regs);
        assert_eq!(restored.machine().cpu().addr, expected_machine.cpu().addr);
        assert_eq!(restored.machine().cpu().rw, expected_machine.cpu().rw);
        assert_eq!(restored.machine().cpu().sync, expected_machine.cpu().sync);
        assert_eq!(
            restored.machine().raster_line(),
            expected_machine.raster_line()
        );
        assert_eq!(
            restored.machine().cycle_in_line(),
            expected_machine.cycle_in_line()
        );
        assert_eq!(
            restored.machine().framebuffer(),
            expected_machine.framebuffer()
        );

        for _ in 0..8 {
            let expected_frame_complete = expected_machine.tick();
            let restored_frame_complete = restored.machine_mut().tick();
            assert_eq!(restored_frame_complete, expected_frame_complete);
            assert_eq!(restored.machine().cpu().regs, expected_machine.cpu().regs);
            assert_eq!(restored.machine().cpu().addr, expected_machine.cpu().addr);
            assert_eq!(restored.machine().cpu().rw, expected_machine.cpu().rw);
            assert_eq!(restored.machine().cpu().sync, expected_machine.cpu().sync);
            assert_eq!(
                restored.machine().cpu().total_cycles,
                expected_machine.cpu().total_cycles
            );
            assert_eq!(
                restored.machine().raster_line(),
                expected_machine.raster_line()
            );
            assert_eq!(
                restored.machine().cycle_in_line(),
                expected_machine.cycle_in_line()
            );
            assert_eq!(restored.machine().vic().irq, expected_machine.vic().irq);
            assert_eq!(
                restored.machine().vic().ba_low,
                expected_machine.vic().ba_low
            );
            assert_eq!(
                restored.machine().framebuffer(),
                expected_machine.framebuffer()
            );
        }
    }

    #[test]
    #[ignore = "requires local C64 ROMs at ~/.emu198x/roms/commodore-c64"]
    fn query_provider_detects_ready_on_real_pal_boot() {
        let firmware = local_rom_firmware();
        let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("real PAL C64 firmware should construct a runtime");
        let mut frame_sink = FrameCollector::default();
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let target = MachineTime::new(u64::from(TIMING_PAL_BREADBIN.cycles_per_frame) * 200);

        runtime
            .run_until(
                target,
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frame_sink,
                    audio_sink: &mut audio_sink,
                    trace_sink: &mut trace_sink,
                },
            )
            .expect("real PAL C64 ROMs should run to boot window");

        let provider = C64SessionQueryProvider;
        let detected = provider
            .query(&runtime, "boot.detected")
            .expect("boot.detected query should not fail")
            .expect("boot.detected should resolve");
        assert_eq!(detected.value, json!(true));

        let offset = provider
            .query(&runtime, "boot.offset")
            .expect("boot.offset query should not fail")
            .expect("boot.offset should resolve");
        assert_ne!(offset.value, json!(null));
    }
}
