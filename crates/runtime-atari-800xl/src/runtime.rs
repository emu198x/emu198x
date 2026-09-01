//! Runtime wrapper for the Atari 800XL.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use format_atari_8bit_atr::AtrImage;
use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

use crate::profiles::{Model, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

const AUDIO_SAMPLE_RATE: u32 = 48_000;
const XEX_AUTOLOAD_FRAME: u64 = 150;
const INITAD: u16 = 0x02E2;
const RUNAD: u16 = 0x02E0;
const INIT_RETURN_STUB: u16 = 0xE4C0;
const INIT_MAX_TICKS: u64 = 10_000_000;

pub struct Atari800xlRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Atari800xl>,
    os_bytes: Option<Vec<u8>>,
    basic_bytes: Option<Vec<u8>>,
    cart_bytes: Option<Vec<u8>>,
    xex_bytes: Option<Vec<u8>>,
    xex_pending: bool,
    /// A disk loaded before there was a machine to put it in. Once a machine
    /// exists the drive on its SIO bus owns the disk, writes and all.
    disk_1: Option<AtrImage>,
    basic_enabled: bool,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    controller_cache: crate::input::ControllerCache,
}

impl Atari800xlRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            os_bytes: None,
            basic_bytes: None,
            cart_bytes: None,
            xex_bytes: None,
            xex_pending: false,
            disk_1: None,
            basic_enabled: false,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            controller_cache: crate::input::ControllerCache::default(),
        }
    }

    /// Build with optional OS / BASIC / cart.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidMedia` if the cart fails to parse.
    pub fn new(
        model: Model,
        os: Option<Vec<u8>>,
        basic: Option<Vec<u8>>,
        cart: Option<Vec<u8>>,
        basic_enabled: bool,
    ) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.os_bytes = os;
        runtime.basic_bytes = basic;
        runtime.cart_bytes = cart;
        runtime.basic_enabled = basic_enabled;
        runtime.rebuild_machine()?;
        Ok(runtime)
    }

    pub fn set_os(&mut self, os: Option<Vec<u8>>) -> Result<(), MachineError> {
        self.os_bytes = os;
        self.rebuild_machine()
    }

    pub fn set_basic(&mut self, basic: Option<Vec<u8>>) -> Result<(), MachineError> {
        self.basic_bytes = basic;
        self.rebuild_machine()
    }

    pub fn set_basic_enabled(&mut self, enabled: bool) -> Result<(), MachineError> {
        self.basic_enabled = enabled;
        self.rebuild_machine()
    }

    pub fn insert_cartridge(&mut self, rom: Option<Vec<u8>>) -> Result<(), MachineError> {
        self.cart_bytes = rom;
        self.rebuild_machine()
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Atari800xl> {
        self.machine.as_ref()
    }
    pub fn machine_mut(&mut self) -> Option<&mut Atari800xl> {
        self.machine.as_mut()
    }
    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    /// Install a machine restored from a snapshot, re-deriving the host RGBA
    /// framebuffer from its live state. Replaces the cold-boot rebuild on the
    /// restore path so the resumed machine keeps its CPU/ANTIC/GTIA/POKEY/PIA
    /// and 64 KB RAM state. The framebuffer sizing mirrors `rebuild_machine`
    /// exactly (same `framebuffer_width()` / `framebuffer_height()` getters)
    /// so a runtime whose `blank()` starts with an empty framebuffer Vec does
    /// not panic when the first frame paints.
    pub(crate) fn set_machine(&mut self, machine: Option<Atari800xl>) {
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

    pub(crate) fn os_bytes(&self) -> Option<&[u8]> {
        self.os_bytes.as_deref()
    }
    pub(crate) fn basic_bytes(&self) -> Option<&[u8]> {
        self.basic_bytes.as_deref()
    }
    pub(crate) fn cart_bytes(&self) -> Option<&[u8]> {
        self.cart_bytes.as_deref()
    }
    pub(crate) fn xex_bytes(&self) -> Option<&[u8]> {
        self.xex_bytes.as_deref()
    }
    /// Whether D1: has a disk in it — held here until a machine exists,
    /// then in the drive on the machine's SIO bus.
    pub(crate) fn disk_in_d1(&self) -> bool {
        self.disk_1.is_some()
            || self
                .machine
                .as_ref()
                .and_then(|machine| machine.sio().drive(1))
                .is_some_and(|drive| drive.has_disk())
    }
    pub(crate) fn xex_pending(&self) -> bool {
        self.xex_pending
    }
    pub(crate) fn set_xex(&mut self, bytes: Option<Vec<u8>>, pending: bool) {
        self.xex_bytes = bytes;
        self.xex_pending = pending;
    }
    pub(crate) fn basic_enabled(&self) -> bool {
        self.basic_enabled
    }

    /// Put `disk` in D1:, or hold it until there is a machine to put it in.
    fn insert_disk(&mut self, disk: AtrImage) {
        match self.machine.as_mut() {
            Some(machine) => machine.sio_mut().insert_disk(1, disk),
            None => self.disk_1 = Some(disk),
        }
    }

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        // The drive keeps its disk across a reset of the computer, so carry
        // the live image over rather than reloading the bytes it came from.
        let disk = self
            .machine
            .as_mut()
            .and_then(|machine| machine.sio_mut().eject_disk(1))
            .or_else(|| self.disk_1.take());
        // The 800XL needs at least a cart OR an OS to boot meaningfully.
        if self.cart_bytes.is_none() && self.os_bytes.is_none() {
            self.machine = None;
            self.disk_1 = disk;
            return Ok(());
        }
        let region = match self.model.region() {
            emu198x_shell::Region::Pal => Atari800xlRegion::Pal,
            _ => Atari800xlRegion::Ntsc,
        };
        let mut machine = Atari800xl::new(
            self.os_bytes.clone(),
            self.basic_bytes.clone(),
            self.cart_bytes.clone(),
            region,
            self.basic_enabled,
        )
        .map_err(|reason| MachineError::InvalidMedia {
            slot: "cartridge-1".to_owned(),
            reason,
        })?;
        if let Some(disk) = disk {
            machine.sio_mut().insert_disk(1, disk);
        }
        let width = machine.framebuffer_width();
        let height = machine.framebuffer_height();
        self.rgba_width = width;
        self.rgba_height = height;
        self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        self.machine = Some(machine);
        self.update_rgba_framebuffer();
        Ok(())
    }

    fn autoload_xex(&mut self) -> Result<(), MachineError> {
        let Some(bytes) = self.xex_bytes.as_deref() else {
            return Ok(());
        };
        let xex = format198x_atari_8bit_xex::parse(bytes).map_err(|reason| {
            MachineError::InvalidMedia {
                slot: "program-1".to_owned(),
                reason: reason.to_string(),
            }
        })?;
        let machine = self.machine.as_mut().ok_or(MachineError::InvalidMedia {
            slot: "program-1".to_owned(),
            reason: "an OS ROM or cartridge is required before an XEX can run".to_owned(),
        })?;

        // DOS begins with an RTS stub in INITAD and defaults RUNAD to the
        // first segment's start. A segment may install INITAD; run it before
        // loading the following segment, then restore the stub.
        machine.load_program_byte(INITAD, INIT_RETURN_STUB as u8);
        machine.load_program_byte(INITAD + 1, (INIT_RETURN_STUB >> 8) as u8);
        let first_start = xex.segments[0].start;
        machine.load_program_byte(RUNAD, first_start as u8);
        machine.load_program_byte(RUNAD + 1, (first_start >> 8) as u8);

        for segment in &xex.segments {
            for (offset, &byte) in segment.data.iter().enumerate() {
                machine.load_program_byte(segment.start.wrapping_add(offset as u16), byte);
            }
            let init = u16::from(machine.peek(INITAD)) | (u16::from(machine.peek(INITAD + 1)) << 8);
            if init != INIT_RETURN_STUB {
                if !machine.call_loaded_subroutine(init, INIT_MAX_TICKS) {
                    let pc = machine.cpu().regs.pc;
                    let sp = machine.cpu().regs.sp;
                    return Err(MachineError::InvalidMedia {
                        slot: "program-1".to_owned(),
                        reason: format!(
                            "INIT routine at ${init:04X} did not return (PC=${pc:04X}, SP=${sp:02X})"
                        ),
                    });
                }
                machine.load_program_byte(INITAD, INIT_RETURN_STUB as u8);
                machine.load_program_byte(INITAD + 1, (INIT_RETURN_STUB >> 8) as u8);
            }
        }

        let run = u16::from(machine.peek(RUNAD)) | (u16::from(machine.peek(RUNAD + 1)) << 8);
        if !machine.launch_loaded_program(run, INIT_MAX_TICKS) {
            return Err(MachineError::InvalidMedia {
                slot: "program-1".to_owned(),
                reason: format!("could not enter RUN routine at ${run:04X}"),
            });
        }
        self.xex_pending = false;
        Ok(())
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

impl MachineCore for Atari800xlRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }
    fn time(&self) -> MachineTime {
        self.time
    }
    fn reset(&mut self, _kind: ResetKind) {
        let _ = self.rebuild_machine();
        self.xex_pending = self.xex_bytes.is_some();
        self.time = MachineTime::default();
    }
    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            match (image.slot.as_ref(), image.kind) {
                ("cartridge-1", MediaKind::Cartridge) => {
                    self.insert_cartridge(Some(image.bytes.to_vec()))?;
                }
                (slot, MediaKind::Cartridge) => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                ("program-1", MediaKind::Program) => {
                    format198x_atari_8bit_xex::parse(image.bytes).map_err(|reason| {
                        MachineError::InvalidMedia {
                            slot: "program-1".to_owned(),
                            reason: reason.to_string(),
                        }
                    })?;
                    self.xex_bytes = Some(image.bytes.to_vec());
                    self.xex_pending = true;
                }
                (slot, MediaKind::Program) => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                ("disk-1", MediaKind::Disk) => {
                    let disk = AtrImage::parse(image.bytes).map_err(|reason| {
                        MachineError::InvalidMedia {
                            slot: "disk-1".to_owned(),
                            reason: reason.to_string(),
                        }
                    })?;
                    self.insert_disk(disk);
                }
                (slot, MediaKind::Disk) => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                (_, kind) => {
                    return Err(MachineError::UnsupportedMediaKind { kind });
                }
            }
        }
        Ok(())
    }
    fn eject_media(&mut self, slot: &str) -> Result<(), MachineError> {
        match slot {
            "disk-1" => {
                self.disk_1 = None;
                if let Some(machine) = self.machine.as_mut() {
                    machine.sio_mut().eject_disk(1);
                }
                Ok(())
            }
            other => Err(MachineError::UnknownMediaSlot {
                slot: other.to_owned(),
            }),
        }
    }
    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        if self.machine.is_none() {
            return Ok(RunResult::new(self.time, StopReason::WaitingForInput));
        }
        // Apply queued host input (keyboard + joystick) before running the
        // frame batch. A key press latches in POKEY and persists across these
        // frames; a later call delivers the matching release event.
        if let Some(machine) = self.machine.as_mut() {
            for event in host.input_events {
                crate::input::apply_input_event(machine, &mut self.controller_cache, event);
            }
        }
        while self.time < target {
            let ticks = self
                .machine
                .as_mut()
                .expect("machine checked above")
                .run_frame();
            self.time = self.time.saturating_add(ticks);
            if self.xex_pending
                && self
                    .machine
                    .as_ref()
                    .is_some_and(|machine| machine.frame_count() >= XEX_AUTOLOAD_FRAME)
            {
                self.autoload_xex()?;
            }
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
    /// Two pixels per colour clock in the hires modes gives 6:7 on NTSC — the
    /// Atari 8-bit's published ratio, and taller than it is wide.
    fn display(&self) -> Option<Display> {
        Display::television_for_region(
            self.profile().region,
            atari_gtia::PAL_PIXEL_CLOCK_HZ,
            atari_gtia::NTSC_PIXEL_CLOCK_HZ,
        )
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();

    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        self.machine
            .is_some()
            .then_some(self as &dyn emu198x_shell::KeyboardTarget)
    }
}

emu198x_shell::impl_6502_debug_primitives!(Atari800xlRuntime);

// Keyboard description for the shared `press_key` / `type_string` tools. The
// 800XL types a character by pressing the keycap of the same name (newline →
// `Return`); the input layer drops names it doesn't recognise, so any
// single character is accepted. Hold 3 / settle 6 match the prior tool.
impl emu198x_shell::KeyboardTarget for Atari800xlRuntime {
    fn key_name_is_valid(&self, name: &str) -> bool {
        !name.is_empty()
    }

    fn key_names_hint(&self) -> &'static str {
        "a single character (case-insensitive) or Return"
    }

    fn keys_for_char(&self, ch: char) -> Option<Vec<String>> {
        Some(vec![if ch == '\n' || ch == '\r' {
            "Return".to_owned()
        } else {
            ch.to_string()
        }])
    }

    fn key_timing(&self) -> emu198x_shell::KeyTiming {
        emu198x_shell::KeyTiming {
            default_hold_frames: 3,
            max_hold_frames: 600,
            press_settle_frames: 6,
            inter_key_settle_frames: 6,
            repeat_settle_frames: 6,
            default_type_settle_frames: 0,
        }
    }
}
