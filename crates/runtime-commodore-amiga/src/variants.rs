//! Per-variant trait + impls for the Amiga family.
//!
//! Today there is one variant: A1000 / A500-family OCS PAL, all
//! served by a single `AmigaOcs` machine struct via different RAM
//! configs and bootstrap modes. The trait `AmigaMachine` exists
//! so that adding ECS / AGA / SAGA / NTSC / Vampire AC68080 /
//! PiStorm / RTG variants becomes a mechanical extension rather
//! than a runtime rewrite. Any type that implements `AmigaMachine`
//! plugs into `AmigaRuntime<M>` with no further runtime changes.
//!
//! See `knowledge/decisions/runtime-internal-shape.md` for the playbook
//! and the Amiga long-term-scope memory note for the full target
//! list (Vampire AC68080 + SAGA + RTG framebuffer slots, plus the
//! PAL/NTSC region matrix with NTSC's short/long line alternation
//! still pending in the chip layer).

use emu198x_shell::QueryError;
use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga_a1200::{AmigaA1200, AmigaA1200Snapshot};
use machine_commodore_amiga_ecs::{AmigaEcs, AmigaEcsSnapshot};
use machine_commodore_amiga_ocs::{
    AgnusRegion, AmigaOcs, AmigaOcsSnapshot, FB_HEIGHT, FB_WIDTH, RamConfig,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::{AmigaRuntime, Model};

/// Per-variant machine surface for the Amiga family.
///
/// Implemented by every concrete machine type that wants to plug
/// into `AmigaRuntime<M>`. The trait is deliberately agnostic to:
///
///   * which CPU is running (Cpu68000 today, the dormant
///     Cpu68020/30/40 variant crates next, eventually a software
///     AC68080 model for Apollo Vampire and a software 68k host
///     for PiStorm)
///   * which chipset is producing the chipset framebuffer (OCS
///     today, ECS / AGA / SAGA later — SAGA is a chip-stack
///     replacement, not an OCS+wrapper)
///   * which graphics output is in use (chipset-only today; RTG
///     cards arrive via a slotted framebuffer accessor without
///     reshaping the trait)
pub trait AmigaMachine {
    /// Chipset framebuffer width in pixels (Denise / Lisa / SAGA
    /// video output, before any host-side scaling).
    const CHIPSET_FB_WIDTH: u32;

    /// Chipset framebuffer height in pixels.
    const CHIPSET_FB_HEIGHT: u32;

    /// Snapshot envelope for the chip stack. Encoded by serde +
    /// postcard inside the runtime's snapshot envelope.
    type Snapshot: Serialize + DeserializeOwned;

    /// Variant-specific extra metadata the runtime carries across
    /// snapshots so a restore can reconstruct the machine. For OCS
    /// this is `RamConfig` (the chip / slow / fast RAM sizes the
    /// chip stack was built around). Future variants may carry a
    /// chipset descriptor (ECS/AGA/SAGA marker), a CPU + accelerator
    /// pair (Vampire AC68080 vs PiStorm 68k), or an RTG card
    /// inventory — whatever needs to be replayed alongside the
    /// chip-stack snapshot to fully reconstruct the machine.
    type SnapshotMetadata: Serialize + DeserializeOwned + Clone;

    // ---------- clock / lifecycle ----------

    /// Rebuild the chip stack from scratch using the supplied firmware
    /// and metadata. Drives `MachineCore::reset`. Variants that do
    /// "construct fresh" (the OCS pattern: `*self =
    /// AmigaOcs::with_ram_config(firmware, metadata)`) use this hook
    /// to replace themselves; future in-place reset variants can
    /// implement a per-field clear instead.
    fn rebuild(&mut self, firmware: &[u8], metadata: &Self::SnapshotMetadata);

    /// Advance the machine by one tick (master / 4 = half-CCK).
    fn tick(&mut self);

    /// Number of machine ticks in one video frame for this variant.
    /// PAL OCS = 312 lines × 227 CCK × 2 ticks/CCK = 141,648. NTSC
    /// variants will return a different value; in the future some
    /// variants (NTSC interlace fields, Vampire-with-clock-mod) may
    /// return field-dependent values rather than a single constant.
    fn frame_ticks(&self) -> u64;

    /// Master CCK rate in Hz. Used by the runtime to downsample
    /// Paula audio to the host sample rate. PAL ≈ 3,546,895.
    fn cck_hz(&self) -> u64;

    // ---------- video ----------

    /// Borrow the chipset framebuffer as ARGB pixels. Length always
    /// equals `CHIPSET_FB_WIDTH * CHIPSET_FB_HEIGHT`.
    fn chipset_framebuffer(&self) -> &[u32];

    // ---------- audio ----------

    /// One stereo audio sample (left, right) at the current CCK.
    /// The runtime's resampler invokes this every machine tick so
    /// the sample stream is continuous across frame boundaries.
    fn mix_audio_stereo(&self) -> (f32, f32);

    // ---------- input ----------

    fn key_event(&mut self, code: u8, pressed: bool);
    fn move_mouse_port0(&mut self, dx: i32, dy: i32);
    fn set_mouse_button_port0(&mut self, button: &str, pressed: bool);
    fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool);

    // ---------- media ----------

    /// Insert an ADF image into DF0. The `change_pending` flag drives
    /// the disk-change-pending bookkeeping the A1000 boot path needs;
    /// post-A1000 firmware boots happily without it.
    fn insert_floppy0(&mut self, adf: Adf, change_pending: bool);

    // ---------- snapshot ----------

    fn snapshot_state(&self) -> Self::Snapshot;
    fn restore_snapshot_state(&mut self, snapshot: Self::Snapshot);

    // ---------- queries ----------

    /// Variant-specific query path catalogue. The runtime adds the
    /// shared `boot.*` and `amiga.machine.*` paths on top of this.
    fn variant_query_paths() -> &'static [&'static str];

    /// Resolve a variant-specific query path. Returns `Ok(None)` if
    /// the variant doesn't recognise the path so the runtime can
    /// surface UnknownPath cleanly.
    fn resolve_variant_query(&self, path: &str) -> Result<Option<Value>, QueryError>;
}

