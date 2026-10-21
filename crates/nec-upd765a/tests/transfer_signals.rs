//! Programmed transfer-mode and IRQ regressions (NEC processor interface/Specify).
//! No DMA bus transfer is claimed by these register-level observations.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

fn controller(nd: u8) -> Upd765a {
    controller_with_writable(nd, false)
}

fn controller_with_writable(nd: u8, writable: bool) -> Upd765a {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
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
                    n: 1,
                    st1: 0,
                    st2: 0,
                    data: (0..=255).collect(),
                }],
            }]],
        },
        writable,
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
    await_byte_limit(fdc, 10_000);
}

fn await_byte_limit(fdc: &mut Upd765a, limit: u32) {
    for hc in 0..limit {
        if fdc.read_status() & 0x80 != 0 {
            return;
        }
        fdc.tick(hc);
    }
    panic!("byte request exceeded {limit} ticks");
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

#[test]
fn shared_payload_engines_honour_programmed_mode_and_request_irq() {
    let mut deviations = Vec::new();
    let mut rows = 0;
    for opcode in [0x46_u8, 0x4c, 0x42, 0x45, 0x49, 0x51, 0x59, 0x5d, 0x4d] {
        for nd in [0, 1] {
            let mut fdc = controller_with_writable(nd, true);
            let command = if opcode == 0x4d {
                vec![opcode, 0, 1, 1, 0x2a, 0xe5]
            } else {
                vec![opcode, 0, 0, 0, 1, 1, 1, 0x2a, 1]
            };
            for byte in command {
                fdc.write_data(byte);
            }
            await_byte_limit(&mut fdc, 50_000);
            let msr = fdc.read_status();
            let irq = fdc.interrupt;
            let expected_exm = nd != 0;
            let observed_exm = msr & 0x20 != 0;
            let expected_irq = nd != 0;
            let is_read = matches!(opcode, 0x46 | 0x4c | 0x42);
            assert_eq!(msr & 0x40 != 0, is_read, "correct payload direction");
            if is_read {
                assert_eq!(fdc.read_data(), 0, "actual first payload byte");
            } else {
                fdc.write_data(0);
            }
            assert_eq!(
                fdc.read_status() & 0x80,
                0,
                "byte accepted in execution, not result"
            );
            println!(
                "SHARED opcode={opcode:02x} nd={nd} msr={msr:02x} irq={irq} expected_exm={expected_exm} expected_irq={expected_irq}"
            );
            rows += 1;
            if observed_exm != expected_exm || irq != expected_irq {
                deviations.push((opcode, nd, observed_exm, irq));
            }
        }
    }
    assert_eq!(
        rows, 18,
        "all nine engines observed in both programmed modes"
    );
    assert!(
        deviations.is_empty(),
        "shared transfer-mode/IRQ mismatches: {deviations:?}"
    );
}

#[test]
fn every_payload_byte_is_acknowledged_and_final_completion_remains_pending() {
    for opcode in [0x46_u8, 0x4c, 0x42, 0x45, 0x49, 0x51, 0x59, 0x5d, 0x4d] {
        for nd in [0, 1] {
            let mut fdc = controller_with_writable(nd, true);
            let (command, payload) = if opcode == 0x4d {
                (vec![opcode, 0, 1, 1, 0x2a, 0xe5], vec![0, 0, 1, 1])
            } else {
                (vec![opcode, 0, 0, 0, 1, 1, 1, 0x2a, 1], (0..=255).collect())
            };
            for byte in command {
                fdc.write_data(byte);
            }
            let is_read = matches!(opcode, 0x46 | 0x4c | 0x42);
            for (index, &byte) in payload.iter().enumerate() {
                await_byte_limit(&mut fdc, 50_000);
                assert_eq!(
                    fdc.interrupt,
                    nd != 0,
                    "request IRQ opcode={opcode:02x} ND={nd} byte={index}"
                );
                assert_eq!(fdc.read_status() & 0x20 != 0, nd != 0, "programmed EXM");
                assert_eq!(fdc.interrupt, nd != 0, "MSR cannot acknowledge byte IRQ");
                if is_read {
                    assert_eq!(fdc.read_data(), byte);
                } else {
                    fdc.write_data(byte);
                }
                assert_eq!(
                    fdc.interrupt,
                    is_read && index + 1 == payload.len(),
                    "accepted byte acknowledges IRQ; final read retains completion opcode={opcode:02x} ND={nd}"
                );
            }
            // Format retains its existing full-track post-ID interval.
            // Bound the wait without asserting physical rotational accuracy.
            await_byte_limit(&mut fdc, 1_000_000);
            assert_eq!(fdc.read_status(), 0xd0, "complete result");
            assert!(fdc.interrupt, "completion IRQ is independent of ND");
            for _ in 0..7 {
                assert_eq!(fdc.read_status(), 0xd0);
                let _ = fdc.read_data();
                assert!(!fdc.interrupt, "first result read acknowledges completion");
            }
            assert_eq!(fdc.read_status(), 0x80);
            assert_eq!(fdc.read_data(), 0xff);
        }
    }
}

#[test]
fn immediate_error_results_raise_completion_irq_and_acknowledge_on_first_byte() {
    for opcode in [0x46_u8, 0x4c, 0x42, 0x45, 0x49, 0x51, 0x59, 0x5d, 0x4d] {
        for nd in [0, 1] {
            let mut fdc = controller(nd);
            let command = if opcode == 0x4d {
                vec![opcode, 0, 1, 1, 0x2a, 0xe5] // write-protected medium
            } else {
                vec![
                    opcode,
                    0,
                    9,
                    0,
                    1,
                    if opcode == 0x42 { 8 } else { 1 },
                    1,
                    0x2a,
                    1,
                ]
            };
            for byte in command {
                fdc.write_data(byte);
            }
            assert_eq!(fdc.read_status(), 0xd0, "error result opcode={opcode:02x}");
            assert!(fdc.interrupt, "error completion also raises IRQ");
            for _ in 0..7 {
                let _ = fdc.read_data();
                assert!(!fdc.interrupt, "error first-result acknowledgement");
            }
            assert_eq!(fdc.read_status(), 0x80);
            assert_eq!(
                fdc.disk_for_export(0).expect("source still mounted").tracks[0][0].sectors[0].data,
                (0..=255).collect::<Vec<u8>>(),
                "failed command must not change payload"
            );
        }
    }
}

#[test]
fn queued_ready_attention_survives_payload_irq_acknowledgements() {
    let mut fdc = controller(1);
    for drive in 0..4 {
        fdc.set_ready(drive, false);
    }
    fdc.reset();
    for byte in [3, 0xdf, 3] {
        fdc.write_data(byte);
    }
    fdc.set_ready(1, true);
    for hc in 0..8_000 {
        fdc.tick(hc);
    }
    assert!(fdc.interrupt, "create independent READY attention");
    fdc.write_data(4);
    fdc.write_data(1);
    let _ = fdc.read_data();
    assert!(fdc.interrupt, "SDS does not acknowledge READY attention");
    fdc.set_ready(0, true);
    start(&mut fdc);
    for expected in 0..=255 {
        await_byte(&mut fdc);
        assert_eq!(fdc.read_data(), expected);
    }
    for _ in 0..7 {
        let _ = fdc.read_data();
    }
    assert!(
        !fdc.interrupt,
        "payload/result acknowledgement deasserts output"
    );
    fdc.write_data(8);
    assert_eq!(
        fdc.read_data(),
        0xc1,
        "the independent READY record is retained"
    );
    assert_eq!(fdc.read_data(), 0, "READY attention PCN");
    assert_eq!(fdc.read_status(), 0x80);
    fdc.write_data(8);
    assert_eq!(fdc.read_data(), 0x80, "no duplicate attention record");
}

#[test]
fn unread_payload_request_becomes_an_acknowledgeable_overrun_result() {
    for nd in [0, 1] {
        let mut fdc = controller(nd);
        start(&mut fdc);
        await_byte(&mut fdc);
        assert_eq!(fdc.interrupt, nd != 0);
        // Observe the existing bounded sector-model timeout, without claiming
        // an exact silicon service deadline from its particular tick count.
        for hc in 0..10_000 {
            if fdc.read_status() == 0xd0 && fdc.interrupt {
                break;
            }
            fdc.tick(hc);
        }
        assert_eq!(fdc.read_status(), 0xd0);
        assert!(fdc.interrupt, "overrun has completion IRQ in both modes");
        assert_eq!(fdc.read_data(), 0x40);
        assert!(!fdc.interrupt, "first error result acknowledges IRQ");
        assert_eq!(fdc.read_data() & 0x10, 0x10, "actual overrun result");
        for _ in 2..7 {
            let _ = fdc.read_data();
        }
        assert_eq!(fdc.read_status(), 0x80);
    }
}
