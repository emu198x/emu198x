//! Shared firmware descriptors and validation helpers.

use std::borrow::Cow;
use std::collections::BTreeSet;

use crate::error::MachineError;
use crate::machine::MachineProfile;

/// One firmware image prepared by the host for a machine profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirmwareImage<'a> {
    /// Stable firmware identifier.
    pub id: Cow<'static, str>,
    /// Raw firmware bytes.
    pub bytes: &'a [u8],
}

impl<'a> FirmwareImage<'a> {
    /// Creates one firmware image descriptor.
    #[must_use]
    pub fn new(id: impl Into<Cow<'static, str>>, bytes: &'a [u8]) -> Self {
        Self {
            id: id.into(),
            bytes,
        }
    }
}

/// One set of firmware images prepared for a machine profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirmwareSet<'a> {
    /// The supplied firmware images.
    pub images: Vec<FirmwareImage<'a>>,
}

impl<'a> FirmwareSet<'a> {
    /// Creates an empty firmware set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one firmware image to the set.
    pub fn push(&mut self, image: FirmwareImage<'a>) {
        self.images.push(image);
    }

    /// Returns `true` if the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Returns one firmware image by stable identifier.
    #[must_use]
    pub fn image(&self, id: &str) -> Option<&FirmwareImage<'a>> {
        self.images.iter().find(|image| image.id.as_ref() == id)
    }

    /// Returns the firmware bytes for one stable identifier.
    #[must_use]
    pub fn bytes(&self, id: &str) -> Option<&'a [u8]> {
        self.image(id).map(|image| image.bytes)
    }

    /// Validates this set against one machine profile's declared requirements.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing, identifiers are
    /// duplicated, or the set contains firmware the profile does not declare.
    pub fn validate_for_profile(&self, profile: &MachineProfile) -> Result<(), MachineError> {
        let mut seen = BTreeSet::new();
        for image in &self.images {
            if !seen.insert(image.id.as_ref()) {
                return Err(MachineError::DuplicateFirmware {
                    id: image.id.as_ref().to_owned(),
                });
            }

            if !profile
                .firmware
                .iter()
                .any(|requirement| requirement.id.as_ref() == image.id.as_ref())
            {
                return Err(MachineError::UnknownFirmware {
                    id: image.id.as_ref().to_owned(),
                });
            }
        }

        for requirement in &profile.firmware {
            if !requirement.optional && self.image(requirement.id.as_ref()).is_none() {
                return Err(MachineError::MissingFirmware {
                    id: requirement.id.as_ref().to_owned(),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CapabilitySet, ClockDesc, ClockRate, Family, MachineId, MediaSlot, ProfileId, Region,
        WritebackPolicy,
    };
    use crate::{FirmwareRequirement, MediaKind};

    fn test_profile() -> MachineProfile {
        MachineProfile {
            machine_id: MachineId::from("test-family"),
            profile_id: ProfileId::from("test-profile"),
            display_name: "Test Profile".into(),
            family: Family::Spectrum,
            region: Region::Pal,
            release_year: 1982,
            summary: "test".into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(1)),
            firmware: vec![
                FirmwareRequirement::new("rom-0", "ROM 0", false),
                FirmwareRequirement::new("rom-1", "ROM 1", true),
            ],
            media_slots: vec![MediaSlot::new(
                "tape-1",
                "Tape Deck",
                MediaKind::Tape,
                false,
                WritebackPolicy::InMemoryOnly,
            )],
            capabilities: CapabilitySet::new(),
        }
    }

    #[test]
    fn validate_accepts_required_and_optional_firmware() {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new("rom-0", &[0x00]));
        firmware.push(FirmwareImage::new("rom-1", &[0x01]));

        let result = firmware.validate_for_profile(&test_profile());
        assert!(
            result.is_ok(),
            "profile-compatible firmware should validate"
        );
    }

    #[test]
    fn validate_rejects_missing_required_firmware() {
        let firmware = FirmwareSet::new();
        let result = firmware.validate_for_profile(&test_profile());

        assert!(matches!(
            result,
            Err(MachineError::MissingFirmware { ref id }) if id == "rom-0"
        ));
    }

    #[test]
    fn validate_rejects_duplicate_ids() {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new("rom-0", &[0x00]));
        firmware.push(FirmwareImage::new("rom-0", &[0x01]));

        let result = firmware.validate_for_profile(&test_profile());
        assert!(matches!(
            result,
            Err(MachineError::DuplicateFirmware { ref id }) if id == "rom-0"
        ));
    }

    #[test]
    fn validate_rejects_unknown_ids() {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new("rom-0", &[0x00]));
        firmware.push(FirmwareImage::new("rom-x", &[0x01]));

        let result = firmware.validate_for_profile(&test_profile());
        assert!(matches!(
            result,
            Err(MachineError::UnknownFirmware { ref id }) if id == "rom-x"
        ));
    }
}
