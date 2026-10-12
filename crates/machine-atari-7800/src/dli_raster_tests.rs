use super::*;

#[test]
fn requested_interrupts_reach_pixels() {
    let suppress = std::env::var_os("MARIA_PROBE_REDIRECT_COLOUR").is_some();
    let mut cases = 0;
    let mut failures = 0;
    for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
        for height in [1_u8, 2, 3, 16] {
            for flags in 0..4 {
                let mut rom = vec![0xea; 0x8000];
                // Initialise stack, background, DLL and DMA; then loop.
                let code = [
                    0x78, 0xd8, 0xa2, 0xff, 0x9a, 0xa9, 0x42, 0x85, 0x20, 0xa9, 0x18, 0x85, 0x2c,
                    0xa9, 0, 0x85, 0x30, 0xa9, 0x40, 0x85, 0x3c, 0x4c, 0x15, 0x80,
                ];
                rom[..code.len()].copy_from_slice(&code);
                // Count in 16 bits, toggle colour, write BACKGRND, RTI.
                let handler = [
                    0xe6,
                    0x40,
                    0xd0,
                    2,
                    0xe6,
                    0x41,
                    0xa5,
                    0x42,
                    0x49,
                    0xc8,
                    0x85,
                    0x42,
                    0x85,
                    if suppress { 0x21 } else { 0x20 },
                    0x40,
                ];
                rom[0x100..0x100 + handler.len()].copy_from_slice(&handler);
                rom[0x7ffa..].copy_from_slice(&[0, 0x81, 0, 0x80, 0, 0x80]);
                let mut sys = Atari7800::new(rom, region).expect("raster guest");
                sys.ram_zp[2] = 0x42;
                for (index, entry) in sys.ram_main[..0x400]
                    .as_chunks_mut::<3>()
                    .0
                    .iter_mut()
                    .enumerate()
                {
                    let selected = match flags {
                        0 => false,
                        1 => true,
                        2 => index.is_multiple_of(2),
                        3 => index == 1,
                        _ => unreachable!(),
                    };
                    entry.copy_from_slice(&[
                        (height - 1) | if selected { 0x80 } else { 0 },
                        0x1c,
                        0,
                    ]);
                }
                let palette = match region {
                    Atari7800Region::Ntsc => &atari_maria::NTSC_PALETTE,
                    Atari7800Region::Pal => &atari_maria::PAL_PALETTE,
                };
                assert_ne!(palette[0x42 >> 1], palette[0x8a >> 1]);
                let mut descriptors = 0;
                let mut requests = 0;
                let mut last_request = None;
                let mut min_age = u64::MAX;
                let mut writes = 0;
                let mut last_colour = 0;
                let mut samples = Vec::new();
                for _ in 0..region.frame_ticks() {
                    let address = sys.bus_address;
                    if sys.maria.dma_read_pending()
                        && (0x1800..0x1c00).contains(&address)
                        && (address - 0x1800).is_multiple_of(3)
                    {
                        let index = usize::from((address - 0x1800) / 3);
                        assert_eq!(index, descriptors, "descriptor progression");
                        descriptors += 1;
                        if sys.ram_main[index * 3] & 0x80 != 0 {
                            requests += 1;
                            last_request = Some(sys.master_clock + 1);
                        }
                    }
                    sys.tick_master_clock();
                    if sys.maria.phi2
                        && !sys.cpu_bus_released
                        && !sys.cpu.rw
                        && sys.bus_address == 0x20
                    {
                        writes += 1;
                        last_colour = sys.cpu.data;
                    }
                    // Pixel 200 is emitted at native column 293. All empty
                    // list shutdowns and these short handlers precede it;
                    // retain the actual bus colour independently as well.
                    let line = sys.maria.scan_line();
                    if sys.master_clock % 908 == 586 && (41..233).contains(&line) {
                        if let Some(request) = last_request {
                            min_age = min_age.min(sys.master_clock - request);
                        }
                        let maria_region = region.maria_region();
                        let x = maria_region.border_left() + 200;
                        let y = u32::from(line - 16) + maria_region.border_top();
                        let index = (y * maria_region.framebuffer_width() + x) as usize;
                        let expected = if requests % 2 == 0 { 0x42 } else { 0x8a };
                        samples.push((
                            index,
                            palette[expected >> 1],
                            palette[usize::from(last_colour >> 1)],
                        ));
                    }
                }
                assert_eq!(samples.len(), 192, "complete visible line inventory");
                assert!(descriptors > 200 / usize::from(height), "DMA must execute");
                assert_eq!(sys.frame_count, 1);
                let handlers = u16::from_le_bytes([sys.ram_zp[0], sys.ram_zp[1]]);
                let pixels = sys.maria.framebuffer();
                let wrong_pixels = samples
                    .iter()
                    .filter(|&&(i, wanted, _)| pixels[i] != wanted)
                    .count();
                let wrong_bus_pixels = samples
                    .iter()
                    .filter(|&&(i, _, written)| pixels[i] != written)
                    .count();
                let wrong_previous = samples
                    .windows(2)
                    .filter(|pair| pixels[pair[1].0] != pair[0].1)
                    .count();
                if flags == 0 {
                    min_age = 0;
                } else {
                    assert!(min_age >= 400, "interrupt-to-pixel margin");
                }
                let passed = usize::from(handlers) == requests
                    && writes == requests + 1
                    && wrong_pixels == 0
                    && wrong_bus_pixels == 0;
                if flags == 0 {
                    assert_eq!(
                        (requests, handlers, writes, wrong_pixels, wrong_bus_pixels),
                        (0, 0, 1, 0, 0)
                    );
                }
                failures += usize::from(!passed);
                cases += 1;
                println!(
                    "region={region:?} height={height} flags={flags} requests={requests} handlers={handlers} writes={writes} pixels={} wrong_pixels={wrong_pixels} wrong_bus_pixels={wrong_bus_pixels} wrong_previous={wrong_previous} min_age={min_age}",
                    samples.len()
                );
            }
        }
    }
    println!("cases={cases} failures={failures}");
    assert_eq!(cases, 32);
    assert_eq!(
        failures, 0,
        "requested DLIs must produce the guest's visible colour changes"
    );
}
