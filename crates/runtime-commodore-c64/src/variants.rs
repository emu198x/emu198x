//! The machines the C64 family runtime can host, and what each must offer.
//!
//! [`C64FamilyRuntime`] is generic over a [`C64FamilyMachine`], per
//! `knowledge/decisions/runtime-internal-shape.md`: the frame loop, the
//! snapshot envelope, the shared query paths, keyboard input and the debug
//! target are written once, and each machine answers the trait. The C64 and
//! the C128 share the 6502-family CPU, the VIC-II, both CIAs, the SID and the
//! keyboard matrix, which is what the required methods cover.
//!
//! What only one machine has comes in two kinds. Machine hardware the other
//! lacks (the C64's tape deck, user port and expansion port) is a provided
//! method that does nothing by default. Runtime features built on it (media
//! slots, transport commands, the BASIC and tape loaders, extra query paths)
//! are hooks that take the whole runtime, so each machine's version is
//! written against its own concrete runtime type and can reach that type's
//! own methods.

use common_commodore_c64::timing::C64Timing;
use common_commodore_iec::IecBus;
use emu198x_mos_6502::M6502;
use emu198x_mos_cia_6526::Cia6526;
use emu198x_mos_vic_ii::Vic;
use emu198x_shell::{
    BasicProgramLoaded, ControlCommand, FirmwareSet, FirmwareSource, HeadlessSession, InputEvent,
    LoaderError, MachineError, MachineProfile, MediaImage, RomConvention, SessionQueryProvider,
    TapeAutoloaded,
};
use machine_commodore_c64::{C64, C64Config, C64Snapshot, KeyboardMatrix};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Model;
use crate::runtime::{C64FamilyRuntime, C64Runtime};

/// A model of one machine in the family: its identity, timing and firmware.
pub trait C64FamilyModel: Copy + Eq + std::fmt::Debug + 'static {
    /// Every short variant id, in menu order.
    fn variant_ids() -> &'static [&'static str];
    /// The model a variant id names.
    fn from_variant_id(id: &str) -> Option<Self>;
    /// This model's short variant id.
    fn variant_id(self) -> &'static str;
    /// The shell profile describing this model.
    fn profile(self) -> MachineProfile;
    /// The board timing.
    fn timing(self) -> C64Timing;
    /// Whether the model is a PAL machine.
    fn is_pal(self) -> bool;
    /// Where the firmware lookup finds each ROM.
    fn firmware_sources(self) -> Vec<FirmwareSource>;
}

impl C64FamilyModel for Model {
    fn variant_ids() -> &'static [&'static str] {
        &Self::VARIANT_IDS
    }

    fn from_variant_id(id: &str) -> Option<Self> {
        Self::from_variant_id(id)
    }

    fn variant_id(self) -> &'static str {
        self.variant_id()
    }

    fn profile(self) -> MachineProfile {
        crate::profile_for(self)
    }

    fn timing(self) -> C64Timing {
        *self.timing()
    }

    fn is_pal(self) -> bool {
        matches!(self, Self::C64PalBreadbin | Self::C64cPal)
    }

    fn firmware_sources(self) -> Vec<FirmwareSource> {
        self.firmware_sources()
    }
}

