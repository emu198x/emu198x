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
//! See `wiki/decisions/runtime-internal-shape.md` for the playbook
//! and the Amiga long-term-scope memory note for the full target
//! list (Vampire AC68080 + SAGA + RTG framebuffer slots, plus the
//! PAL/NTSC region matrix with NTSC's short/long line alternation
//! still pending in the chip layer).

use emu198x_shell::QueryError;
use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga_ocs::{AmigaOcs, AmigaOcsSnapshot, FB_HEIGHT, FB_WIDTH, RamConfig};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::AmigaRuntime;

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
        crate::A500_PAL_FRAME_TICKS
    }

    fn cck_hz(&self) -> u64 {
        crate::A500_PAL_CCK_HZ
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

/// Type alias for the OCS PAL runtime — covers A1000 + A500 family +
/// A500+ + maxed-A500. The same `AmigaOcs` machine struct serves all
/// of them via different `RamConfig` presets and bootstrap modes;
/// the runtime layer distinguishes them through `Model`.
pub type AmigaOcsRuntime = AmigaRuntime<AmigaOcs>;

#[cfg(test)]
mod tests {
    use super::*;

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
}
