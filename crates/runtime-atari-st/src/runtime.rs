//! The Atari ST runtime: the machine behind the shell's traits.
//!
//! The TOS image is kept so a hard reset, a model switch or a monitor change
//! can rebuild the machine; the disks in the drives move across to the new
//! one, so nothing written to them is lost. The machine's `0x00RRGGBB`
//! picture is converted to the host's RGBA after every frame. Time is
//! counted in CPU cycles.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, InputEvent,
    MachineCore, MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat,
    ResetKind, RunResult, StopReason,
};
use machine_atari_st::{AUDIO_SAMPLE_RATE, AtariSt, Monitor, disk_from_image};

use crate::Model;
use crate::input::{MOUSE_DEVICE, apply_input_event};
use crate::pointer::{self, GemPointer};
use crate::profiles::{TOS_FIRMWARE_ID, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

/// The floppy slots, drive A then drive B.
pub(crate) const FLOPPY_SLOTS: [&str; 2] = ["floppy-0", "floppy-1"];

pub struct AtariStRuntime {
    model: Model,
    monitor: Monitor,
    profile: MachineProfile,
    pub(crate) machine: Option<AtariSt>,
    tos: Option<Vec<u8>>,
    pub(crate) time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    /// Whether GEM's pointer variables follow the mouse. Not saved.
    gem_pointer: GemPointer,
}

impl AtariStRuntime {
    /// A runtime with no TOS, on a colour monitor.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            model,
            monitor: Monitor::Colour,
            profile: profile_for(model),
            machine: None,
            tos: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            gem_pointer: GemPointer::default(),
        }
    }

    /// A runtime running `tos`, powered on, on a colour monitor.
    ///
    /// # Errors
    ///
    /// [`MachineError::InvalidFirmware`] when `tos` is not 192 KB or 256 KB.
    pub fn new(model: Model, tos: Vec<u8>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.install_tos(tos)?;
        Ok(runtime)
    }

    /// Install `tos` and power on.
    ///
    /// # Errors
    ///
    /// [`MachineError::InvalidFirmware`] when `tos` is not 192 KB or 256 KB.
    pub fn install_tos(&mut self, tos: Vec<u8>) -> Result<(), MachineError> {
        let machine = AtariSt::new(
            self.model.machine_model(),
            self.model.standard(),
            self.monitor,
            tos.clone(),
        )
        .map_err(|error| MachineError::InvalidFirmware {
            id: TOS_FIRMWARE_ID.to_owned(),
            reason: error.to_string(),
        })?;
        self.tos = Some(tos);
        self.install(machine);
        Ok(())
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    #[must_use]
    pub fn monitor(&self) -> Monitor {
        self.monitor
    }

    /// Plug in a different monitor: the machine powers off and on, as it
    /// must, since TOS reads the monitor type only at boot.
    pub fn set_monitor(&mut self, monitor: Monitor) {
        if monitor != self.monitor {
            self.monitor = monitor;
            self.rebuild_machine();
            self.time = MachineTime::default();
        }
    }

    #[must_use]
    pub fn machine(&self) -> Option<&AtariSt> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut AtariSt> {
        self.machine.as_mut()
    }

    /// The model id recorded in snapshots.
    #[must_use]
    pub fn model_id(&self) -> &'static str {
        self.model.model_id()
    }

    pub(crate) fn tos(&self) -> Option<&[u8]> {
        self.tos.as_deref()
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    /// Install a machine restored from a snapshot, giving it the TOS this
    /// runtime holds. A snapshot does not carry TOS.
    pub(crate) fn set_restored_machine(
        &mut self,
        mut machine: AtariSt,
    ) -> Result<(), MachineError> {
        let tos = self.tos.clone().ok_or(MachineError::MissingFirmware {
            id: TOS_FIRMWARE_ID.to_owned(),
        })?;
        machine
            .finish_restore(tos)
            .map_err(|error| MachineError::InvalidFirmware {
                id: TOS_FIRMWARE_ID.to_owned(),
                reason: error.to_string(),
            })?;
        self.monitor = machine.monitor();
        self.install(machine);
        Ok(())
    }

    fn install(&mut self, machine: AtariSt) {
        self.machine = Some(machine);
        self.gem_pointer = GemPointer::default();
        self.update_rgba_framebuffer();
    }

    /// Power off and on with the same TOS, moving the disks across.
    fn rebuild_machine(&mut self) {
        let Some(tos) = self.tos.clone() else {
            self.machine = None;
            return;
        };
        let mut disks = [None, None];
        if let Some(old) = self.machine.as_mut() {
            for (drive, disk) in disks.iter_mut().enumerate() {
                *disk = old.eject_disk(drive);
            }
        }
        let Ok(mut machine) = AtariSt::new(
            self.model.machine_model(),
            self.model.standard(),
            self.monitor,
            tos,
        ) else {
            // The image was checked when it was installed.
            self.machine = None;
            return;
        };
        for (drive, disk) in disks.into_iter().enumerate() {
            if let Some(disk) = disk {
                machine.insert_disk(drive, disk);
            }
        }
        self.install(machine);
    }

    /// Insert an `.st` or `.msa` image into `drive` (0 = A, 1 = B).
    ///
    /// # Errors
    ///
    /// [`MachineError::InvalidMedia`] for an image that cannot be read, and
    /// [`MachineError::MissingFirmware`] with no TOS to run it.
    pub fn insert_disk(&mut self, drive: usize, image: &[u8]) -> Result<(), MachineError> {
        let slot = FLOPPY_SLOTS[drive.min(1)];
        let disk = disk_from_image(image).map_err(|error| MachineError::InvalidMedia {
            slot: slot.to_owned(),
            reason: error.to_string(),
        })?;
        let machine = self.machine.as_mut().ok_or(MachineError::MissingFirmware {
            id: TOS_FIRMWARE_ID.to_owned(),
        })?;
        machine.insert_disk(drive, disk);
        Ok(())
    }

    /// Convert the machine's `0x00RRGGBB` picture to packed RGBA bytes. The
    /// size follows the machine, which changes when TOS switches between the
    /// colour and the monochrome picture.
    pub(crate) fn update_rgba_framebuffer(&mut self) {
        let Some(machine) = self.machine.as_ref() else {
            self.rgba_framebuffer.fill(0);
            return;
        };
        let (width, height) = (machine.frame_width() as u32, machine.frame_height() as u32);
        if (width, height) != (self.rgba_width, self.rgba_height) {
            self.rgba_width = width;
            self.rgba_height = height;
            self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        }
        for (rgba, &pixel) in self
            .rgba_framebuffer
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(machine.framebuffer())
        {
            *rgba = [(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8, 0xFF];
        }
    }

    /// Where GEM's mouse pointer is, in framebuffer pixels, or `None` when
    /// it is hidden or GEM's pointer variables have not been seen to follow
    /// the mouse, as when a game has the mouse to itself.
    #[must_use]
    pub fn mouse_pointer(&self) -> Option<(f64, f64)> {
        self.gem_pointer.position(self.machine.as_ref()?)
    }

    /// Mouse counts that move GEM's pointer one framebuffer pixel across and
    /// down in the current resolution.
    #[must_use]
    pub fn mouse_counts_per_pixel(&self) -> (f64, f64) {
        self.machine
            .as_ref()
            .map_or((1.0, 1.0), pointer::counts_per_pixel)
    }

    fn drive_for_slot(slot: &str) -> Result<usize, MachineError> {
        FLOPPY_SLOTS
            .iter()
            .position(|&s| s == slot)
            .ok_or_else(|| MachineError::UnknownMediaSlot {
                slot: slot.to_owned(),
            })
    }
}

impl emu198x_shell::FamilyRuntime for AtariStRuntime {
    type Model = Model;
    fn variant_ids() -> &'static [&'static str] {
        &Model::VARIANT_IDS
    }
    fn model_from_id(id: &str) -> Option<Model> {
        Model::from_variant_id(id)
    }
    fn variant_id(model: Model) -> &'static str {
        model.variant_id()
    }
    fn profile_for(model: Model) -> MachineProfile {
        profile_for(model)
    }
    fn rom_convention() -> emu198x_shell::RomConvention {
        emu198x_shell::RomConvention {
            env_var: Some("EMU198X_ATARI_ST_ROM_DIR"),
            dirs: &["atari-st"],
        }
    }
    fn firmware_sources(model: Model) -> Vec<emu198x_shell::FirmwareSource> {
        model.firmware_sources()
    }
    fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        firmware.validate_for_profile(&profile_for(model))?;
        let tos = firmware
            .bytes(TOS_FIRMWARE_ID)
            .ok_or(MachineError::MissingFirmware {
                id: TOS_FIRMWARE_ID.to_owned(),
            })?;
        Self::new(model, tos.to_vec())
    }
    /// The machine is not `Clone`, so a switch cold-boots the new model with
    /// the same monitor, and the disks move across.
    fn replacement(&self, model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let mut replacement = Self::from_firmware(model, firmware)?;
        replacement.set_monitor(self.monitor);
        if let (Some(old), Some(new)) = (self.machine.as_ref(), replacement.machine.as_mut()) {
            for drive in 0..2 {
                if let Some(disk) = old.disk(drive) {
                    new.insert_disk(drive, disk.clone());
                }
            }
        }
        Ok(replacement)
    }
    fn native_frame_ticks(&self) -> u64 {
        self.model.frame_ticks(self.monitor)
    }
}

