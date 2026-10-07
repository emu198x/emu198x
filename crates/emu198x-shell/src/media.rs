//! Media and firmware descriptors shared by machine runtimes.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Host-visible media class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MediaKind {
    /// Cassette or tape image.
    Tape,
    /// Floppy or similar disk image.
    Disk,
    /// Cartridge or ROM-board image.
    Cartridge,
    /// Optical-disc image.
    Optical,
    /// Machine snapshot image.
    Snapshot,
    /// Directly loadable program image.
    Program,
}

/// Persistence policy for writes originating from the emulated machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WritebackPolicy {
    /// All writes stay in memory unless explicitly exported elsewhere.
    InMemoryOnly,
    /// Writes persist to a sidecar artifact, never to the source image.
    SidecarOnly,
}

/// Firmware required by a machine profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareRequirement {
    /// Stable identifier for the firmware blob.
    pub id: Cow<'static, str>,
    /// Human-readable display name.
    pub display_name: Cow<'static, str>,
    /// Whether this firmware is optional for the profile.
    pub optional: bool,
}

impl FirmwareRequirement {
    /// Creates a firmware requirement descriptor.
    #[must_use]
    pub fn new(
        id: impl Into<Cow<'static, str>>,
        display_name: impl Into<Cow<'static, str>>,
        optional: bool,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            optional,
        }
    }
}

/// One loadable slot on a machine profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaSlot {
    /// Stable slot identifier, for example `tape-1` or `drive-0`.
    pub id: Cow<'static, str>,
    /// User-facing display name.
    pub display_name: Cow<'static, str>,
    /// Media kind accepted by this slot.
    pub kind: MediaKind,
    /// Whether the profile requires this slot to be populated to boot normally.
    pub required: bool,
    /// Persistence policy for writes originating from the machine.
    pub writeback: WritebackPolicy,
}

impl MediaSlot {
    /// Creates a media-slot descriptor.
    #[must_use]
    pub fn new(
        id: impl Into<Cow<'static, str>>,
        display_name: impl Into<Cow<'static, str>>,
        kind: MediaKind,
        required: bool,
        writeback: WritebackPolicy,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            kind,
            required,
            writeback,
        }
    }
}

/// One media image prepared for machine loading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaImage<'a> {
    /// Target slot identifier.
    pub slot: Cow<'static, str>,
    /// Media kind supplied for this image.
    pub kind: MediaKind,
    /// Raw image bytes.
    pub bytes: &'a [u8],
    /// Whether the machine may write back to this image. Defaults to `false`
    /// so archive media stays read-only; a learner's work disk opts in. See
    /// `knowledge/decisions/disk-save-write-back.md`.
    pub writable: bool,
    /// The cartridge's banking scheme, by the name the machine lists in
    /// [`crate::MachineCore::cartridge_types`], for an image whose bytes do
    /// not say (a headerless dump). `None` lets the machine identify it.
    /// [`crate::HeadlessSession::load_media`] refuses a name the machine
    /// does not list, so a machine that ignores the field never sees one.
    pub cart_type: Option<Cow<'static, str>>,
}

impl<'a> MediaImage<'a> {
    /// Creates one read-only media image descriptor.
    #[must_use]
    pub fn new(slot: impl Into<Cow<'static, str>>, kind: MediaKind, bytes: &'a [u8]) -> Self {
        Self {
            slot: slot.into(),
            kind,
            bytes,
            writable: false,
            cart_type: None,
        }
    }

    /// Marks this image writable (the machine may persist changes to it).
    #[must_use]
    pub fn writable(mut self, writable: bool) -> Self {
        self.writable = writable;
        self
    }

    /// Names the cartridge's banking scheme (see [`Self::cart_type`]).
    #[must_use]
    pub fn cart_type(mut self, cart_type: impl Into<Cow<'static, str>>) -> Self {
        self.cart_type = Some(cart_type.into());
        self
    }
}

/// Refuse a [`MediaImage::cart_type`] the
/// machine does not list in `accepted`, naming the ones it does. A machine
/// that never reads the field lists none, so the request fails here instead
/// of loading the image under a guess the caller asked to override.
pub(crate) fn check_cart_types(
    media: &MediaSet<'_>,
    accepted: &[&str],
) -> Result<(), crate::MachineError> {
    for image in &media.images {
        let Some(name) = image.cart_type.as_deref() else {
            continue;
        };
        let reason = if accepted.is_empty() {
            format!("this machine takes no cartridge type, got `{name}`")
        } else if image.kind != MediaKind::Cartridge {
            format!("a cartridge type applies to cartridge media only, got `{name}`")
        } else if accepted.contains(&name) {
            continue;
        } else {
            format!(
                "unknown cartridge type `{name}`; expected {}",
                accepted.join(" | ")
            )
        };
        return Err(crate::MachineError::InvalidMedia {
            slot: image.slot.to_string(),
            reason,
        });
    }
    Ok(())
}

/// One machine load request containing zero or more media images.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaSet<'a> {
    /// The images to insert or apply.
    pub images: Vec<MediaImage<'a>>,
}

impl<'a> MediaSet<'a> {
    /// Creates an empty media set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one image to the set.
    pub fn push(&mut self, image: MediaImage<'a>) {
        self.images.push(image);
    }

    /// Returns `true` if the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}