// ===================================================================
// AmigaOcs impl — covers A1000 + A500 family + A500+ + maxed-A500.
// ===================================================================

const OCS_VARIANT_QUERY_PATHS: &[&str] = &[
    "amiga.a1000.boot_rom_visible",
    "amiga.a1000.wom_locked",
    "amiga.memory.overlay",
    "amiga.cpu.pc",
    "amiga.cpu.sr",
    "amiga.cpu.ipl",
    "amiga.agnus.vpos",
    "amiga.agnus.hpos",
    "amiga.agnus.dmacon",
    "amiga.agnus.bplcon0",
    "amiga.paula.intena",
    "amiga.paula.intreq",
    "amiga.debug.dsk_write_count",
    "amiga.debug.last_dsk_write",
    "amiga.display.color00",
    "amiga.display.color01",
    "amiga.disk.inserted",
    "amiga.disk.change_pending",
    "amiga.disk.cylinder",
    "amiga.disk.head",
    "amiga.disk.motor_on",
    "amiga.disk.motor_spinning",
    "amiga.disk.step_events",
    "amiga.keyboard.state",
    "amiga.keyboard.queued",
];

impl AmigaMachine for AmigaOcs {
    const CHIPSET_FB_WIDTH: u32 = FB_WIDTH;
    const CHIPSET_FB_HEIGHT: u32 = FB_HEIGHT;

    type Snapshot = AmigaOcsSnapshot;
    type SnapshotMetadata = RamConfig;

    fn rebuild(&mut self, firmware: &[u8], metadata: &Self::SnapshotMetadata) {
        // Two OCS construction paths sit behind one trait method.
        // The 64 KiB image is unambiguously an A1000 bootstrap ROM
        // (the only valid size at that length); 256/512 KiB images
        // are A500-family Kickstart. The runtime's
        // `validate_firmware_rom` already gates these sizes per
        // model, so we can dispatch on length here without re-
        // checking the model.
        *self = if firmware.len() == 64 * 1024 {
            AmigaOcs::with_a1000_bootstrap_rom(firmware.to_vec(), *metadata)
        } else {
            AmigaOcs::with_ram_config(firmware.to_vec(), *metadata)
        };
    }

    fn tick(&mut self) {
        AmigaOcs::tick(self);
    }

