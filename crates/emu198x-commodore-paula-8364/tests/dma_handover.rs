//! Original reference transitions drive the expected handover observations.
#[path = "../examples/dma_handover_probe.rs"]
mod reference_probe;

#[test]
fn dma_and_manual_handover_matches_reference() {
    reference_probe::main();
}
