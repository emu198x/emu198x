use super::*;
use timing::MCLKS_PER_LINE;

/// Mode 5 with DMA enabled and auto-increment `inc`, in H40 or H32, with
/// the display on or off.
fn vdp(h40: bool, display: bool, inc: u8) -> Vdp {
    let mut vdp = Vdp::new(false);
    vdp.write_control(0x8100 | 0x14 | if display { 0x40 } else { 0 });
    vdp.write_control(0x8C00 | u16::from(h40));
    vdp.write_control(0x8F00 | u16::from(inc));
    vdp
}

/// Send a command: code `cd` (CD5-CD0) at `address`.
fn command(vdp: &mut Vdp, cd: u8, address: u32) {
    vdp.write_control((u16::from(cd & 3) << 14) | (address & 0x3FFF) as u16);
    vdp.write_control((u16::from(cd >> 2) << 4) | (address >> 14) as u16);
}

fn tick_until(vdp: &mut Vdp, mut done: impl FnMut(&Vdp) -> bool) {
    for _ in 0..MCLKS_PER_LINE * 700 {
        if done(vdp) {
            return;
        }
        vdp.tick();
    }
    panic!("condition not reached within two frames");
}

/// Run to the first slot of `line`, `hslot`, in the active picture.
fn run_to(vdp: &mut Vdp, line: u16, hslot: u8) {
    tick_until(vdp, |v| v.state() == State::Active);
    tick_until(vdp, |v| {
        v.vcounter() == line && v.hslot() == hslot && v.slot_left == 0
    });
}

/// Run a machine's DMA service loop for `mclks`: answer every source read
/// from `memory`, a 68000 byte address to word map.
fn run_serving(vdp: &mut Vdp, mclks: u32, memory: impl Fn(u32) -> u16) {
    for _ in 0..mclks {
        vdp.tick();
        if let Some(address) = vdp.dma_read() {
            vdp.dma_word(memory(address));
        }
    }
}

#[test]
fn a_control_word_with_10_on_top_writes_a_register() {
    let mut vdp = Vdp::new(false);
    vdp.write_control(0x87FF);
    assert_eq!(vdp.register(7), 0x3F, "the backdrop index is six bits");
    vdp.write_control(0x8104);
    vdp.write_control(0x9612);
    assert_eq!(vdp.register(0x16), 0x12);
    vdp.write_control(0x9812);
    assert_eq!(vdp.register(0x18), 0, "there is no register $18");
}

#[test]
fn outside_mode_5_only_the_first_eleven_registers_exist() {
    let mut vdp = Vdp::new(false);
    vdp.write_control(0x8A55);
    vdp.write_control(0x8F02);
    assert_eq!(vdp.register(0x0A), 0x55);
    assert_eq!(vdp.register(0x0F), 0);
}

#[test]
fn a_status_read_cancels_half_a_command() {
    let mut vdp = vdp(true, false, 2);
    vdp.write_control(0x4000);
    assert!(vdp.pending);
    vdp.read_control();
    assert!(!vdp.pending);
    vdp.write_control(0x8F04);
    assert_eq!(
        vdp.register(0x0F),
        4,
        "the next word is a register write again"
    );
}

#[test]
fn a_vram_word_lands_big_endian_and_an_odd_address_swaps_it() {
    let mut vdp = vdp(true, false, 2);
    command(&mut vdp, VRAM_WRITE, 0x1000);
    vdp.write_data(0x1234).expect("room in the FIFO");
    command(&mut vdp, VRAM_WRITE, 0x2001);
    vdp.write_data(0x5678).expect("room in the FIFO");
    for _ in 0..MCLKS_PER_LINE {
        vdp.tick();
    }
    assert_eq!(&vdp.vram()[0x1000..0x1002], &[0x12, 0x34]);
    assert_eq!(&vdp.vram()[0x2000..0x2002], &[0x78, 0x56]);
}

