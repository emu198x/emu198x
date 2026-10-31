//! Amstrad CPC family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

/// The CPC models this runtime can build.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Model {
    /// 64 KB and a cassette deck.
    #[default]
    Cpc464,
    /// 64 KB and a 3" disc drive.
    Cpc664,
    /// 128 KB, banked, and a 3" disc drive.
    Cpc6128,
}

impl Model {
    pub const VARIANT_IDS: [&'static str; 3] =
        ["amstrad-cpc464", "amstrad-cpc664", "amstrad-cpc6128"];

    const ALL: [Self; 3] = [Self::Cpc464, Self::Cpc664, Self::Cpc6128];

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|model| model.profile_id() == id)
    }

    /// The machine this model builds.
    #[must_use]
    pub const fn machine_model(self) -> common_amstrad_cpc::CpcModel {
        match self {
            Self::Cpc464 => common_amstrad_cpc::CpcModel::Cpc464,
            Self::Cpc664 => common_amstrad_cpc::CpcModel::Cpc664,
            Self::Cpc6128 => common_amstrad_cpc::CpcModel::Cpc6128,
        }
    }

    /// Whether the model has a disc drive, and so AMSDOS and disc slots.
    #[must_use]
    pub const fn has_disc(self) -> bool {
        self.machine_model().has_disc()
    }

    /// The id of the model's 32 KB OS + BASIC firmware.
    #[must_use]
    pub const fn firmware_id(self) -> &'static str {
        match self {
            Self::Cpc464 => ROM_FIRMWARE_ID,
            Self::Cpc664 => "amstrad-cpc664-firmware",
            Self::Cpc6128 => "amstrad-cpc6128-firmware",
        }
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        let (files, env): (&'static [&'static str], _) = match self {
            Self::Cpc464 => (&["cpc464.rom"], "EMU198X_CPC464_ROM"),
            Self::Cpc664 => (&["cpc664.rom"], "EMU198X_CPC664_ROM"),
            Self::Cpc6128 => (&["cpc6128.rom"], "EMU198X_CPC6128_ROM"),
        };
        let mut sources = vec![
            emu198x_shell::FirmwareSource::required(self.firmware_id(), files).with_env_var(env),
        ];
        if self.has_disc() {
            sources.push(
                emu198x_shell::FirmwareSource::required(
                    AMSDOS_FIRMWARE_ID,
                    &["amsdos.rom", "cpcados.rom"],
                )
                .with_env_var("EMU198X_CPC_AMSDOS_ROM"),
            );
        }
        sources
    }

    /// Existing host frame budget; chip timing is unchanged.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        64 * 312 * 4
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Cpc464 => "amstrad-cpc464",
            Self::Cpc664 => "amstrad-cpc664",
            Self::Cpc6128 => "amstrad-cpc6128",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Cpc464 => "Amstrad CPC464",
            Self::Cpc664 => "Amstrad CPC664",
            Self::Cpc6128 => "Amstrad CPC6128",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

/// The CPC464's firmware: 16 KB OS + 16 KB BASIC.
pub const ROM_FIRMWARE_ID: &str = "amstrad-cpc464-firmware";
/// AMSDOS, the 16 KB disc ROM fitted as upper ROM 7 on a 664 or 6128.
pub const AMSDOS_FIRMWARE_ID: &str = "amstrad-cpc-amsdos";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    let (release_year, summary) = match model {
        Model::Cpc464 => (
            1984,
            "Amstrad CPC464 — Z80A at 4 MHz, Gate Array, HD6845S CRTC, AY-3-8912, Intel 8255 PPI, 64 KB RAM, 32 KB firmware, cassette.",
        ),
        Model::Cpc664 => (
            1985,
            "Amstrad CPC664 — the CPC464 with a 3\" disc drive in place of the cassette deck: µPD765A FDC and AMSDOS, 64 KB RAM.",
        ),
        Model::Cpc6128 => (
            1985,
            "Amstrad CPC6128 — Z80A at 4 MHz, 128 KB banked RAM, a 3\" disc drive with µPD765A FDC and AMSDOS, cassette port.",
        ),
    };
    let mut firmware = vec![FirmwareRequirement::new(
        model.firmware_id(),
        "CPC firmware (32 KB: 16 KB OS + 16 KB BASIC)",
        false,
    )];
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
    if model.has_disc() {
        firmware.push(FirmwareRequirement::new(
            AMSDOS_FIRMWARE_ID,
            "AMSDOS (16 KB, upper ROM 7)",
            false,
        ));
        for (slot, label) in [("disk-a", "Disc Drive A:"), ("disk-b", "Disc Drive B:")] {
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
    }
    MachineProfile {
        machine_id: MachineId::from(model.profile_id()),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year,
        summary: summary.into(),
        // The Z80 runs at 4 MHz off a 16 MHz crystal; the runtime's time unit
        // is that T-state, which is what `AmstradCpc::run_frame` returns.
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(4_000_000)),
        firmware,
        media_slots,
        capabilities,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_disc_models_add_amsdos_and_two_drives() {
        for model in [Model::Cpc664, Model::Cpc6128] {
            let p = profile_for(model);
            let firmware: Vec<_> = p.firmware.iter().map(|f| f.id.as_ref()).collect();
            assert_eq!(firmware, [model.firmware_id(), AMSDOS_FIRMWARE_ID]);
            let slots: Vec<_> = p.media_slots.iter().map(|s| s.id.as_ref()).collect();
            assert_eq!(slots, ["tape-1", "disk-a", "disk-b"]);
            assert_eq!(Model::from_variant_id(model.profile_id()), Some(model));
        }
        assert_eq!(profiles().len(), 3);
    }

    #[test]
    fn profile_declares_firmware_and_a_tape_slot() {
        let p = profile_for(Model::Cpc464);
        assert_eq!(p.firmware.len(), 1);
        assert_eq!(p.firmware[0].id, ROM_FIRMWARE_ID);
        assert_eq!(p.media_slots.len(), 1);
        assert_eq!(p.media_slots[0].id, "tape-1");
    }
}
