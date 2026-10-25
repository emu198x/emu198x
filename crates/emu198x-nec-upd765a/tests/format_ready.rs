//! Independent READY must gate Format Track even while a writable image exists.
use emu198x_nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn controller() -> Upd765a {
    let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
    fdc.insert_disk_with_writable(
        0,
        DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![DiskTrack {
                sectors: vec![DiskSector {
                    c: 0,
                    h: 0,
                    id: 1,
                    n: 2,
                    st1: 0,
                    st2: 0,
                    data: vec![0xe5; 512],
                }],
            }]],
        },
        true,
    );
    fdc.set_ready(0, true);
    for b in [3, 0xdf, 3] {
        fdc.write_data(b);
    }
    fdc
}

fn start(fdc: &mut Upd765a) {
    for b in [0x4d, 0, 2, 1, 0x2a, 0x69] {
        fdc.write_data(b);
    }
}

fn result(fdc: &mut Upd765a) -> [u8; 7] {
    assert_eq!(
        fdc.read_status() & 0xf0,
        0xd0,
        "must enter result, not execution"
    );
    assert!(fdc.interrupt, "format completion IRQ");
    let bytes = std::array::from_fn(|_| fdc.read_data());
    assert_eq!(fdc.read_status() & 0xf0, 0x80);
    assert!(!fdc.interrupt);
    bytes
}

fn original(fdc: &Upd765a) {
    let image = fdc.disk_for_export(0).expect("inactive readable disk");
    assert_eq!(image.tracks[0][0].sectors.len(), 1);
    assert_eq!(image.tracks[0][0].sectors[0].id, 1);
    assert_eq!(image.tracks[0][0].sectors[0].data, vec![0xe5; 512]);
}

#[test]
fn not_ready_mounted_image_rejects_format_without_execution() {
    for nd in [false, true] {
        for opcode in [0x0d, 0x4d] {
            for write_protected in [false, true] {
                let mut fdc = controller();
                for byte in [3, 0xdf, 2 | u8::from(nd)] {
                    fdc.write_data(byte);
                }
                fdc.set_write_protect(0, write_protected);
                fdc.set_ready(0, false);
                for byte in [opcode, 0, 2, 1, 0x2a, 0x69] {
                    fdc.write_data(byte);
                }
                assert_eq!(result(&mut fdc), [0x48, 0, 0, 0, 0, 0, 2]);
                original(&fdc);
                // Elapsed time cannot restart a rejected format.
                for _ in 0..100_000 {
                    fdc.tick();
                }
                original(&fdc);
                assert_eq!(fdc.read_status() & 0xf0, 0x80);
            }
        }
    }
}

#[test]
fn ready_assertion_after_rejection_allows_a_fresh_format() {
    let mut fdc = controller();
    fdc.set_ready(0, false);
    start(&mut fdc);
    assert_eq!(result(&mut fdc), [0x48, 0, 0, 0, 0, 0, 2]);
    original(&fdc);
    fdc.set_ready(0, true);
    start(&mut fdc);
    for byte in [0, 0, 7, 2] {
        wait_request(&mut fdc);
        assert_eq!(fdc.read_status() & 0xf0, 0xb0);
        assert!(fdc.interrupt);
        fdc.write_data(byte);
        assert!(!fdc.interrupt);
    }
    wait_request(&mut fdc);
    assert_eq!(result(&mut fdc), [0, 0, 0, 0, 0, 8, 2]);
    let image = fdc.disk_for_export(0).expect("completed format");
    assert_eq!(image.tracks[0][0].sectors.len(), 1);
    assert_eq!(image.tracks[0][0].sectors[0].id, 7);
    assert_eq!(image.tracks[0][0].sectors[0].data, vec![0x69; 512]);
}

fn wait_request(fdc: &mut Upd765a) {
    for _ in 0..1_000_000 {
        if fdc.read_status() & 0x80 != 0 {
            return;
        }
        fdc.tick();
    }
    panic!("format request/result exceeded bounded wait");
}