#[test]
fn the_fifo_holds_four_words_and_then_the_cpu_waits() {
    let mut vdp = vdp(true, false, 2);
    command(&mut vdp, CRAM_WRITE, 0);
    for value in 0..4 {
        assert!(vdp.write_data(value).is_some());
    }
    assert_eq!(vdp.status() & 0x100, 0x100, "FIFO full");
    assert!(vdp.write_data(4).is_none(), "the fifth write waits");
    tick_until(&mut vdp, |v| v.status() & 0x100 == 0);
    assert!(vdp.write_data(4).is_some());
    tick_until(&mut vdp, |v| v.status() & 0x200 != 0);
    assert_eq!(
        &vdp.cram()[..5],
        &[0, 0, 2, 2, 4],
        "CRAM keeps three bits per channel"
    );
}

#[test]
fn a_written_word_waits_three_slots_before_it_lands() {
    let mut vdp = vdp(true, false, 2);
    command(&mut vdp, VSRAM_WRITE, 0);
    vdp.write_data(0x0123).expect("room");
    for _ in 0..2 * timing::MCLKS_SLOT_H40 {
        vdp.tick();
    }
    assert_eq!(vdp.vsram()[0], 0, "not yet");
    for _ in 0..2 * timing::MCLKS_SLOT_H40 {
        vdp.tick();
    }
    assert_eq!(vdp.vsram()[0], 0x0123);
}

/// Keep the FIFO full of `cd` writes and count the words written in one
/// whole line.
fn words_per_line(vdp: &mut Vdp, cd: u8) -> u32 {
    command(vdp, cd, 0);
    let mut accepted = 0;
    // Prime the FIFO and let one line pass to reach a steady state.
    for _ in 0..MCLKS_PER_LINE {
        if vdp.write_data(0).is_some() {
            accepted += 1;
        }
        vdp.tick();
    }
    let primed = accepted;
    for _ in 0..MCLKS_PER_LINE {
        if vdp.write_data(0).is_some() {
            accepted += 1;
        }
        vdp.tick();
    }
    accepted - primed
}

#[test]
fn during_the_picture_the_fifo_drains_only_in_external_slots() {
    for (h40, vram, cram) in [(true, 9, 18), (false, 8, 16)] {
        let mut v = vdp(h40, true, 2);
        run_to(&mut v, 20, 0);
        assert_eq!(
            words_per_line(&mut v, VRAM_WRITE),
            vram,
            "VRAM words, H40 {h40}"
        );
        let mut v = vdp(h40, true, 2);
        run_to(&mut v, 20, 0);
        assert_eq!(
            words_per_line(&mut v, CRAM_WRITE),
            cram,
            "CRAM words, H40 {h40}"
        );
    }
}

#[test]
fn with_the_display_off_every_slot_but_refresh_is_external() {
    for (h40, cram) in [(true, 204), (false, 166)] {
        let mut v = vdp(h40, false, 2);
        assert_eq!(words_per_line(&mut v, CRAM_WRITE), cram, "H40 {h40}");
    }
}

#[test]
fn a_vram_fill_writes_the_high_byte_to_the_address_with_bit_0_flipped() {
    let mut vdp = vdp(true, false, 1);
    vdp.write_control(0x9303); // length 3
    vdp.write_control(0x9400);
    vdp.write_control(0x9780); // fill
    command(&mut vdp, VRAM_WRITE | CD_DMA, 0x200);
    assert_eq!(vdp.status() & 2, 2, "DMA in progress");
    vdp.write_data(0x5566).expect("room");
    tick_until(&mut vdp, |v| v.status() & 2 == 0);
    assert_eq!(&vdp.vram()[0x200..0x204], &[0x55, 0x66, 0x55, 0x55]);
    assert!(!vdp.dma_running());
}

