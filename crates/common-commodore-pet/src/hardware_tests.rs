use super::*;
fn machine(board: Board) -> Pet {
    let mut k = vec![0xea; 4096];
    k[..3].copy_from_slice(&[0x4c, 0, 0xf0]);
    k[0xffc..0xffe].copy_from_slice(&[0, 0xf0]);
    let mut chars = vec![0; 2048];
    chars[8..16].fill(0x80);
    chars[0x408..0x410].fill(0x01);
    Pet::new(
        k,
        vec![
            0x42;
            if board == Board::Pet2001n32k {
                8192
            } else {
                12288
            }
        ],
        vec![0; 2048],
        chars,
        board,
    )
}
fn pixel(m: &Pet, x: u32, y: u32) -> u32 {
    m.framebuffer()[((y + BORDER_TOP) * m.framebuffer_width() + x + BORDER_LEFT) as usize]
}
#[test]
fn small_video_ram_mirrors_but_8032_has_two_kilobytes() {
    for board in [Board::Pet2001n32k, Board::Pet4032, Board::Pet8032] {
        let mut m = machine(board);
        m.poke(0x8000, 0x42);
        m.poke(0x8400, 0x31);
        assert_eq!(
            m.peek(0x8000),
            if board == Board::Pet8032 { 0x42 } else { 0x31 }
        );
        assert_eq!(m.peek(0x8400), 0x31);
    }
}
#[test]
fn basic_four_maps_the_extra_b000_socket() {
    for board in [Board::Pet2001n32k, Board::Pet4032, Board::Pet8032] {
        let m = machine(board);
        assert_eq!(
            m.peek(0xb000),
            if board == Board::Pet2001n32k {
                0xff
            } else {
                0x42
            }
        );
        assert_eq!(m.peek(0xc000), 0x42);
    }
}
#[test]
fn ca2_selects_the_character_bank_and_bit_seven_inverts() {
    let mut m = machine(Board::Pet2001n32k);
    m.poke(0x8000, 1);
    m.poke(0xe84c, 0x0c);
    m.run_frame();
    assert_eq!(pixel(&m, 0, 0), 0xff00_ff00);
    assert_eq!(pixel(&m, 7, 0), 0xff00_0000);
    m.poke(0xe84c, 0x0e);
    m.run_frame();
    assert_eq!(pixel(&m, 0, 0), 0xff00_0000);
    assert_eq!(pixel(&m, 7, 0), 0xff00_ff00);
    m.poke(0x8000, 0x81);
    m.run_frame();
    assert_eq!(pixel(&m, 0, 0), 0xff00_ff00);
    assert_eq!(pixel(&m, 7, 0), 0xff00_0000);
}
#[test]
fn paired_8032_fetches_and_blank_tenth_rows_reach_last_text_row() {
    let mut m = machine(Board::Pet8032);
    m.poke(0xe84c, 0x0c);
    m.poke(0x8000, 1);
    m.poke(0x8001, 0x81);
    m.poke(0x87cf, 0x81);
    m.run_frame();
    m.run_frame();
    assert_eq!(m.framebuffer_height(), 298);
    assert_eq!(pixel(&m, 0, 0), 0xff00_ff00);
    assert_eq!(pixel(&m, 8, 0), 0xff00_0000);
    assert_eq!(pixel(&m, 639, 247), 0xff00_ff00);
    for y in [8, 9, 248, 249] {
        assert!((0..640).all(|x| pixel(&m, x, y) == 0xff00_0000));
    }
}
#[test]
fn display_start_changes_fetch_not_beam_position() {
    let mut m = machine(Board::Pet4032);
    m.poke(0xe84c, 0x0c);
    m.poke(0x8001, 1);
    m.poke(0xe880, 13);
    m.poke(0xe881, 1);
    m.run_frame();
    m.run_frame();
    assert_eq!(pixel(&m, 0, 0), 0xff00_ff00);
    assert_eq!(pixel(&m, 8, 0), 0xff00_0000);
}
#[test]
fn discrete_video_on_has_the_reference_edges_and_no_crtc_registers() {
    let mut video = Video::new(Board::Pet2001n32k);
    let mut falls = Vec::new();
    let mut rises = Vec::new();
    let mut previous = video.retrace_level();
    let mut clocks = 0;
    loop {
        let b = video.tick();
        clocks += 1;
        let level = video.retrace_level();
        if level != previous {
            if level {
                rises.push((b.x, b.y));
            } else {
                falls.push((b.x, b.y));
            }
        }
        previous = level;
        if b.frame {
            break;
        }
    }
    assert_eq!(clocks, 16640);
    assert_eq!(falls, [(39, 199)]);
    assert_eq!(rises, [(39, 259)]);
    let mut m = machine(Board::Pet2001n32k);
    m.poke(0xe880, 0);
    m.poke(0xe881, 0);
    assert_eq!(m.mem_read(0xe880), 0xff);
    assert_eq!(m.mem_read(0xe881), 0xff);
    assert_eq!(m.run_frame(), 16640);
}
#[test]
fn via_pb5_obeys_the_real_input_and_direction_register() {
    let mut m = machine(Board::Pet4032);
    m.video.tick();
    m.via.pb_in = 0xdf;
    m.poke(0xe842, 0);
    assert_eq!(m.mem_read(0xe840) & 0x20, 0);
    m.poke(0xe840, 0x20);
    m.poke(0xe842, 0x20);
    assert_eq!(m.mem_read(0xe840) & 0x20, 0x20);
}
#[test]
fn business_symbol_release_does_not_release_an_independently_held_shift() {
    let mut m = machine(Board::Pet8032);
    m.press_key(PetKey::ShiftLeft);
    m.press_key(PetKey::Plus);
    m.release_key(PetKey::Plus);
    assert_eq!(m.keyboard.read_row(6) & 1, 0);
    assert_ne!(m.keyboard.read_row(2) & 0x40, 0);
    m.release_key(PetKey::ShiftLeft);
    assert_ne!(m.keyboard.read_row(6) & 1, 0);
}
#[test]
fn malformed_framebuffer_is_refused_before_owner_binding() {
    let mut m = machine(Board::Pet4032);
    m.framebuffer.pop();
    assert!(m.validate(Board::Pet4032).is_err());
}
