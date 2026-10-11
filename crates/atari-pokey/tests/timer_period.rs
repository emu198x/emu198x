use atari_pokey::Pokey;

#[test]
fn fast_timer_irq_spacing_includes_the_propagation_clocks() {
    let mut mismatches = Vec::new();
    let mut cases = 0;
    for clock in [1_789_772, 1_773_447] {
        let mut chip = Pokey::new(clock);
        chip.write(0x0f, 3);
        chip.write(0x08, 0x40);
        for frequency in 0..=255_u8 {
            chip.write(0x0e, 0);
            chip.write(0, frequency);
            chip.write(0x09, 0);
            chip.write(0x0e, 1);
            let period = u32::from(frequency) + 4;
            let mut edges = Vec::new();
            for tick in 1..=period * 5 {
                chip.tick();
                if chip.irq_pending() {
                    assert_eq!(chip.read(0x0e) & 1, 0);
                    edges.push(tick);
                    chip.write(0x0e, 0);
                    chip.write(0x0e, 1);
                    if edges.len() == 5 {
                        break;
                    }
                }
            }
            assert_eq!(edges.len(), 5, "missing public IRQ events");
            if edges.windows(2).any(|pair| pair[1] - pair[0] != period) {
                mismatches.push((clock, frequency, edges));
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 512);
    assert!(
        mismatches.is_empty(),
        "{} IRQ period mismatches; first: {:?}",
        mismatches.len(),
        mismatches.first()
    );
}