    fn frame_ticks(&self) -> u64 {
        // Region-aware: PAL = 141,648 ticks (312 × 227 × 2). NTSC =
        // 119,210 ticks (131 × 227 + 131 × 228, then × 2). The
        // chip-layer alternation handles the per-line short/long
        // distinction; the runtime needs only the frame total.
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_FRAME_TICKS,
            AgnusRegion::Ntsc => crate::A500_NTSC_FRAME_TICKS,
        }
    }

    fn cck_hz(&self) -> u64 {
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_CCK_HZ,
            AgnusRegion::Ntsc => crate::A500_NTSC_CCK_HZ,
        }
    }

    fn chipset_framebuffer(&self) -> &[u32] {
        self.denise().framebuffer()
    }

    fn mix_audio_stereo(&self) -> (f32, f32) {
        self.paula().mix_audio_stereo()
    }

    fn key_event(&mut self, code: u8, pressed: bool) {
        AmigaOcs::key_event(self, code, pressed);
    }

    fn move_mouse_port0(&mut self, dx: i32, dy: i32) {
        AmigaOcs::move_mouse_port0(self, dx, dy);
    }

    fn set_mouse_button_port0(&mut self, button: &str, pressed: bool) {
        AmigaOcs::set_mouse_button_port0(self, button, pressed);
    }

    fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool) {
        let _ = AmigaOcs::set_joystick_control(self, port, name, pressed);
    }

    fn insert_floppy0(&mut self, adf: Adf, change_pending: bool) {
        if change_pending {
            self.insert_adf_with_change_pending(adf);
        } else {
            self.insert_adf(adf);
        }
    }

    fn snapshot_state(&self) -> Self::Snapshot {
        AmigaOcs::snapshot_state(self)
    }

    fn restore_snapshot_state(&mut self, snapshot: Self::Snapshot) {
        AmigaOcs::restore_snapshot_state(self, snapshot);
    }

    fn variant_query_paths() -> &'static [&'static str] {
        OCS_VARIANT_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<Value>, QueryError> {
        let drive = self.drive();
        let drive_status = drive.status();
        let value = match path {
            "amiga.a1000.boot_rom_visible" => json!(self.memory().a1000_boot_rom_visible()),
            "amiga.a1000.wom_locked" => json!(self.memory().a1000_wom_locked()),
            "amiga.memory.overlay" => json!(self.memory().overlay()),
            "amiga.cpu.pc" => json!(self.cpu().regs.pc),
            "amiga.cpu.sr" => json!(self.cpu().regs.sr),
            "amiga.cpu.ipl" => json!(self.cpu().ipl),
            "amiga.agnus.vpos" => json!(self.agnus().vpos),
            "amiga.agnus.hpos" => json!(self.agnus().hpos),
            "amiga.agnus.dmacon" => json!(self.dmacon()),
            "amiga.agnus.bplcon0" => json!(self.bplcon0()),
            "amiga.paula.intena" => json!(self.intena()),
            "amiga.paula.intreq" => json!(self.intreq()),
            "amiga.debug.dsk_write_count" => json!(self.debug_dsk_log.len()),
            "amiga.debug.last_dsk_write" => {
                json!(self.debug_dsk_log.last().map(|(cck, pc, reg, val)| {
                    json!({"cck": cck, "pc": pc, "reg": reg, "val": val})
                }))
            }
            "amiga.display.color00" => json!(self.color(0)),
            "amiga.display.color01" => json!(self.color(1)),
            "amiga.disk.inserted" => json!(drive.has_disk()),
            "amiga.disk.change_pending" => json!(drive_status.disk_change),
            "amiga.disk.cylinder" => json!(drive.cylinder()),
            "amiga.disk.head" => json!(drive.head()),
            "amiga.disk.motor_on" => json!(drive.motor_on()),
            "amiga.disk.motor_spinning" => json!(drive_status.ready),
            "amiga.disk.step_events" => json!(drive.step_event_counter()),
            "amiga.keyboard.state" => json!(self.keyboard().debug_state_name()),
            "amiga.keyboard.queued" => json!(self.keyboard().queued_key_count()),
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}

/// Type alias for the OCS runtime — covers the A1000 + A500 family.
/// The A500+ Models (`A500PlusEcsPal/Ntsc`) are listed in `Model`
/// alongside the OCS variants but are *technically* misrouted when
/// constructed through `AmigaOcsRuntime` (A500+ shipped with ECS
/// chips per Commodore). For the canonical ECS chip stack, use
/// `AmigaEcsRuntime`. The OCS path remains the verifier-binary
/// default until the dispatch refactor lands in a follow-up session.
pub type AmigaOcsRuntime = AmigaRuntime<AmigaOcs>;

// ===================================================================
// AmigaEcs impl — A500+ today; A600 / A2000B / A3000 to follow once
// Gayle / Ramsey / Fat Gary are ported. The trait body is mechanically
// identical to the AmigaOcs impl: the chip-level differences (BEAMCON0
// register handling, BPLCON3 register, programmable sync generator)
// are absorbed inside AgnusEcs / DeniseEcs via Deref/DerefMut, so the
// machine layer's call sites are unchanged. The two impls coexist so
// a future ECS-only behaviour can be carved out without touching OCS.
// ===================================================================

const ECS_VARIANT_QUERY_PATHS: &[&str] = OCS_VARIANT_QUERY_PATHS;

impl AmigaMachine for AmigaEcs {
    const CHIPSET_FB_WIDTH: u32 = FB_WIDTH;
    const CHIPSET_FB_HEIGHT: u32 = FB_HEIGHT;

    type Snapshot = AmigaEcsSnapshot;
    type SnapshotMetadata = RamConfig;

    fn rebuild(&mut self, firmware: &[u8], metadata: &Self::SnapshotMetadata) {
        // The A500+ never shipped with an A1000-style bootstrap ROM,
        // so for now ECS only routes through the standard Kickstart
        // path. When A1000-NTSC-ECS-equivalent (or A3000 with its
        // 64KiB bootstrap) lands we can extend this to mirror the
        // OCS rebuild's size-based dispatch.
        *self = AmigaEcs::with_ram_config(firmware.to_vec(), *metadata);
    }

    fn tick(&mut self) {
        AmigaEcs::tick(self);
    }

    fn frame_ticks(&self) -> u64 {
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_FRAME_TICKS,
            AgnusRegion::Ntsc => crate::A500_NTSC_FRAME_TICKS,
        }
    }

    fn cck_hz(&self) -> u64 {
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_CCK_HZ,
            AgnusRegion::Ntsc => crate::A500_NTSC_CCK_HZ,
        }
    }

    fn chipset_framebuffer(&self) -> &[u32] {
        self.denise().framebuffer()
    }

    fn mix_audio_stereo(&self) -> (f32, f32) {
        self.paula().mix_audio_stereo()
    }

    fn key_event(&mut self, code: u8, pressed: bool) {
        AmigaEcs::key_event(self, code, pressed);
    }

    fn move_mouse_port0(&mut self, dx: i32, dy: i32) {
        AmigaEcs::move_mouse_port0(self, dx, dy);
    }

    fn set_mouse_button_port0(&mut self, button: &str, pressed: bool) {
        AmigaEcs::set_mouse_button_port0(self, button, pressed);
    }

    fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool) {
        let _ = AmigaEcs::set_joystick_control(self, port, name, pressed);
    }

    fn insert_floppy0(&mut self, adf: Adf, change_pending: bool) {
        if change_pending {
            self.insert_adf_with_change_pending(adf);
        } else {
            self.insert_adf(adf);
        }
    }

    fn snapshot_state(&self) -> Self::Snapshot {
        AmigaEcs::snapshot_state(self)
    }

    fn restore_snapshot_state(&mut self, snapshot: Self::Snapshot) {
        AmigaEcs::restore_snapshot_state(self, snapshot);
    }

    fn variant_query_paths() -> &'static [&'static str] {
        ECS_VARIANT_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<Value>, QueryError> {
        // Same chip-state surface as OCS for now — every query path
        // ECS carries is also valid on OCS. The ECS-only paths
        // (BEAMCON0, BPLCON3 reads) will land alongside whichever
        // verifier flow needs them first.
        let drive = self.drive();
        let drive_status = drive.status();
        let value = match path {
            "amiga.a1000.boot_rom_visible" => json!(self.memory().a1000_boot_rom_visible()),
            "amiga.a1000.wom_locked" => json!(self.memory().a1000_wom_locked()),
            "amiga.memory.overlay" => json!(self.memory().overlay()),
            "amiga.cpu.pc" => json!(self.cpu().regs.pc),
            "amiga.cpu.sr" => json!(self.cpu().regs.sr),
            "amiga.cpu.ipl" => json!(self.cpu().ipl),
            "amiga.agnus.vpos" => json!(self.agnus().vpos),
            "amiga.agnus.hpos" => json!(self.agnus().hpos),
            "amiga.agnus.dmacon" => json!(self.dmacon()),
            "amiga.agnus.bplcon0" => json!(self.bplcon0()),
            "amiga.paula.intena" => json!(self.intena()),
            "amiga.paula.intreq" => json!(self.intreq()),
            "amiga.debug.dsk_write_count" => json!(self.debug_dsk_log.len()),
            "amiga.debug.last_dsk_write" => {
                json!(self.debug_dsk_log.last().map(|(cck, pc, reg, val)| {
                    json!({"cck": cck, "pc": pc, "reg": reg, "val": val})
                }))
            }
            "amiga.display.color00" => json!(self.color(0)),
            "amiga.display.color01" => json!(self.color(1)),
            "amiga.disk.inserted" => json!(drive.has_disk()),
            "amiga.disk.change_pending" => json!(drive_status.disk_change),
            "amiga.disk.cylinder" => json!(drive.cylinder()),
            "amiga.disk.head" => json!(drive.head()),
            "amiga.disk.motor_on" => json!(drive.motor_on()),
            "amiga.disk.motor_spinning" => json!(drive_status.ready),
            "amiga.disk.step_events" => json!(drive.step_event_counter()),
            "amiga.keyboard.state" => json!(self.keyboard().debug_state_name()),
            "amiga.keyboard.queued" => json!(self.keyboard().queued_key_count()),
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}

/// Type alias for the ECS runtime — currently A500+. A600 / A2000B /
/// A3000 land here in later sessions once their machine-specific
/// chips (Gayle, Ramsey, Fat Gary) are ported. The chip stack is
/// AgnusEcs + DeniseEcs over the existing OCS Paula + CIA pair.
pub type AmigaEcsRuntime = AmigaRuntime<AmigaEcs>;

// ===================================================================
// AmigaA1200 impl — AGA chipset, 68EC020, A1200 / (future) CD32 / A4000.
//
// The chip stack uses AGA Alice (Agnus replacement) + AGA Lisa (Denise
// replacement, exposed as `Denise<DeniseAga>`) + Paula 8364 + the same
// two-CIA pair + the AGA-specific Gayle controller (IDE + control
// registers). For the trait impl, the surface is mechanically the same
// as OCS / ECS — only the snapshot type and query-path catalogue differ
// (A1000 paths drop, future Gayle / Akiko paths arrive in Phase 2).
// ===================================================================

/// AGA query paths. Drops the A1000-only paths (no A1200 bootstrap ROM
/// or WOM) and adds AGA-specific paths as Phase 2 chip-level tools
/// land. CPU / Agnus / Paula / disk / keyboard paths share the same
/// names as OCS / ECS so curriculum scripts targeting "amiga.cpu.pc"
/// work across the family.
const AGA_VARIANT_QUERY_PATHS: &[&str] = &[
    "amiga.memory.overlay",
    "amiga.cpu.pc",
    "amiga.cpu.sr",
    "amiga.cpu.ipl",
    "amiga.agnus.vpos",
    "amiga.agnus.hpos",
    "amiga.agnus.dmacon",
    "amiga.agnus.bplcon0",
    "amiga.paula.intena",
    "amiga.paula.intreq",
    "amiga.debug.dsk_write_count",
    "amiga.debug.last_dsk_write",
    "amiga.display.color00",
    "amiga.display.color01",
    "amiga.disk.inserted",
    "amiga.disk.change_pending",
    "amiga.disk.cylinder",
    "amiga.disk.head",
    "amiga.disk.motor_on",
    "amiga.disk.motor_spinning",
    "amiga.disk.step_events",
    "amiga.keyboard.state",
    "amiga.keyboard.queued",
];

impl AmigaMachine for AmigaA1200 {
    const CHIPSET_FB_WIDTH: u32 = FB_WIDTH;
    const CHIPSET_FB_HEIGHT: u32 = FB_HEIGHT;

    type Snapshot = AmigaA1200Snapshot;
    type SnapshotMetadata = RamConfig;

    fn rebuild(&mut self, firmware: &[u8], metadata: &Self::SnapshotMetadata) {
        // A1200 only ever boots from Kickstart 3.0 / 3.1 (512 KiB).
        // There's no A1000-style bootstrap path here; the runtime's
        // `validate_firmware_rom` already gates ROM sizes against
        // `Model::is_a1000()`, which returns false for every AGA
        // model, so the firmware reaches this method already sized
        // for a Kickstart image.
        *self = AmigaA1200::with_ram_config(firmware.to_vec(), *metadata);
    }

    fn tick(&mut self) {
        AmigaA1200::tick(self);
    }

    fn frame_ticks(&self) -> u64 {
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_FRAME_TICKS,
            AgnusRegion::Ntsc => crate::A500_NTSC_FRAME_TICKS,
        }
    }

    fn cck_hz(&self) -> u64 {
        // AGA uses the same master clock as OCS / ECS (28.375160 MHz
        // PAL, 28.636360 MHz NTSC); the chip-RAM bus is double-pumped
        // for 32-bit fetches but the CCK rate at master/8 is unchanged.
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_CCK_HZ,
            AgnusRegion::Ntsc => crate::A500_NTSC_CCK_HZ,
        }
    }

    fn chipset_framebuffer(&self) -> &[u32] {
        self.denise().framebuffer()
    }

    fn mix_audio_stereo(&self) -> (f32, f32) {
        self.paula().mix_audio_stereo()
    }

    fn key_event(&mut self, code: u8, pressed: bool) {
        AmigaA1200::key_event(self, code, pressed);
    }

    fn move_mouse_port0(&mut self, dx: i32, dy: i32) {
        AmigaA1200::move_mouse_port0(self, dx, dy);
    }

    fn set_mouse_button_port0(&mut self, button: &str, pressed: bool) {
        AmigaA1200::set_mouse_button_port0(self, button, pressed);
    }

    fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool) {
        let _ = AmigaA1200::set_joystick_control(self, port, name, pressed);
    }

    fn insert_floppy0(&mut self, adf: Adf, change_pending: bool) {
        if change_pending {
            self.insert_adf_with_change_pending(adf);
        } else {
            self.insert_adf(adf);
        }
    }

    fn snapshot_state(&self) -> Self::Snapshot {
        AmigaA1200::snapshot_state(self)
    }

    fn restore_snapshot_state(&mut self, snapshot: Self::Snapshot) {
        AmigaA1200::restore_snapshot_state(self, snapshot);
    }

    fn variant_query_paths() -> &'static [&'static str] {
        AGA_VARIANT_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<Value>, QueryError> {
        let drive = self.drive();
        let drive_status = drive.status();
        let value = match path {
            "amiga.memory.overlay" => json!(self.memory().overlay()),
            "amiga.cpu.pc" => json!(self.cpu().regs.pc),
            "amiga.cpu.sr" => json!(self.cpu().regs.sr),
            "amiga.cpu.ipl" => json!(self.cpu().ipl),
            "amiga.agnus.vpos" => json!(self.agnus().vpos),
            "amiga.agnus.hpos" => json!(self.agnus().hpos),
            "amiga.agnus.dmacon" => json!(self.dmacon()),
            "amiga.agnus.bplcon0" => json!(self.bplcon0()),
            "amiga.paula.intena" => json!(self.intena()),
            "amiga.paula.intreq" => json!(self.intreq()),
            "amiga.debug.dsk_write_count" => json!(self.debug_dsk_log.len()),
            "amiga.debug.last_dsk_write" => {
                json!(self.debug_dsk_log.last().map(|(cck, pc, reg, val)| {
                    json!({"cck": cck, "pc": pc, "reg": reg, "val": val})
                }))
            }
            "amiga.display.color00" => json!(self.color(0)),
            "amiga.display.color01" => json!(self.color(1)),
            "amiga.disk.inserted" => json!(drive.has_disk()),
            "amiga.disk.change_pending" => json!(drive_status.disk_change),
            "amiga.disk.cylinder" => json!(drive.cylinder()),
            "amiga.disk.head" => json!(drive.head()),
            "amiga.disk.motor_on" => json!(drive.motor_on()),
            "amiga.disk.motor_spinning" => json!(drive_status.ready),
            "amiga.disk.step_events" => json!(drive.step_event_counter()),
            "amiga.keyboard.state" => json!(self.keyboard().debug_state_name()),
            "amiga.keyboard.queued" => json!(self.keyboard().queued_key_count()),
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}

/// Type alias for the AGA runtime — covers A1200 today, with A4000 /
/// CD32 to land here once their machine-specific chips (Fat Gary +
/// Ramsey for A4000, Akiko for CD32) are ported.
pub type AmigaA1200Runtime = AmigaRuntime<AmigaA1200>;

// ===================================================================
// AmigaRuntimeKind — runtime-time dispatch over OCS / ECS / AGA.
//
// Verifier binaries (emu198x-amiga, emu198x-script-amiga) take a
// `--model` argument that may pick either an OCS or an ECS variant.
// Storing a concrete `AmigaOcsRuntime` field in the binary forces
// every model through OCS chips even when the Model is ECS-flavoured
// (e.g. `A500PlusEcsPal`). `AmigaRuntimeKind` is the dispatcher: it
// wraps either runtime type and forwards the `MachineCore` surface
// to the inner case based on `Model::is_ecs()`.
// ===================================================================

/// Runtime-time dispatch over the available Amiga machine kinds.
/// Constructed via `AmigaRuntimeKind::new(model, firmware)` (or
/// `from_firmware` / `blank`); the inner case is picked by
/// `Model::is_ecs()`. Implements `MachineCore` so callers can drive
/// it like any other runtime.
pub enum AmigaRuntimeKind {
    /// OCS chip stack — A1000, A500, A500-A501, A500-Maxed (PAL/NTSC).
    Ocs(AmigaOcsRuntime),
    /// ECS chip stack — A500+ today (PAL/NTSC); A600 / A2000B / A3000
    /// will land here once their machine-specific chips are ported.
    Ecs(AmigaEcsRuntime),
    /// AGA chip stack — A1200 today (PAL/NTSC); A4000 / CD32 land
    /// here once their machine-specific chips (Fat Gary + Ramsey for
    /// A4000, Akiko for CD32) are ported.
    Aga(AmigaA1200Runtime),
}

impl AmigaRuntimeKind {
    /// Construct using the model's preset RAM layout. Picks OCS or
    /// ECS based on `Model::is_ecs()`.
    ///
    /// # Errors
    /// Returns the underlying `MachineError` from the dispatched
    /// runtime constructor.
    pub fn new(model: Model, firmware_rom: Vec<u8>) -> Result<Self, emu198x_shell::MachineError> {
        if model.is_aga() {
            AmigaA1200Runtime::new(model, firmware_rom).map(Self::Aga)
        } else if model.is_ecs() {
            AmigaEcsRuntime::new(model, firmware_rom).map(Self::Ecs)
        } else {
            AmigaOcsRuntime::new(model, firmware_rom).map(Self::Ocs)
        }
    }

    /// Construct from the profile's firmware set.
    ///
    /// # Errors
    /// Returns the underlying `MachineError` from the dispatched
    /// runtime constructor.
    pub fn from_firmware(
        model: Model,
        firmware: &emu198x_shell::FirmwareSet<'_>,
    ) -> Result<Self, emu198x_shell::MachineError> {
        if model.is_aga() {
            AmigaA1200Runtime::from_firmware(model, firmware).map(Self::Aga)
        } else if model.is_ecs() {
            AmigaEcsRuntime::from_firmware(model, firmware).map(Self::Ecs)
        } else {
            AmigaOcsRuntime::from_firmware(model, firmware).map(Self::Ocs)
        }
    }

    /// Construct with a zero-filled placeholder firmware. Useful for
    /// tests and verifier dry-runs.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        if model.is_aga() {
            Self::Aga(AmigaA1200Runtime::blank(model))
        } else if model.is_ecs() {
            Self::Ecs(AmigaEcsRuntime::blank(model))
        } else {
            Self::Ocs(AmigaOcsRuntime::blank(model))
        }
    }

    /// Active model — same on each inner case.
    #[must_use]
    pub fn model(&self) -> Model {
        match self {
            Self::Ocs(rt) => rt.model(),
            Self::Ecs(rt) => rt.model(),
            Self::Aga(rt) => rt.model(),
        }
    }

    /// Read-back: was this runtime constructed against the ECS chip
    /// stack? Equivalent to `self.model().is_ecs()` but reads the
    /// dispatched-case directly.
    #[must_use]
    pub fn is_ecs(&self) -> bool {
        matches!(self, Self::Ecs(_))
    }

    /// Read-back: was this runtime constructed against the AGA chip
    /// stack? Equivalent to `self.model().is_aga()` but reads the
    /// dispatched-case directly.
    #[must_use]
    pub fn is_aga(&self) -> bool {
        matches!(self, Self::Aga(_))
    }
}

