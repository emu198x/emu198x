//! `C64RuntimeKind`: one runtime type for every machine in the family.
//!
//! The enum holds a [`C64Runtime`] or a [`C128Runtime`] and forwards the
//! shell's traits to whichever it holds, so a session can start on either and
//! `set_machine` can swap between them, per the dispatcher shape in
//! `knowledge/decisions/runtime-internal-shape.md`. Two variants read fine as
//! explicit matches, as the Amiga's three do.
//!
//! The C64's own features (drives, tape, cartridges, expansions, the user
//! port, its BASIC and tape loaders) are reached through
//! [`AsC64Runtime::as_c64`], which is `None` while a C128 is running.

use emu198x_shell::{
    CapabilitySet, ControlCommand, DebugPrimitives, FamilyRuntime, FirmwareSet, FirmwareSource,
    HeadlessSession, HostIo, IoEvent, KeyTiming, KeyboardTarget, LoaderError, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaSet, QueryError, QueryResult, ResetKind,
    RomConvention, RunResult, SessionQueryProvider, display::Display,
};

use crate::c128::C128Runtime;
use crate::profiles::{C128Model, Model};
use crate::queries::C64SessionQueryProvider;
use crate::runtime::C64Runtime;
use crate::variants::{C64FamilyMachine, C64FamilyModel};

/// A runtime that may hold a C64, for the C64-only session helpers: the tape
/// and disk autoloaders, the BASIC loader and host file import.
///
/// [`C64Runtime`] always answers `Some`; [`C64RuntimeKind`] answers `Some`
/// while it holds a C64, so the helpers report a C128 as having no tape, no
/// drive or no BASIC V2 layout rather than doing the wrong thing to it.
pub trait AsC64Runtime: MachineCore + Sized {
    /// The C64 runtime, when this is one.
    fn as_c64(&self) -> Option<&C64Runtime>;
    /// The C64 runtime, mutably, when this is one.
    fn as_c64_mut(&mut self) -> Option<&mut C64Runtime>;
}

impl AsC64Runtime for C64Runtime {
    fn as_c64(&self) -> Option<&C64Runtime> {
        Some(self)
    }

    fn as_c64_mut(&mut self) -> Option<&mut C64Runtime> {
        Some(self)
    }
}

/// Every model the family app can run: a C64 or a C128.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum C64Variant {
    /// One of the four C64 models.
    C64(Model),
    /// One of the two C128 models.
    C128(C128Model),
}

impl C64Variant {
    /// Every variant id, the C64's first.
    pub const VARIANT_IDS: [&'static str; 6] = [
        Model::VARIANT_IDS[0],
        Model::VARIANT_IDS[1],
        Model::VARIANT_IDS[2],
        Model::VARIANT_IDS[3],
        C128Model::VARIANT_IDS[0],
        C128Model::VARIANT_IDS[1],
    ];

    /// The variant an id names, including the C64's and C128's aliases.
    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Model::from_variant_id(id)
            .map(Self::C64)
            .or_else(|| C128Model::from_variant_id(id).map(Self::C128))
    }

    /// This variant's id.
    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::C64(model) => model.variant_id(),
            Self::C128(model) => model.variant_id(),
        }
    }

    /// The label a model menu shows.
    #[must_use]
    pub const fn menu_label(self) -> &'static str {
        match self {
            Self::C64(model) => model.menu_label(),
            Self::C128(model) => model.menu_label(),
        }
    }

    /// The board timing: clock, lines and cycles per line.
    #[must_use]
    pub fn timing(self) -> common_commodore_c64::timing::C64Timing {
        match self {
            Self::C64(model) => C64FamilyModel::timing(model),
            Self::C128(model) => C64FamilyModel::timing(model),
        }
    }

    /// Whether this is a C128.
    #[must_use]
    pub const fn is_c128(self) -> bool {
        matches!(self, Self::C128(_))
    }

    /// The shell profile.
    #[must_use]
    pub fn profile(self) -> MachineProfile {
        match self {
            Self::C64(model) => C64FamilyModel::profile(model),
            Self::C128(model) => C64FamilyModel::profile(model),
        }
    }

    /// Where the firmware lookup finds each ROM.
    #[must_use]
    pub fn firmware_sources(self) -> Vec<FirmwareSource> {
        match self {
            Self::C64(model) => model.firmware_sources(),
            Self::C128(model) => model.firmware_sources(),
        }
    }

    /// Every variant, in menu order.
    #[must_use]
    pub fn all() -> Vec<Self> {
        Model::ALL
            .into_iter()
            .map(Self::C64)
            .chain(C128Model::ALL.into_iter().map(Self::C128))
            .collect()
    }
}

