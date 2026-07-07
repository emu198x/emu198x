//! Per-port IEC drive dispatch.
//!
//! The C64 IEC bus carries devices 8–11. Each port can hold any of the three
//! C64-compatible disk drives, and the user chooses the model per port. This
//! module wraps the three drive cores behind one [`IecDrive`] enum so the
//! runtime can hold a heterogeneous set and drive them through a single
//! dispatch surface (tick, sync, snapshot, media load).
//!
//! The drives keep their own memory-bus wiring internally; this enum only
//! forwards the runtime-facing calls, so it does not reintroduce a shared
//! CPU-bus trait (see `RULES.md`).

use common_commodore_iec::IecBus;
use format_commodore_c64_g64::{G64_SIGNATURE, G71_SIGNATURE};
use machine_commodore_1541::{Drive1541, Drive1541Config, Drive1541Snapshot};
use machine_commodore_1571::{Drive1571, Drive1571Config, Drive1571Snapshot};
use machine_commodore_1581::{Drive1581, Drive1581Config, Drive1581Snapshot};
use serde::{Deserialize, Serialize};

/// A standard double-sided `D71` image is a `D64` per side; these are the two
/// canonical byte lengths (without and with the trailing error-info block).
const D71_STANDARD_SIZE: usize = 349_696;
const D71_STANDARD_WITH_ERROR_INFO_SIZE: usize = 351_062;

/// The disk-drive model attached to an IEC port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriveKind {
    /// Commodore 1541 — single-sided 5.25", two 6522 VIAs with GCR serialiser.
    C1541,
    /// Commodore 1571 — double-sided 5.25", 1541-compatible in C64 mode.
    C1571,
    /// Commodore 1581 — 3.5" 800 KB, 8520 CIA plus a WD177x controller.
    C1581,
}

impl DriveKind {
    /// The firmware id of the DOS ROM this model needs.
    #[must_use]
    pub const fn dos_rom_id(self) -> &'static str {
        match self {
            Self::C1541 => "commodore-1541-dos-rom",
            Self::C1571 => "commodore-1571-dos-rom",
            Self::C1581 => "commodore-1581-dos-rom",
        }
    }
}

/// One drive attached to an IEC port, dispatched by model. The three cores
/// differ greatly in size (the 1571 is double-sided), so each is boxed to keep
/// the enum — and the runtime's `[IecDrive; 4]` port array — small.
pub enum IecDrive {
    /// A 1541.
    C1541(Box<Drive1541>),
    /// A 1571.
    C1571(Box<Drive1571>),
    /// A 1581.
    C1581(Box<Drive1581>),
}

/// Model-tagged snapshot of an [`IecDrive`], so restore rebuilds the right core.
#[derive(Serialize, Deserialize)]
pub(crate) enum IecDriveSnapshot {
    C1541(Box<Drive1541Snapshot>),
    C1571(Box<Drive1571Snapshot>),
    C1581(Box<Drive1581Snapshot>),
}

impl IecDrive {
    /// Builds a drive of `kind` from its DOS ROM, assigns it `device_number`,
    /// and syncs it onto `bus`. The device number is set before the sync so the
    /// drive presents the right address on the bus immediately.
    ///
    /// # Errors
    ///
    /// Returns the core's init-error message if the ROM is invalid.
    pub(crate) fn build(
        kind: DriveKind,
        dos_rom: &[u8],
        device_number: u8,
        bus: &mut IecBus,
    ) -> Result<Self, String> {
        let mut drive = match kind {
            DriveKind::C1541 => Self::C1541(Box::new(
                Drive1541::new(Drive1541Config { dos_rom }).map_err(|e| e.to_string())?,
            )),
            DriveKind::C1571 => Self::C1571(Box::new(
                Drive1571::new(Drive1571Config { dos_rom }).map_err(|e| e.to_string())?,
            )),
            DriveKind::C1581 => Self::C1581(Box::new(
                Drive1581::new(Drive1581Config { dos_rom }).map_err(|e| e.to_string())?,
            )),
        };
        drive.set_device_number(device_number);
        drive.sync_iec_bus(bus);
        Ok(drive)
    }

    /// The drive model.
    #[must_use]
    pub(crate) const fn kind(&self) -> DriveKind {
        match self {
            Self::C1541(_) => DriveKind::C1541,
            Self::C1571(_) => DriveKind::C1571,
            Self::C1581(_) => DriveKind::C1581,
        }
    }

