use super::*;

fn flagged(mode: u8, index: usize) -> bool {
    match mode {
        0 => false,
        1 => true,
        2 => index.is_multiple_of(2),
        3 => index == 1,
        _ => panic!("unknown flag mode"),
    }
}

#[test]
fn every_native_request_reaches_an_executed_handler() {
    let suppress_handler = std::env::var_os("MARIA_PROBE_SUPPRESS_HANDLER").is_some();
    let mut cases = 0;
    let mut mismatches = 0;
    let mut handler_only_mismatches = 0;
    for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
        for height in [1_u8, 2, 3, 16] {
            for mode in 0..4 {
                for guest in 0..3 {
                    for enabled in [false, true] {
                        let mut rom = vec![0xea; 0x8000];
                        // SEI, CLD, stack=$FF, DPP=$1800, CTRL=DMA mode.
                        let mut code = vec![
                            0x78,
                            0xd8,
                            0xa2,
                            0xff,
                            0x9a,
                            0xa9,
                            0x18,
                            0x85,
                            0x2c,
                            0xa9,
                            0,
                            0x85,
                            0x30,
                            0xa9,
                            if enabled { 0x40 } else { 0 },
                            0x85,
                            0x3c,
                        ];
                        match guest {
                            0 => (),
                            1 => code.extend_from_slice(&[0xad, 0x80, 0x02]),
                            2 => code.extend_from_slice(&[0x85, 0x24]),
                            _ => unreachable!(),
                        }
                        code.extend_from_slice(&[0x4c, 0x11, 0x80]);
                        rom[..code.len()].copy_from_slice(&code);
                        // Increment a 16-bit count, then return from NMI.
                        rom[0x100..0x107].copy_from_slice(&[0xe6, 0x40, 0xd0, 2, 0xe6, 0x41, 0x40]);
                        if suppress_handler {
                            rom[0x100] = 0x40;
                        }
                        rom[0x7ffa..].copy_from_slice(&[0, 0x81, 0, 0x80, 0, 0x80]);
                        let mut sys = Atari7800State::new(rom, region).expect("probe guest");
                        // Exercise the handler's carry path in every row
                        // with two or more delivered interrupts.
                        sys.ram_zp[0] = 0xfe;
                        for (index, entry) in sys.ram_main[..0x400]
                            .as_chunks_mut::<3>()
                            .0
                            .iter_mut()
                            .enumerate()
                        {
                            entry.copy_from_slice(&[
                                (if flagged(mode, index) { 0x80 } else { 0 }) | (height - 1),
                                0x1c,
                                0,
                            ]);
                        }
                        let mut descriptors = 0;
                        let mut requests = 0;
                        let mut edges = 0;
                        let mut vectors = [0; 2];
                        let mut first_request = None;
                        let mut first_edge = None;
                        let mut halted = 0;
                        let mut waiting = 0;
                        let mut slow_reads = 0;
                        for _ in 0..region.frame_ticks() {
                            // Observe the actual flag-byte DMA strobe before
                            // the tick consumes it, not an independent renderer.
                            let address = sys.bus_address;
                            if sys.maria.dma_read_pending()
                                && (0x1800..0x1c00).contains(&address)
                                && (address - 0x1800).is_multiple_of(3)
                            {
                                let index = usize::from((address - 0x1800) / 3);
                                assert_eq!(index, descriptors, "descriptor skipped/repeated");
                                let selected = sys.ram_main[index * 3] & 0x80 != 0;
                                assert_eq!(selected, flagged(mode, index));
                                descriptors += 1;
                                if selected {
                                    requests += 1;
                                    first_request.get_or_insert((
                                        sys.master_clock + 1,
                                        sys.maria.scan_line(),
                                    ));
                                }
                            }
                            let old_nmi = sys.cpu.nmi;
                            sys.tick_master_clock();
                            if sys.cpu.nmi && !old_nmi {
                                edges += 1;
                                first_edge.get_or_insert((sys.master_clock, sys.maria.scan_line()));
                            }
                            if sys.maria.phi2 && !sys.cpu_bus_released && sys.cpu.rw {
                                match sys.bus_address {
                                    0xfffa => vectors[0] += 1,
                                    0xfffb => vectors[1] += 1,
                                    0x0280 => slow_reads += 1,
                                    _ => (),
                                }
                            }
                            halted += usize::from(sys.cpu_bus_released);
                            waiting += usize::from(sys.maria.wsync_halt());
                        }
                        let handlers = usize::from(
                            u16::from_le_bytes([sys.ram_zp[0], sys.ram_zp[1]]).wrapping_sub(0x00fe),
                        );
                        assert_eq!(sys.frame_count, 1);
                        assert!(sys.cpu.total_cycles > 100, "guest did not execute");
                        if enabled {
                            assert!(descriptors > 200 / usize::from(height));
                            assert!(halted > 100);
                        } else {
                            assert_eq!(descriptors, 0);
                        }
                        assert_eq!(waiting > 0, guest == 2);
                        assert_eq!(slow_reads > 0, guest == 1);
                        if !enabled || mode == 0 {
                            assert_eq!((requests, edges, vectors, handlers), (0, 0, [0, 0], 0));
                        }
                        let delivered = edges == requests && vectors == [requests; 2];
                        let passed = delivered && handlers == requests;
                        handler_only_mismatches += usize::from(delivered && handlers != requests);
                        mismatches += usize::from(!passed);
                        cases += 1;
                        println!(
                            "region={region:?} height={height} flags={mode} guest={guest} enabled={enabled} descriptors={descriptors} requests={requests} edges={edges} vector_low={} vector_high={} handlers={handlers} first_request={first_request:?} first_edge={first_edge:?}",
                            vectors[0], vectors[1]
                        );
                    }
                }
            }
        }
    }
    println!(
        "cases={cases} mismatches={mismatches} handler_only_mismatches={handler_only_mismatches}"
    );
    assert_eq!(cases, 192, "complete case inventory");
    assert_eq!(
        mismatches, 0,
        "every native DLI request must reach its handler"
    );
}

