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

    /// Loads a disk image, dispatching to the drive's native format. A 1571
    /// takes a `D71` (double-sided) or a `D64` (single-sided); the others take
    /// their one format.
    ///
    /// # Errors
    ///
    /// Returns the format parser's error message when the bytes do not match.
    pub(crate) fn load_disk(&mut self, bytes: &[u8], writable: bool) -> Result<(), String> {
        match self {
            Self::C1541(d) => d
                .load_d64_bytes_writable(bytes, writable)
                .map_err(|e| e.to_string()),
            Self::C1571(d) => {
                if is_d71_length(bytes.len()) {
                    d.load_d71_bytes(bytes).map_err(|e| e.to_string())
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
