//! Retained constructor failures and capabilities from retired profile wrappers.
#[cfg(test)]
mod old_48k_constructors {
    use common_sinclair_zx_spectrum::audio::SpeakerChannel;
    use common_sinclair_zx_spectrum::error::RomImageError;
    use emu198x_shell::FirmwareImage;
    use emu198x_shell::{FirmwareSet, MachineError};
    use machine_sinclair_zx_spectrum_issue3::SpectrumIssue3With48k;
    use runtime_sinclair_zx_spectrum::*;

    #[test]
    fn from_firmware_rejects_missing_rom() {
        // Empty firmware set — `validate_for_profile` flags the missing
        // 48K ROM, which `from_firmware` then surfaces verbatim.
        let firmware = FirmwareSet::new();
        match SpectrumRuntime::<SpectrumIssue3With48k>::from_firmware(&firmware) {
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
        match SpectrumRuntime::<SpectrumIssue3With48k>::from_firmware(&firmware) {
            Err(MachineError::InvalidFirmware { .. }) => {}
            Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
            Ok(_) => panic!("wrong-size 48K ROM must be rejected"),
        }
    }

    #[test]
    fn from_rom_bytes_rejects_wrong_size_slice() {
        match SpectrumRuntime::<SpectrumIssue3With48k>::from_rom_bytes(&[0u8; 1024]) {
            Err(RomImageError::WrongSize { actual }) => assert_eq!(actual, 1024),
            Ok(_) => panic!("wrong-size slice must be rejected at construction time"),
        }
    }

    #[test]
    fn audio_controls_round_trip_through_runtime() {
        let mut runtime = SpectrumRuntime::<SpectrumIssue3With48k>::blank();
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
    fn constructed_profile_advertises_snapshot_export() {
        use emu198x_shell::MachineCore;
        let runtime = SpectrumIssue3Ram48kRuntime::blank();
        let caps = runtime.profile().capabilities.clone();
        // The bespoke 48K profile advertises snapshot-export beyond the
        // base profile_for(...) bundle.
        assert!(caps.contains(&emu198x_shell::known_capability("snapshot-export")));
    }

    #[test]
    fn live_profile_keeps_every_base_profile_capability() {
        use emu198x_shell::MachineCore;
        let runtime = SpectrumIssue3Ram48kRuntime::blank();
        let live = &runtime.profile().capabilities;
        let base = profile_for(Model::SpectrumIssue3Ram48kPal);
        let missing: Vec<&str> = base
            .capabilities
            .iter()
            .filter(|cap| !live.contains(cap))
            .map(|cap| cap.as_str())
            .collect();
        assert!(
            missing.is_empty(),
            "the live 48K drops capabilities its profile declares: {missing:?}"
        );
    }
}

#[cfg(test)]
mod old_16k_constructors {
    use common_sinclair_zx_spectrum::error::RomImageError;
    use emu198x_shell::FirmwareImage;
    use emu198x_shell::{FirmwareSet, MachineError};
    use machine_sinclair_zx_spectrum_issue3::SpectrumIssue3With16k;
    use runtime_sinclair_zx_spectrum::SpectrumIssue3Ram16kRuntime;
    use runtime_sinclair_zx_spectrum::*;

    #[test]
    fn from_firmware_rejects_missing_rom() {
        let firmware = FirmwareSet::new();
        match SpectrumRuntime::<SpectrumIssue3With16k>::from_firmware(&firmware) {
            Err(MachineError::MissingFirmware { .. }) => {}
            Err(other) => panic!("expected MissingFirmware, got {other:?}"),
            Ok(_) => panic!("empty firmware set should fail to boot the 16K runtime"),
        }
    }

    #[test]
    fn from_firmware_reports_invalid_rom_when_size_mismatches() {
        let mut firmware = FirmwareSet::new();
        let too_small = [0u8; 1024];
        firmware.push(FirmwareImage::new(
            "sinclair-zx-spectrum-48k-rom",
            &too_small,
        ));
        match SpectrumRuntime::<SpectrumIssue3With16k>::from_firmware(&firmware) {
            Err(MachineError::InvalidFirmware { .. }) => {}
            Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
            Ok(_) => panic!("wrong-size 48K ROM must be rejected"),
        }
    }

    #[test]
    fn from_rom_bytes_rejects_wrong_size_slice() {
        match SpectrumRuntime::<SpectrumIssue3With16k>::from_rom_bytes(&[0u8; 1024]) {
            Err(RomImageError::WrongSize { actual }) => assert_eq!(actual, 1024),
            Ok(_) => panic!("wrong-size slice must be rejected at construction time"),
        }
    }

    #[test]
    fn blank_runtime_carries_the_16k_profile_id() {
        use emu198x_shell::MachineCore;
        let runtime = SpectrumIssue3Ram16kRuntime::blank();
        assert_eq!(
            runtime.profile().profile_id.as_str(),
            "sinclair-zx-spectrum-issue3-16k-pal"
        );
    }
}

#[cfg(test)]
mod old_plus_constructors {
    use common_sinclair_zx_spectrum::error::RomImageError;
    use emu198x_shell::FirmwareImage;
    use emu198x_shell::{FirmwareSet, MachineError};
    use machine_sinclair_zx_spectrum_issue3::SpectrumIssue3With48k;
    use runtime_sinclair_zx_spectrum::SpectrumIssue3Ram48kRuntime;
    use runtime_sinclair_zx_spectrum::*;

    #[test]
    fn from_firmware_rejects_missing_rom() {
        let firmware = FirmwareSet::new();
        match SpectrumRuntime::<SpectrumIssue3With48k>::from_firmware(&firmware) {
            Err(MachineError::MissingFirmware { .. }) => {}
            Err(other) => panic!("expected MissingFirmware, got {other:?}"),
            Ok(_) => panic!("empty firmware set should fail to boot the Spectrum+ runtime"),
        }
    }

    #[test]
    fn from_firmware_reports_invalid_rom_when_size_mismatches() {
        let mut firmware = FirmwareSet::new();
        let too_small = [0u8; 1024];
        firmware.push(FirmwareImage::new(
            "sinclair-zx-spectrum-48k-rom",
            &too_small,
        ));
        match SpectrumRuntime::<SpectrumIssue3With48k>::from_firmware(&firmware) {
            Err(MachineError::InvalidFirmware { .. }) => {}
            Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
            Ok(_) => panic!("wrong-size 48K ROM must be rejected"),
        }
    }

    #[test]
    fn from_rom_bytes_rejects_wrong_size_slice() {
        match SpectrumRuntime::<SpectrumIssue3With48k>::from_rom_bytes(&[0u8; 1024]) {
            Err(RomImageError::WrongSize { actual }) => assert_eq!(actual, 1024),
            Ok(_) => panic!("wrong-size slice must be rejected at construction time"),
        }
    }

    #[test]
    fn blank_runtime_uses_the_explicit_board_profile_id() {
        use emu198x_shell::MachineCore;
        let runtime = SpectrumIssue3Ram48kRuntime::blank();
        assert_eq!(
            runtime.profile().profile_id.as_str(),
            "sinclair-zx-spectrum-issue3-48k-pal"
        );
    }
}