/// A machine the C64 family runtime can host.
pub trait C64FamilyMachine: Sized + 'static {
    /// The models of this machine.
    type Model: C64FamilyModel;
    /// The ROM set, owned, kept by the runtime so a reset can rebuild.
    type Roms: Clone;
    /// The machine's state as a snapshot carries it.
    type Snapshot: Serialize + DeserializeOwned;

    /// Where the shell looks for this machine's ROMs.
    fn rom_convention() -> RomConvention;

    /// Build a powered-on machine.
    ///
    /// # Errors
    ///
    /// A ROM the machine refuses.
    fn build(model: Self::Model, roms: &Self::Roms) -> Result<Self, MachineError>;

    /// Zero-filled ROMs of the right sizes, for a runtime that only needs
    /// to exist.
    fn blank_roms() -> Self::Roms;

    /// A runtime from the firmware the shell resolved.
    ///
    /// # Errors
    ///
    /// Missing or invalid firmware.
    fn runtime_from_firmware(
        model: Self::Model,
        firmware: &FirmwareSet<'_>,
    ) -> Result<C64FamilyRuntime<Self>, MachineError>;

    /// One phi2 cycle; `true` when it finished a frame.
    fn tick(&mut self) -> bool;
    /// Phi2 cycles since power-on.
    fn phi2_cycles(&self) -> u64;
    /// Frames since power-on.
    fn frame_count(&self) -> u64;
    /// The board timing.
    fn timing(&self) -> C64Timing;
    /// The main CPU.
    fn cpu(&self) -> &M6502;
    /// The VIC-II.
    fn vic(&self) -> &Vic;
    /// CIA 1.
    fn cia1(&self) -> &Cia6526;
    /// CIA 2.
    fn cia2(&self) -> &Cia6526;
    /// A VIC register without side effects.
    fn vic_register(&self, index: u8) -> u8;
    /// The VIC's raster line.
    fn raster_line(&self) -> u16;
    /// The cycle within the raster line.
    fn cycle_in_line(&self) -> u8;
    /// The VIC's picture.
    fn framebuffer(&self) -> &[u32];
    /// Drain the mixed audio.
    fn take_audio_buffer(&mut self) -> Vec<f32>;
    /// The audio sample rate, in Hz.
    fn audio_sample_rate(&self) -> u32;
    /// A byte of bank 0 RAM, where both machines keep the 40-column screen.
    fn ram_read(&self, addr: u16) -> u8;
    /// Load a PRG into RAM, returning its load address.
    ///
    /// # Errors
    ///
    /// A malformed PRG.
    fn load_prg(&mut self, data: &[u8]) -> Result<u16, String>;
    /// The keyboard matrix.
    fn keyboard_mut(&mut self) -> &mut KeyboardMatrix;
    /// Apply one host input event.
    fn apply_input_event(&mut self, event: &InputEvent);
    /// The machine's snapshot state.
    fn snapshot_state(&self) -> Self::Snapshot;
    /// Restore the machine from snapshot state.
    ///
    /// # Errors
    ///
    /// State the machine cannot take.
    fn restore_snapshot_state(&mut self, snapshot: Self::Snapshot) -> Result<(), String>;

    /// Drain diagnostic audio stems produced beside the mix.
    fn drain_audio_stems(&mut self) {}
    /// One phi2 cycle with drives on the serial bus.
    fn tick_with_iec_bus(&mut self, _bus: &mut IecBus) -> bool {
        self.tick()
    }
    /// Bring the serial bus up to date after a drive moved.
    fn sync_iec_bus(&mut self, _bus: &mut IecBus) {}
    /// User port pin M.
    fn user_port_pa2(&self) -> bool {
        true
    }
    /// User port pin C.
    fn set_user_port_pb0(&mut self, _high: bool) {}
    /// User port pin B.
    fn set_user_port_flag2(&mut self, _high: bool) {}
    /// Let a fitted Ultimate move bytes.
    fn poll_ultimate_uci(&mut self) {}
    /// A second picture the machine draws, with its width and height: the
    /// C128's 80-column VDC.
    fn alternate_framebuffer(&self) -> Option<(&[u32], u32, u32)> {
        None
    }

    /// A second text screen's screen codes and its width in characters, for
    /// boot detection: the C128's 80-column screen in VDC RAM.
    fn alternate_screen_codes(&self) -> Option<(&[u8], u16)> {
        None
    }

    /// A key name only this machine has, beyond the C64's matrix: the
    /// C128's 40/80 DISPLAY key.
    fn is_extra_key_name(name: &str) -> bool {
        let _ = name;
        false
    }

    /// Insert one image into a media slot.
    ///
    /// # Errors
    ///
    /// A slot the machine does not have, or an image it cannot use.
    fn load_media_image(
        runtime: &mut C64FamilyRuntime<Self>,
        image: &MediaImage<'_>,
    ) -> Result<(), MachineError> {
        let _ = runtime;
        Err(MachineError::UnknownMediaSlot {
            slot: image.slot.as_ref().to_owned(),
        })
    }

    /// Eject a media slot.
    ///
    /// # Errors
    ///
    /// A slot the machine does not have.
    fn eject_media_slot(
        runtime: &mut C64FamilyRuntime<Self>,
        slot: &str,
    ) -> Result<(), MachineError> {
        let _ = runtime;
        Err(MachineError::UnknownMediaSlot {
            slot: slot.to_owned(),
        })
    }

    /// Carry out a control command.
    ///
    /// # Errors
    ///
    /// A command the machine has no use for.
    fn command(
        runtime: &mut C64FamilyRuntime<Self>,
        command: &ControlCommand,
    ) -> Result<(), MachineError> {
        let _ = runtime;
        Err(MachineError::UnsupportedOperation {
            operation: command.operation_name(),
        })
    }

    /// Put back, after a reset rebuilt the machine, whatever stays plugged in
    /// across one.
    ///
    /// # Errors
    ///
    /// A retained image that no longer loads.
    fn reattach_after_reset(runtime: &mut C64FamilyRuntime<Self>) -> Result<(), MachineError> {
        let _ = runtime;
        Ok(())
    }

    /// Query paths only this machine answers, live for the current runtime.
    fn extra_query_paths(runtime: &C64FamilyRuntime<Self>) -> Vec<String> {
        let _ = runtime;
        Vec::new()
    }

    /// Answer a query path only this machine has. `Ok(None)` for one it does
    /// not know.
    ///
    /// # Errors
    ///
    /// A path it knows but cannot answer now.
    fn extra_query(
        runtime: &C64FamilyRuntime<Self>,
        path: &str,
    ) -> Result<Option<serde_json::Value>, emu198x_shell::QueryError> {
        let _ = (runtime, path);
        Ok(None)
    }

    /// Type a BASIC program in and optionally run it.
    ///
    /// # Errors
    ///
    /// No loader for this machine, or the loader failed.
    fn load_basic_program<Q: SessionQueryProvider<C64FamilyRuntime<Self>>>(
        session: &mut HeadlessSession<C64FamilyRuntime<Self>, Q>,
        source: &str,
        run: bool,
    ) -> Result<BasicProgramLoaded, LoaderError> {
        let _ = (session, source, run);
        Err(LoaderError::Unsupported {
            step: "load_basic_program",
        })
    }

    /// Load and start the program on a tape.
    ///
    /// # Errors
    ///
    /// No tape loader for this machine, or the loader failed.
    fn autoload_tape<Q: SessionQueryProvider<C64FamilyRuntime<Self>>>(
        session: &mut HeadlessSession<C64FamilyRuntime<Self>, Q>,
        slot: &str,
        max_boot_frames: u32,
    ) -> Result<TapeAutoloaded, LoaderError> {
        let _ = (session, slot, max_boot_frames);
        Err(LoaderError::Unsupported {
            step: "autoload_tape",
        })
    }
}

