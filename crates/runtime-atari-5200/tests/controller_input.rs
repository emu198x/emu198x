//! Original reset-vector BIOS and 6502 cartridge. All observations are guest RAM.
use emu198x_shell::{HostIo, InputEvent, MachineCore, NullAudioSink, NullFrameSink, NullTraceSink};
use runtime_atari_5200::{Atari5200Runtime, Model};

fn firmware(selector: u8, skctl: u8) -> (Vec<u8>, Vec<u8>) {
    let mut code = vec![0x78, 0xd8, 0xa2, 0xff, 0x9a]; // SEI; CLD; LDX #$ff; TXS
    for (addr, value) in [
        (0xd400u16, 0),
        (0xc01d, 0),
        (0xc01f, selector | 4),
        (0xe80f, skctl),
        (0xe80e, 0xc0),
        (0xe80b, 0),
        (0x0230, 0xa5),
    ] {
        code.extend([0xa9, value, 0x8d, addr as u8, (addr >> 8) as u8]);
    }
    let loop_addr = 0x4000u16 + code.len() as u16;
    for (source, dest) in (0..4)
        .map(|n| (0xc010u16 + n, 0x0200u16 + n))
        .chain((0..8).map(|n| (0xe800 + n, 0x0210 + n)))
        .chain([(0xe809, 0x0220), (0xe80f, 0x0221), (0xe80e, 0x0222)])
    {
        code.extend([
            0xad,
            source as u8,
            (source >> 8) as u8,
            0x8d,
            dest as u8,
            (dest >> 8) as u8,
        ]);
    }
    // Record every observed active-low key/shift/IRQ bit, not one arbitrary scan phase.
    for (source, dest) in [(0x0221u16, 0x0224u16), (0x0222, 0x0225)] {
        code.extend([
            0xad,
            source as u8,
            2,
            0x49,
            0xff,
            0x0d,
            dest as u8,
            2,
            0x8d,
            dest as u8,
            2,
        ]);
    }
    code.extend([0xee, 0x31, 2, 0xd0, 3, 0xee, 0x32, 2]); // 16-bit execution counter
    code.extend([0x4c, loop_addr as u8, (loop_addr >> 8) as u8]);
    let mut cart = vec![0xea; 32768];
    cart[..code.len()].copy_from_slice(&code);
    let mut bios = vec![0xea; 2048];
    for i in [0x7fa, 0x7fc, 0x7fe] {
        bios[i..i + 2].copy_from_slice(&[0, 0x40]);
    }
    (cart, bios)
}
fn button(port: u8, name: &'static str, pressed: bool) -> InputEvent {
    InputEvent::Button {
        port,
        name: name.into(),
        pressed,
    }
}
fn axis(port: u8, name: &'static str, value: i16) -> InputEvent {
    InputEvent::Axis {
        port,
        name: name.into(),
        value,
    }
}
fn run(runtime: &mut Atari5200Runtime, events: &[InputEvent]) {
    let (mut frames, mut audio, mut trace) = (NullFrameSink, NullAudioSink, NullTraceSink);
    let mut host = HostIo {
        input_events: events,
        frame_sink: &mut frames,
        audio_sink: &mut audio,
        trace_sink: &mut trace,
    };
    runtime
        .run_until(runtime.time().saturating_add(262 * 228 * 4), &mut host)
        .expect("guest runs");
}
fn capture(runtime: &Atari5200Runtime) -> ([u8; 4], [u8; 8], u8, u8, u8, u8, u8, u16) {
    let m = runtime.machine().expect("machine");
    assert_eq!(m.peek(0x230), 0xa5, "guest did not initialize");
    let count = u16::from_le_bytes([m.peek(0x231), m.peek(0x232)]);
    assert!(count > 100, "guest did not repeatedly capture registers");
    (
        std::array::from_fn(|i| m.peek(0x200 + i as u16)),
        std::array::from_fn(|i| m.peek(0x210 + i as u16)),
        m.peek(0x220),
        m.peek(0x221),
        m.peek(0x222),
        m.peek(0x224),
        m.peek(0x225),
        count,
    )
}
fn hex(values: &[u8]) -> String {
    values
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}
#[test]
fn all_controller_routes_execute_through_the_guest() {
    let mut failures = 0;
    let mut rows = 0;
    for extra in [false, true] {
        println!(
            "case,port,selected,skctl,triggers,pots,kbcode,skstat,irqst,seen_status,seen_irq,guest_loops,passed"
        );
        for port in [0, 1, 2, 3, 4, 5, 255] {
            for kind in ["idle", "fire1", "x", "y", "fire2", "keypad"] {
                for selector in 0..4 {
                    if extra && selector != 0 {
                        continue;
                    }
                    for skctl in if extra { [1, 3] } else { [0, 2] } {
                        let events = match kind {
                            "idle" => vec![],
                            "x" => vec![axis(port, "x", i16::MAX)],
                            "y" => vec![axis(port, "y", i16::MAX)],
                            "keypad" => vec![button(port, "1", true)],
                            _ => vec![button(port, kind, true)],
                        };
                        let (cart, bios) = firmware(selector, skctl);
                        let mut runtime = Atari5200Runtime::new(Model::Ca018087, cart, bios)
                            .expect("original cartridge");
                        run(&mut runtime, &events);
                        let (trig, pots, kb, sk, irq, seen, seen_irq, count) = capture(&runtime);
                        let index = match port {
                            0 | 1 => Some(0usize),
                            2..=4 => Some((port - 1) as usize),
                            _ => None,
                        };
                        let selected = index == Some(selector as usize);
                        let mut expected_trig = [1; 4];
                        if kind == "fire1"
                            && let Some(i) = index
                        {
                            expected_trig[i] = 0;
                        }
                        // Do not presume the cold target of unconnected pots. Compare all
                        // other channels against a fresh released control through the same guest.
                        let (cart, bios) = firmware(selector, skctl);
                        let mut idle =
                            Atari5200Runtime::new(Model::Ca018087, cart, bios).expect("control");
                        run(&mut idle, &[]);
                        let mut expected_pots = capture(&idle).1;
                        if let Some(i) = index
                            && (kind == "x" || kind == "y")
                        {
                            expected_pots[i * 2 + usize::from(kind == "y")] = 228;
                        }
                        let expected_key = kind == "keypad" && selected && skctl == 2;
                        let expected_shift = kind == "fire2" && selected && skctl & 2 != 0;
                        let pass = trig == expected_trig
                            && pots == expected_pots
                            && (seen & 4 != 0) == expected_key
                            && (seen & 8 != 0) == expected_shift
                            && (seen_irq & 0x80 != 0) == expected_shift;
                        failures += usize::from(!pass);
                        rows += 1;
                        println!(
                            "{kind},{port},{selector},{skctl},{},{},{kb:02x},{sk:02x},{irq:02x},{seen:02x},{seen_irq:02x},{count},{pass}",
                            hex(&trig),
                            hex(&pots)
                        );
                    }
                }
            }
        }
        eprintln!("{failures} mismatches across {rows} guest captures");
        assert_eq!(
            failures, 0,
            "controller observations differ from connector routing and scan-enable invariants"
        );
    }
}