impl From<Model> for C64Variant {
    fn from(model: Model) -> Self {
        Self::C64(model)
    }
}

impl From<C128Model> for C64Variant {
    fn from(model: C128Model) -> Self {
        Self::C128(model)
    }
}

/// The family runtime: a C64 or a C128.
#[allow(clippy::large_enum_variant)]
pub enum C64RuntimeKind {
    /// A Commodore 64.
    C64(C64Runtime),
    /// A Commodore 128.
    C128(C128Runtime),
}

impl C64RuntimeKind {
    /// A runtime on zero-filled ROMs.
    #[must_use]
    pub fn blank(variant: C64Variant) -> Self {
        match variant {
            C64Variant::C64(model) => Self::C64(C64Runtime::blank(model)),
            C64Variant::C128(model) => Self::C128(C128Runtime::blank(model)),
        }
    }

    /// The variant running.
    #[must_use]
    pub fn variant(&self) -> C64Variant {
        match self {
            Self::C64(rt) => C64Variant::C64(rt.model()),
            Self::C128(rt) => C64Variant::C128(rt.model()),
        }
    }

    /// The C128 runtime, when this is one.
    #[must_use]
    pub fn as_c128(&self) -> Option<&C128Runtime> {
        match self {
            Self::C128(rt) => Some(rt),
            Self::C64(_) => None,
        }
    }

    /// The C128 runtime, mutably, when this is one.
    pub fn as_c128_mut(&mut self) -> Option<&mut C128Runtime> {
        match self {
            Self::C128(rt) => Some(rt),
            Self::C64(_) => None,
        }
    }

    /// Show the machine's second picture (the C128's VDC) or the VIC-II's.
    pub fn set_alternate_screen(&mut self, on: bool) {
        match self {
            Self::C64(rt) => rt.set_alternate_screen(on),
            Self::C128(rt) => rt.set_alternate_screen(on),
        }
    }

    /// Whether the second picture is showing.
    #[must_use]
    pub fn alternate_screen(&self) -> bool {
        match self {
            Self::C64(rt) => rt.alternate_screen(),
            Self::C128(rt) => rt.alternate_screen(),
        }
    }

    /// The picture a frame shows: RGBA bytes, width and height.
    #[must_use]
    pub fn rgba_frame(&self) -> (&[u8], u32, u32) {
        match self {
            Self::C64(rt) => rt.rgba_frame(),
            Self::C128(rt) => rt.rgba_frame(),
        }
    }

    /// Load a PRG straight into RAM, returning its load address.
    ///
    /// # Errors
    ///
    /// A malformed PRG.
    pub fn load_prg_bytes(&mut self, data: &[u8]) -> Result<u16, String> {
        match self {
            Self::C64(rt) => rt.load_prg_bytes(data),
            Self::C128(rt) => rt.load_prg_bytes(data),
        }
    }

    /// Change the 40/80 DISPLAY key, which picks the screen the C128's KERNAL
    /// comes up on at its next reset. The C64 has no such key.
    pub fn set_80_column_key(&mut self, down: bool) {
        if let Self::C128(rt) = self {
            rt.machine_mut().set_80_column_key(down);
        }
    }
}

impl AsC64Runtime for C64RuntimeKind {
    fn as_c64(&self) -> Option<&C64Runtime> {
        match self {
            Self::C64(rt) => Some(rt),
            Self::C128(_) => None,
        }
    }

    fn as_c64_mut(&mut self) -> Option<&mut C64Runtime> {
        match self {
            Self::C64(rt) => Some(rt),
            Self::C128(_) => None,
        }
    }
}

impl FamilyRuntime for C64RuntimeKind {
    type Model = C64Variant;

    fn from_firmware(
        variant: C64Variant,
        firmware: &FirmwareSet<'_>,
    ) -> Result<Self, MachineError> {
        match variant {
            C64Variant::C64(model) => C64Runtime::from_firmware(model, firmware).map(Self::C64),
            C64Variant::C128(model) => {
                <machine_commodore_c128::C128 as C64FamilyMachine>::runtime_from_firmware(
                    model, firmware,
                )
                .map(Self::C128)
            }
        }
    }

