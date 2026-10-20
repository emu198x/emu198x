//! Complete digital READY event/result contract, separate from drive mechanics.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::Upd765a;

fn controller() -> Upd765a {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    for drive in 0..4 {
        fdc.set_ready(drive, false);
    }
    fdc
}
fn ticks(fdc: &mut Upd765a, count: u32) {
    for hc in 0..count {
        fdc.tick(hc);
    }
}
fn result(fdc: &mut Upd765a) -> Vec<u8> {
    fdc.write_data(8);
    assert_eq!(fdc.read_status(), 0xd0);
    let st0 = fdc.read_data();
    let mut out = vec![st0];
    if st0 != 0x80 {
        assert_eq!(fdc.read_status(), 0xd0);
        out.push(fdc.read_data());
    }
    assert_eq!(fdc.read_status(), 0x80);
    out
}

#[test]
fn both_ready_polarities_each_unit_and_unchanged_levels() {
    for unit in 0..4 {
        let mut fdc = controller();
        assert_eq!(result(&mut fdc), [0x80]);
        fdc.set_ready(unit, true);
        ticks(&mut fdc, 8000);
        assert!(fdc.interrupt);
        assert_eq!(result(&mut fdc), [0xc0 | unit as u8, 0]);
        fdc.set_ready(unit, true);
        ticks(&mut fdc, 8000);
        assert_eq!(result(&mut fdc), [0x80], "no duplicate level event");
        fdc.set_ready(unit, false);
        ticks(&mut fdc, 8000);
        assert_eq!(result(&mut fdc), [0xc8 | unit as u8, 0]);
    }
}

#[test]
fn all_units_drain_in_order_without_reasserting_acknowledged_irq() {
    let mut fdc = controller();
    for unit in (0..4).rev() {
        fdc.set_ready(unit, true);
    }
    ticks(&mut fdc, 8000);
    for unit in 0..4 {
        assert_eq!(result(&mut fdc), [0xc0 | unit, 0]);
        assert!(!fdc.interrupt, "pending records are not the IRQ level");
    }
    assert_eq!(result(&mut fdc), [0x80]);
    fdc.set_ready(3, false);
    ticks(&mut fdc, 8000);
    assert!(fdc.interrupt, "a later event raises IRQ again");
    assert_eq!(result(&mut fdc), [0xcb, 0]);
}

#[test]
fn first_attention_is_not_overwritten_by_a_second_transition() {
    let mut fdc = controller();
    fdc.set_ready(0, true);
    ticks(&mut fdc, 8000);
    fdc.set_ready(0, false);
    ticks(&mut fdc, 8000);
    assert_eq!(result(&mut fdc), [0xc0, 0]);
    ticks(&mut fdc, 8000);
    assert_eq!(
        result(&mut fdc),
        [0x80],
        "sampled second level is not queued twice"
    );
}

#[test]
fn command_and_result_phases_defer_ready_sampling() {
    let mut fdc = controller();
    fdc.write_data(3);
    fdc.set_ready(0, true);
    ticks(&mut fdc, 8000);
    assert!(!fdc.interrupt, "incomplete command is not an idle poll");
    fdc.write_data(0xdf);
    fdc.write_data(1);
    ticks(&mut fdc, 8000);
    assert_eq!(result(&mut fdc), [0xc0, 0]);
    fdc.write_data(4);
    fdc.write_data(0);
    fdc.set_ready(0, false);
    ticks(&mut fdc, 8000);
    assert!(!fdc.interrupt, "undrained result suppresses polling");
    assert_ne!(
        fdc.read_data() & 0x20,
        0,
        "ST3 captures the command-time level"
    );
    ticks(&mut fdc, 8000);
    assert_eq!(result(&mut fdc), [0xc8, 0]);
}

#[test]
fn ready_attention_preserves_nonzero_present_cylinder() {
    let mut fdc = controller();
    fdc.set_ready(1, true);
    for b in [0x0f, 1, 23] {
        fdc.write_data(b);
    }
    ticks(&mut fdc, 1024);
    assert_eq!(result(&mut fdc), [0x21, 23]);
    ticks(&mut fdc, 8000);
    assert_eq!(result(&mut fdc), [0xc1, 23]);
    fdc.set_ready(1, false);
    ticks(&mut fdc, 8000);
    assert_eq!(result(&mut fdc), [0xc9, 23]);
}

#[test]
fn reset_discards_pending_records_but_preserves_external_levels() {
    let mut fdc = controller();
    fdc.set_ready(2, true);
    ticks(&mut fdc, 8000);
    assert!(fdc.interrupt);
    fdc.reset();
    assert!(!fdc.interrupt);
    assert_eq!(result(&mut fdc), [0x80]);
    ticks(&mut fdc, 8000);
    assert_eq!(
        result(&mut fdc),
        [0xc2, 0],
        "asserted reset input is polled again"
    );
    fdc.reset();
    fdc.reset();
    ticks(&mut fdc, 8000);
    assert_eq!(result(&mut fdc), [0xc2, 0]);
}

#[test]
fn polling_uses_the_four_mhz_fdc_interval_without_per_period_rounding_drift() {
    let mut fdc = controller();
    let mut events = Vec::new();
    for tick in 1..=116_225 {
        fdc.set_ready(0, events.len() % 2 == 0);
        fdc.tick(0);
        if fdc.interrupt {
            events.push(tick);
            result(&mut fdc);
        }
    }
    // Independently calculated ceil(k * 2.048ms * 3,546,900 CPU Tstates/s).
    assert_eq!(
        &events[..8],
        &[7265, 14529, 21793, 29057, 36321, 43585, 50849, 58113]
    );
    assert_eq!(events.len(), 16);
}

#[test]
fn controller_reset_preserves_logical_pcn_and_physical_position() {
    let mut fdc = controller();
    fdc.set_ready(2, true);
    for byte in [15, 2, 23] {
        fdc.write_data(byte);
    }
    ticks(&mut fdc, 1024);
    assert_eq!(result(&mut fdc), [0x22, 23]);
    fdc.reset();
    ticks(&mut fdc, 8000);
    assert_eq!(result(&mut fdc), [0xc2, 23], "reset is not a recalibration");
}