#[test]
fn restore_retains_held_controls_and_rejects_old_layout_without_mutation() {
    for selector in 0..4 {
        for skctl in [1, 2, 3] {
            let (cart, bios) = firmware(selector, skctl);
            let mut original = Atari5200Runtime::new(Model::Ca018087, cart, bios).expect("guest");
            let mut events = Vec::new();
            for port in 1..=4 {
                for key in ["left", "right", "up", "fire1", "fire2", "1", "2"] {
                    events.push(button(port, key, true));
                }
            }
            run(&mut original, &events);
            // Save between scan edges and at differing positions in the raster.
            original.machine_mut().expect("machine").step_instruction();
            let state = original.snapshot().expect("save held controls");
            let mut restored = Atari5200Runtime::blank(Model::Ca018087);
            restored.restore(&state).expect("restore held controls");
            assert_eq!(restored.snapshot().expect("restored bytes"), state);
            // Releasing right must uncover the still-held left direction.
            // Releasing one key must retain another switch and upper fire.
            let releases: Vec<_> = (1..=4)
                .flat_map(|port| [button(port, "right", false), button(port, "1", false)])
                .collect();
            for events in [releases.as_slice(), &[], &[button(4, "fire2", false)]] {
                run(&mut original, events);
                run(&mut restored, events);
                assert_eq!(capture(&restored), capture(&original));
                if skctl == 2 {
                    assert_eq!(
                        capture(&restored).2 & 0x1e,
                        0x1c,
                        "releasing key 1 leaves physical key 2 held"
                    );
                }
                assert_eq!(
                    restored.snapshot().expect("continued restored"),
                    original.snapshot().expect("continued original")
                );
            }
            let before = restored.snapshot().expect("before failed restore");
            let old = postcard::to_allocvec(&19u16).expect("old version");
            assert!(restored.restore(&old).is_err());
            assert_eq!(restored.snapshot().expect("after failed restore"), before);
            let mut truncated = before.clone();
            truncated.truncate(truncated.len() - 1);
            assert!(restored.restore(&truncated).is_err());
            assert_eq!(
                restored.snapshot().expect("after truncated restore"),
                before
            );
            original.reset(emu198x_shell::ResetKind::Hard);
            run(&mut original, &[]);
            let triggers = capture(&original).0;
            assert_eq!(triggers, [1; 4], "reset releases host controls");
        }
    }
}

#[test]
fn two_port_guest_ignores_absent_connectors_and_selectors() {
    for port in 1..=4 {
        for selector in 0..4 {
            for kind in ["fire1", "fire2", "keypad", "x", "y"] {
                let events = match kind {
                    "keypad" => vec![button(port, "1", true)],
                    "x" | "y" => vec![axis(port, kind, i16::MAX)],
                    _ => vec![button(port, kind, true)],
                };
                let (cart, bios) = firmware(selector, 2);
                let mut active = Atari5200Runtime::new(Model::Ca021374, cart.clone(), bios.clone())
                    .expect("two-port probe");
                let mut idle =
                    Atari5200Runtime::new(Model::Ca021374, cart, bios).expect("released control");
                run(&mut active, &events);
                run(&mut idle, &[]);
                let (trig, pots, _, _, _, seen, seen_irq, _) = capture(&active);
                let connected = port <= 2;
                let selected = connected && port == selector + 1;
                let mut expected_trig = [1; 4];
                if kind == "fire1" && connected {
                    expected_trig[usize::from(port - 1)] = 0;
                }
                assert_eq!(trig, expected_trig, "{kind}/{port}/{selector}");
                assert_eq!(
                    seen & 4 != 0,
                    kind == "keypad" && selected,
                    "key/{port}/{selector}"
                );
                assert_eq!(
                    seen & 8 != 0,
                    kind == "fire2" && selected,
                    "upper fire/{port}/{selector}"
                );
                assert_eq!(
                    seen_irq & 0x80 != 0,
                    kind == "fire2" && selected,
                    "break/{port}/{selector}"
                );
                if !connected {
                    assert_eq!(
                        pots,
                        capture(&idle).1,
                        "absent axis must not drive a POKEY pin"
                    );
                    assert!(
                        active.snapshot().expect("active") == idle.snapshot().expect("idle"),
                        "absent input must not enter the machine or host cache"
                    );
                }
            }
        }
    }
}
