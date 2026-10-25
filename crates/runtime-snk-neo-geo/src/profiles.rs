//! SNK Neo Geo profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};
use machine_snk_neo_geo::MASTER_HZ;

use crate::NeoGeoRuntime;

/// The AES BIOS: the 68000's system ROM.
pub const BIOS_FIRMWARE_ID: &str = "neo-geo-aes-bios";
/// The L0 ROM: the LSPC2's vertical shrink table. Without it the LSPC2
/// computes an approximation.
pub const ZOOM_FIRMWARE_ID: &str = "neo-geo-l0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// The AES home console, NTSC timing.
    Aes,
}

impl Model {
    pub const ALL: [Self; 1] = [Self::Aes];
    pub const VARIANT_IDS: [&'static str; 1] = ["aes"];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::Aes => "aes",
        }
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|model| model.variant_id() == id || model.profile_id() == id)
    }

    /// One frame in master clocks: 264 lines of 384 pixels, 4 clocks each.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        264 * 384 * 4
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Aes => "snk-neo-geo-aes",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Aes => "SNK Neo Geo AES",
        }
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        vec![
            emu198x_shell::FirmwareSource::required(
                BIOS_FIRMWARE_ID,
                &["neo-epo.bin", "neo-po.bin"],
            )
            .with_env_var("EMU198X_NEOGEO_BIOS"),
            emu198x_shell::FirmwareSource::optional(ZOOM_FIRMWARE_ID, &["000-lo.lo"])
                .with_env_var("EMU198X_NEOGEO_L0"),
        ]
    }
}

/// A runtime with no BIOS and no cartridge.
#[must_use]
pub fn blank(model: Model) -> NeoGeoRuntime {
    NeoGeoRuntime::blank(model)
}

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("snk-neo-geo"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: Region::Ntsc,
        release_year: 1990,
        summary: "SNK Neo Geo AES — 68000 + Z80 + LSPC2 + YM2610, 64 KB work RAM, 381 shrinking sprites, BIOS-driven boot, P/S/M/V/C ROM-set cartridges, memory card.".into(),
        clock: ClockDesc::new("master-cycle", ClockRate::from_hz(MASTER_HZ)),
        firmware: vec![
            FirmwareRequirement::new(BIOS_FIRMWARE_ID, "Neo Geo AES BIOS (128 KB)", false),
            FirmwareRequirement::new(ZOOM_FIRMWARE_ID, "L0 shrink table ROM (128 KB)", true),
        ],
        media_slots: vec![MediaSlot::new(
            "cartridge-1",
            "Cartridge Slot",
            MediaKind::Cartridge,
            false,
            WritebackPolicy::InMemoryOnly,
        )],
        capabilities: CapabilitySet::with_all([
            known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
            known_capability("controller-input"),
            known_capability("scripted-input"),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_one_model_round_trips_its_ids() {
        for (model, id) in Model::ALL.into_iter().zip(Model::VARIANT_IDS) {
            assert_eq!(model.variant_id(), id);
            assert_eq!(Model::from_variant_id(id), Some(model));
            assert_eq!(Model::from_variant_id(model.profile_id()), Some(model));
        }
        assert_eq!(Model::from_variant_id("mvs"), None);
    }

    /// The clock is the master clock, the unit `MachineTime` counts; a frame
    /// is 59.19 Hz.
    #[test]
    fn a_frame_is_264_lines_of_the_master_clock() {
        let profile = profile_for(Model::Aes);
        assert_eq!(profile.clock.rate.numerator_hz, 24_000_000);
        let hz = MASTER_HZ as f64 / Model::Aes.frame_ticks() as f64;
        assert!((hz - 59.19).abs() < 0.01, "{hz}");
        assert_eq!(profile.machine_id.as_str(), "snk-neo-geo");
    }

    #[test]
    fn the_bios_is_required_and_the_l0_is_not() {
        let profile = profile_for(Model::Aes);
        assert_eq!(profile.firmware.len(), 2);
        assert!(!profile.firmware[0].optional);
        assert!(profile.firmware[1].optional);
        assert_eq!(Model::Aes.firmware_sources().len(), 2);
        assert_eq!(profiles().len(), 1);
    }
}