impl AtariStRuntime {
    fn run_window(
        &mut self,
        target: MachineTime,
        one_frame: bool,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        let Some(machine) = self.machine.as_mut() else {
            return Ok(RunResult::new(self.time, StopReason::WaitingForInput));
        };
        for event in host.input_events {
            apply_input_event(machine, event);
            if let InputEvent::PointerMotion { device, dx, dy } = event
                && device.as_ref() == MOUSE_DEVICE
            {
                self.gem_pointer.sent(*dx, *dy);
            }
        }

        while self.time < target {
            let ticks = self
                .machine
                .as_mut()
                .expect("machine checked above")
                .run_frame();
            self.time = self.time.saturating_add(ticks);
            if let Some(machine) = self.machine.as_ref() {
                self.gem_pointer.after_frame(machine);
            }
            self.update_rgba_framebuffer();

            host.frame_sink.push_frame(FramePacket {
                signal: None,
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
                .take_audio();
            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: AUDIO_SAMPLE_RATE,
                channels: 1,
                samples: &audio,
            })?;
            if one_frame {
                break;
            }
        }

        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
    }
}

impl MachineCore for AtariStRuntime {
    fn set_machine<Q: emu198x_shell::SessionQueryProvider<Self>>(
        session: &mut emu198x_shell::HeadlessSession<Self, Q>,
        machine: &str,
    ) -> Result<emu198x_shell::VariantSwitched, emu198x_shell::LoaderError> {
        emu198x_shell::swap_variant(session, machine)
    }

    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    /// A soft reset is the reset button: the 68000 restarts and the reset
    /// line clears the peripherals, but RAM keeps what it had. Any other
    /// powers the machine off and on.
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Soft {
            if let Some(machine) = self.machine.as_mut() {
                machine.reset();
            }
        } else {
            self.rebuild_machine();
            self.time = MachineTime::default();
        }
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            let drive = Self::drive_for_slot(image.slot.as_ref())?;
            if image.kind != MediaKind::Disk {
                return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
            }
            self.insert_disk(drive, image.bytes)?;
        }
        Ok(())
    }

    fn eject_media(&mut self, slot: &str) -> Result<(), MachineError> {
        let drive = Self::drive_for_slot(slot)?;
        if let Some(machine) = self.machine.as_mut() {
            machine.eject_disk(drive);
        }
        Ok(())
    }

    /// A drive's disk as it now stands, writes included: `st` gives the raw
    /// sectors, `msa` a Magic Shadow archive of them.
    fn export_media(&self, slot: &str, format: &str) -> Result<Vec<u8>, MachineError> {
        let drive = Self::drive_for_slot(slot)?;
        let disk = self
            .machine
            .as_ref()
            .and_then(|machine| machine.disk(drive))
            .ok_or_else(|| MachineError::InvalidMedia {
                slot: slot.to_owned(),
                reason: "the drive is empty".to_owned(),
            })?;
        match format {
            "st" => Ok(disk.data().to_vec()),
            "msa" => {
                machine_atari_st::msa_from_disk(disk).ok_or_else(|| MachineError::InvalidMedia {
                    slot: slot.to_owned(),
                    reason: "the disk's geometry has no .msa form".to_owned(),
                })
            }
            _ => Err(MachineError::UnsupportedOperation {
                operation: "export_media: the formats are st and msa",
            }),
        }
    }

    fn run_frame(&mut self, host: &mut HostIo<'_>) -> Result<RunResult, MachineError> {
        self.run_window(MachineTime::new(u64::MAX), true, host)
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        self.run_window(target, false, host)
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

    /// A colour ST drives a television or a colour monitor at the TV's
    /// rates: the framebuffer's 768 pixels a line are 16 MHz, twice the CPU
    /// clock. The SM124 monochrome monitor shows its 640 × 400 raster on a
    /// tube of the same 16:10 shape, so its pixels are square.
    fn display(&self) -> Option<Display> {
        if self
            .machine
            .as_ref()
            .is_some_and(|m| m.frame_width() == 640)
        {
            return Some(Display::Monitor { aspect: 1.6 });
        }
        let pixel_clock = 2.0 * self.model.cpu_hz() as f64;
        Display::television_for_region(self.profile().region, pixel_clock, pixel_clock)
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();
}