/// The C64's three ROMs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct C64RomSet {
    /// The 8 KB KERNAL.
    pub kernal: Vec<u8>,
    /// The 8 KB BASIC V2.
    pub basic: Vec<u8>,
    /// The 4 KB character generator.
    pub character: Vec<u8>,
}

impl C64FamilyMachine for C64 {
    type Model = Model;
    type Roms = C64RomSet;
    type Snapshot = C64Snapshot;

    fn rom_convention() -> RomConvention {
        RomConvention {
            env_var: Some("EMU198X_C64_ROM_DIR"),
            dirs: &["commodore-c64", "c64"],
        }
    }

    fn build(model: Model, roms: &C64RomSet) -> Result<Self, MachineError> {
        Self::new(C64Config {
            model: model.to_machine_model(),
            kernal_rom: &roms.kernal,
            basic_rom: &roms.basic,
            character_rom: &roms.character,
        })
        .map_err(|reason| MachineError::InvalidRequest {
            reason: reason.to_string(),
        })
    }

    fn blank_roms() -> C64RomSet {
        C64RomSet {
            kernal: vec![0; crate::runtime::KERNAL_ROM_SIZE],
            basic: vec![0; crate::runtime::BASIC_ROM_SIZE],
            character: vec![0; crate::runtime::CHARACTER_ROM_SIZE],
        }
    }

    fn runtime_from_firmware(
        model: Model,
        firmware: &FirmwareSet<'_>,
    ) -> Result<C64Runtime, MachineError> {
        C64Runtime::from_firmware(model, firmware)
    }

    fn tick(&mut self) -> bool {
        Self::tick(self)
    }

    fn phi2_cycles(&self) -> u64 {
        Self::phi2_cycles(self)
    }

