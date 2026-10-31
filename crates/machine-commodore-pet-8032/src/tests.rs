use super::*;
use common_commodore_pet::{Board, Pet as Raw};
fn roms() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut k = vec![0xea; 4096];
    k[..8].copy_from_slice(&[0xa9, 0x42, 0x8d, 0, 2, 0x4c, 5, 0xf0]);
    k[0xffc..0xffe].copy_from_slice(&[0, 0xf0]);
    let basic = vec![
        0;
        if Configuration::BOARD == Board::Pet2001n32k {
            8192
        } else {
            12288
        }
    ];
    let mut c = vec![0; 2048];
    c[8..16].fill(0xff);
    c[0x408..0x410].fill(0xff);
    (k, basic, vec![0; 2048], c)
}
fn machine() -> Pet {
    let (k, b, e, c) = roms();
    Pet::new(k, b, e, c).expect("firmware")
}
#[test]
fn fixed_owner_has_its_own_identity() {
    assert_eq!(machine().board(), Configuration::BOARD);
    assert_eq!(Configuration::MACHINE_ID, "commodore-pet-8032");
}
#[test]
fn reset_vector_executes_guest_code() {
    let mut m = machine();
    for _ in 0..20 {
        m.step_instruction();
    }
    assert_eq!(m.peek(0x200), 0x42);
}
#[test]
fn malformed_firmware_is_refused() {
    let (k, b, e, c) = roms();
    assert!(Pet::new(k[..2048].to_vec(), b, e, c).is_err());
}
#[test]
fn native_boundaries_agree_with_reference_cycles() {
    let mut m = machine();
    m.run_frame();
    let before = m.master_clock();
    let clocks = m.run_frame();
    assert_eq!(clocks, 19992);
    assert_eq!(m.master_clock() - before, clocks);
    assert_eq!(m.frame_count(), 2);
}
#[test]
fn screen_write_changes_pixels_but_blank_control_does_not() {
    let mut m = machine();
    m.run_frame();
    m.run_frame();
    assert!(m.framebuffer().iter().all(|p| *p == 0xff00_0000));
    m.poke(0x8000, 1);
    m.run_frame();
    assert!(m.framebuffer().contains(&0xff00_ff00));
}
#[test]
fn via_output_reaches_queued_audio() {
    let mut m = machine();
    m.poke(0xe84c, 0xc0);
    m.run_frame();
    let low = m.take_audio_buffer();
    assert!(!low.is_empty() && low.iter().all(|v| *v < 0.0));
    m.poke(0xe84c, 0xe0);
    m.run_frame();
    assert!(m.take_audio_buffer().iter().all(|v| *v > 0.0));
}
#[test]
fn exact_state_rebinds_and_foreign_owner_is_refused() {
    let mut m = machine();
    m.press_key(PetKey::Plus);
    for _ in 0..127 {
        m.step_instruction();
    }
    let bytes = postcard::to_allocvec(&*m).expect("save");
    let raw: Raw = postcard::from_bytes(&bytes).expect("raw");
    let mut restored = Pet::from_state(raw).expect("bind");
    assert_eq!(m.take_audio_buffer(), restored.take_audio_buffer());
    m.run_frame();
    restored.run_frame();
    assert_eq!(
        postcard::to_allocvec(&*m).expect("live"),
        postcard::to_allocvec(&*restored).expect("restored")
    );
    let (k, _, e, c) = roms();
    let foreign = Raw::new(k, vec![0; 12288], e, c, Board::Pet4032);
    assert!(Pet::from_state(foreign).is_err());
}
