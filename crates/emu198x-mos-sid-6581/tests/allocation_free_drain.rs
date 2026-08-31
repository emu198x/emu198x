use emu198x_mos_sid_6581::Sid6581;

#[test]
fn repeated_mixed_audio_drains_allocate_nothing_after_warm_up() {
    const TICKS_PER_WINDOW: usize = 30_000;

    let mut sid = Sid6581::new(985_248, 48_000);
    let mut output = Vec::with_capacity(2_048);

    // The first cycle grows both the SID's initial frame-sized buffer and the
    // caller's destination to this deliberately larger window.
    for _ in 0..TICKS_PER_WINDOW {
        sid.tick();
    }
    sid.drain_buffer_into(&mut output);

    let allocation_info = allocation_counter::measure(|| {
        for _ in 0..8 {
            for _ in 0..TICKS_PER_WINDOW {
                sid.tick();
            }
            sid.drain_buffer_into(&mut output);
        }
    });

    assert_eq!(allocation_info.count_total, 0, "{allocation_info:?}");
}
