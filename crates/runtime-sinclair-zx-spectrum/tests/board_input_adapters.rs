//! Shared host adapters exercise the physical keyboard and Kempston inputs.
use emu198x_shell::InputEvent;
use machine_sinclair_zx_spectrum_48k::{KempstonButton, Spectrum48k};
use runtime_sinclair_zx_spectrum::{ApplyInputEvent, ApplyKempstonEvent};
#[test]
fn machine_applies_host_key_events() {
    let mut machine = Spectrum48k::new();
    let pressed = InputEvent::Key {
        name: "q".into(),
        pressed: true,
    };

    assert!(machine.apply_input_event(&pressed));
    assert_eq!(machine.read_fe(0xfbfe) & 0x01, 0x00);
}

#[test]
fn apply_input_event_returns_false_for_non_key_events() {
    let mut machine = Spectrum48k::new();
    // Button events go through the Kempston path, not the keyboard
    // matrix; the trait declines them.
    let button = InputEvent::Button {
        port: 0,
        name: "fire".into(),
        pressed: true,
    };
    assert!(!machine.apply_input_event(&button));
    // Axis events similarly aren't keyboard input.
    let axis = InputEvent::Axis {
        port: 0,
        name: "horizontal".into(),
        value: 16_000,
    };
    assert!(!machine.apply_input_event(&axis));
}

#[test]
fn apply_input_event_returns_false_for_unknown_key_names() {
    let mut machine = Spectrum48k::new();
    // The Spectrum doesn't have an "escape" or "F12" key; the
    // name lookup falls through to None and the trait returns
    // false without touching the matrix.
    for unknown in ["escape", "F12", "", "πø"] {
        let event = InputEvent::Key {
            name: unknown.into(),
            pressed: true,
        };
        assert!(
            !machine.apply_input_event(&event),
            "name {unknown:?} should not match any SpectrumKey",
        );
    }
    // Matrix must remain untouched (all keys released).
    assert_eq!(machine.read_fe(0xfffe) & 0x1F, 0x1F);
}

#[test]
fn apply_input_event_releases_pressed_keys() {
    let mut machine = Spectrum48k::new();
    let q_press = InputEvent::Key {
        name: "q".into(),
        pressed: true,
    };
    let q_release = InputEvent::Key {
        name: "q".into(),
        pressed: false,
    };
    assert!(machine.apply_input_event(&q_press));
    assert_eq!(machine.read_fe(0xfbfe) & 0x01, 0x00);

    assert!(machine.apply_input_event(&q_release));
    // Q released — bit reads high again.
    assert_eq!(machine.read_fe(0xfbfe) & 0x01, 0x01);
}

#[test]
fn apply_kempston_event_attaches_and_flips_bits() {
    let mut machine = Spectrum48k::new();
    // Defaults: unattached, all bits clear.
    assert!(!machine.kempston_mut().attached);
    assert_eq!(machine.kempston_mut().state, 0);

    machine.apply_kempston_event(KempstonButton::Fire, true);
    assert!(
        machine.kempston_mut().attached,
        "first event must attach the interface"
    );
    assert_eq!(machine.kempston_mut().state, 0b0001_0000, "fire bit");

    machine.apply_kempston_event(KempstonButton::Right, true);
    assert_eq!(machine.kempston_mut().state, 0b0001_0001, "fire + right");

    machine.apply_kempston_event(KempstonButton::Fire, false);
    assert_eq!(
        machine.kempston_mut().state,
        0b0000_0001,
        "only right after fire release"
    );
}