#[test]
fn a_vram_copy_moves_bytes_at_a_read_and_a_write_slot_each() {
    // A register write rewrites the command's address and low code bits,
    // so the auto-increment is set before the command, not between it and
    // its data.
    let mut vdp = vdp(true, false, 2);
    command(&mut vdp, VRAM_WRITE, 0x1000);
    vdp.write_data(0x0102).expect("room");
    vdp.write_data(0x0304).expect("room");
    tick_until(&mut vdp, |v| v.status() & 0x200 != 0);
    vdp.write_control(0x8F01);
    vdp.write_control(0x9304);
    vdp.write_control(0x9400);
    vdp.write_control(0x9500); // source $1000
    vdp.write_control(0x9610);
    vdp.write_control(0x97C0); // copy
    command(&mut vdp, VRAM_READ | 0x10 | CD_DMA, 0x2000);
    assert!(vdp.dma_running(), "a copy starts at once");
    let start = vdp.mclk();
    tick_until(&mut vdp, |v| !v.dma_running());
    assert_eq!(&vdp.vram()[0x2000..0x2004], &[1, 2, 3, 4]);
    // Eight external slots for four bytes, give or take where the line was.
    assert!(vdp.mclk() - start <= 9 * u64::from(timing::MCLKS_SLOT_H40));
}

#[test]
fn a_68000_dma_takes_the_bus_and_reads_after_three_slots() {
    let mut vdp = vdp(true, false, 2);
    vdp.write_control(0x9303);
    vdp.write_control(0x9400);
    // Source: 68000 byte address $FF0000, word $7F8000.
    vdp.write_control(0x9500);
    vdp.write_control(0x9680);
    vdp.write_control(0x977F);
    command(&mut vdp, CRAM_WRITE | CD_DMA, 0);
    assert!(vdp.bus_request(), "the bus is requested at once");
    let mut reads = Vec::new();
    let start = vdp.mclk();
    let mut first_read = None;
    for _ in 0..MCLKS_PER_LINE {
        vdp.tick();
        if let Some(address) = vdp.dma_read() {
            first_read.get_or_insert(vdp.mclk() - start);
            reads.push(address);
            vdp.dma_word(0x0222 * (reads.len() as u16));
        }
    }
    assert_eq!(reads, vec![0xFF_0000, 0xFF_0002, 0xFF_0004]);
    assert!(first_read.expect("a read") >= 3 * u64::from(timing::MCLKS_SLOT_H40));
    assert!(!vdp.bus_request(), "released when the length runs out");
    assert_eq!(&vdp.cram()[..3], &[0x0222, 0x0444, 0x0666]);
}

#[test]
fn a_68000_dma_source_wraps_inside_its_128_kb_window() {
    let mut vdp = vdp(true, false, 2);
    vdp.write_control(0x9302);
    vdp.write_control(0x9400);
    // Word $00FFFF: byte $01FFFE. The next word is $000000, not $010000.
    vdp.write_control(0x95FF);
    vdp.write_control(0x96FF);
    vdp.write_control(0x9700);
    command(&mut vdp, VSRAM_WRITE | CD_DMA, 0);
    let mut reads = Vec::new();
    for _ in 0..MCLKS_PER_LINE {
        vdp.tick();
        if let Some(address) = vdp.dma_read() {
            reads.push(address);
            vdp.dma_word(1);
        }
    }
    assert_eq!(reads, vec![0x1FFFE, 0x0000]);
}

#[test]
fn a_68000_dma_moves_a_word_per_external_slot_during_the_picture() {
    let mut vdp = vdp(true, true, 2);
    run_to(&mut vdp, 30, 0);
    vdp.write_control(0x9300);
    vdp.write_control(0x9410); // 4096 words: longer than the test
    vdp.write_control(0x9500);
    vdp.write_control(0x9600);
    vdp.write_control(0x9700);
    command(&mut vdp, CRAM_WRITE | CD_DMA, 0);
    run_serving(&mut vdp, MCLKS_PER_LINE * 2, |_| 0);
    let before =
        u16::from(vdp.register(REG_DMA_LEN_H)) << 8 | u16::from(vdp.register(REG_DMA_LEN_L));
    run_serving(&mut vdp, MCLKS_PER_LINE, |_| 0);
    let after =
        u16::from(vdp.register(REG_DMA_LEN_H)) << 8 | u16::from(vdp.register(REG_DMA_LEN_L));
    assert_eq!(before - after, 18, "18 words a line in H40");
}

