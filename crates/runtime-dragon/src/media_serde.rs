//! The save-state encoding of the Dragon runtime's mounted media.
//!
//! The Dragon Data format crates have no dependencies, so they carry no serde
//! derives; Format198x leaves save states to the emulator. These remote
//! definitions mirror their types field for field and variant for variant, in
//! order, so a snapshot keeps the layout it had when the layouts lived in this
//! workspace. `tests::media_save_state_layout_is_unchanged` pins it.

use format198x_dragon_data_dragon_bin::DragonBinImage;
use format198x_dragon_data_dragon_cas::{
    CasBlock, CasBlockKind, CasFileType, CasHeader, CasIgnoredBytes, CasImage,
};
use format198x_dragon_data_dragon_pak::{
    DragonCartridgeKind, DragonPakImage, PcDragonPeripherals, PcDragonRegisters, PcDragonSnapshot,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A `with` module for an `Option` of a remote type.
macro_rules! option_with {
    ($module:ident, $ty:ty, $def:ident) => {
        pub(crate) mod $module {
            use super::*;

            struct Borrowed<'a>(&'a $ty);

            impl Serialize for Borrowed<'_> {
                fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                    $def::serialize(self.0, serializer)
                }
            }

            struct Owned($ty);

            impl<'de> Deserialize<'de> for Owned {
                fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                    $def::deserialize(deserializer).map(Owned)
                }
            }

            pub(crate) fn serialize<S: Serializer>(
                value: &Option<$ty>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                value.as_ref().map(Borrowed).serialize(serializer)
            }

            pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<$ty>, D::Error> {
                Ok(Option::<Owned>::deserialize(deserializer)?.map(|Owned(value)| value))
            }
        }
    };
}

/// A `with` module for a `Vec` of a remote type.
macro_rules! vec_with {
    ($module:ident, $ty:ty, $def:ident) => {
        mod $module {
            use super::*;

            struct Borrowed<'a>(&'a $ty);

            impl Serialize for Borrowed<'_> {
                fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                    $def::serialize(self.0, serializer)
                }
            }

            struct Owned($ty);

            impl<'de> Deserialize<'de> for Owned {
                fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                    $def::deserialize(deserializer).map(Owned)
                }
            }

            pub(super) fn serialize<S: Serializer>(
                values: &[$ty],
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.collect_seq(values.iter().map(Borrowed))
            }

            pub(super) fn deserialize<'de, D: Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Vec<$ty>, D::Error> {
                Ok(Vec::<Owned>::deserialize(deserializer)?
                    .into_iter()
                    .map(|Owned(value)| value)
                    .collect())
            }
        }
    };
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "CasFileType")]
enum CasFileTypeDef {
    Basic,
    Data,
    MachineCode,
    Unknown(u8),
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "CasHeader")]
struct CasHeaderDef {
    raw_name: [u8; 8],
    name: String,
    #[serde(with = "CasFileTypeDef")]
    file_type: CasFileType,
    ascii_flag: u8,
    gap_flag: u8,
    first_address: u16,
    second_address: u16,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "CasBlockKind")]
enum CasBlockKindDef {
    Header,
    Data,
    EndOfFile,
    Unknown(u8),
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "CasBlock")]
struct CasBlockDef {
    offset: usize,
    sync_offset: usize,
    leader_len: usize,
    #[serde(with = "CasBlockKindDef")]
    kind: CasBlockKind,
    block_type: u8,
    data: Vec<u8>,
    checksum: u8,
    calculated_checksum: u8,
    checksum_valid: bool,
    #[serde(with = "option_cas_header")]
    header: Option<CasHeader>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "CasIgnoredBytes")]
struct CasIgnoredBytesDef {
    offset: usize,
    len: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "CasImage")]
struct CasImageDef {
    #[serde(with = "vec_cas_block")]
    blocks: Vec<CasBlock>,
    #[serde(with = "vec_cas_ignored_bytes")]
    ignored_ranges: Vec<CasIgnoredBytes>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "DragonBinImage")]
struct DragonBinImageDef {
    file_type: u8,
    load_address: u16,
    exec_address: u16,
    payload: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "DragonCartridgeKind")]
