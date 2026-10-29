use machine_spectravideo_svi_328_pal::Svi328;
#[test]
fn fresh_board_completes_its_native_raster() {
    let mut rom = vec![0; 0x8000];
    rom[..2].copy_from_slice(&[0x18, 0xFE]);
    let mut board = Svi328::new(rom);
    assert_eq!(board.vdp().framebuffer_height(), 288);
    for _ in 0..3 {
        assert_eq!(board.run_native_frame(), (228 * 313, true));
    }
    assert_eq!(board.frame_count(), 3);
}
