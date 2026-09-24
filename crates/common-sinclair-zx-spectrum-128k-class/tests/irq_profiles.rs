//! IRQ edges from the pinned SpecIde grey +2 trace and validated early
//! Toastrack profile. Native ULA edges: two per CPU T-state.
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum_128k_class::{
    AmstradPlus2Marker, Class128kVariant, Sinclair128KMarker, Spectrum128kClassCore,
};

fn transitions<V: Class128kVariant>(restore: bool) -> Vec<(usize, bool)> {
    let mut machine = Spectrum128kClassCore::<V>::new();
    machine.reset();
    let mut previous = false;
    let mut edges = Vec::new();
    for edge in 0..456 * 311 * 2 {
        // Restore before, during and after the first IRQ pulse. The
        // serialized engine omits its config; the marker must reattach it.
        if restore && [112_000, 113_100, 113_170].contains(&edge) {
            let state = serde_json::to_vec(&machine).expect("serialize machine");
            machine = serde_json::from_slice(&state).expect("deserialize machine");
            machine.restore_volatile_refs();
            machine.reset();
        }
        machine.ula.tick(
            &machine.memory,
            0,
            false,
            false,
            false,
            &mut machine.framebuffer,
        );
        let active = machine.ula.interrupt_active();
        if active != previous {
            edges.push((edge, active));
            previous = active;
        }
    }
    edges
}

#[test]
fn grey_plus2_keeps_its_36t_late_pulse_across_reset_and_restore() {
    let expected = vec![
        (113_091, true),
        (113_163, false),
        (254_907, true),
        (254_979, false),
    ];
    assert_eq!(transitions::<AmstradPlus2Marker>(false), expected);
    assert_eq!(transitions::<AmstradPlus2Marker>(true), expected);
}

#[test]
fn toastrack_keeps_its_validated_early_pulse_across_reset_and_restore() {
    let expected = vec![
        (113_093, true),
        (113_165, false),
        (254_909, true),
        (254_981, false),
    ];
    assert_eq!(transitions::<Sinclair128KMarker>(false), expected);
    assert_eq!(transitions::<Sinclair128KMarker>(true), expected);
}