enum DragonCartridgeKindDef {
    Rom,
    GamesMaster,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "DragonPakImage")]
struct DragonPakImageDef {
    #[serde(with = "DragonCartridgeKindDef")]
    kind: DragonCartridgeKind,
    header_offset: usize,
    rom: Box<[u8]>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PcDragonRegisters")]
struct PcDragonRegistersDef {
    pc: u16,
    x: u16,
    y: u16,
    u: u16,
    s: u16,
    dp: u8,
    b: u8,
    a: u8,
    cc: u8,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PcDragonPeripherals")]
struct PcDragonPeripheralsDef {
    ff02: u8,
    ff03: u8,
    ff22: u8,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PcDragonSnapshot")]
struct PcDragonSnapshotDef {
    ram: Box<[u8]>,
    load_address: u16,
    #[serde(with = "PcDragonRegistersDef")]
    registers: PcDragonRegisters,
    #[serde(with = "option_pc_dragon_peripherals")]
    peripherals: Option<PcDragonPeripherals>,
    display_base: Option<u16>,
}

option_with!(option_cas_header, CasHeader, CasHeaderDef);
option_with!(
    option_pc_dragon_peripherals,
    PcDragonPeripherals,
    PcDragonPeripheralsDef
);
vec_with!(vec_cas_block, CasBlock, CasBlockDef);
vec_with!(vec_cas_ignored_bytes, CasIgnoredBytes, CasIgnoredBytesDef);

option_with!(tape, CasImage, CasImageDef);
option_with!(cartridge, DragonPakImage, DragonPakImageDef);
option_with!(program, DragonBinImage, DragonBinImageDef);
option_with!(snapshot, PcDragonSnapshot, PcDragonSnapshotDef);

#[cfg(test)]
mod tests {
    use super::*;

    /// The four media fields as `DragonRuntimeSnapshotV1` declares them.
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Media {
        #[serde(with = "tape")]
        tape: Option<CasImage>,
        #[serde(with = "cartridge")]
        cartridge: Option<DragonPakImage>,
        #[serde(with = "program")]
        program: Option<DragonBinImage>,
        #[serde(with = "snapshot")]
        snapshot: Option<PcDragonSnapshot>,
    }

    fn media_fixture() -> Media {
        Media {
            tape: Some(CasImage {
                blocks: vec![
                    CasBlock {
                        offset: 3,
                        sync_offset: 131,
                        leader_len: 128,
                        kind: CasBlockKind::Header,
                        block_type: 0x00,
                        data: vec![
                            b'G', b'A', b'M', b'E', b' ', b' ', b' ', b' ', 2, 0, 0, 0x30, 0x00,
                            0x40, 0x00,
                        ],
                        checksum: 0x5A,
                        calculated_checksum: 0x5A,
                        checksum_valid: true,
                        header: Some(CasHeader {
                            raw_name: *b"GAME    ",
                            name: "GAME".to_owned(),
                            file_type: CasFileType::MachineCode,
                            ascii_flag: 0,
                            gap_flag: 0,
                            first_address: 0x3000,
                            second_address: 0x4000,
                        }),
                    },
                    CasBlock {
                        offset: 160,
                        sync_offset: 161,
                        leader_len: 1,
                        kind: CasBlockKind::Unknown(0x7E),
                        block_type: 0x7E,
                        data: vec![1, 2, 3],
                        checksum: 9,
                        calculated_checksum: 6,
                        checksum_valid: false,
                        header: None,
                    },
                ],
                ignored_ranges: vec![CasIgnoredBytes { offset: 0, len: 3 }],
            }),
            cartridge: Some(DragonPakImage {
                kind: DragonCartridgeKind::GamesMaster,
                header_offset: 16,
                rom: vec![0xA5, 0x5A, 0x00, 0xFF].into_boxed_slice(),
            }),
            program: Some(DragonBinImage {
                file_type: 2,
                load_address: 0x3000,
                exec_address: 0x3010,
                payload: vec![0x86, 0x41, 0x39],
            }),
            snapshot: Some(PcDragonSnapshot {
                ram: vec![0x12, 0x34, 0x56].into_boxed_slice(),
                load_address: 0x0000,
                registers: PcDragonRegisters {
                    pc: 0xC000,
                    x: 0x1111,
                    y: 0x2222,
                    u: 0x3333,
                    s: 0x7F00,
                    dp: 0x01,
                    b: 0x02,
                    a: 0x03,
                    cc: 0x50,
                },
                peripherals: Some(PcDragonPeripherals {
                    ff02: 0xFF,
                    ff03: 0x34,
                    ff22: 0x08,
                }),
                display_base: Some(0x0400),
            }),
        }
    }

