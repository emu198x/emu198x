//! Lisa's 35 ns serial stream through real Agnus grants and chip RAM.
//! Fetch cadence: reference/by-system/commodore-amiga/amiga-graphics-display.md.
//! Scroll masks: vendored WinUAE drawing.cpp update_bplcon1().
use commodore_agnus_ocs::Agnus;
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::{
    denise::{BitplaneDmaFetch, Denise},
    memory::Memory,
};

const GREEN: u32 = 0xFF00_FF00;
const BLACK: u32 = 0xFF00_0000;
const WORDS: [(u32, u16); 4] = [(16, 0x8000), (18, 0xA5A5), (19, 0x5AA5), (21, 0x0001)];

fn render(width: u8, scroll: u16, restore: bool) -> Vec<u32> {
    let mut agnus = Agnus::new();
    agnus.agnus_id = 0x2300;
    agnus.dmacon = 0x0300;
    agnus.bplcon0 = 0x1040;
    agnus.fmode = match width {
        1 => 0,
        2 => 1,
        4 => 3,
        _ => panic!("invalid fetch width"),
    };
    agnus.ddfstrt = 0x38;
    agnus.ddfstop = 0xD0;
    agnus.diwstrt = 0x2C81;
    agnus.diwstop = 0xF4C1;
    agnus.vpos = 0x30;
    agnus.bpl_pt[0] = 0x1000;
    let mut memory = Memory::new(vec![0; 256 * 1024]);
    for (word, value) in WORDS {
        memory.write_word(0x1000 + word * 2, value);
    }
    let mut denise = Denise::<DeniseAga>::new();
    denise.write_word(0x08E, agnus.diwstrt);
    denise.write_word(0x090, agnus.diwstop);
    denise.write_word(0x100, agnus.bplcon0);
    denise.write_word(0x102, scroll);
    denise.write_word(0x1FC, agnus.fmode);
    denise.write_word(0x182, 0x0F0);
    agnus.hpos = 0x37;
    agnus.tick_cck();
    loop {
        let fetch = agnus
            .cck_bus_plan()
            .bitplane_dma_fetch_plane
            .map(|plane| BitplaneDmaFetch {
                plane,
                width_words: width,
            });
        let ccks = agnus.current_line_ccks();
        denise.tick(0, fetch, true, &mut agnus, &memory, ccks);
        if restore && agnus.hpos == 0x60 {
            // Keep an in-flight DMA delivery, FIFO and serial history together.
            let bytes = postcard::to_allocvec(&denise).expect("serialize DMA pipeline");
            denise = postcard::from_bytes(&bytes).expect("restore DMA pipeline");
        }
        denise.tick(1, None, true, &mut agnus, &memory, ccks);
        if agnus.hpos == 0xE2 {
            break;
        }
        agnus.tick_cck();
    }
    assert_eq!(
        agnus.bpl_pt[0],
        0x1000 + 80 * 2,
        "exactly 80 words per line"
    );
    let stride = denise.framebuffer_size().0 as usize;
    let row = (0x30 - 0x19) * 2 * stride;
    let pixels = denise.framebuffer[row..row + stride].to_vec();
    assert!(
        pixels.contains(&GREEN),
        "DMA fixture must emit foreground samples"
    );
    pixels
}

#[test]
fn superhires_dma_preserves_every_bit_at_all_fetch_widths() {
    let expected: Vec<_> = (16..22)
        .flat_map(|word| {
            let value = WORDS
                .iter()
                .find(|&&(index, _)| index == word)
                .map_or(0, |&(_, value)| value);
            (0..16).map(move |bit| {
                if value & (0x8000 >> bit) != 0 {
                    GREEN
                } else {
                    BLACK
                }
            })
        })
        .collect();
    for width in [1, 2, 4] {
        let baseline = render(width, 0, false);
        // Counter-traced origins remove four samples of producer line padding.
        let start = match width {
            1 => 372,
            2 => 388,
            4 => 420,
            _ => unreachable!(),
        };
        assert!(!baseline[..start].contains(&GREEN));
        assert_eq!(
            &baseline[start..start + expected.len()],
            expected,
            "width={width}"
        );
        assert_eq!(baseline.iter().filter(|&&pixel| pixel == GREEN).count(), 18);
        assert_eq!(
            render(width, 0, true),
            baseline,
            "in-flight restore, width={width}"
        );
        println!("SHRES width={width}: marker sample={start}");
    }
}

#[test]
fn superhires_scroll_moves_the_same_stream_in_single_sample_steps() {
    for width in [1, 2, 4] {
        let baseline = render(width, 0, false);
        // PF1H is in lores units; PF1H0/PF1H1 supply the 35/70 ns bits.
        let mask = (16 * width - 1) >> 2;
        for coarse in 0..=mask {
            for fine in 0..4u16 {
                let scroll = u16::from(coarse) | (fine << 8);
                let delay = usize::from(coarse) * 4 + usize::from(fine);
                let actual = render(width, scroll, false);
                assert_eq!(
                    &actual[320 + delay..700 + delay],
                    &baseline[320..700],
                    "width={width} scroll={scroll:04x}"
                );
            }
        }
    }
}

