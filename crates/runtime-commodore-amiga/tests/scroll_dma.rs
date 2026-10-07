//! Render scroll delays through Agnus grants, chip RAM and Denise output.
//! HRM PF1H/PF2H mapping: reference/by-system/commodore-amiga/amiga-graphics-display.md.
//! Resolution/fetch masks: vendored WinUAE drawing.cpp update_bplcon1().
use commodore_agnus_ocs::Agnus;
use commodore_denise_aga::DeniseAga;
use commodore_denise_ocs::DeniseOcs;
use common_commodore_amiga::{
    denise::{BitplaneDmaFetch, Denise},
    denise_chip::DeniseChip,
    memory::Memory,
};
use machine_commodore_amiga_ecs::DeniseEcs;

fn render<C: DeniseChip>(
    hires: bool,
    width: u8,
    scroll: u16,
    plane: usize,
    dual: bool,
    aga: bool,
) -> Vec<u32> {
    render_with_restore::<C>(hires, width, scroll, plane, dual, aga, (false, None))
}

fn render_with_restore<C: DeniseChip>(
    hires: bool,
    width: u8,
    scroll: u16,
    plane: usize,
    dual: bool,
    aga: bool,
    (restore, scroll_change): (bool, Option<u16>),
) -> Vec<u32> {
    let mut a = Agnus::new();
    if aga {
        a.agnus_id = 0x2300;
    }
    a.dmacon = 0x0300;
    let planes = if dual { if hires { 4 } else { 6 } } else { 2 };
    a.bplcon0 = (planes << 12) | if hires { 0x8000 } else { 0 } | if dual { 0x0400 } else { 0 };
    a.fmode = match width {
        1 => 0,
        2 => 1,
        4 => 3,
        _ => panic!("invalid width"),
    };
    // The physical wide-fetch copy boundary needs an aligned input window
    // for these translation invariants. DDF=$30 puts the first holding group
    // halfway through the wide-copy period, so high offsets can select the
    // preceding phase rather than a delayed copy of the zero-offset stream.
    // Both origins are separately checked against the full reference raster.
    a.ddfstrt = if aga { 0x38 } else { 0x30 };
    a.ddfstop = 0xD0;
    a.diwstrt = 0x2C81;
    a.diwstop = 0x2CC1;
    a.vpos = 0x30;
    a.bpl_pt[0] = 0x1000;
    a.bpl_pt[1] = 0x2000;
    let mut memory = Memory::new(vec![0; 256 * 1024]);
    for active_plane in 0..2 {
        if plane != 2 && active_plane != plane {
            continue;
        }
        for word in 0..128u32 {
            memory.write_word(
                0x1000 + active_plane as u32 * 0x1000 + word * 2,
                (word as u16).wrapping_mul(0x9E37)
                    ^ if active_plane == 0 { 0xA55A } else { 0x5AA5 },
            );
        }
    }
    let mut d = Denise::<C>::new();
    // Both chips receive the window registers. The output ticks below
    // deliver Denise's copy before the horizontal start comparison.
    d.write_word(0x08E, a.diwstrt);
    d.write_word(0x090, a.diwstop);
    d.write_word(0x100, a.bplcon0);
    d.write_word(0x102, scroll);
    d.write_word(0x1FC, a.fmode);
    for _ in 0..3 {
        d.ocs.advance_register_output_pipeline();
    }
    for (color, rgb) in [(1, 0xF00), (2, 0x0F0), (9, 0x00F)] {
        d.write_word(0x180 + color * 2, rgb);
    }
    if !aga {
        a.write_diwstop(a.diwstop);
        a.write_diwstrt(0x3081);
        a.write_diwstrt(0x2C81);
        for _ in 0..8 {
            a.tick_cck();
        }
    }
    a.hpos = a.ddfstrt - 1;
    a.tick_cck();
    assert_eq!(a.ddf_start_match(), Some(a.ddfstrt));
    loop {
        if a.hpos == 0x70
            && let Some(value) = scroll_change
        {
            d.write_word(0x102, value);
        }
        let fetch = a
            .cck_bus_plan()
            .bitplane_dma_fetch_plane
            .map(|plane| BitplaneDmaFetch {
                plane,
                width_words: width,
            });
        let ccks = a.current_line_ccks();
        d.tick(0, fetch, true, &mut a, &memory, ccks);
        if restore && a.hpos == 0x70 {
            let bytes = postcard::to_allocvec(&d).expect("serialize live DMA/scroller");
            d = postcard::from_bytes(&bytes).expect("restore live DMA/scroller");
        }
        d.tick(1, None, true, &mut a, &memory, ccks);
        if a.hpos == 0xE2 {
            break;
        }
        a.tick_cck();
    }
    // Existing scroll-distance expectations use hires units. Lisa retains
    // the intervening 35 ns samples; native transport has its own DMA probes.
    let width = d.framebuffer_size().0 as usize;
    let row = (0x30 - 0x19) * 2 * width;
    let pixels: Vec<u32> = d.framebuffer[row..row + width]
        .iter()
        .step_by(C::OUTPUT_SAMPLES_PER_LORES as usize / 2)
        .copied()
        .collect();
    assert!(
        pixels[260..440].windows(2).any(|pair| pair[0] != pair[1]),
        "every scroll/replay fixture must render visible texture"
    );
    pixels
}

