//! Deliberately failing current-source probe; installed temporarily by run.py.
//! No DMA bus transfer is claimed by these register-level observations.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

fn controller(nd: u8) -> Upd765a {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    fdc.insert_disk(
        0,
        DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![DiskTrack {
                sectors: vec![DiskSector {
                    c: 0,
                    h: 0,
                    id: 1,
                    n: 1,
                    st1: 0,
                    st2: 0,
                    data: (0..=255).collect(),
                }],
            }]],
        },
    );
    for byte in [3, 0xdf, 2 | nd] {
        fdc.write_data(byte);
    }
    assert_eq!(
        fdc.read_status(),
        0x80,
        "Specify returns to idle without result"
    );
    assert!(!fdc.interrupt, "Specify itself must not raise an IRQ");
    fdc
}

fn start(fdc: &mut Upd765a) {
    for byte in [0x46, 0, 0, 0, 1, 1, 1, 0x2a, 0xff] {
        fdc.write_data(byte);
    }
}

fn await_byte(fdc: &mut Upd765a) {
    for hc in 0..10_000 {
        if fdc.read_status() & 0x80 != 0 {
            return;
        }
        fdc.tick(hc);
    }
    panic!("Read Data byte request exceeded10000ticks");
}

fn complete(nd: u8) -> Upd765a {
    let mut fdc = controller(nd);
    start(&mut fdc);
    for expected in 0..=255 {
        await_byte(&mut fdc);
        assert_eq!(fdc.read_data(), expected, "actual payload transfer");
    }
    assert_eq!(fdc.read_status(), 0xd0, "native result entered");
    fdc
}

#[test]
fn non_dma_mode_sets_execution_bit() {
    let mut fdc = controller(1);
    start(&mut fdc);
    assert_ne!(fdc.read_status() & 0x20, 0, "non-DMA EXM during preamble");
    await_byte(&mut fdc);
    assert_ne!(
        fdc.read_status() & 0x20,
        0,
        "non-DMA EXM during byte request"
    );
}

#[test]
fn dma_mode_clears_execution_bit() {
    let mut fdc = controller(0);
    start(&mut fdc);
    assert_eq!(fdc.read_status() & 0x20, 0, "DMA mode must clear EXM");
}

#[test]
fn programmed_dma_mode_survives_an_intervening_drive_status_command() {
    let mut fdc = controller(0);
    fdc.write_data(4);
    fdc.write_data(0);
    let _ = fdc.read_data();
    assert_eq!(fdc.read_status(), 0x80);
    start(&mut fdc);
    assert_eq!(
        fdc.read_status() & 0x20,
        0,
        "Specify ND persists across commands"
    );
}

#[test]
fn non_dma_byte_request_raises_irq_and_data_read_acknowledges() {
    let mut fdc = controller(1);
    start(&mut fdc);
    assert!(!fdc.interrupt, "no byte IRQ during preamble");
    await_byte(&mut fdc);
    assert!(fdc.interrupt, "non-DMA byte request raises IRQ");
    let _ = fdc.read_status();
    assert!(fdc.interrupt, "MSR polling cannot acknowledge byte IRQ");
    assert_eq!(fdc.read_data(), 0);
    assert!(!fdc.interrupt, "accepted payload byte acknowledges IRQ");
    await_byte(&mut fdc);
    assert!(fdc.interrupt, "next byte raises a new IRQ");
}

#[test]
fn programmed_dma_mode_has_no_byte_irq() {
    let mut fdc = controller(0);
    start(&mut fdc);
    await_byte(&mut fdc);
    assert!(!fdc.interrupt, "DMA does not raise execution byte IRQ");
}

#[test]
fn read_data_completion_already_asserts_irq() {
    let mut fdc = complete(1);
    assert!(fdc.interrupt, "Read Data completion IRQ already exists");
    assert_eq!(fdc.read_data(), 0x40, "normal no-TC EOT termination ST0");
    assert_eq!(fdc.read_data(), 0x80, "EOT without TC sets EN");
}

#[test]
fn first_result_byte_acknowledges_completion_irq() {
    let mut fdc = complete(1);
    assert!(fdc.interrupt, "completion IRQ must be pending");
    let _ = fdc.read_status();
    assert!(fdc.interrupt, "MSR does not acknowledge completion IRQ");
    assert_eq!(fdc.read_data(), 0x40);
    assert!(!fdc.interrupt, "first Read Data result byte clears IRQ");
}
