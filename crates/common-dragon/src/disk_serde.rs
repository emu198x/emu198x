//! The save-state encoding of a mounted DragonDOS disk.
//!
//! `format198x-dragon-data-dragon-disk` has no dependencies, so its
//! `DragonDiskImage` carries no serde derives; Format198x leaves save states to
//! the emulator. The image keeps its sectors private, so a remote definition
//! cannot reach them. These modules encode the geometry and the sectors in the
//! order the derive used when the type lived in this workspace, and rebuild the
//! image through `DragonDiskImage::from_parts`. `tests::disk_save_state_layout_is_unchanged`
//! pins the layout.

use format198x_dragon_data_dragon_disk::DragonDiskImage;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The fields of a disk image, borrowed for encoding.
#[derive(Serialize)]
#[serde(rename = "DragonDiskImage")]
struct Borrowed<'a> {
    tracks: u8,
    sides: u8,
    sectors_per_track: u8,
    sector_size: u16,
    data: &'a [u8],
}

impl<'a> From<&'a DragonDiskImage> for Borrowed<'a> {
    fn from(image: &'a DragonDiskImage) -> Self {
        Self {
            tracks: image.tracks,
            sides: image.sides,
            sectors_per_track: image.sectors_per_track,
            sector_size: image.sector_size,
            data: image.data(),
        }
    }
}

/// The fields of a disk image, decoded before the image is rebuilt.
#[derive(Deserialize)]
#[serde(rename = "DragonDiskImage")]
struct Owned {
    tracks: u8,
    sides: u8,
    sectors_per_track: u8,
    sector_size: u16,
    data: Box<[u8]>,
}

impl Owned {
    fn into_image<E: serde::de::Error>(self) -> Result<DragonDiskImage, E> {
        DragonDiskImage::from_parts(
            self.tracks,
            self.sides,
            self.sectors_per_track,
            self.sector_size,
            self.data,
        )
        .map_err(E::custom)
    }
}

/// A `with` module for an optional mounted disk.
pub mod option {
    use super::*;

    /// Encodes an optional disk image.
    ///
    /// # Errors
    ///
    /// Returns the serializer's error.
    pub fn serialize<S: Serializer>(
        value: &Option<DragonDiskImage>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_ref().map(Borrowed::from).serialize(serializer)
    }

    /// Decodes an optional disk image.
    ///
    /// # Errors
    ///
    /// Returns the deserializer's error, or a custom error when the sectors do
    /// not fit the stored geometry.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<DragonDiskImage>, D::Error> {
        Option::<Owned>::deserialize(deserializer)?
            .map(Owned::into_image)
            .transpose()
    }
}

/// A `with` module for the disks in the DragonDOS controller's drives.
pub(crate) mod drives {
    use super::*;
    use crate::DRAGON_DISK_DRIVES;

    pub(crate) fn serialize<S: Serializer>(
        value: &[Option<DragonDiskImage>; DRAGON_DISK_DRIVES],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value
            .each_ref()
            .map(|disk| disk.as_ref().map(Borrowed::from))
            .serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[Option<DragonDiskImage>; DRAGON_DISK_DRIVES], D::Error> {
        let [a, b, c, d] = <[Option<Owned>; DRAGON_DISK_DRIVES]>::deserialize(deserializer)?
            .map(|disk| disk.map(Owned::into_image::<D::Error>).transpose());
        Ok([a?, b?, c?, d?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Fixture {
        #[serde(with = "option")]
        mounted: Option<DragonDiskImage>,
        #[serde(with = "drives")]
        drives: [Option<DragonDiskImage>; crate::DRAGON_DISK_DRIVES],
    }

    fn image(tracks: u8, fill: &[u8]) -> DragonDiskImage {
        let mut data = vec![0; usize::from(tracks) * 18 * 256];
        data[..fill.len()].copy_from_slice(fill);
        DragonDiskImage::from_parts(tracks, 1, 18, 256, data).expect("geometry fits")
    }

    /// Bytes and JSON written by the derive on the in-workspace
    /// `format-dragon-disk` type, captured before the switch to Format198x.
    #[test]
    fn disk_save_state_layout_is_unchanged() {
        let full = image(40, &[0x55, 0xAA]);
        let empty = image(0, &[]);
        let fixture = Fixture {
            mounted: Some(full.clone()),
            drives: [Some(empty.clone()), None, Some(empty), None],
        };

        let bytes = postcard::to_allocvec(&fixture).expect("fixture serialises");
        let mounted_len = 184_329;
        assert_eq!(
            &bytes[..11],
            &[
                0x01, 0x28, 0x01, 0x12, 0x80, 0x02, 0x80, 0xA0, 0x0B, 0x55, 0xAA
            ]
        );
        assert_eq!(&bytes[9..mounted_len], full.data());
        assert_eq!(
            &bytes[mounted_len..],
            &[
                0x01, 0x00, 0x01, 0x12, 0x80, 0x02, 0x00, 0x00, 0x01, 0x00, 0x01, 0x12, 0x80, 0x02,
                0x00, 0x00,
            ]
        );
        let decoded: Fixture = postcard::from_bytes(&bytes).expect("old bytes decode");
        assert_eq!(decoded, fixture);

        let drives = Fixture {
            mounted: None,
            drives: fixture.drives.clone(),
        };
        let json = serde_json::to_string(&drives).expect("drives serialise");
        assert_eq!(
            json,
            r#"{"mounted":null,"drives":[{"tracks":0,"sides":1,"sectors_per_track":18,"sector_size":256,"data":[]},null,{"tracks":0,"sides":1,"sectors_per_track":18,"sector_size":256,"data":[]},null]}"#
        );
        let mounted = serde_json::to_string(&fixture).expect("fixture serialises");
        assert!(mounted.starts_with(
            r#"{"mounted":{"tracks":40,"sides":1,"sectors_per_track":18,"sector_size":256,"data":[85,170,0"#
        ));
        let decoded: Fixture = serde_json::from_str(&mounted).expect("JSON decodes");
        assert_eq!(decoded, fixture);
    }

    #[test]
    fn sectors_that_do_not_fit_the_geometry_are_rejected() {
        let json = r#"{"mounted":{"tracks":1,"sides":1,"sectors_per_track":18,"sector_size":256,"data":[1,2,3]},"drives":[null,null,null,null]}"#;
        assert!(serde_json::from_str::<Fixture>(json).is_err());
        let json = r#"{"mounted":null,"drives":[null,{"tracks":1,"sides":1,"sectors_per_track":18,"sector_size":256,"data":[1]},null,null]}"#;
        assert!(serde_json::from_str::<Fixture>(json).is_err());
    }
}
