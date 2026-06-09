//! Runtime wrapper for the fresh-workspace NES baseline.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, DebugPrimitives, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, ResetKind, RunResult,
    StopReason,
};
use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::{ApuChannel, AudioControls, FB_HEIGHT, FB_WIDTH, Nes};

use crate::input::apply_input_event;
use crate::snapshot;
use crate::{Model, profile_for};

/// Firmwareless NES runtime over the concrete machine crate.
pub struct NesRuntime {
    profile: MachineProfile,
    machine: Option<Nes>,
    time: MachineTime,
    cartridge_bytes: Option<Vec<u8>>,
    cartridge_mapper: Option<u16>,
    rgba_framebuffer: Vec<u8>,
}

impl NesRuntime {
    /// Creates a blank runtime with no cartridge inserted.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            machine: None,
            time: MachineTime::default(),
            cartridge_bytes: None,
            cartridge_mapper: None,
            rgba_framebuffer: vec![0; (FB_WIDTH * FB_HEIGHT * 4) as usize],
        }
    }

    /// Returns the wrapped NES machine when a cartridge is loaded.
    #[must_use]
    pub fn machine(&self) -> Option<&Nes> {
        self.machine.as_ref()
    }

    /// Returns the wrapped NES machine mutably when a cartridge is loaded.
    pub fn machine_mut(&mut self) -> Option<&mut Nes> {
        self.machine.as_mut()
    }

    /// Current host-side APU audio controls, if a cartridge is loaded.
    #[must_use]
    pub fn audio_controls(&self) -> Option<AudioControls> {
        self.machine.as_ref().map(Nes::audio_controls)
    }

    /// Replace all host-side APU audio controls when a cartridge is loaded.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        if let Some(machine) = self.machine.as_mut() {
            machine.set_audio_controls(controls);
        }
    }

    /// Enable or disable one APU channel in the host mixer.
    pub fn set_audio_channel_enabled(&mut self, channel: ApuChannel, enabled: bool) {
        if let Some(machine) = self.machine.as_mut() {
            machine.set_audio_channel_enabled(channel, enabled);
        }
    }

    /// Set one APU channel's host mixer gain.
    pub fn set_audio_channel_gain(&mut self, channel: ApuChannel, gain: f32) {
        if let Some(machine) = self.machine.as_mut() {
            machine.set_audio_channel_gain(channel, gain);
        }
    }

    /// Cartridge mapper id (iNES header value) when a cartridge is
    /// loaded. Used by the query module for `nes.cartridge.mapper`,
    /// the snapshot envelope, and the MCP `query_mapper` tool.
    #[must_use]
    pub fn cartridge_mapper(&self) -> Option<u16> {
        self.cartridge_mapper
    }

    /// Raw cartridge image bytes. Used by the snapshot module to
    /// preserve the loaded cartridge across save/restore.
    #[must_use]
    pub(crate) fn cartridge_bytes(&self) -> Option<&[u8]> {
        self.cartridge_bytes.as_deref()
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn set_cartridge_bytes(&mut self, bytes: Option<Vec<u8>>) {
        self.cartridge_bytes = bytes;
    }

    pub(crate) fn set_cartridge_mapper(&mut self, mapper: Option<u16>) {
        self.cartridge_mapper = mapper;
    }

    pub(crate) fn set_machine(&mut self, machine: Option<Nes>) {
        self.machine = machine;
    }

    /// Repack the machine's framebuffer into the runtime's RGBA8888
    /// host buffer; called after restore so the next frame draw sees
    /// the post-snapshot contents instead of zeros.
    pub(crate) fn refresh_rgba_framebuffer(&mut self) {
        self.update_rgba_framebuffer();
    }

    fn load_cartridge_bytes(&mut self, slot: &str, bytes: &[u8]) -> Result<(), MachineError> {
        let parsed = parse_ines(bytes).map_err(|reason| MachineError::InvalidMedia {
            slot: slot.to_owned(),
            reason,
        })?;

        self.machine = Some(Nes::new(parsed.mapper));
        self.cartridge_bytes = Some(bytes.to_vec());
        self.cartridge_mapper = Some(parsed.header.mapper_number);
        self.time = MachineTime::default();
        self.rgba_framebuffer.fill(0);
        Ok(())
    }

    fn rebuild_loaded_machine(&mut self) {
        let Some(bytes) = self.cartridge_bytes.as_deref() else {
            self.machine = None;
            self.cartridge_mapper = None;
            self.rgba_framebuffer.fill(0);
            return;
        };

        match parse_ines(bytes) {
            Ok(parsed) => {
                self.machine = Some(Nes::new(parsed.mapper));
                self.cartridge_mapper = Some(parsed.header.mapper_number);
                self.rgba_framebuffer.fill(0);
            }
            Err(_) => {
                self.machine = None;
                self.cartridge_mapper = None;
                self.rgba_framebuffer.fill(0);
            }
        }
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

impl MachineCore for NesRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_loaded_machine();
        self.time = MachineTime::default();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            if image.slot.as_ref() != "cartridge-1" {
                return Err(MachineError::UnknownMediaSlot {
                    slot: image.slot.as_ref().to_owned(),
                });
            }

            if image.kind != MediaKind::Cartridge {
                return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
            }

            self.load_cartridge_bytes(image.slot.as_ref(), image.bytes)?;
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
                apply_input_event(machine, event);
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
                format: emu198x_shell::PixelFormat::Rgba8888,
                width: FB_WIDTH,
                height: FB_HEIGHT,
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
                sample_rate: 48_000,
                channels: 1,
                samples: &audio,
            })?;
        }

        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
    }

    fn run_ticks(&mut self, ticks: u64, host: &mut HostIo<'_>) -> Result<RunResult, MachineError> {
        if self.machine.is_none() {
            return Ok(RunResult::new(self.time, StopReason::WaitingForInput));
        }

        for event in host.input_events {
            if let Some(machine) = self.machine.as_mut() {
                apply_input_event(machine, event);
            }
        }

        {
            let machine = self.machine.as_mut().expect("machine checked above");
            for _ in 0..ticks {
                machine.tick();
            }
        }
        self.time = self.time.saturating_add(ticks);
        self.update_rgba_framebuffer();

        // Emit the (possibly mid-frame) framebuffer so screenshots and
        // the latest-frame query reflect post-tick state, and drain any
        // audio produced during the ticks.
        host.frame_sink.push_frame(FramePacket {
            timestamp: self.time,
            format: emu198x_shell::PixelFormat::Rgba8888,
            width: FB_WIDTH,
            height: FB_HEIGHT,
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
            sample_rate: 48_000,
            channels: 1,
            samples: &audio,
        })?;

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

// Minimal debug surface. NES is a memory-mapped 6502, so the wired debug verb is
// `memory_read`, served by the side-effect-free `Nes::peek`. This is hand-written
// rather than via `impl_6502_debug_primitives!` because `Nes` exposes `cpu` as a
// field and has no `poke`/`step_instruction`, which don't match the macro's
// `cpu()`/`poke()`/`step_instruction()` shape. poke/step/disasm are stubbed until
// a NES debugger needs them.
impl DebugPrimitives for NesRuntime {
    fn dbg_pc(&self) -> u32 {
        self.machine
            .as_ref()
            .map_or(0, |m| u32::from(m.cpu.regs.pc))
    }
    fn dbg_peek(&self, addr: u32) -> u8 {
        self.machine.as_ref().map_or(0xFF, |m| m.peek(addr as u16))
    }
    fn dbg_poke(&mut self, _addr: u32, _value: u8) {}
    fn dbg_cpu_state(&self) -> serde_json::Value {
        self.machine.as_ref().map_or_else(
            || serde_json::json!({}),
            |m| serde_json::json!({ "pc": format!("${:04X}", m.cpu.regs.pc) }),
        )
    }
    fn dbg_disassemble(&self, _addr: u32) -> Option<(String, u8)> {
        None
    }
    fn dbg_step(&mut self) -> u64 {
        0
    }
}
