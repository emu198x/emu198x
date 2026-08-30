use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet, ResetKind};
use runtime_tatung_einstein::{EinsteinRuntime, Model};

const SECTOR_SIZE: usize = 512;

/// Build a minimal extended CPCEMU DSK whose sectors each contain a distinct
/// marker. This exercises the same container path as Einstein TOSEC media.
fn synthetic_dsk(tracks: u8, sectors: u8) -> Vec<u8> {
    let track_len = 256 + usize::from(sectors) * SECTOR_SIZE;
    let mut dsk = vec![0; 256 + usize::from(tracks) * track_len];
    dsk[..23].copy_from_slice(b"EXTENDED CPC DSK File\r\n");
    dsk[0x30] = tracks;
    dsk[0x31] = 1;
    for track in 0..usize::from(tracks) {
        dsk[0x34 + track] = (track_len / 256) as u8;
    }

    let mut offset = 0x100;
    for track in 0..tracks {
        dsk[offset..offset + 12].copy_from_slice(b"Track-Info\r\n");
        dsk[offset + 0x10] = track;
        dsk[offset + 0x14] = 2;
        dsk[offset + 0x15] = sectors;
        for sector in 0..sectors {
            let descriptor = offset + 0x18 + usize::from(sector) * 8;
            dsk[descriptor] = track;
            dsk[descriptor + 2] = sector;
            dsk[descriptor + 3] = 2;
            dsk[descriptor + 6] = (SECTOR_SIZE & 0xff) as u8;
            dsk[descriptor + 7] = (SECTOR_SIZE >> 8) as u8;
            let data = offset + 256 + usize::from(sector) * SECTOR_SIZE;
            dsk[data..data + SECTOR_SIZE].fill(track ^ (sector << 4) ^ 0x5a);
        }
        offset += track_len;
    }
    dsk
}

fn loaded_runtime() -> EinsteinRuntime {
    EinsteinRuntime::new(Model::Einstein, vec![0; 8 * 1024]).expect("runtime should construct")
}

#[test]
fn profile_exposes_standard_floppy_slot() {
    let runtime = loaded_runtime();
    let slot = &runtime.profile().media_slots[0];
    assert_eq!(slot.id, "floppy-0");
    assert_eq!(slot.kind, MediaKind::Disk);
}

#[test]
fn runtime_loads_dsk_and_preserves_sector_contents_across_reset() {
    let mut runtime = loaded_runtime();
    let dsk = synthetic_dsk(5, 10);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &dsk));

    runtime.load_media(&media).expect("DSK should load");
    let marker = 3_u8 ^ (7 << 4) ^ 0x5a;
    let sector_offset = (3 * 10 + 7) * SECTOR_SIZE;
    assert_eq!(
        runtime
            .machine()
            .expect("machine")
            .disk(0)
            .expect("disk")
            .data()[sector_offset],
        marker
    );

    runtime.reset(ResetKind::Hard);
    assert_eq!(
        runtime
            .machine()
            .expect("machine after reset")
            .disk(0)
            .expect("disk after reset")
            .data()[sector_offset],
        marker
    );
}

#[test]
fn runtime_rejects_bad_dsk_and_supports_eject() {
    let mut runtime = loaded_runtime();
    let bad = b"not a disk";
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, bad));
    let err = runtime.load_media(&media).expect_err("bad DSK should fail");
    assert!(matches!(err, MachineError::InvalidMedia { ref slot, .. } if slot == "floppy-0"));

    let dsk = synthetic_dsk(1, 1);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &dsk));
    runtime.load_media(&media).expect("DSK should load");
    runtime.eject_media("floppy-0").expect("eject should work");
    assert!(runtime.machine().expect("machine").disk(0).is_none());
}