    /// The drive CPU clock in Hz (the 1581 runs at 2 MHz; the others at 1 MHz).
    #[must_use]
    pub(crate) const fn cpu_hz(&self) -> u64 {
        match self {
            Self::C1541(_) => machine_commodore_1541::DRIVE1541_CPU_HZ,
            Self::C1571(_) => machine_commodore_1571::DRIVE1571_CPU_HZ,
            Self::C1581(_) => machine_commodore_1581::DRIVE1581_CPU_HZ,
        }
    }

    /// The drive CPU cycle count, used by the interleave scheduler.
    #[must_use]
    pub(crate) fn cycles(&self) -> u64 {
        match self {
            Self::C1541(d) => d.cycles(),
            Self::C1571(d) => d.cycles(),
            Self::C1581(d) => d.cycles(),
        }
    }

    /// Sets the IEC device number (8–11).
    pub(crate) fn set_device_number(&mut self, device_number: u8) {
        match self {
            Self::C1541(d) => d.set_device_number(device_number),
            Self::C1571(d) => d.set_device_number(device_number),
            Self::C1581(d) => d.set_device_number(device_number),
        }
    }

    /// Advances the drive one CPU cycle against the shared IEC bus.
    pub(crate) fn tick_with_iec_bus(&mut self, bus: &mut IecBus) {
        match self {
            Self::C1541(d) => {
                d.tick_with_iec_bus(bus);
            }
            Self::C1571(d) => {
                d.tick_with_iec_bus(bus);
            }
            Self::C1581(d) => {
                d.tick_with_iec_bus(bus);
            }
        }
    }

    /// Re-samples the drive's view of the IEC bus after another participant
    /// drove it this cycle.
    pub(crate) fn sync_iec_bus(&mut self, bus: &mut IecBus) {
        match self {
            Self::C1541(d) => d.sync_iec_bus(bus),
            Self::C1571(d) => d.sync_iec_bus(bus),
            Self::C1581(d) => d.sync_iec_bus(bus),
        }
    }

    /// Loads a disk image, dispatching to the drive's native format. A `G64`
    /// raw-GCR image (detected by signature) loads read-only into a 1541 or
    /// 1571; a 1571 also takes a `D71` (double-sided) or `D64`; the others take
    /// their one sector format.
    ///
    /// # Errors
    ///
    /// Returns the format parser's error message when the bytes do not match, or
    /// when a `G64` is offered to the 1581 (which has no GCR mechanism).
    pub(crate) fn load_disk(&mut self, bytes: &[u8], writable: bool) -> Result<(), String> {
        let is_g64 = bytes.starts_with(G64_SIGNATURE) || bytes.starts_with(G71_SIGNATURE);
        match self {
            Self::C1541(d) if is_g64 => d
                .load_g64_bytes_writable(bytes, writable)
                .map_err(|e| e.to_string()),
            Self::C1571(d) if is_g64 => d
                .load_g64_bytes_writable(bytes, writable)
                .map_err(|e| e.to_string()),
            Self::C1581(_) if is_g64 => Err(
                "G64 raw-GCR images are not supported on the 1581 (it has no GCR \
                     mechanism); use a 1541 or 1571"
                    .to_owned(),
            ),
            Self::C1541(d) => d
                .load_d64_bytes_writable(bytes, writable)
                .map_err(|e| e.to_string()),
            Self::C1571(d) => {
                if is_d71_length(bytes.len()) {
                    d.load_d71_bytes_writable(bytes, writable)
                        .map_err(|e| e.to_string())
                } else {
                    d.load_d64_bytes_writable(bytes, writable)
                        .map_err(|e| e.to_string())
                }
            }
            Self::C1581(d) => d
                .load_d81_bytes_writable(bytes, writable)
                .map_err(|e| e.to_string()),
        }
    }

    /// Whether a disk is currently inserted, whatever the model.
    #[must_use]
    pub(crate) fn disk_inserted(&self) -> bool {
        match self {
            Self::C1541(d) => d.disk_inserted(),
            Self::C1571(d) => d.disk_inserted(),
            Self::C1581(d) => d.disk_inserted(),
        }
    }

    /// Ejects any inserted disk.
    pub(crate) fn eject_disk(&mut self) {
        match self {
            Self::C1541(d) => d.eject_disk(),
            Self::C1571(d) => d.eject_disk(),
            Self::C1581(d) => d.eject_disk(),
        }
    }

