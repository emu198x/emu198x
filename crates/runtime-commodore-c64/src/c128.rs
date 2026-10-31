//! The Commodore 128 as a [`C64FamilyMachine`].
//!
//! The C128 shares the C64's chips and keyboard, so the family runtime's
//! frame loop, snapshots, shared queries and debug target serve it unchanged.
//! What is its own: four ROMs, the Z80 and the MMU, and the 8563 VDC, whose
//! 80-column picture the runtime can show in place of the VIC-IIe's
//! ([`C64FamilyRuntime::set_alternate_screen`]).
//!
//! The machine models C128 mode only, so the hooks for the C64's tape deck,
//! drives, cartridges and user port keep their defaults: no media slots, no
//! transport, no loaders.

use crate::boards::C128;
use common_commodore_c64::KeyboardMatrix;
use common_commodore_c64::timing::C64Timing;
use common_commodore_c128::C128Roms;
use emu198x_mos_6502::M6502;
use emu198x_mos_cia_6526::Cia6526;
use emu198x_mos_vic_ii::Vic;
use emu198x_shell::{
    FirmwareSet, FirmwareSource, InputEvent, MachineError, MachineProfile, QueryError,
    RomConvention,
};
use serde_json::json;

use crate::profiles::{C128Model, c128_profile_for};
use crate::runtime::C64FamilyRuntime;
use crate::variants::{C64FamilyMachine, C64FamilyModel};

const KERNAL_ROM_SIZE: usize = 0x4000;
const BASIC_HALF_ROM_SIZE: usize = 0x4000;
const CHARACTER_ROM_SIZE: usize = 0x2000;

/// The Commodore 128 runtime.
pub type C128Runtime = C64FamilyRuntime<C128>;

/// The C128's four ROMs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct C128RomSet {
    /// 16 KB: the screen editor, the Z80 BIOS and the KERNAL.
    pub kernal: Vec<u8>,
    /// 16 KB: BASIC 7.0's lower half.
    pub basic_lo: Vec<u8>,
    /// 16 KB: BASIC 7.0's upper half.
    pub basic_hi: Vec<u8>,
    /// 8 KB: the C64's character set, then the C128's.
    pub chargen: Vec<u8>,
}

/// Query paths only the C128 answers.
pub(crate) const C128_QUERY_PATHS: &[&str] = &[
    "c128.cpu_8502_active",
    "c128.screen.80_column",
    "c128.z80.pc",
    "c128.vdc.text.lines",
];

impl C64FamilyModel for C128Model {
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
        c128_profile_for(self)
    }

    fn timing(self) -> C64Timing {
        self.machine_model().timing()
    }

    fn is_pal(self) -> bool {
        self == Self::C128Pal
    }

    fn firmware_sources(self) -> Vec<FirmwareSource> {
        self.firmware_sources()
    }
}

impl C64FamilyMachine for C128 {
    type Model = C128Model;
    type Roms = C128RomSet;
    type Snapshot = common_commodore_c128::C128;

    /// The directory the machine's own boot tests read, by the same
    /// variable, plus the usual places under `~/.emu198x/roms`.
    fn rom_convention() -> RomConvention {
        RomConvention {
            env_var: Some("EMU198X_C128_ROM_DIR"),
            dirs: &["commodore-c128", "c128"],
        }
    }

    fn build(model: C128Model, roms: &C128RomSet) -> Result<Self, MachineError> {
        Self::with_model(
            &C128Roms {
                kernal: &roms.kernal,
                basic_lo: &roms.basic_lo,
                basic_hi: &roms.basic_hi,
                chargen: &roms.chargen,
            },
            model.machine_model(),
        )
        .map_err(|reason| MachineError::InvalidRequest {
            reason: reason.to_string(),
        })
    }

    fn blank_roms() -> C128RomSet {
        C128RomSet {
            kernal: vec![0; KERNAL_ROM_SIZE],
            basic_lo: vec![0; BASIC_HALF_ROM_SIZE],
            basic_hi: vec![0; BASIC_HALF_ROM_SIZE],
            chargen: vec![0; CHARACTER_ROM_SIZE],
        }
    }

