//! Pulse lengths are measured at the public pins, in elapsed character clocks.
//! Sources: Motorola MC6845 (1977), PDF p. 12; Hitachi U70-A (1987),
//! printed p. 148 / PDF p. 159. See the shared 6845 primary reference.
use motorola_6845::{Crtc6845, Crtc6845Variant};

const VARIANTS: [Crtc6845Variant; 2] = [Crtc6845Variant::Mc6845, Crtc6845Variant::Hd6845s];

fn configured(variant: Crtc6845Variant, registers: &[(u8, u8)]) -> Crtc6845 {
    let mut crtc = Crtc6845::new();
    crtc.set_variant(variant);
    for &(register, value) in registers {
        crtc.write_address(register);
        crtc.write_data(value);
    }
    crtc
}

fn pulse_lengths(crtc: &mut Crtc6845, clocks: usize, horizontal: bool) -> (Vec<usize>, usize) {
    let mut complete = Vec::new();
    let mut high_clocks = 0;
    for _ in 0..clocks {
        crtc.tick();
        if if horizontal { crtc.hsync } else { crtc.vsync } {
            high_clocks += 1;
        } else if high_clocks != 0 {
            complete.push(high_clocks);
            high_clocks = 0;
        }
    }
    (complete, high_clocks)
}

#[test]
fn horizontal_pulses_keep_their_full_width_across_line_wrap() {
    let mut failures = Vec::new();
    for variant in VARIANTS {
        for width in 0..16 {
            for position in [0, 30, 60, 63] {
                let mut crtc = configured(variant, &[(0, 63), (2, position), (3, width)]);
                let (complete, trailing) = pulse_lengths(&mut crtc, 64 * 5, true);
                // Width zero is specified suppression on MC6845, and an
                // existing implementation control for the unprogrammable S value.
                let correct = if width == 0 {
                    complete.is_empty() && trailing == 0
                } else {
                    complete.len() >= 3 && complete.iter().all(|&n| n == usize::from(width))
                };
                if !correct {
                    failures.push((variant, width, position, complete));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} wrong HSYNC cases; first: {:?}",
        failures.len(),
        failures.first()
    );
}

#[test]
fn vertical_pulses_keep_their_full_width_through_adjustment_and_frame_restart() {
    let mut failures = Vec::new();
    for variant in VARIANTS {
        for width in 0..16 {
            for max_raster in [0, 7] {
                for adjust in [0, 1, 3, 31] {
                    for position in [1, 20] {
                        let mut crtc = configured(
                            variant,
                            &[
                                (0, 15),
                                (3, width << 4),
                                (4, 20),
                                (5, adjust),
                                (7, position),
                                (9, max_raster),
                            ],
                        );
                        let lines = if variant == Crtc6845Variant::Mc6845 || width == 0 {
                            16
                        } else {
                            usize::from(width)
                        };
                        let frame_clocks =
                            16 * (21 * (usize::from(max_raster) + 1) + usize::from(adjust));
                        let (complete, _) = pulse_lengths(&mut crtc, frame_clocks * 4, false);
                        if complete.len() < 2 || complete.iter().any(|&n| n != 16 * lines) {
                            failures.push((variant, width, max_raster, adjust, position, complete));
                        }
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} wrong VSYNC cases; first: {:?}",
        failures.len(),
        failures.first()
    );
}

#[test]
fn restoring_an_active_sync_pulse_preserves_its_remaining_intervals() {
    for variant in VARIANTS {
        let mut restored_pulses = 0;
        for phase in 0..32 * 24 * 2 {
            let mut original = configured(
                variant,
                &[
                    (0, 31),
                    (2, 30),
                    (3, 0x3f),
                    (4, 20),
                    (5, 3),
                    (7, 20),
                    (9, 0),
                ],
            );
            for _ in 0..phase {
                original.tick();
            }
            if !original.hsync && !original.vsync {
                continue;
            }
            let saved = original.save_state();
            // The legacy explicit format does not store the part selection;
            // it is supplied by the host before restoring its registers.
            let mut restored = configured(variant, &[]);
            restored
                .load_state(&saved)
                .expect("valid restored snapshot");
            // The original continues without ever loading a snapshot. The
            // other path reloads at each interval, including pulse/frame edges.
            for _ in 0..32 * 17 {
                let frame = original.tick();
                assert_eq!(restored.tick(), frame);
                assert_eq!(
                    (restored.hsync, restored.vsync),
                    (original.hsync, original.vsync)
                );
                let intermediate = restored.save_state();
                restored = configured(variant, &[]);
                restored
                    .load_state(&intermediate)
                    .expect("valid intermediate snapshot");
            }
            restored_pulses += 1;
        }
        assert!(restored_pulses > 100, "must exercise active pulses");
    }
}
