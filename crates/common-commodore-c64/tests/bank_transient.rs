//! CPU-visible consequences of the C64C PRA bank-switch transient.
use common_commodore_c64::{C64, C64Config, C64Model};
use common_commodore_iec::IecBus;
use std::collections::HashSet;

fn machine(model: C64Model) -> C64 {
    let mut kernal = [0xEA; 0x2000];
    kernal[0x1FFC..0x1FFE].copy_from_slice(&0x0800u16.to_le_bytes());
    C64::new(C64Config {
        model,
        kernal_rom: &kernal,
        basic_rom: &[0; 0x2000],
        character_rom: &[0; 0x1000],
    })
    .expect("synthetic firmware")
}

fn step(machine: &mut C64, bus: &mut IecBus, iec: bool) {
    if iec {
        machine.tick_with_iec_bus(bus);
    } else {
        machine.tick();
    }
}

#[test]
fn cpu_bank_swaps_expose_exactly_one_fetch_of_bank_three() {
    for (model, custom, cycles, lines) in [
        (C64Model::PalBreadbin, false, 63u64, 312u64),
        (C64Model::PalC64c, true, 63, 312),
        (C64Model::NtscBreadbin, false, 65, 263),
        (C64Model::NtscC64c, true, 65, 263),
    ] {
        for iec in [false, true] {
            let mut pictures = Vec::new();
            let mut runs = Vec::new();
            for program in [
                include_bytes!("../test-data/vic-bank-transient-00.prg"),
                include_bytes!("../test-data/vic-bank-transient-ff.prg"),
            ] {
                let mut machine = machine(model);
                machine.load_prg(program).expect("original guest");
                let mut bus = IecBus::new();
                let mut stores = Vec::new();
                let mut done = false;
                for _ in 0..200_000 {
                    let cpu = machine.cpu();
                    let pins = (cpu.addr, cpu.data, cpu.rw, cpu.total_cycles);
                    step(&mut machine, &mut bus, iec);
                    if !pins.2 && machine.cpu().total_cycles != pins.3 {
                        if pins.0 == 0xDD00 {
                            stores.push((machine.phi2_cycles(), pins.1));
                        } else if pins.0 == 0xD7FF {
                            assert_eq!(pins.1, 0xA5);
                            done = true;
                            break;
                        }
                    }
                }
                assert!(done, "CPU guest completion: {model:?}, IEC={iec}");
                assert_eq!(machine.peek(0x6000), 0xA5);
                assert!(machine.frame_count() > 2);
                assert_eq!(stores.len(), 8193);
                assert_eq!(stores[0].1, 3);
                for (index, (_, value)) in stores[1..].iter().enumerate() {
                    assert_eq!(*value, if index % 2 == 0 { 2 } else { 1 });
                }
                let stop = machine.phi2_cycles();
                let width = machine.vic().framebuffer_width() as usize;
                let base = stop
                    - u64::from(machine.raster_line()) * cycles
                    - u64::from(machine.cycle_in_line())
                    + 1;
                runs.push((stores, stop, width, base));
                pictures.push(machine.framebuffer().to_vec());
            }
            assert_eq!(
                runs[0], runs[1],
                "glyph bytes cannot change the CPU schedule"
            );
            assert_eq!(pictures[0].len(), pictures[1].len());
            let (stores, stop, width, base) = &runs[0];
            let writes: HashSet<_> = stores[2..].iter().map(|(clock, _)| *clock).collect();
            let actual: Vec<_> = pictures[0]
                .iter()
                .zip(&pictures[1])
                .map(|(a, b)| a != b)
                .collect();
            let mut expected = vec![false; actual.len()];
            if custom {
                // Native CPU-store/pixel traces qualify column n against a
                // swap at canonical cycle 15+n. The un-cropped framebuffer
                // starts the character window at x=48; rows retain raster y.
                // A fetched cell leaves the colour stage two ticks later,
                // making its framebuffer write three ticks after the PRA store.
                // Docs PR 89 retains bank_phase.py and shifted-window controls.
                for line in 51..251u64 {
                    for column in 0..40usize {
                        let mut clock = base + line * cycles + 15 + column as u64;
                        if clock + 3 > *stop {
                            clock -= cycles * lines;
                        }
                        if writes.contains(&clock) {
                            let start = line as usize * width + 48 + 8 * column;
                            expected[start..start + 8].fill(true);
                        }
                    }
                }
                assert!(expected.iter().any(|v| *v), "positive transient control");
            }
            let differences = actual.iter().zip(&expected).filter(|(a, b)| a != b).count();
            assert_eq!(differences, 0, "bank-3 pixel mask: {model:?}, IEC={iec}");
        }
    }
}

#[test]
fn pra_mirrors_preserve_the_selected_bank_after_the_transient_fetch() {
    for (model, custom) in [
        (C64Model::PalBreadbin, false),
        (C64Model::PalC64c, true),
        (C64Model::NtscBreadbin, false),
        (C64Model::NtscC64c, true),
    ] {
        for iec in [false, true] {
            let mut machine = machine(model);
            let mut bus = IecBus::new();
            machine.cpu_write(0xDD02, 3);
            for mirror in 0..16u16 {
                for old in 0..4 {
                    for new in 0..4 {
                        machine.cpu_write(0xDD00, 3 - old);
                        step(&mut machine, &mut bus, iec);
                        assert_eq!(machine.vic_bank(), old);
                        machine.cpu_write(0xDD00 + 16 * mirror, 3 - new);
                        let transient = custom && matches!((old, new), (1, 2) | (2, 1));
                        assert_eq!(machine.vic_bank(), if transient { 3 } else { new });
                        step(&mut machine, &mut bus, iec);
                        assert_eq!(machine.vic_bank(), new);
                        step(&mut machine, &mut bus, iec);
                        assert_eq!(machine.vic_bank(), new, "transient must not re-arm");
                    }
                }
            }
        }
    }
}