    fn native_frame_ticks(&self) -> u64 {
        match self {
            Self::C64(rt) => rt.native_frame_ticks(),
            Self::C128(rt) => rt.native_frame_ticks(),
        }
    }

    fn variant_ids() -> &'static [&'static str] {
        &C64Variant::VARIANT_IDS
    }

    fn model_from_id(id: &str) -> Option<C64Variant> {
        C64Variant::from_variant_id(id)
    }

    fn variant_id(variant: C64Variant) -> &'static str {
        variant.variant_id()
    }

    fn profile_for(variant: C64Variant) -> MachineProfile {
        variant.profile()
    }

    /// The C64's ROM directory; the C128's ROMs sit beside the C64's or in a
    /// `c128/` directory inside it.
    fn rom_convention() -> RomConvention {
        <machine_commodore_c64::C64 as C64FamilyMachine>::rom_convention()
    }

    fn firmware_sources(variant: C64Variant) -> Vec<FirmwareSource> {
        variant.firmware_sources()
    }
}

impl MachineCore for C64RuntimeKind {
    fn set_machine<Q: SessionQueryProvider<Self>>(
        session: &mut HeadlessSession<Self, Q>,
        machine: &str,
    ) -> Result<emu198x_shell::VariantSwitched, LoaderError> {
        emu198x_shell::swap_variant(session, machine)
    }

    fn profile(&self) -> &MachineProfile {
        match self {
            Self::C64(rt) => rt.profile(),
            Self::C128(rt) => rt.profile(),
        }
    }

    fn time(&self) -> MachineTime {
        match self {
            Self::C64(rt) => rt.time(),
            Self::C128(rt) => rt.time(),
        }
    }

    fn reset(&mut self, kind: ResetKind) {
        match self {
            Self::C64(rt) => rt.reset(kind),
            Self::C128(rt) => rt.reset(kind),
        }
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        match self {
            Self::C64(rt) => rt.load_media(media),
            Self::C128(rt) => rt.load_media(media),
        }
    }

    fn eject_media(&mut self, slot: &str) -> Result<(), MachineError> {
        match self {
            Self::C64(rt) => rt.eject_media(slot),
            Self::C128(rt) => rt.eject_media(slot),
        }
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        match self {
            Self::C64(rt) => rt.run_until(target, host),
            Self::C128(rt) => rt.run_until(target, host),
        }
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        match self {
            Self::C64(rt) => rt.snapshot(),
            Self::C128(rt) => rt.snapshot(),
        }
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        match self {
            Self::C64(rt) => rt.restore(bytes),
            Self::C128(rt) => rt.restore(bytes),
        }
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        match self {
            Self::C64(rt) => rt.command(command),
            Self::C128(rt) => rt.command(command),
        }
    }

    fn capabilities(&self) -> CapabilitySet {
        match self {
            Self::C64(rt) => rt.capabilities(),
            Self::C128(rt) => rt.capabilities(),
        }
    }

    // Forwarded like everything else: the default of `None` would compile
    // and silently drop the aspect ratio the inner runtime states.
    fn display(&self) -> Option<Display> {
        match self {
            Self::C64(rt) => rt.display(),
            Self::C128(rt) => rt.display(),
        }
    }

    fn debug_target(&self) -> Option<&dyn emu198x_shell::DebugTarget> {
        Some(self)
    }

    fn debug_target_mut(&mut self) -> Option<&mut dyn emu198x_shell::DebugTarget> {
        Some(self)
    }

    fn keyboard_target(&self) -> Option<&dyn KeyboardTarget> {
        Some(self)
    }

    fn load_basic_program<Q: SessionQueryProvider<Self>>(
        session: &mut HeadlessSession<Self, Q>,
        source: &str,
        run: bool,
    ) -> Result<emu198x_shell::BasicProgramLoaded, LoaderError> {
        crate::basic_loader::load_basic_program_step(session, source, run)
    }

    fn autoload_tape<Q: SessionQueryProvider<Self>>(
        session: &mut HeadlessSession<Self, Q>,
        slot: &str,
        max_boot_frames: u32,
    ) -> Result<emu198x_shell::TapeAutoloaded, LoaderError> {
        crate::autoload::autoload_tape_step(session, slot, max_boot_frames)
    }
}