impl emu198x_shell::MachineCore for AmigaRuntimeKind {
    fn profile(&self) -> &emu198x_shell::MachineProfile {
        match self {
            Self::Ocs(rt) => rt.profile(),
            Self::Ecs(rt) => rt.profile(),
            Self::Aga(rt) => rt.profile(),
        }
    }

    fn time(&self) -> emu198x_shell::MachineTime {
        match self {
            Self::Ocs(rt) => rt.time(),
            Self::Ecs(rt) => rt.time(),
            Self::Aga(rt) => rt.time(),
        }
    }

    fn reset(&mut self, kind: emu198x_shell::ResetKind) {
        match self {
            Self::Ocs(rt) => rt.reset(kind),
            Self::Ecs(rt) => rt.reset(kind),
            Self::Aga(rt) => rt.reset(kind),
        }
    }

    fn load_media(
        &mut self,
        media: &emu198x_shell::MediaSet<'_>,
    ) -> Result<(), emu198x_shell::MachineError> {
        match self {
            Self::Ocs(rt) => rt.load_media(media),
            Self::Ecs(rt) => rt.load_media(media),
            Self::Aga(rt) => rt.load_media(media),
        }
    }

    fn run_until(
        &mut self,
        target: emu198x_shell::MachineTime,
        host: &mut emu198x_shell::HostIo<'_>,
    ) -> Result<emu198x_shell::RunResult, emu198x_shell::MachineError> {
        match self {
            Self::Ocs(rt) => rt.run_until(target, host),
            Self::Ecs(rt) => rt.run_until(target, host),
            Self::Aga(rt) => rt.run_until(target, host),
        }
    }

