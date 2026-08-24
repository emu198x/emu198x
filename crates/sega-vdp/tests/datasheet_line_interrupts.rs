//! The line-interrupt counter, R10.
//!
//! One 8-bit counter decrements once per scanline across the picture **and
//! for one line after it**, reloading from R10 outside that range. On
//! underflow it latches the line-IRQ pending flag and reloads. An R10 of `n`
//! therefore fires every `n + 1` lines.
//!
//! The "one line after" is the part that is easy to lose, and losing it costs
//! a real interrupt: the counter is checked on the first line of vblank
//! without being decremented, which is the interrupt a game uses to start its
//! vblank work. Genesis Plus GX's SMS frame loop has that check as a separate
//! block before the vblank section, ahead of the code that sets the frame
//! flag, and reloads only afterwards.

use sega_vdp::{SegaVdp, VdpRegion, VdpVariant};

fn write_register(vdp: &mut SegaVdp, reg: u8, value: u8) {
    vdp.write_control(value);
    vdp.write_control(0x80 | (reg & 0x0F));
}

/// A VDP with the display on and line interrupts enabled.
fn vdp(reg10: u8) -> SegaVdp {
    let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
    write_register(&mut vdp, 0, 0x14); // Mode 4 + line interrupts
    write_register(&mut vdp, 1, 0x40); // display on
    write_register(&mut vdp, 10, reg10);
    vdp
}

/// Scan one whole frame from a boundary, returning the scanline number of
/// every line interrupt raised. The status read that clears each one is what
/// a handler would do.
fn interrupt_lines(vdp: &mut SegaVdp) -> Vec<u16> {
    while !vdp.tick_scanline() {}
    // The settling frame raises interrupts too, and nothing has read status
    // to clear them. Start from a quiet chip or the first line of the frame
    // under test inherits one.
    let _ = vdp.read_status();
    let mut lines = Vec::new();
    let mut line = 0u16;
    loop {
        let wrapped = vdp.tick_scanline();
        if vdp.interrupt {
            lines.push(line);
            let _ = vdp.read_status();
        }
        line += 1;
        if wrapped {
            break;
        }
    }
    lines
}

/// "An R10 value of `n` fires the IRQ every `n + 1` lines."
#[test]
fn an_r10_of_n_fires_every_n_plus_one_lines() {
    for n in [0u8, 1, 7, 31] {
        let mut vdp = vdp(n);
        let lines = interrupt_lines(&mut vdp);
        // The counter enters line 0 holding R10, so the first underflow is
        // on line n and they repeat every n + 1 after that.
        let n = u16::from(n);
        let expected: Vec<u16> = (0..=192u16)
            .filter(|&line| line >= n && (line - n) % (n + 1) == 0)
            .collect();
        assert_eq!(
            lines,
            expected,
            "R10 = {n} should first fire on line {n}, then every {} lines",
            n + 1
        );
    }
}

/// The counter runs one line past the picture. With R10 set so that it
/// reaches zero exactly there, the only interrupt of the frame lands on the
/// first line of vblank — and an implementation that reloads on that line
/// instead of checking it raises nothing at all.
#[test]
fn the_counter_is_checked_on_the_first_line_of_vblank() {
    // 192 counts from the top of the frame lands the underflow exactly on
    // the line after the picture, and nowhere else in the frame.
    let mut on_vblank = vdp(192);
    assert_eq!(
        interrupt_lines(&mut on_vblank),
        vec![192],
        "R10 = 192 should fire once, on the line after the picture"
    );

    // One less, and it lands on the picture's last line instead — the two
    // are adjacent, so an implementation that skips the vblank line looks
    // right until you ask for exactly this value.
    let mut on_last_line = vdp(191);
    assert_eq!(interrupt_lines(&mut on_last_line), vec![191]);
}

/// One line past, and no further: nothing in the rest of vblank or the
/// border raises an interrupt, because the counter is being reloaded there.
#[test]
fn the_counter_does_not_run_through_the_rest_of_the_frame() {
    for n in [0u8, 3, 192] {
        let mut vdp = vdp(n);
        let lines = interrupt_lines(&mut vdp);
        assert!(
            lines.iter().all(|&line| line <= 192),
            "R10 = {n} raised an interrupt past the first vblank line: {lines:?}"
        );
    }
}

/// Writing R10 part-way down a frame does not shorten the segment already
/// counting: the counter reloads from R10 when it underflows, so a new value
/// takes effect from the next interrupt rather than immediately.
#[test]
fn a_new_r10_takes_effect_at_the_next_reload() {
    let mut vdp = vdp(31);
    while !vdp.tick_scanline() {}
    let _ = vdp.read_status();

    // Ten lines in, halve the period. The segment in progress keeps its
    // original length; the one after it is the new one.
    for _ in 0..10 {
        vdp.tick_scanline();
    }
    write_register(&mut vdp, 10, 15);

    let mut lines = Vec::new();
    for line in 10..192u16 {
        vdp.tick_scanline();
        if vdp.interrupt {
            lines.push(line);
            let _ = vdp.read_status();
        }
    }
    assert_eq!(
        lines.first(),
        Some(&31),
        "the segment already counting keeps the old period"
    );
    assert_eq!(
        lines.get(1),
        Some(&47),
        "and the next one uses the new value"
    );
}

/// The line interrupt only reaches the CPU when R0 bit 4 enables it. The
/// counter runs either way, so a game can turn the interrupt on mid-frame and
/// get it at the phase the counter is already at.
#[test]
fn the_counter_runs_whether_or_not_the_interrupt_is_enabled() {
    let mut enabled = vdp(15);
    let with = interrupt_lines(&mut enabled);

    let mut disabled = vdp(15);
    write_register(&mut disabled, 0, 0x04); // Mode 4, line interrupts off
    let without = interrupt_lines(&mut disabled);
    assert!(
        without.is_empty(),
        "with R0 bit 4 clear nothing should reach the CPU"
    );

    // Turn it on at the top of the next frame and the phase is unchanged.
    write_register(&mut disabled, 0, 0x14);
    let after = interrupt_lines(&mut disabled);
    assert_eq!(
        after, with,
        "the counter kept running while the interrupt was masked"
    );
}

/// R10 is reloaded on every line outside the picture, which makes the
/// effective sample point the *end* of vblank rather than its start. A game
/// that writes R10 during vblank — the natural place to set up the next
/// frame's split — gets the new value from line 0.
///
/// Genesis Plus GX samples it in the same place, with a single
/// `h_counter = reg[10]` after the vblank section and before the active
/// display, so a write anywhere in vblank is picked up.
#[test]
fn an_r10_written_during_vblank_takes_effect_from_the_next_frame() {
    let mut vdp = vdp(100);
    while !vdp.tick_scanline() {}
    let _ = vdp.read_status();

    // Scan the picture and step into vblank, then move the split.
    for _ in 0..200 {
        vdp.tick_scanline();
    }
    let _ = vdp.read_status();
    write_register(&mut vdp, 10, 10);

    while !vdp.tick_scanline() {}
    let _ = vdp.read_status();
    let mut first = None;
    for line in 0..192u16 {
        vdp.tick_scanline();
        if vdp.interrupt {
            first = Some(line);
            break;
        }
    }
    assert_eq!(
        first,
        Some(10),
        "the frame after a vblank write should use the new R10"
    );
}
