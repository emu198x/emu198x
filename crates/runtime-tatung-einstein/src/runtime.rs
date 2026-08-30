//! Runtime wrapper for the Tatung Einstein.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use machine_tatung_einstein::{Einstein, EinsteinRegion};

use crate::input::apply_input_event;
use crate::profiles::{Model, ROM_FIRMWARE_ID, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

const ROM_SIZE: usize = 8 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// This machine's keyboard for the shared `press_key` / `type_string` tools:
/// the standard layout, backed by this machine's own key-name resolver so a
/// character it cannot type is refused rather than silently dropped (#1196).
/// The character each keycap carries above its own, read off the machine:
/// hold SHIFT with every key in turn and let the MOS echo the result (#1206).
const SHIFTED_LEGENDS: &[(char, &str)] = &[
    ('!', "1"),
    ('"', "2"),
    ('#', "3"),
    ('$', "4"),
    ('%', "5"),
    ('&', "6"),
    ('\'', "7"),
    ('(', "8"),
    (')', "9"),
    ('@', "0"),
    ('<', ","),
    ('>', "."),
    ('?', "/"),
    ('*', ":"),
    ('+', ";"),
    ('-', "="),
];

static KEYBOARD: emu198x_shell::StandardKeyboard = emu198x_shell::StandardKeyboard::with_legends(
    emu198x_shell::STANDARD_KEY_TIMING,
    crate::input::knows_key_name,
    "shift",
    SHIFTED_LEGENDS,
);

pub struct EinsteinRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Einstein>,
    rom_bytes: Option<Vec<u8>>,
    floppy0_bytes: Option<Vec<u8>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    controller_cache: crate::input::ControllerCache,
}

impl EinsteinRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            rom_bytes: None,
            floppy0_bytes: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            controller_cache: crate::input::ControllerCache::default(),
        }
    }

    /// Build from explicit 8 KB MOS ROM.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the ROM is not 8 KB.
    pub fn new(model: Model, rom: Vec<u8>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.set_rom(rom)?;
        Ok(runtime)
    }

    /// Build from a firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or ROM is missing.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let bytes =
            firmware
                .bytes(ROM_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: ROM_FIRMWARE_ID.to_owned(),
                })?;
        Self::new(model, bytes.to_vec())
    }

    /// Replace the MOS ROM and rebuild.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if size is wrong.
    pub fn set_rom(&mut self, rom: Vec<u8>) -> Result<(), MachineError> {
        if rom.len() != ROM_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: ROM_FIRMWARE_ID.to_owned(),
                reason: format!("MOS ROM is {} bytes; expected {ROM_SIZE}", rom.len()),
            });
        }
        self.rom_bytes = Some(rom);
        self.rebuild_machine();
        Ok(())
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Einstein> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Einstein> {
        self.machine.as_mut()
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    /// Install a machine restored from a snapshot. Re-sizes the host RGBA
    /// framebuffer from the restored machine's dimensions before repainting —
    /// `blank()` starts with an empty buffer, so skipping this would panic on
    /// the first repaint. Mirrors the sizing `rebuild_machine` does.
    pub(crate) fn set_machine(&mut self, machine: Option<Einstein>) {
        if let Some(machine) = &machine {
            let width = machine.framebuffer_width();
            let height = machine.framebuffer_height();
            self.rgba_width = width;
            self.rgba_height = height;
            self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        }
        self.machine = machine;
        self.update_rgba_framebuffer();
    }

    fn rebuild_machine(&mut self) {
        let Some(rom) = self.rom_bytes.clone() else {
            self.machine = None;
            return;
        };
        let mut machine = Einstein::new(rom, EinsteinRegion::Pal);
        if let Some(bytes) = &self.floppy0_bytes {
            machine
                .insert_cpc_dsk(0, bytes)
                .expect("cached floppy image was validated when loaded");
        }
        let width = machine.framebuffer_width();
        let height = machine.framebuffer_height();
        self.rgba_width = width;
        self.rgba_height = height;
        self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        self.machine = Some(machine);
        self.update_rgba_framebuffer();
    }

    fn update_rgba_framebuffer(&mut self) {
        let Some(machine) = self.machine.as_ref() else {
            self.rgba_framebuffer.fill(0);
            return;
        };
        for (index, &pixel) in machine.framebuffer().iter().enumerate() {
            let base = index * 4;
            self.rgba_framebuffer[base] = ((pixel >> 16) & 0xff) as u8;
            self.rgba_framebuffer[base + 1] = ((pixel >> 8) & 0xff) as u8;
            self.rgba_framebuffer[base + 2] = (pixel & 0xff) as u8;
            self.rgba_framebuffer[base + 3] = ((pixel >> 24) & 0xff) as u8;
        }
    }
}