#[test]
fn the_h_and_v_counters_follow_the_hardware_sequence() {
    let mut vdp = vdp(true, true, 2);
    let mut h = Vec::new();
    run_to(&mut vdp, 10, 0);
    for _ in 0..MCLKS_PER_LINE {
        let value = vdp.read_hv();
        if h.last() != Some(&value) {
            h.push(value);
        }
        vdp.tick();
    }
    let low: Vec<u8> = h.iter().map(|&hv| hv as u8).collect();
    let b6 = low.iter().position(|&x| x == 0xB6).expect("H reaches $B6");
    assert_eq!(low[b6 + 1], 0xE5);
    // The V counter steps at slot 165 ($A5).
    let step = h.iter().position(|&hv| hv >> 8 == 11).expect("V steps");
    assert_eq!(h[step] & 0xFF, 165);

    // NTSC V28: 0-$EA then $1E5-$1FF.
    let mut lines = Vec::new();
    let mut last = vdp.vcounter();
    for _ in 0..MCLKS_PER_LINE * 263 {
        vdp.tick();
        if vdp.vcounter() != last {
            last = vdp.vcounter();
            lines.push(last);
        }
    }
    let ea = lines.iter().position(|&v| v == 0xEA).expect("reaches $EA");
    assert_eq!(lines[ea + 1], 0x1E5);
}

#[test]
fn v_int_fires_once_a_frame_on_the_first_blank_line() {
    let mut vdp = vdp(true, true, 2);
    vdp.write_control(0x8100 | 0x14 | 0x40 | 0x20);
    run_to(&mut vdp, 100, 0);
    // Clear the V-int left pending by the frame the run-up passed through.
    vdp.acknowledge_interrupt();
    let mut fired = Vec::new();
    for _ in 0..MCLKS_PER_LINE * 262 {
        vdp.tick();
        if vdp.interrupt_level() == 6 {
            fired.push((vdp.vcounter(), vdp.hslot()));
            vdp.acknowledge_interrupt();
        }
    }
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].0, 224);
    assert_eq!(vdp.status() & 0x80, 0, "acknowledged");
}

#[test]
fn h_int_fires_every_n_plus_one_lines_in_the_picture() {
    let mut vdp = vdp(true, true, 2);
    vdp.write_control(0x8014); // H-int enabled
    vdp.write_control(0x8A0F); // every 16 lines
    run_to(&mut vdp, 0, 0);
    let mut lines = Vec::new();
    for _ in 0..MCLKS_PER_LINE * 230 {
        vdp.tick();
        if vdp.interrupt_level() == 4 {
            lines.push(vdp.vcounter());
            vdp.acknowledge_interrupt();
        }
    }
    let gaps: Vec<u16> = lines.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(gaps.iter().all(|&g| g == 16), "{lines:?}");
    assert_eq!(lines.len(), 14, "224 lines / 16");
}

#[test]
fn the_status_register_reports_blanking_region_and_fifo() {
    let mut vdp = Vdp::new(true);
    vdp.write_control(0x8144);
    assert_eq!(vdp.status() & 0x0209, 0x0209, "FIFO empty, V-blank, PAL");
    run_to(&mut vdp, 50, 10);
    assert_eq!(vdp.status() & 0x0C, 0, "in the picture, not in H-blank");
    // H32 here: the counter jumps from $93 to $E9.
    tick_until(&mut vdp, |v| v.hslot() == 0xF0);
    assert_eq!(vdp.status() & 0x04, 0x04, "H-blank");
}

#[test]
fn a_vram_read_returns_the_word_after_its_latency() {
    let mut vdp = vdp(true, false, 2);
    command(&mut vdp, VRAM_WRITE, 0x400);
    vdp.write_data(0xBEEF).expect("room");
    vdp.write_data(0xCAFE).expect("room");
    tick_until(&mut vdp, |v| v.status() & 0x200 != 0);
    command(&mut vdp, VRAM_READ, 0x400);
    assert!(vdp.read_data().is_none(), "not fetched yet");
    tick_until(&mut vdp, |v| v.read_fetched);
    assert_eq!(vdp.read_data(), Some(0xBEEF));
    tick_until(&mut vdp, |v| v.read_fetched);
    assert_eq!(vdp.read_data(), Some(0xCAFE));
}