    /// The postcard and JSON encodings, captured from the in-workspace Dragon
    /// crates' serde derives before their layouts moved to Format198x. A
    /// runtime snapshot holding media must decode the same after the switch.
    #[test]
    fn media_save_state_layout_is_unchanged() {
        let media = media_fixture();
        let expected: &[u8] = &[
            0x01, 0x02, 0x03, 0x83, 0x01, 0x80, 0x01, 0x00, 0x00, 0x0F, 0x47, 0x41, 0x4D, 0x45,
            0x20, 0x20, 0x20, 0x20, 0x02, 0x00, 0x00, 0x30, 0x00, 0x40, 0x00, 0x5A, 0x5A, 0x01,
            0x01, 0x47, 0x41, 0x4D, 0x45, 0x20, 0x20, 0x20, 0x20, 0x04, 0x47, 0x41, 0x4D, 0x45,
            0x02, 0x00, 0x00, 0x80, 0x60, 0x80, 0x80, 0x01, 0xA0, 0x01, 0xA1, 0x01, 0x01, 0x03,
            0x7E, 0x7E, 0x03, 0x01, 0x02, 0x03, 0x09, 0x06, 0x00, 0x00, 0x01, 0x00, 0x03, 0x01,
            0x01, 0x10, 0x04, 0xA5, 0x5A, 0x00, 0xFF, 0x01, 0x02, 0x80, 0x60, 0x90, 0x60, 0x03,
            0x86, 0x41, 0x39, 0x01, 0x03, 0x12, 0x34, 0x56, 0x00, 0x80, 0x80, 0x03, 0x91, 0x22,
            0xA2, 0x44, 0xB3, 0x66, 0x80, 0xFE, 0x01, 0x01, 0x02, 0x03, 0x50, 0x01, 0xFF, 0x34,
            0x08, 0x01, 0x80, 0x08,
        ];
        assert_eq!(
            postcard::to_allocvec(&media).expect("media serialises"),
            expected
        );
        let decoded: Media = postcard::from_bytes(expected).expect("old bytes decode");
        assert_eq!(decoded, media);
        assert_eq!(
            serde_json::to_string(&media).expect("media serialises"),
            r#"{"tape":{"blocks":[{"offset":3,"sync_offset":131,"leader_len":128,"kind":"Header","block_type":0,"data":[71,65,77,69,32,32,32,32,2,0,0,48,0,64,0],"checksum":90,"calculated_checksum":90,"checksum_valid":true,"header":{"raw_name":[71,65,77,69,32,32,32,32],"name":"GAME","file_type":"MachineCode","ascii_flag":0,"gap_flag":0,"first_address":12288,"second_address":16384}},{"offset":160,"sync_offset":161,"leader_len":1,"kind":{"Unknown":126},"block_type":126,"data":[1,2,3],"checksum":9,"calculated_checksum":6,"checksum_valid":false,"header":null}],"ignored_ranges":[{"offset":0,"len":3}]},"cartridge":{"kind":"GamesMaster","header_offset":16,"rom":[165,90,0,255]},"program":{"file_type":2,"load_address":12288,"exec_address":12304,"payload":[134,65,57]},"snapshot":{"ram":[18,52,86],"load_address":0,"registers":{"pc":49152,"x":4369,"y":8738,"u":13107,"s":32512,"dp":1,"b":2,"a":3,"cc":80},"peripherals":{"ff02":255,"ff03":52,"ff22":8},"display_base":1024}}"#
        );

        let none = Media {
            tape: None,
            cartridge: None,
            program: None,
            snapshot: None,
        };
        assert_eq!(
            postcard::to_allocvec(&none).expect("media serialises"),
            [0x00, 0x00, 0x00, 0x00]
        );
    }
}