    fn frame_count(&self) -> u64 {
        Self::frame_count(self)
    }

    fn timing(&self) -> C64Timing {
        Self::timing(self)
    }

    fn cpu(&self) -> &M6502 {
        Self::cpu(self)
    }

    fn vic(&self) -> &Vic {
        Self::vic(self)
    }

    fn cia1(&self) -> &Cia6526 {
        Self::cia1(self)
    }

    fn cia2(&self) -> &Cia6526 {
        Self::cia2(self)
    }

    fn vic_register(&self, index: u8) -> u8 {
        Self::vic_register(self, index)
    }

    fn raster_line(&self) -> u16 {
        Self::raster_line(self)
    }

    fn cycle_in_line(&self) -> u8 {
        Self::cycle_in_line(self)
    }

    fn framebuffer(&self) -> &[u32] {
        Self::framebuffer(self)
    }

    fn take_audio_buffer(&mut self) -> Vec<f32> {
        Self::take_audio_buffer(self)
    }

    fn audio_sample_rate(&self) -> u32 {
        Self::audio_sample_rate(self)
    }

    fn ram_read(&self, addr: u16) -> u8 {
        self.memory().ram_read(addr)
    }

    fn load_prg(&mut self, data: &[u8]) -> Result<u16, String> {
        Self::load_prg(self, data)
    }

    fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        Self::keyboard_mut(self)
    }

    fn apply_input_event(&mut self, event: &InputEvent) {
        crate::input::apply_input_event(self, event);
    }

    fn snapshot_state(&self) -> C64Snapshot {
        Self::snapshot_state(self)
    }

    fn restore_snapshot_state(&mut self, snapshot: C64Snapshot) -> Result<(), String> {
        Self::restore_snapshot_state(self, snapshot)
    }

    fn drain_audio_stems(&mut self) {
        drop(self.take_audio_channel_buffers());
    }

    fn tick_with_iec_bus(&mut self, bus: &mut IecBus) -> bool {
        Self::tick_with_iec_bus(self, bus)
    }

    fn sync_iec_bus(&mut self, bus: &mut IecBus) {
        Self::sync_iec_bus(self, bus);
    }

    fn user_port_pa2(&self) -> bool {
        Self::user_port_pa2(self)
    }

    fn set_user_port_pb0(&mut self, high: bool) {
        Self::set_user_port_pb0(self, high);
    }

    fn set_user_port_flag2(&mut self, high: bool) {
        Self::set_user_port_flag2(self, high);
    }

    fn poll_ultimate_uci(&mut self) {
        Self::poll_ultimate_uci(self);
    }

    fn load_media_image(
        runtime: &mut C64Runtime,
        image: &MediaImage<'_>,
    ) -> Result<(), MachineError> {
        runtime.c64_load_media_image(image)
    }

    fn eject_media_slot(runtime: &mut C64Runtime, slot: &str) -> Result<(), MachineError> {
        runtime.c64_eject_media_slot(slot)
    }

    fn command(runtime: &mut C64Runtime, command: &ControlCommand) -> Result<(), MachineError> {
        runtime.c64_command(command)
    }

    fn reattach_after_reset(runtime: &mut C64Runtime) -> Result<(), MachineError> {
        runtime.c64_reattach_after_reset()
    }

    fn extra_query_paths(runtime: &C64Runtime) -> Vec<String> {
        crate::queries::c64_extra_query_paths(runtime)
    }

    fn extra_query(
        runtime: &C64Runtime,
        path: &str,
    ) -> Result<Option<serde_json::Value>, emu198x_shell::QueryError> {
        crate::queries::c64_extra_query(runtime, path)
    }

    fn load_basic_program<Q: SessionQueryProvider<C64Runtime>>(
        session: &mut HeadlessSession<C64Runtime, Q>,
        source: &str,
        run: bool,
    ) -> Result<BasicProgramLoaded, LoaderError> {
        crate::basic_loader::load_basic_program_step(session, source, run)
    }

    fn autoload_tape<Q: SessionQueryProvider<C64Runtime>>(
        session: &mut HeadlessSession<C64Runtime, Q>,
        slot: &str,
        max_boot_frames: u32,
    ) -> Result<TapeAutoloaded, LoaderError> {
        crate::autoload::autoload_tape_step(session, slot, max_boot_frames)
    }
}
