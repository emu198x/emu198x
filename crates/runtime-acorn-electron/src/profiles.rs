//! Acorn Electron family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Model {
    /// The Electron as sold: tape only.
    #[default]
    Electron,
    /// The Electron with the Acorn Plus 3 fitted: a WD1770 disc interface,
    /// two 3.5" drives and ADFS 1.00 in paged ROM 4.
    ElectronPlus3,
}

impl Model {
    pub const ALL: [Self; 2] = [Self::Electron, Self::ElectronPlus3];
    pub const VARIANT_IDS: [&'static str; 2] = [
        Self::Electron.variant_id(),
        Self::ElectronPlus3.variant_id(),
    ];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.profile_id()
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|model| model.variant_id() == id)
    }

    /// Whether the model has the Plus 3, and so ADFS and disc drives.
    #[must_use]
    pub const fn has_plus3(self) -> bool {
        matches!(self, Self::ElectronPlus3)
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        let mut sources = vec![
            emu198x_shell::FirmwareSource::required(OS_FIRMWARE_ID, &["os.rom"])
                .with_env_var("EMU198X_ELECTRON_OS"),
            emu198x_shell::FirmwareSource::required(BASIC_FIRMWARE_ID, &["basic.rom"])
                .with_env_var("EMU198X_ELECTRON_BASIC"),
        ];
        if self.has_plus3() {
            sources.push(
                emu198x_shell::FirmwareSource::optional(ADFS_FIRMWARE_ID, &["adfs.rom"])
                    .with_env_var("EMU198X_ELECTRON_ADFS"),
            );
        }
        sources
    }

    /// Existing host frame budget in native machine ticks.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        39_936
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Electron => "acorn-electron",
            Self::ElectronPlus3 => "acorn-electron-plus3",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Electron => "Acorn Electron",
            Self::ElectronPlus3 => "Acorn Electron with Plus 3",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

/// OS ROM firmware identifier (16 KB).
pub const OS_FIRMWARE_ID: &str = "acorn-electron-os";
/// BASIC ROM firmware identifier (16 KB).
pub const BASIC_FIRMWARE_ID: &str = "acorn-electron-basic";
/// The Plus 3's ADFS 1.00 ROM (16 KB), answering in paged ROM 4.
pub const ADFS_FIRMWARE_ID: &str = "acorn-electron-adfs";

/// The Plus 3's disc slots, by drive number.
pub const DISK_SLOTS: [&str; 2] = ["drive-0", "drive-1"];

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    let mut firmware = vec![
        FirmwareRequirement::new(OS_FIRMWARE_ID, "Electron OS ROM (16 KB)", false),
        FirmwareRequirement::new(BASIC_FIRMWARE_ID, "BBC BASIC II ROM (16 KB)", false),
    ];
    let mut media_slots = vec![MediaSlot::new(
        "tape-1",
        "Cassette Tape",
        MediaKind::Tape,
        false,
        WritebackPolicy::SidecarOnly,
    )];
    let mut capabilities = CapabilitySet::with_all([
        known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
        known_capability("keyboard-input"),
        known_capability("scripted-input"),
    ]);
    let summary = if model.has_plus3() {
        firmware.push(FirmwareRequirement::new(
            ADFS_FIRMWARE_ID,
            "Plus 3 ADFS 1.00 ROM (16 KB, paged ROM 4)",
            true,
        ));
        for (slot, label) in DISK_SLOTS.into_iter().zip(["Disc Drive 0", "Disc Drive 1"]) {
            media_slots.push(MediaSlot::new(
                slot,
                label,
                MediaKind::Disk,
                false,
                WritebackPolicy::InMemoryOnly,
            ));
        }
        capabilities.insert(known_capability("disk-input"));
        capabilities.insert(known_capability(
            emu198x_shell::capability::ids::MEDIA_EXPORT,
        ));
        "Acorn Electron with the Plus 3 — a WD1770 disc interface, two 3.5\" drives and ADFS 1.00."
    } else {
        "Acorn Electron — 6502A + ULA + 32 KB RAM, dual 16 KB ROMs (OS + BBC BASIC II)."
    };
    MachineProfile {
        machine_id: MachineId::from("acorn-electron"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: if model.has_plus3() { 1984 } else { 1983 },
        summary: summary.into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(2_000_000)),
        firmware,
        media_slots,
        capabilities,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_declares_dual_firmware() {
        let p = profile_for(Model::Electron);
        assert_eq!(p.firmware.len(), 2);
        assert_eq!(p.firmware[0].id.as_ref(), OS_FIRMWARE_ID);
        assert_eq!(p.firmware[1].id.as_ref(), BASIC_FIRMWARE_ID);
    }

    #[test]
    fn the_plus3_adds_adfs_and_two_drives() {
        let p = profile_for(Model::ElectronPlus3);
        let firmware: Vec<_> = p.firmware.iter().map(|f| f.id.as_ref()).collect();
        assert_eq!(
            firmware,
            [OS_FIRMWARE_ID, BASIC_FIRMWARE_ID, ADFS_FIRMWARE_ID]
        );
        let slots: Vec<_> = p.media_slots.iter().map(|s| s.id.as_ref()).collect();
        assert_eq!(slots, ["tape-1", "drive-0", "drive-1"]);
        assert_eq!(
            Model::from_variant_id("acorn-electron-plus3"),
            Some(Model::ElectronPlus3)
        );
        assert_eq!(profiles().len(), 2);
        assert_eq!(profile_for(Model::Electron).media_slots.len(), 1);
        assert_eq!(Model::Electron.firmware_sources().len(), 2);
    }
}