    fn runtime_from_firmware(
        model: C128Model,
        firmware: &FirmwareSet<'_>,
    ) -> Result<C128Runtime, MachineError> {
        firmware.validate_for_profile(&model.profile())?;
        // Each image is checked against its own size, so a wrong one is named
        // by its firmware id rather than by the machine's ROM label.
        let rom = |id: &str, size: usize| {
            let bytes = firmware
                .bytes(id)
                .ok_or_else(|| MachineError::MissingFirmware { id: id.to_owned() })?;
            if bytes.len() != size {
                return Err(MachineError::InvalidFirmware {
                    id: id.to_owned(),
                    reason: format!("expected exactly {size} bytes, got {}", bytes.len()),
                });
            }
            Ok(bytes.to_vec())
        };
        let roms = C128RomSet {
            kernal: rom("commodore-c128-kernal-rom", KERNAL_ROM_SIZE)?,
            basic_lo: rom("commodore-c128-basic-lo-rom", BASIC_HALF_ROM_SIZE)?,
            basic_hi: rom("commodore-c128-basic-hi-rom", BASIC_HALF_ROM_SIZE)?,
            chargen: rom("commodore-c128-character-rom", CHARACTER_ROM_SIZE)?,
        };
        let machine = Self::build(model, &roms)?;
        Ok(C128Runtime::assemble(model, machine, roms))
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
        self.ram(u32::from(addr))
    }

    fn load_prg(&mut self, data: &[u8]) -> Result<u16, String> {
        Self::load_prg(self, data)
    }

    fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        Self::keyboard_mut(self)
    }

    fn apply_input_event(&mut self, event: &InputEvent) {
        apply_c128_input_event(self, event);
    }

    fn snapshot_state(&self) -> common_commodore_c128::C128 {
        self.snapshot_state()
    }

    fn restore_snapshot_state(
        &mut self,
        snapshot: common_commodore_c128::C128,
    ) -> Result<(), String> {
        self.restore_snapshot_state(snapshot)
    }

    fn alternate_framebuffer(&self) -> Option<(&[u32], u32, u32)> {
        Some((
            self.vdc_framebuffer(),
            Self::VDC_WIDTH as u32,
            Self::VDC_HEIGHT as u32,
        ))
    }

    fn is_extra_key_name(name: &str) -> bool {
        is_40_80_key(name)
    }

    /// The KERNAL's 80-column screen: 25 rows of 80 at VDC address `$0000`.
    fn alternate_screen_codes(&self) -> Option<(&[u8], u16)> {
        Some((&self.vdc_ram()[..80 * 25], 80))
    }

    fn extra_query_paths(_runtime: &C128Runtime) -> Vec<String> {
        C128_QUERY_PATHS
            .iter()
            .copied()
            .map(str::to_owned)
            .collect()
    }

    fn extra_query(
        runtime: &C128Runtime,
        path: &str,
    ) -> Result<Option<serde_json::Value>, QueryError> {
        let c128 = runtime.machine();
        Ok(Some(match path {
            "c128.cpu_8502_active" => json!(c128.running_8502()),
            "c128.screen.80_column" => json!(runtime.alternate_screen()),
            "c128.z80.pc" => json!(c128.z80().regs.pc),
            "c128.vdc.text.lines" => json!(vdc_text_lines(c128)),
            _ => return Ok(None),
        }))
    }
}

/// The KERNAL's 80-column screen as text: 25 rows of 80 screen codes from
/// VDC address `$0000`, where the screen editor keeps them.
fn vdc_text_lines(c128: &C128) -> Vec<String> {
    let vram = c128.vdc_ram();
    (0..25)
        .map(|row| {
            (0..80)
                .map(|col| crate::queries::decode_screen_code(vram[row * 80 + col] & 0x7F))
                .collect()
        })
        .collect()
}

/// Keys reach the matrix the C64 shares; the `40/80` key latches which
/// screen the KERNAL comes up on at the next reset. The C128's extra keys
/// (keypad, ESC, TAB, ALT, HELP, LINE FEED, NO SCROLL), the joystick ports
/// and RESTORE are not wired into the machine yet, so those events are
/// dropped.
pub(crate) fn apply_c128_input_event(machine: &mut C128, event: &InputEvent) {
    let InputEvent::Key { name, pressed } = event else {
        return;
    };
    let name = name.as_ref();
    if let Some((row, col)) = crate::input::c64_key_position(name) {
        machine.keyboard_mut().set_key(row, col, *pressed);
    } else if name.eq_ignore_ascii_case("shiftlock") {
        machine.keyboard_mut().set_shift_lock(*pressed);
    } else if is_40_80_key(name) {
        machine.set_80_column_key(*pressed);
    }
}

/// The names the 40/80 DISPLAY key answers to.
pub(crate) fn is_40_80_key(name: &str) -> bool {
    ["40/80", "4080", "display4080"]
        .iter()
        .any(|key| name.eq_ignore_ascii_case(key))
}