fn matrix<C: DeniseChip>(hires: bool, width: u8, dual: bool, aga: bool) {
    for plane in 0..2 {
        let baseline = render::<C>(hires, width, 0, plane, dual, aga);
        assert!(
            baseline[260..440].windows(2).any(|p| p[0] != p[1]),
            "fixture must contain visible texture"
        );
        let mask = (16 * width - 1) >> u8::from(hires);
        for delay in 0..=(16 * width - 1) {
            // Independent nibbles, with the other playfield deliberately different.
            let other = (delay + 3) & mask;
            let (odd, even) = if plane == 0 {
                (delay, other)
            } else {
                (other, delay)
            };
            let scroll = u16::from(odd & 15)
                | (u16::from(even & 15) << 4)
                | (u16::from(odd & 48) << 6)
                | (u16::from(even & 48) << 10);
            let pixels = render::<C>(hires, width, scroll, plane, dual, aga);
            let offset = usize::from(delay & mask) * 2;
            assert!(
                pixels[260 + offset..440 + offset] == baseline[260..440],
                "hires={hires} width={width} dual={dual} plane={plane} delay={delay} must preserve the identical fetched stream"
            );
        }
    }
}
#[test]
fn ocs_hires_scroll() {
    matrix::<DeniseOcs>(true, 1, false, false);
}
#[test]
fn ocs_dual_playfield_scroll() {
    matrix::<DeniseOcs>(false, 1, true, false);
    matrix::<DeniseOcs>(true, 1, true, false);
}
#[test]
fn aga_scroll_fetch_widths() {
    for width in [1, 2, 4] {
        for hires in [false, true] {
            matrix::<DeniseAga>(hires, width, true, true);
        }
    }
}

fn combined<C: DeniseChip>(hires: bool, width: u8, aga: bool) {
    let odd = render::<C>(hires, width, 0, 0, true, aga);
    let even = render::<C>(hires, width, 0, 1, true, aga);
    let mask = (16 * width - 1) >> u8::from(hires);
    for odd_delay in 0..=mask {
        let even_delay = (odd_delay.wrapping_mul(7) + 3) & mask;
        let scroll = u16::from(odd_delay & 15)
            | (u16::from(even_delay & 15) << 4)
            | (u16::from(odd_delay & 48) << 6)
            | (u16::from(even_delay & 48) << 10);
        let actual = render::<C>(hires, width, scroll, 2, true, aga);
        for x in 260..440 {
            let pf1 = odd[x - usize::from(odd_delay) * 2];
            let pf2 = even[x - usize::from(even_delay) * 2];
            let expected = if pf1 != 0xFF00_0000 { pf1 } else { pf2 };
            assert_eq!(
                actual[x], expected,
                "independent overlapping fields: width={width} hires={hires} odd={odd_delay} even={even_delay} x={x}"
            );
        }
    }
}
#[test]
fn simultaneous_dual_playfield_scroll() {
    for hires in [false, true] {
        combined::<DeniseOcs>(hires, 1, false);
        for width in [1, 2, 4] {
            combined::<DeniseAga>(hires, width, true);
        }
    }
}
#[test]
fn aga_hires_sub_lores_scroll() {
    for width in [1, 2, 4] {
        for plane in 0..2 {
            let baseline = render::<DeniseAga>(true, width, 0, plane, true, true);
            let fractional = if plane == 0 { 0x0200 } else { 0x2000 };
            for delay in 0..(8 * width) {
                let encoded = u16::from(delay & 15) | (u16::from(delay & 48) << 6);
                let scroll = if plane == 0 { encoded } else { encoded << 4 };
                let actual =
                    render::<DeniseAga>(true, width, scroll | fractional, plane, true, true);
                let offset = usize::from(delay) * 2 + 1;
                assert!(
                    actual[260 + offset..440 + offset] == baseline[260..440],
                    "AGA hires half-lores step: width={width} plane={plane} delay={delay}"
                );
            }
        }
    }
}

#[test]
fn aga_serial_scroll_history_survives_mid_line_restore() {
    for width in [1, 2, 4] {
        for hires in [false, true] {
            let scroll = if width == 4 { 0xCEAF } else { 0x22AF };
            let uninterrupted = render::<DeniseAga>(hires, width, scroll, 2, true, true);
            let restored =
                render_with_restore::<DeniseAga>(hires, width, scroll, 2, true, true, (true, None));
            assert_eq!(
                uninterrupted, restored,
                "AGA serial/FIFO restore width={width} hires={hires}"
            );
        }
    }
}

#[test]
fn ecs_hires_and_dual_playfield_scroll() {
    for hires in [false, true] {
        matrix::<DeniseEcs>(hires, 1, true, false);
        combined::<DeniseEcs>(hires, 1, false);
    }
}

#[test]
fn midline_scroll_changes_only_the_selected_dma_playfield() {
    for hires in [false, true] {
        for width in [1, 2, 4] {
            for plane in 0..2 {
                let baseline = render::<DeniseAga>(hires, width, 0x0035, plane, true, true);
                let peer_change = if plane == 0 { 0x00C5 } else { 0x003B };
                let own_change = if plane == 0 { 0x003B } else { 0x00C5 };
                let peer = render_with_restore::<DeniseAga>(
                    hires,
                    width,
                    0x0035,
                    plane,
                    true,
                    true,
                    (false, Some(peer_change)),
                );
                let own = render_with_restore::<DeniseAga>(
                    hires,
                    width,
                    0x0035,
                    plane,
                    true,
                    true,
                    (false, Some(own_change)),
                );
                assert_eq!(
                    peer, baseline,
                    "peer scroll changed active plane {plane}, width {width}, hires {hires}"
                );
                assert_eq!(
                    own[..260],
                    baseline[..260],
                    "mid-line write changed earlier pixels"
                );
                assert_ne!(
                    own[300..440],
                    baseline[300..440],
                    "own scroll change must reach visible DMA texture"
                );
            }
        }
    }
}