    fn snapshot(&self) -> Result<Vec<u8>, emu198x_shell::MachineError> {
        match self {
            Self::Ocs(rt) => rt.snapshot(),
            Self::Ecs(rt) => rt.snapshot(),
            Self::Aga(rt) => rt.snapshot(),
        }
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), emu198x_shell::MachineError> {
        match self {
            Self::Ocs(rt) => rt.restore(bytes),
            Self::Ecs(rt) => rt.restore(bytes),
            Self::Aga(rt) => rt.restore(bytes),
        }
    }

    fn command(
        &mut self,
        command: &emu198x_shell::ControlCommand,
    ) -> Result<(), emu198x_shell::MachineError> {
        match self {
            Self::Ocs(rt) => rt.command(command),
            Self::Ecs(rt) => rt.command(command),
            Self::Aga(rt) => rt.command(command),
        }
    }

    fn capabilities(&self) -> emu198x_shell::CapabilitySet {
        match self {
            Self::Ocs(rt) => rt.capabilities(),
            Self::Ecs(rt) => rt.capabilities(),
            Self::Aga(rt) => rt.capabilities(),
        }
    }
}

// Audio-control surface. AudioControls and PaulaChannel are the same
// types in both machine crates (re-exported from commodore-paula-8364),
// so the wrapper just dispatches.
impl AmigaRuntimeKind {
    #[must_use]
    pub fn audio_controls(&self) -> machine_commodore_amiga_ocs::AudioControls {
        match self {
            Self::Ocs(rt) => rt.audio_controls(),
            Self::Ecs(rt) => rt.audio_controls(),
            Self::Aga(rt) => rt.audio_controls(),
        }
    }

