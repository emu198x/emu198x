//! 48K-specific runtime extras: firmware constructors, blank-runtime
//! helpers, and the audio-control wrappers that the wider Spectrum
//! tooling depends on. The query-provider plumbing is generic across
//! every variant — it lives in [`crate::queries`].
//!
//! The runtime itself is the generic `SpectrumRuntime<Spectrum48k>` —
//! everything below builds on that, not around it.

use common_sinclair_zx_spectrum::audio::{AudioControls, SpeakerChannel};
use common_sinclair_zx_spectrum::error::RomImageError;
use emu198x_shell::{
    CapabilitySet, FirmwareSet, MachineError, MachineProfile, SupportTier, known_capability,
};
use machine_sinclair_zx_spectrum_48k::{BoardIssue, Spectrum48k};

use crate::runtime::SpectrumRuntime;
use crate::{Model, profile_for};

impl SpectrumRuntime<Spectrum48k> {
    /// Builds a 48K runtime around the given Issue 3 ROM image.
    #[must_use]
    pub fn new_48k(rom: [u8; 16 * 1024]) -> Self {
        let mut runtime = SpectrumRuntime::new(
            Model::Spectrum48KPal,
            Spectrum48k::with_rom(BoardIssue::Issue3, rom),
        );
        *runtime.profile_mut() = boots_profile_with_export();
        runtime
    }

    /// Builds an Issue 3 runtime from a borrowed 16 KiB ROM byte slice.
    ///
    /// # Errors
    ///
    /// Returns an error if the slice is not exactly 16 KiB.
    pub fn from_rom_bytes(rom: &[u8]) -> Result<Self, RomImageError> {
        let rom: [u8; 16 * 1024] = rom
            .try_into()
            .map_err(|_| RomImageError::WrongSize { actual: rom.len() })?;
        Ok(Self::new_48k(rom))
    }

    /// Builds an Issue 3 runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing, if unknown
    /// firmware is supplied, or if the ROM image fails validation.
    pub fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(Model::Spectrum48KPal);
        let rom_id = "sinclair-zx-spectrum-48k-rom";
        firmware.validate_for_profile(&profile)?;
        let rom = firmware
            .bytes(rom_id)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: rom_id.to_owned(),
            })?;

        Self::from_rom_bytes(rom).map_err(|reason| MachineError::InvalidFirmware {
            id: rom_id.to_owned(),
            reason: reason.to_string(),
        })
    }

    /// Builds a runtime backed by a zero-filled ROM image.
    #[must_use]
    pub fn blank() -> Self {
        Self::new_48k([0; 16 * 1024])
    }

    /// Current host-side speaker audio controls.
    #[must_use]
    pub fn audio_controls(&self) -> AudioControls {
        self.machine().audio_controls()
    }

    /// Replace all host-side speaker audio controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.machine_mut().set_audio_controls(controls);
    }

    /// Enable or disable the speaker in host output.
    pub fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        self.machine_mut()
            .set_audio_channel_enabled(channel, enabled);
    }

    /// Set speaker host-side gain.
    pub fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        self.machine_mut().set_audio_channel_gain(channel, gain);
    }
}

/// 48K-only profile that bumps `support_tier` to `Boots` and advertises
/// the snapshot-export capability the bespoke runtime used to declare.
fn boots_profile_with_export() -> MachineProfile {
    let mut profile = profile_for(Model::Spectrum48KPal);
    profile.support_tier = SupportTier::Boots;
    profile.capabilities = CapabilitySet::with_all([
        known_capability("beeper-audio"),
        known_capability("keyboard-matrix"),
        known_capability("snapshot-export"),
        known_capability("snapshot-import"),
        known_capability("tape-input"),
        known_capability("tape-transport-control"),
        known_capability("scripted-input"),
    ]);
    profile
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variants::Spectrum48kRuntime;
    use emu198x_shell::FirmwareImage;

    #[test]
    fn from_firmware_rejects_missing_rom() {
        // Empty firmware set — `validate_for_profile` flags the missing
        // 48K ROM, which `from_firmware` then surfaces verbatim.
        let firmware = FirmwareSet::new();
        match SpectrumRuntime::<Spectrum48k>::from_firmware(&firmware) {
            Err(MachineError::MissingFirmware { .. }) => {}
            Err(other) => panic!("expected MissingFirmware, got {other:?}"),
            Ok(_) => panic!("empty firmware set should fail to boot the 48K runtime"),
        }
    }

    #[test]
    fn from_firmware_reports_invalid_rom_when_size_mismatches() {
        // Wrong-size ROM passes `validate_for_profile` (only the id is
        // checked) but fails the `from_rom_bytes` round-trip — that's the
        // InvalidFirmware arm.
        let mut firmware = FirmwareSet::new();
        let too_small = [0u8; 1024];
        firmware.push(FirmwareImage::new(
            "sinclair-zx-spectrum-48k-rom",
            &too_small,
        ));
        match SpectrumRuntime::<Spectrum48k>::from_firmware(&firmware) {
            Err(MachineError::InvalidFirmware { .. }) => {}
            Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
            Ok(_) => panic!("wrong-size 48K ROM must be rejected"),
        }
    }

    #[test]
    fn from_rom_bytes_rejects_wrong_size_slice() {
        match SpectrumRuntime::<Spectrum48k>::from_rom_bytes(&[0u8; 1024]) {
            Err(RomImageError::WrongSize { actual }) => assert_eq!(actual, 1024),
            Ok(_) => panic!("wrong-size slice must be rejected at construction time"),
        }
    }

    #[test]
    fn audio_controls_round_trip_through_runtime() {
        let mut runtime = SpectrumRuntime::<Spectrum48k>::blank();
        let mut controls = runtime.audio_controls();
        controls.set_channel_gain(SpeakerChannel::Speaker, 0.125);
        runtime.set_audio_controls(controls);
        assert!(
            (runtime
                .audio_controls()
                .channel(SpeakerChannel::Speaker)
                .gain()
                - 0.125)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn boots_profile_with_export_promotes_support_tier_and_capabilities() {
        use emu198x_shell::MachineCore;
        let runtime = Spectrum48kRuntime::blank();
        let caps = runtime.profile().capabilities.clone();
        // The bespoke 48K profile bumps the support tier and advertises
        // snapshot-export beyond the base profile_for(...) bundle.
        assert_eq!(runtime.profile().support_tier, SupportTier::Boots);
        assert!(caps.contains(&known_capability("snapshot-export")));
    }
}
