//! Runtime wrapper for the Atari 2600.
//!
//! Each frame drains the TIA's two-channel audio (≈31.4 kHz NTSC / ≈31.2 kHz
//! PAL) and pushes it at its native rate; the host resamples to the output
//! device rate.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use machine_atari_2600::{Atari2600, Atari2600Region};

use crate::input::{ControllerCache, apply_input_event};
use crate::profiles::{Model, profile_for};
use crate::snapshot;

pub struct Atari2600Runtime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Atari2600>,
    cart_bytes: Option<Vec<u8>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    controller_cache: ControllerCache,
}

impl Atari2600Runtime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            cart_bytes: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            controller_cache: ControllerCache::default(),
        }
    }

    /// Build directly from a cartridge ROM image.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidMedia` if the cart fails to parse.
    pub fn new(model: Model, cart_rom: Vec<u8>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.insert_cartridge(cart_rom)?;
        Ok(runtime)
    }

    /// Insert a cartridge (replaces any existing); rebuilds the machine.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidMedia` if the cart fails to parse.
    pub fn insert_cartridge(&mut self, rom: Vec<u8>) -> Result<(), MachineError> {
        self.cart_bytes = Some(rom);
        self.rebuild_machine()
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Atari2600> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Atari2600> {
        self.machine.as_mut()
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn cart_bytes(&self) -> Option<&[u8]> {
        self.cart_bytes.as_deref()
    }

    /// Install a machine restored from a snapshot, re-deriving the host RGBA
    /// framebuffer from its live state. Replaces the cold-boot rebuild on the
    /// restore path so the resumed machine keeps its CPU/TIA/RIOT/cart state.
    /// The rgba sizing mirrors [`Self::rebuild_machine`] exactly (the visible
    /// window, not the full raster) so a restore into a `blank()` runtime — whose
    /// framebuffer Vec starts empty — does not panic on the repaint.
    pub(crate) fn set_machine(&mut self, machine: Option<Atari2600>) {
        if let Some(machine) = &machine {
            self.rgba_width = machine.visible_framebuffer_width();
            self.rgba_height = machine.visible_framebuffer_height();
            self.rgba_framebuffer = vec![0; (self.rgba_width * self.rgba_height * 4) as usize];
        }
        self.machine = machine;
        self.update_rgba_framebuffer();
    }

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        let Some(rom) = self.cart_bytes.clone() else {
            self.machine = None;
            return Ok(());
        };
        let region = match self.model.region() {
            emu198x_shell::Region::Pal => Atari2600Region::Pal,
            _ => Atari2600Region::Ntsc,
        };
        let machine = Atari2600::new(rom, region).map_err(|reason| MachineError::InvalidMedia {
            slot: "cartridge-1".to_owned(),
            reason,
        })?;
        // Display the visible window only. The TIA's framebuffer is the full
        // 228-clock × full-frame raster, but the leading 68-clock HBLANK and the
        // VSYNC/VBLANK/overscan lines are blanking that must not be shown (they
        // would band the picture in black and shove it right). Crop to the 160
        // visible columns and the region's visible scanline window.
        self.rgba_width = machine.visible_framebuffer_width();
        self.rgba_height = machine.visible_framebuffer_height();
        self.rgba_framebuffer = vec![0; (self.rgba_width * self.rgba_height * 4) as usize];
        self.machine = Some(machine);
        self.update_rgba_framebuffer();
        Ok(())
    }

    fn update_rgba_framebuffer(&mut self) {
        let Some(machine) = self.machine.as_ref() else {
            self.rgba_framebuffer.fill(0);
            return;
        };
        let source = machine.framebuffer();
        let full_width = machine.framebuffer_width() as usize;
        let hblank = machine.hblank_clocks() as usize;
        let first_line = machine.visible_first_line() as usize;
        let visible = self.rgba_width as usize;
        let height = self.rgba_height as usize;
        for y in 0..height {
            let row = (first_line + y) * full_width + hblank;
            for x in 0..visible {
                let pixel = source[row + x];
                let base = (y * visible + x) * 4;
                self.rgba_framebuffer[base] = ((pixel >> 16) & 0xff) as u8;
                self.rgba_framebuffer[base + 1] = ((pixel >> 8) & 0xff) as u8;
                self.rgba_framebuffer[base + 2] = (pixel & 0xff) as u8;
                self.rgba_framebuffer[base + 3] = ((pixel >> 24) & 0xff) as u8;
            }
        }
    }
}

impl MachineCore for Atari2600Runtime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        let _ = self.rebuild_machine();
        self.time = MachineTime::default();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            match (image.slot.as_ref(), image.kind) {
                ("cartridge-1", MediaKind::Cartridge) => {
                    self.insert_cartridge(image.bytes.to_vec())?;
                }
                (slot, MediaKind::Cartridge) => {
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
            let machine = self.machine.as_mut().expect("machine checked above");
            let ticks = machine.run_frame();
            // Drain this frame's TIA audio before re-borrowing self for the
            // framebuffer update.
            let audio_samples = machine.take_audio_samples();
            let sample_rate = machine.audio_sample_rate();
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

            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate,
                channels: 1,
                samples: &audio_samples,
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

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();
}

emu198x_shell::impl_6502_debug_primitives!(Atari2600Runtime);