#[test]
fn a_cram_read_fills_the_missing_bits_from_the_fifo() {
    let mut vdp = vdp(true, false, 0);
    command(&mut vdp, CRAM_WRITE, 0);
    // Four writes, so every FIFO entry holds $FFFF: the bits come from the
    // entry due to be written next, which is the oldest.
    for _ in 0..4 {
        vdp.write_data(0xFFFF).expect("room");
    }
    tick_until(&mut vdp, |v| v.status() & 0x200 != 0);
    command(&mut vdp, CRAM_READ, 0);
    tick_until(&mut vdp, |v| v.read_fetched);
    assert_eq!(vdp.read_data(), Some(0xFFFF));
    assert_eq!(vdp.cram()[0], 0x0EEE);
}

#[test]
fn setting_the_latch_bit_freezes_the_hv_counter() {
    let mut vdp = vdp(true, true, 2);
    run_to(&mut vdp, 40, 0);
    vdp.write_control(0x8006);
    let frozen = vdp.read_hv();
    for _ in 0..1000 {
        vdp.tick();
    }
    assert_eq!(vdp.read_hv(), frozen);
    vdp.write_control(0x8004);
    assert_ne!(vdp.read_hv(), frozen);
}

#[test]
fn the_external_interrupt_is_level_2_when_enabled() {
    let mut vdp = vdp(true, true, 2);
    vdp.external_interrupt();
    assert_eq!(vdp.interrupt_level(), 0, "disabled");
    vdp.write_control(0x8B08);
    assert_eq!(vdp.interrupt_level(), 2);
    vdp.acknowledge_interrupt();
    assert_eq!(vdp.interrupt_level(), 0);
}

#[test]
fn a_snapshot_resumes_in_lockstep() {
    let mut vdp = vdp(true, true, 2);
    run_to(&mut vdp, 5, 0);
    command(&mut vdp, CRAM_WRITE, 0);
    vdp.write_data(0x0EEE).expect("room");
    let bytes = postcard::to_allocvec(&vdp).expect("encode");
    let mut restored: Vdp = postcard::from_bytes(&bytes).expect("decode");
    for _ in 0..MCLKS_PER_LINE * 3 {
        vdp.tick();
        restored.tick();
        assert_eq!(vdp.read_hv(), restored.read_hv());
        assert_eq!(vdp.status(), restored.status());
    }
    assert_eq!(vdp.cram(), restored.cram());
}

#[test]
fn interlace_toggles_the_field_and_folds_bit_8_into_the_v_counter() {
    let mut vdp = vdp(true, true, 2);
    vdp.write_control(0x8C03); // H40, interlace mode 1
    run_to(&mut vdp, 100, 0);
    let odd = vdp.status() & 0x10;
    tick_until(&mut vdp, |v| v.state() == State::Inactive);
    tick_until(&mut vdp, |v| {
        v.vcounter() == 100 && v.state() == State::Active
    });
    assert_ne!(vdp.status() & 0x10, odd, "the field flips once a frame");
    // Interlace mode 1 drops bit 0 of the line and shows bit 8 there.
    // NTSC never reaches $100-$1E4; line $1F0 has bit 8 set.
    tick_until(&mut vdp, |v| v.vcounter() == 0x1F0);
    assert_eq!(vdp.read_hv() >> 8, 0xF1);
    // Mode 2 doubles the line number.
    vdp.write_control(0x8C07);
    tick_until(&mut vdp, |v| v.vcounter() == 0x45);
    assert_eq!(vdp.read_hv() >> 8, 0x8A);
}

#[test]
fn a_128_kb_mode_word_write_uses_the_128_kb_wiring() {
    let mut vdp = vdp(true, false, 2);
    vdp.write_control(0x8100 | 0x14 | 0x80);
    command(&mut vdp, VRAM_WRITE, 0x0402);
    vdp.write_data(0x00AB).expect("room");
    tick_until(&mut vdp, |v| v.status() & 0x200 != 0);
    // ((0x402 & 0x3FC) | (0x402 >> 1 & 0xFC01) | (0x402 >> 9 & 2)) ^ 1
    // = (0x000 | 0x001 | 0x002) ^ 1 = 0x002.
    assert_eq!(vdp.vram()[0x0002], 0xAB);
}