    pub fn set_audio_controls(&mut self, controls: machine_commodore_amiga_ocs::AudioControls) {
        match self {
            Self::Ocs(rt) => rt.set_audio_controls(controls),
            Self::Ecs(rt) => rt.set_audio_controls(controls),
            Self::Aga(rt) => rt.set_audio_controls(controls),
        }
    }

    pub fn set_audio_channel_enabled(
        &mut self,
        channel: machine_commodore_amiga_ocs::PaulaChannel,
        enabled: bool,
    ) {
        match self {
            Self::Ocs(rt) => rt.set_audio_channel_enabled(channel, enabled),
            Self::Ecs(rt) => rt.set_audio_channel_enabled(channel, enabled),
            Self::Aga(rt) => rt.set_audio_channel_enabled(channel, enabled),
        }
    }

    pub fn set_audio_channel_gain(
        &mut self,
        channel: machine_commodore_amiga_ocs::PaulaChannel,
        gain: f32,
    ) {
        match self {
            Self::Ocs(rt) => rt.set_audio_channel_gain(channel, gain),
            Self::Ecs(rt) => rt.set_audio_channel_gain(channel, gain),
            Self::Aga(rt) => rt.set_audio_channel_gain(channel, gain),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Model;

    /// Spec invariant: every advertised variant query path is unique.
    /// Doubles would silently clobber each other in a sorted listing.
    #[test]
    fn ocs_variant_query_paths_are_unique() {
        let mut sorted: Vec<&&str> = OCS_VARIANT_QUERY_PATHS.iter().collect();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(sorted.len(), deduped.len(), "duplicate variant query paths");
    }

    #[test]
    fn aga_variant_query_paths_are_unique() {
        let mut sorted: Vec<&&str> = AGA_VARIANT_QUERY_PATHS.iter().collect();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(sorted.len(), deduped.len(), "duplicate AGA variant query paths");
    }

    #[test]
    fn blank_dispatches_a1200_to_aga_variant() {
        let kind = AmigaRuntimeKind::blank(Model::A1200AgaPal);
        assert!(kind.is_aga(), "A1200 should land in the Aga arm");
        assert!(!kind.is_ecs());
        assert_eq!(kind.model(), Model::A1200AgaPal);
    }

    #[test]
    fn blank_dispatches_a500_to_ocs_variant() {
        let kind = AmigaRuntimeKind::blank(Model::A500OcsPal);
        assert!(!kind.is_aga());
        assert!(!kind.is_ecs());
        assert_eq!(kind.model(), Model::A500OcsPal);
    }

    #[test]
    fn blank_dispatches_a500plus_to_ecs_variant() {
        let kind = AmigaRuntimeKind::blank(Model::A500PlusEcsPal);
        assert!(kind.is_ecs());
        assert!(!kind.is_aga());
        assert_eq!(kind.model(), Model::A500PlusEcsPal);
    }
}
