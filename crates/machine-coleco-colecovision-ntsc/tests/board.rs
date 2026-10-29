use machine_coleco_colecovision_ntsc::ColecoVision;
#[test]
fn fresh_board_completes_its_native_raster() {
    let mut rom = vec![0; 8192];
    rom[..2].copy_from_slice(&[0x18, 0xFE]);
    let mut board = ColecoVision::new(rom, vec![]);
    assert_eq!(board.framebuffer_height(), 240);
    for _ in 0..3 {
        assert_eq!(board.run_native_frame(), (228 * 262, true));
    }
    assert_eq!(board.frame_count(), 3);
}
