//! Register widths and access permissions: Motorola 1977 Table 2, PDF p. 11;
//! Hitachi U70-A (1987), printed p. 147 / PDF p. 158.
use motorola_6845::{Crtc6845, Crtc6845Variant};

const VARIANTS: [Crtc6845Variant; 2] = [Crtc6845Variant::Mc6845, Crtc6845Variant::Hd6845s];

fn write(crtc: &mut Crtc6845, register: u8, value: u8) {
    crtc.write_address(register);
    crtc.write_data(value);
}

#[test]
fn register_writes_keep_only_the_bits_present_on_each_part() {
    // Manufacturer register tables; not derived from the implementation.
    for (variant, masks) in [
        (
            Crtc6845Variant::Mc6845,
            [
                0xff, 0xff, 0xff, 0x0f, 0x7f, 0x1f, 0x7f, 0x7f, 0x03, 0x1f, 0x7f, 0x1f, 0x3f, 0xff,
                0x3f, 0xff,
            ],
        ),
        (
            Crtc6845Variant::Hd6845s,
            [
                0xff, 0xff, 0xff, 0xff, 0x7f, 0x1f, 0x7f, 0x7f, 0xf3, 0x1f, 0x7f, 0x1f, 0x3f, 0xff,
                0x3f, 0xff,
            ],
        ),
    ] {
        let mut crtc = Crtc6845::new();
        crtc.set_variant(variant);
        for (register, mask) in masks.into_iter().enumerate() {
            for value in 0..=255 {
                write(&mut crtc, register as u8, value);
                assert_eq!(
                    crtc.regs()[register],
                    value & mask,
                    "{variant:?} R{register} write {value:#04x}"
                );
                if variant.reads_back(register) {
                    assert_eq!(crtc.read_data(), value & mask);
                }
            }
        }
    }
}

#[test]
fn software_cannot_overwrite_the_light_pen_address() {
    for variant in VARIANTS {
        let mut crtc = Crtc6845::new();
        crtc.set_variant(variant);
        // Seed an already-captured address through the existing explicit
        // snapshot layout. This tests preservation, not an absent LPSTB input.
        let mut state = crtc.save_state();
        state[1 + 16] = 0x2a;
        state[1 + 17] = 0x59;
        crtc.load_state(&state).expect("valid prior snapshot");
        for value in 0..=255 {
            for (register, expected) in [(16, 0x2a), (17, 0x59)] {
                write(&mut crtc, register, value);
                assert_eq!(
                    crtc.read_data(),
                    expected,
                    "{variant:?} R{register} is read-only"
                );
            }
        }
        for selected in 18..32 {
            write(&mut crtc, selected, 0xff);
            assert_eq!(crtc.read_data(), 0);
        }
        for (register, expected) in [(16, 0x2a), (17, 0x59)] {
            crtc.write_address(register);
            assert_eq!(crtc.read_data(), expected);
        }
    }
}

#[test]
fn unused_vertical_display_bit_cannot_change_the_display_enable_signal() {
    for variant in VARIANTS {
        for rows in 0..128 {
            let mut plain = Crtc6845::new();
            let mut unused_bit_set = Crtc6845::new();
            for crtc in [&mut plain, &mut unused_bit_set] {
                crtc.set_variant(variant);
                for (register, value) in [(0, 15), (1, 8), (4, 127), (6, rows), (9, 0)] {
                    write(crtc, register, value);
                }
            }
            write(&mut unused_bit_set, 6, rows | 0x80);
            let mut displayed = 0;
            for clock in 0..16 * 128 * 2 {
                assert_eq!(plain.tick(), unused_bit_set.tick());
                assert_eq!(
                    plain.display_enable, unused_bit_set.display_enable,
                    "{variant:?} R6={rows:#04x}, clock {clock}"
                );
                displayed += usize::from(plain.display_enable);
            }
            assert_eq!(displayed, usize::from(rows) * 8 * 2);
        }
    }
}

#[test]
fn new_writes_mask_old_snapshot_values_without_rejecting_the_snapshot() {
    let mut crtc = Crtc6845::new();
    crtc.set_variant(Crtc6845Variant::Hd6845s);
    let mut old = crtc.save_state();
    old[1 + 6] = 0x80;
    old[1 + 12] = 0xff;
    assert_eq!(
        crtc.load_state(&old)
            .expect("older register bytes remain loadable"),
        old.len()
    );
    write(&mut crtc, 6, 0x80);
    write(&mut crtc, 12, 0xff);
    assert_eq!(crtc.regs()[6], 0);
    assert_eq!(crtc.read_data(), 0x3f);
    let normalised = crtc.save_state();
    let mut restored = Crtc6845::new();
    restored.set_variant(Crtc6845Variant::Hd6845s);
    restored
        .load_state(&normalised)
        .expect("valid corrected snapshot");
    assert_eq!(restored.regs(), crtc.regs());
    assert_eq!(restored.read_data(), 0x3f);
}