#[test]
fn vsram_and_byte_reads_come_through_the_prefetch() {
    let mut vdp = vdp(true, false, 2);
    command(&mut vdp, VSRAM_WRITE, 2);
    vdp.write_data(0x0155).expect("room");
    command(&mut vdp, VRAM_WRITE, 0x10);
    vdp.write_data(0x1234).expect("room");
    tick_until(&mut vdp, |v| v.status() & 0x200 != 0);
    command(&mut vdp, VSRAM_READ, 2);
    tick_until(&mut vdp, |v| v.read_fetched);
    assert_eq!(vdp.read_data().map(|w| w & VSRAM_BITS), Some(0x0155));
    // Past the 40 entries, a VSRAM read returns entry 0.
    command(&mut vdp, VSRAM_READ, 0x60);
    tick_until(&mut vdp, |v| v.read_fetched);
    assert_eq!(vdp.read_data().map(|w| w & VSRAM_BITS), Some(0));
    command(&mut vdp, VRAM_READ_8, 0x10);
    tick_until(&mut vdp, |v| v.read_fetched);
    assert_eq!(vdp.read_data().map(|w| w & 0xFF), Some(0x34));
}

#[test]
fn a_write_with_a_read_code_takes_two_slots_and_writes_nothing() {
    let mut vdp = vdp(true, false, 2);
    command(&mut vdp, VRAM_READ, 0x10);
    vdp.write_data(0xFFFF).expect("room");
    tick_until(&mut vdp, |v| v.status() & 0x200 != 0);
    assert!(vdp.vram().iter().all(|&b| b == 0));
}

#[test]
fn cd5_only_changes_with_dma_enabled() {
    let mut vdp = Vdp::new(false);
    vdp.write_control(0x8104); // Mode 5, DMA disabled
    command(&mut vdp, VRAM_WRITE | CD_DMA, 0);
    assert_eq!(vdp.status() & 2, 0, "no DMA");
}

#[test]
fn a_cram_fill_writes_the_word_due_out_of_the_fifo_next() {
    let mut vdp = vdp(true, false, 2);
    vdp.write_control(0x9302);
    vdp.write_control(0x9400);
    vdp.write_control(0x9780);
    command(&mut vdp, CRAM_WRITE | CD_DMA, 0);
    vdp.write_data(0x0222).expect("room");
    tick_until(&mut vdp, |v| v.status() & 2 == 0);
    // The data word lands at 0; the fill's two entries take the value in
    // the FIFO's next write slot, still zero.
    assert_eq!(&vdp.cram()[..3], &[0x0222, 0, 0]);
}

#[test]
fn a_data_access_cancels_a_half_written_command() {
    let mut vdp = vdp(true, false, 2);
    vdp.write_control(0x4000);
    vdp.write_data(0).expect("room");
    assert!(!vdp.pending);
    vdp.write_control(0x4000);
    let _ = vdp.read_data();
    assert!(!vdp.pending);
    // A read with a write code returns the stale prefetch rather than wait.
    command(&mut vdp, VRAM_WRITE, 0);
    assert_eq!(vdp.read_data(), Some(vdp.prefetch));
}

#[test]
fn the_external_interrupt_latches_the_hv_counter_when_told_to() {
    let mut vdp = vdp(true, true, 2);
    run_to(&mut vdp, 60, 20);
    vdp.write_control(0x8006);
    for _ in 0..500 {
        vdp.tick();
    }
    let live = vdp.hv_counter_live();
    vdp.external_interrupt();
    assert_eq!(vdp.read_hv(), live);
}

#[test]
fn reading_status_clears_the_sprite_flags() {
    let mut vdp = vdp(true, false, 2);
    vdp.sprite_overflow = true;
    vdp.sprite_collision = true;
    assert_eq!(vdp.read_control() & 0x60, 0x60);
    assert_eq!(vdp.status() & 0x60, 0);
}