fn interrupt_guest(region: Atari7800Region, guest: u8) -> Atari7800State {
    let mut rom = vec![0xea; 0x8000];
    let mut code = vec![
        0x78, 0xd8, 0xa2, 0xff, 0x9a, 0xa9, 0x18, 0x85, 0x2c, 0xa9, 0, 0x85, 0x30, 0xa9, 0x40,
        0x85, 0x3c,
    ];
    match guest {
        0 => (),
        1 => code.extend_from_slice(&[0xad, 0x80, 0x02]),
        2 => code.extend_from_slice(&[0x85, 0x24]),
        _ => panic!("guest mode"),
    }
    code.extend_from_slice(&[0x4c, 0x11, 0x80]);
    rom[..code.len()].copy_from_slice(&code);
    rom[0x100..0x107].copy_from_slice(&[0xe6, 0x40, 0xd0, 2, 0xe6, 0x41, 0x40]);
    rom[0x7ffa..].copy_from_slice(&[0, 0x81, 0, 0x80, 0, 0x80]);
    let mut sys = Atari7800State::new(rom, region).expect("interrupt guest");
    for entry in sys.ram_main[..0x400].as_chunks_mut::<3>().0 {
        entry.copy_from_slice(&[0x80, 0x1c, 0]);
    }
    sys
}