impl MachineCore for EinsteinRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }
    fn time(&self) -> MachineTime {
        self.time
    }
    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_machine();
        self.time = MachineTime::default();
    }
    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            let slot = image.slot.as_ref();
            if slot != "floppy-0" {
                return Err(MachineError::UnknownMediaSlot {
                    slot: slot.to_owned(),
                });
            }
            if image.kind != emu198x_shell::MediaKind::Disk {
                return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
            }
            if let Some(machine) = self.machine.as_mut() {
                machine.insert_cpc_dsk(0, image.bytes).map_err(|reason| {
                    MachineError::InvalidMedia {
                        slot: slot.to_owned(),
                        reason,
                    }
                })?;
            } else {
                Einstein::validate_cpc_dsk(image.bytes).map_err(|reason| {
                    MachineError::InvalidMedia {
                        slot: slot.to_owned(),
                        reason,
                    }
                })?;
            }
            self.floppy0_bytes = Some(image.bytes.to_vec());
        }
        Ok(())
    }
    fn eject_media(&mut self, slot: &str) -> Result<(), MachineError> {
        if slot != "floppy-0" {
            return Err(MachineError::UnknownMediaSlot {
                slot: slot.to_owned(),
            });
        }
        if let Some(machine) = self.machine.as_mut() {
            machine.eject_disk(0);
        }
        self.floppy0_bytes = None;
        Ok(())
    }
    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        if self.machine.is_none() {
            return Ok(RunResult::new(self.time, StopReason::WaitingForInput));
        }
        for event in host.input_events {
            if let Some(machine) = self.machine.as_mut() {
                apply_input_event(machine, &mut self.controller_cache, event);
            }
        }
        while self.time < target {
            let ticks = self
                .machine
                .as_mut()
                .expect("machine checked above")
                .run_frame();
            self.time = self.time.saturating_add(ticks);
            self.update_rgba_framebuffer();
            host.frame_sink.push_frame(FramePacket {
                timestamp: self.time,
                format: PixelFormat::Rgba8888,
                width: self.rgba_width,
                height: self.rgba_height,
                palette: None,
                pixels: &self.rgba_framebuffer,
            })?;
            let audio = self
                .machine
                .as_mut()
                .expect("machine checked above")
                .take_audio_buffer();
            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: AUDIO_SAMPLE_RATE,
                channels: 1,
                samples: &audio,
            })?;
        }
        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
    }
    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        snapshot::encode(self)
    }
    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        snapshot::decode(self, bytes)
    }
    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: command.operation_name(),
        })
    }
    /// The TMS9918 family drove a television through a colour-subcarrier
    /// crystal, so its dots are not square: 8:7 on the NTSC parts, about
    /// 1.382 on the PAL TMS9929A. Presenting the 288x240 framebuffer unstretched
    /// claimed otherwise.
    fn display(&self) -> Option<Display> {
        Display::television_for_region(
            self.profile().region,
            ti_tms9918::PAL_DOT_CLOCK_HZ,
            ti_tms9918::NTSC_DOT_CLOCK_HZ,
        )
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();

    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        self.machine
            .is_some()
            .then_some(&KEYBOARD as &dyn emu198x_shell::KeyboardTarget)
    }

    fn watch_target(&self) -> Option<&dyn emu198x_shell::WatchTarget> {
        self.machine
            .is_some()
            .then_some(self as &dyn emu198x_shell::WatchTarget)
    }
    fn watch_target_mut(&mut self) -> Option<&mut dyn emu198x_shell::WatchTarget> {
        if self.machine.is_some() {
            Some(self as &mut dyn emu198x_shell::WatchTarget)
        } else {
            None
        }
    }
}

emu198x_shell::impl_z80_debug_primitives!(EinsteinRuntime);

// AY register-write watch. The Einstein has no memory-write watch, so only the
// AY surface is implemented; the shared `watch_ay_*` tools drive it.
impl emu198x_shell::WatchTarget for EinsteinRuntime {
    fn supports_ay_watch(&self) -> bool {
        true
    }

    fn start_ay_watch(&mut self) -> Result<u32, emu198x_shell::WatchError> {
        match self.machine.as_mut() {
            Some(m) => Ok(m.start_ay_write_watch()),
            None => Err(emu198x_shell::WatchError::Unsupported),
        }
    }

    fn clear_ay_watch(&mut self) -> (bool, u32) {
        let Some(m) = self.machine.as_mut() else {
            return (false, 0);
        };
        let captured = m.ay_write_watch_records().map_or(0, |r| r.len() as u32);
        let had_watch = m.ay_write_watch_records().is_some();
        m.stop_ay_write_watch();
        (had_watch, captured)
    }

    fn ay_watch_records(&self) -> Option<Vec<emu198x_shell::WatchAyRecord>> {
        self.machine
            .as_ref()?
            .ay_write_watch_records()
            .map(|records| {
                records
                    .iter()
                    .map(|r| emu198x_shell::WatchAyRecord {
                        pc: u32::from(r.pc),
                        register: r.register,
                        value: r.value,
                    })
                    .collect()
            })
    }
}
