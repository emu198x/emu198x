//! Contracts specific to the checked-in Amiga catalogue manifest.

use std::path::PathBuf;

use emu198x_catalogue::{BootIgnoreRect, ScriptStep, StartupStep, load_manifest};

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("manifest/amiga.toml")
}

#[test]
fn joystick_scripts_use_labelled_amiga_control_ports() {
    let manifest = load_manifest(&manifest_path()).expect("Amiga manifest should parse");

    for entry in &manifest.entry {
        for step in &entry.script {
            let ScriptStep::Button { port, .. } = step else {
                continue;
            };

            assert!(
                matches!(port, 0 | 2),
                "Amiga entry {} sends joystick input to control port {port}; use labelled port 2 or primary-stick alias 0",
                entry.id
            );
        }
        for step in &entry.startup {
            let StartupStep::PressButton { port, .. } = step else {
                continue;
            };

            assert!(
                matches!(port, 0 | 2),
                "Amiga entry {} sends startup joystick input to control port {port}; use labelled port 2 or primary-stick alias 0",
                entry.id
            );
        }
    }
}

#[test]
fn arkanoid_uses_bounded_sequential_startup_navigation() {
    let manifest = load_manifest(&manifest_path()).expect("Amiga manifest should parse");
    let entry = manifest
        .entry
        .iter()
        .find(|entry| entry.id == "arkanoid-revenge-of-doh")
        .expect("Arkanoid catalogue entry should exist");

    assert!(
        entry.script.is_empty(),
        "Arkanoid should not mix legacy absolute-frame script steps with startup actions"
    );
    assert_eq!(
        entry.startup,
        [
            StartupStep::WaitFrames { frames: 6500 },
            StartupStep::ClickMouse {
                button: "left".into(),
                hold_frames: 3,
            },
        ]
    );
    assert_eq!(
        entry.boot.wait_frames, 5999,
        "the explicit release-observation frame must retain Arkanoid's established capture instant"
    );
}

#[test]
fn release_screens_and_trainer_prompts_use_sequential_startup_navigation() {
    let manifest = load_manifest(&manifest_path()).expect("Amiga manifest should parse");

    for id in [
        "barbarian",
        "1943",
        "arkanoid-revenge-of-doh",
        "bad-dudes-ecs",
        "banshee-demo-aga",
    ] {
        let entry = manifest
            .entry
            .iter()
            .find(|entry| entry.id == id)
            .unwrap_or_else(|| panic!("{id} catalogue entry should exist"));
        assert!(
            entry.script.is_empty(),
            "{id} must not retain legacy absolute-frame navigation"
        );
        assert!(
            !entry.startup.is_empty(),
            "{id} must declare bounded startup navigation"
        );
    }

    let bad_dudes = manifest
        .entry
        .iter()
        .find(|entry| entry.id == "bad-dudes-ecs")
        .expect("Bad Dudes catalogue entry should exist");
    assert_eq!(
        bad_dudes.startup,
        [
            StartupStep::WaitFrames { frames: 7000 },
            StartupStep::PressKey {
                key: "return".into(),
                hold_frames: 3,
            },
            StartupStep::WaitFrames { frames: 5496 },
            StartupStep::PressKey {
                key: "return".into(),
                hold_frames: 3,
            },
        ],
        "sequential Bad Dudes navigation must retain the established absolute input timeline"
    );
    assert_eq!(
        bad_dudes.boot.wait_frames, 7999,
        "the final release-observation frame must retain Bad Dudes' established capture instant"
    );
}

#[test]
fn workbench_13_excludes_only_the_volatile_free_memory_readout() {
    let manifest = load_manifest(&manifest_path()).expect("Amiga manifest should parse");
    let entry = manifest
        .entry
        .iter()
        .find(|entry| entry.id == "workbench-1.3-desktop")
        .expect("Workbench 1.3 catalogue entry should exist");

    assert_eq!(
        entry.boot.ignore_rects,
        [BootIgnoreRect {
            x: 278,
            y: 38,
            width: 50,
            height: 18,
        }],
        "only the allocator-derived free-memory field may be excluded"
    );
}