    /// Decodes the live surface back into an image for a SAVE write-back, or
    /// `None` when no disk is present.
    #[must_use]
    pub(crate) fn flush_image(&self) -> Option<Vec<u8>> {
        match self {
            Self::C1541(d) => d.flush_image(),
            Self::C1571(d) => d.flush_image(),
            Self::C1581(d) => d.flush_image(),
        }
    }

    /// Captures a model-tagged snapshot of the drive.
    #[must_use]
    pub(crate) fn snapshot(&self) -> IecDriveSnapshot {
        match self {
            Self::C1541(d) => IecDriveSnapshot::C1541(Box::new(d.snapshot_state())),
            Self::C1571(d) => IecDriveSnapshot::C1571(Box::new(d.snapshot_state())),
            Self::C1581(d) => IecDriveSnapshot::C1581(Box::new(d.snapshot_state())),
        }
    }

    /// Rebuilds a drive from a model-tagged snapshot.
    ///
    /// # Errors
    ///
    /// Returns the core's error message when the snapshot is malformed.
    pub(crate) fn from_snapshot(snapshot: IecDriveSnapshot) -> Result<Self, String> {
        Ok(match snapshot {
            IecDriveSnapshot::C1541(s) => Self::C1541(Box::new(Drive1541::from_snapshot(*s)?)),
            IecDriveSnapshot::C1571(s) => Self::C1571(Box::new(Drive1571::from_snapshot(*s)?)),
            IecDriveSnapshot::C1581(s) => Self::C1581(Box::new(Drive1581::from_snapshot(*s)?)),
        })
    }

    /// The wrapped 1541, when this port holds one.
    #[must_use]
    pub(crate) fn as_1541(&self) -> Option<&Drive1541> {
        match self {
            Self::C1541(d) => Some(d),
            _ => None,
        }
    }

    /// The wrapped 1581, when this port holds one.
    #[must_use]
    pub(crate) fn as_1581(&self) -> Option<&Drive1581> {
        match self {
            Self::C1581(d) => Some(d),
            _ => None,
        }
    }
}

/// Whether a byte length is a standard `D71` image (a 1571 double-sided disk).
const fn is_d71_length(len: usize) -> bool {
    matches!(len, D71_STANDARD_SIZE | D71_STANDARD_WITH_ERROR_INFO_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal valid G64 with a short raw-GCR stream on track 1 (slot 0).
    fn minimal_g64() -> Vec<u8> {
        let num_half = 84usize;
        let mut buf = Vec::new();
        buf.extend_from_slice(G64_SIGNATURE);
        buf.push(0);
        buf.push(num_half as u8);
        buf.extend_from_slice(&7928u16.to_le_bytes());
        let data_base = 12 + num_half * 8;
        let mut offsets = vec![0u32; num_half];
        let mut speeds = vec![0u32; num_half];
        offsets[0] = data_base as u32;
        speeds[0] = 3;
        for o in &offsets {
            buf.extend_from_slice(&o.to_le_bytes());
        }
        for s in &speeds {
            buf.extend_from_slice(&s.to_le_bytes());
        }
        buf.extend_from_slice(&64u16.to_le_bytes());
        buf.extend_from_slice(&[0x55u8; 64]);
        buf
    }

    #[test]
    fn load_disk_routes_g64_to_gcr_drives_and_rejects_the_1581() {
        let mut bus = IecBus::new();
        let g64 = minimal_g64();

        for kind in [DriveKind::C1541, DriveKind::C1571] {
            // The 1541 DOS ROM is 16 KB, the 1571's is 32 KB.
            let rom_bytes = match kind {
                DriveKind::C1541 => vec![0xEAu8; 0x4000],
                _ => vec![0xEAu8; 0x8000],
            };
            let mut drive =
                IecDrive::build(kind, &rom_bytes, 8, &mut bus).expect("stub ROM builds a drive");
            drive
                .load_disk(&g64, false)
                .expect("G64 loads into a GCR drive");
            assert!(drive.disk_inserted());
            assert!(drive.flush_image().is_none(), "G64 is read-only");
        }

        // The 1581 has no GCR mechanism, so a G64 is rejected there.
        let rom_1581 = vec![0xEAu8; 0x8000];
        let mut d1581 =
            IecDrive::build(DriveKind::C1581, &rom_1581, 9, &mut bus).expect("stub 1581 builds");
        assert!(d1581.load_disk(&g64, false).is_err());
    }
}