fn render_midline_width_change(old: u16, new: u16, varying: bool) -> Vec<u32> {
    render_midline_width_change_at(old, new, varying, 130)
}

fn render_midline_width_change_at(old: u16, new: u16, varying: bool, write_cck: u16) -> Vec<u32> {
    let mut agnus = Agnus::new();
    agnus.agnus_id = 0x2300;
    agnus.max_bitplanes = 8;
    agnus.dmacon = 0x0300;
    agnus.bplcon0 = 0x1040;
    agnus.fmode = old;
    agnus.ddfstrt = 0x38;
    agnus.ddfstop = 0xD0;
    agnus.diwstrt = 0x2C81;
    agnus.diwstop = 0xF4C1;
    agnus.vpos = 0x30;
    agnus.bpl_pt[0] = 0x1000;
    let mut memory = Memory::new(vec![0; 256 * 1024]);
    for word in 0..100 {
        memory.write_word(
            0x1000 + word * 2,
            if varying {
                ((word * 0x1F3D) ^ 0xA5A5) as u16
            } else {
                0xA5A5
            },
        );
    }
    let mut denise = Denise::<DeniseAga>::new();
    denise.write_word(0x08E, agnus.diwstrt);
    denise.write_word(0x090, agnus.diwstop);
    denise.write_word(0x100, 0x1040);
    denise.write_word(0x1FC, old);
    denise.write_word(0x182, 0x0F0);
    while agnus.hpos < 0xE2 {
        agnus.tick_cck();
        let plane = agnus.cck_bus_plan().bitplane_dma_fetch_plane;
        if agnus.hpos == write_cck {
            agnus.write_fmode(new);
            denise.write_word(0x1FC, new);
        }
        let fetch = plane.map(|plane| BitplaneDmaFetch {
            plane,
            width_words: agnus.bpl_fetch_width(),
        });
        let ccks = agnus.current_line_ccks();
        denise.tick(0, fetch, true, &mut agnus, &memory, ccks);
        denise.tick(1, None, true, &mut agnus, &memory, ccks);
    }
    let stride = denise.framebuffer_size().0 as usize;
    let row = (0x30 - 0x19) * 2 * stride;
    let pixels = denise.framebuffer[row..row + stride].to_vec();
    assert!(
        pixels.contains(&GREEN),
        "DMA fixture must emit foreground samples"
    );
    pixels
}

#[test]
fn widening_to_32_bits_reveals_the_retained_upper_half_of_the_previous_word() {
    let pixels = render_midline_width_change_at(0, 1, true, 132);
    // Phase-sweep field, DDF $38, line145/WAIT $82: the reference repeats
    // word37 while the new 32-bit group waits for its parallel-copy boundary.
    // Its 16-bit tap had shifted those bits into the same register's upper half.
    let value = ((37u32 * 0x1F3D) ^ 0xA5A5) as u16;
    let expected: Vec<_> = (0..16)
        .map(|bit| {
            if value & (0x8000 >> bit) != 0 {
                GREEN
            } else {
                BLACK
            }
        })
        .collect();
    assert_eq!(&pixels[724..740], expected);
}

#[test]
fn midline_32_to_64_bit_fetch_waits_for_the_new_parallel_copy_boundary() {
    let pixels = render_midline_width_change(1, 3, false);
    // Recorded producer coordinates: counter-domain 692..724 + native origin16.
    assert_eq!(&pixels[708..740], &[BLACK; 32]);
    assert!(pixels[676..708].contains(&GREEN));
    assert!(pixels[740..772].contains(&GREEN));
}

#[test]
fn delayed_parallel_copy_keeps_all_words_from_the_latest_wide_transfer() {
    let pixels = render_midline_width_change(1, 3, true);
    let expected: Vec<_> = (40..44u32)
        .flat_map(|word| {
            let value = ((word * 0x1F3D) ^ 0xA5A5) as u16;
            (0..16).map(move |bit| {
                if value & (0x8000 >> bit) != 0 {
                    GREEN
                } else {
                    BLACK
                }
            })
        })
        .collect();
    assert_eq!(&pixels[740..804], expected);
}

#[test]
fn midline_16_to_32_bit_fetch_retires_the_last_narrow_word_before_switching_copy_width() {
    let changed = render_midline_width_change(0, 1, false);
    let unchanged = render_midline_width_change(0, 0, false);
    // Verify the register boundary, not an invented identical line-end:
    // switching widths can change how many words the final group transfers.
    for sample in 672..744 {
        assert_eq!(changed[sample], unchanged[sample], "native sample {sample}");
    }
}
