//! Complete envelopes retain firmware with and without a cartridge.
#[allow(
    dead_code,
    reason = "The shared fixture also exposes guest counters for runtime tests."
)]
mod common;
use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore, ResetKind};
use runtime_nintendo_snes::{Model, SnesRuntime};

#[test]
fn every_profile_retains_blank_and_loaded_firmware_across_restore_and_reset() {
    let mut custom = [0; 64];
    custom[..2].copy_from_slice(&[0x2f, 0xfe]);
    custom[62..].copy_from_slice(&[0xc0, 0xff]);
    for model in Model::ALL {
        for rom in [None, Some(common_ipl()), Some(custom)] {
            for loaded in [false, true] {
                let mut firmware = FirmwareSet::new();
                if let Some(bytes) = rom.as_ref() {
                    firmware.push(FirmwareImage::new("snes-ipl", bytes));
                }
                let mut live = SnesRuntime::from_firmware(model, &firmware).expect("firmware");
                if loaded {
                    live.insert_cartridge(common::cartridge())
                        .expect("cartridge");
                }
                let bytes = live.snapshot().expect("snapshot");
                assert_eq!(
                    postcard::take_from_bytes::<u16>(&bytes).expect("version").0,
                    2
                );
                let mut restored = SnesRuntime::blank(model);
                restored.restore(&bytes).expect("restore");
                assert_eq!(restored.snapshot().expect("same envelope"), bytes);
                assert_eq!(restored.machine().is_some(), loaded);
                assert_eq!(restored.has_original_ipl(), rom == Some(custom));
                if !loaded {
                    restored
                        .insert_cartridge(common::cartridge())
                        .expect("insert later");
                }
                restored.reset(ResetKind::Hard);
                let expected = rom.unwrap_or_else(common_ipl);
                assert_eq!(restored.machine().expect("machine").apu.ipl, expected);
                let again = restored.snapshot().expect("after reset");
                live.restore(&again).expect("restore after reset");
                assert_eq!(live.snapshot().expect("same state"), again);
            }
        }
    }
}
fn common_ipl() -> [u8; 64] {
    common_nintendo_snes::ipl::CLEAN_ROOM
}