#[test]
fn snapshots_resume_pending_and_asserted_interrupts() {
    let mut cases = 0;
    let mut asserted = 0;
    let mut waiting = 0;
    for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
        for guest in 0..3 {
            let mut sys = interrupt_guest(region, guest);
            for _ in 0..14500 {
                sys.tick_master_clock();
            }
            for _ in 0..160 {
                sys.tick_master_clock();
                asserted += usize::from(sys.maria.nmi);
                waiting += usize::from(sys.maria.wsync_halt() && sys.maria.nmi);
                sys.take_audio_samples();
                let saved = postcard::to_allocvec(&sys).expect("interrupt save");
                let mut restored: Atari7800State =
                    postcard::from_bytes(&saved).expect("interrupt restore");
                for _ in 0..2048 {
                    sys.tick_master_clock();
                    restored.tick_master_clock();
                    assert_eq!(
                        (
                            sys.cpu.nmi,
                            sys.maria.nmi,
                            sys.cpu.addr,
                            sys.cpu.rw,
                            sys.bus_address,
                            sys.cpu_bus_released,
                            sys.cpu.total_cycles
                        ),
                        (
                            restored.cpu.nmi,
                            restored.maria.nmi,
                            restored.cpu.addr,
                            restored.cpu.rw,
                            restored.bus_address,
                            restored.cpu_bus_released,
                            restored.cpu.total_cycles
                        )
                    );
                }
                assert!(sys.ram_zp[0] >= 2, "resumed guest must execute handlers");
                assert_eq!(sys.ram_zp, restored.ram_zp);
                assert_eq!(sys.framebuffer(), restored.framebuffer());
                assert_eq!(sys.take_audio_samples(), restored.take_audio_samples());
                assert_eq!(
                    postcard::to_allocvec(&sys).expect("future state"),
                    postcard::to_allocvec(&restored).expect("restored future state")
                );
                sys = postcard::from_bytes(&saved).expect("rewind fixture");
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 960);
    assert!(asserted >= 24, "saves must cover asserted pulses");
    assert!(waiting >= 8, "saves must cover NMI during WSYNC");
}

#[test]
fn wsync_keeps_native_nmi_edge_detection_running() {
    for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
        let mut sys = interrupt_guest(region, 2);
        let mut held_edges = 0;
        let mut detected = 0;
        let mut releases = 0;
        for _ in 0..region.frame_ticks() {
            let previous = sys.cpu.nmi_prev();
            let cycles = sys.cpu.total_cycles;
            sys.tick_master_clock();
            if sys.cpu.nmi_prev() && !previous && sys.maria.wsync_halt() {
                assert_eq!(sys.cpu.total_cycles, cycles);
                held_edges += 1;
            }
            detected += usize::from(!previous && sys.cpu.nmi_prev());
            releases += usize::from(previous && !sys.cpu.nmi_prev());
        }
        assert!(
            held_edges > 0 && held_edges < detected,
            "must observe NMI both during WSYNC and running execution"
        );
        assert_eq!(detected, releases);
        assert_eq!(
            u16::from_le_bytes([sys.ram_zp[0], sys.ram_zp[1]]) as usize,
            detected
        );
    }
}

#[test]
fn native_pulses_span_one_cpu_clock_through_slow_accesses() {
    let mut pulses = 0;
    let mut widths = std::collections::BTreeSet::new();
    for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
        for guest in 0..3 {
            let mut sys = interrupt_guest(region, guest);
            let mut start = None;
            let mut phase_edges = 0;
            for _ in 0..region.frame_ticks() {
                let old = sys.maria.nmi;
                sys.tick_master_clock();
                assert_eq!(
                    sys.cpu.nmi, sys.maria.nmi,
                    "pin wiring on every native tick"
                );
                if sys.maria.nmi && !old {
                    assert!(start.replace(sys.master_clock).is_none());
                    phase_edges = 0;
                }
                if sys.maria.nmi && sys.maria.phi1 {
                    phase_edges += 1;
                }
                if !sys.maria.nmi && old {
                    let width = sys.master_clock - start.take().expect("pulse assertion");
                    assert_eq!(phase_edges, 1, "pulse duration follows the live CPU clock");
                    assert!([8, 10, 12].contains(&width), "unexpected width {width}");
                    widths.insert(width);
                    pulses += 1;
                }
            }
            assert!(start.is_none(), "no truncated pulse at frame end");
        }
    }
    assert!(pulses > 1500);
    assert!(widths.contains(&8));
    assert!(
        widths.iter().any(|&width| width > 8),
        "must exercise stretched clocks"
    );
}