impl DebugPrimitives for C64RuntimeKind {
    fn dbg_pc(&self) -> u32 {
        match self {
            Self::C64(rt) => rt.dbg_pc(),
            Self::C128(rt) => rt.dbg_pc(),
        }
    }

    fn dbg_peek(&self, addr: u32) -> u8 {
        match self {
            Self::C64(rt) => rt.dbg_peek(addr),
            Self::C128(rt) => rt.dbg_peek(addr),
        }
    }

    fn dbg_poke(&mut self, addr: u32, value: u8) {
        match self {
            Self::C64(rt) => rt.dbg_poke(addr, value),
            Self::C128(rt) => rt.dbg_poke(addr, value),
        }
    }

    fn dbg_cpu_state(&self) -> serde_json::Value {
        match self {
            Self::C64(rt) => rt.dbg_cpu_state(),
            Self::C128(rt) => rt.dbg_cpu_state(),
        }
    }

    fn dbg_instruction_boundary_count(&self) -> Option<u64> {
        match self {
            Self::C64(rt) => rt.dbg_instruction_boundary_count(),
            Self::C128(rt) => rt.dbg_instruction_boundary_count(),
        }
    }

    fn dbg_disassemble(&self, addr: u32) -> Option<(String, u8)> {
        match self {
            Self::C64(rt) => rt.dbg_disassemble(addr),
            Self::C128(rt) => rt.dbg_disassemble(addr),
        }
    }

    fn dbg_step(&mut self) -> u64 {
        match self {
            Self::C64(rt) => rt.dbg_step(),
            Self::C128(rt) => rt.dbg_step(),
        }
    }

    fn dbg_step_no_resync(&mut self) -> u64 {
        match self {
            Self::C64(rt) => rt.dbg_step_no_resync(),
            Self::C128(rt) => rt.dbg_step_no_resync(),
        }
    }

    fn dbg_resync(&mut self) {
        match self {
            Self::C64(rt) => rt.dbg_resync(),
            Self::C128(rt) => rt.dbg_resync(),
        }
    }

    fn dbg_supports_io_trace(&self) -> bool {
        match self {
            Self::C64(rt) => rt.dbg_supports_io_trace(),
            Self::C128(rt) => rt.dbg_supports_io_trace(),
        }
    }

    fn dbg_start_io_trace(&mut self) {
        match self {
            Self::C64(rt) => rt.dbg_start_io_trace(),
            Self::C128(rt) => rt.dbg_start_io_trace(),
        }
    }

    fn dbg_take_io_trace(&mut self) -> Vec<IoEvent> {
        match self {
            Self::C64(rt) => rt.dbg_take_io_trace(),
            Self::C128(rt) => rt.dbg_take_io_trace(),
        }
    }
}

impl KeyboardTarget for C64RuntimeKind {
    fn key_name_is_valid(&self, name: &str) -> bool {
        match self {
            Self::C64(rt) => rt.key_name_is_valid(name),
            Self::C128(rt) => rt.key_name_is_valid(name),
        }
    }

    fn key_names_hint(&self) -> &'static str {
        match self {
            Self::C64(rt) => rt.key_names_hint(),
            Self::C128(rt) => rt.key_names_hint(),
        }
    }

    fn keys_for_char(&self, ch: char) -> Option<Vec<String>> {
        match self {
            Self::C64(rt) => rt.keys_for_char(ch),
            Self::C128(rt) => rt.keys_for_char(ch),
        }
    }

    fn key_timing(&self) -> KeyTiming {
        match self {
            Self::C64(rt) => rt.key_timing(),
            Self::C128(rt) => rt.key_timing(),
        }
    }
}

impl SessionQueryProvider<C64RuntimeKind> for C64SessionQueryProvider {
    fn query_paths(&self, machine: &C64RuntimeKind, prefix: Option<&str>) -> Vec<String> {
        match machine {
            C64RuntimeKind::C64(rt) => self.query_paths(rt, prefix),
            C64RuntimeKind::C128(rt) => self.query_paths(rt, prefix),
        }
    }

    fn query(
        &self,
        machine: &C64RuntimeKind,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        match machine {
            C64RuntimeKind::C64(rt) => self.query(rt, path),
            C64RuntimeKind::C128(rt) => self.query(rt, path),
        }
    }
}
