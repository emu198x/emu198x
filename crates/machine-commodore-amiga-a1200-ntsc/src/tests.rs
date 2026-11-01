use super::AmigaA1200 as Board;
use common_commodore_amiga_aga::{
    A1200Core, AgnusRegion, Configuration as BoardConfiguration, CpuModel,
};

const ACTIVE: &[u8] =
    include_bytes!("../../../test-data/commodore-amiga/a1200-v66/active-progressive.rom");
const INACTIVE: &[u8] = include_bytes!("../../../test-data/commodore-amiga/a1200-v66/inactive.rom");
const EXPECTED_REGION: AgnusRegion = AgnusRegion::Ntsc;
const EXPECTED_ID: &str = "commodore-amiga-a1200-ntsc";
// Complete progressive fields measured in the immutable original v66 fixture.
const ORIGINAL_FIELD_TICKS: u64 = 119210;

struct Field {
    ticks: u64,
    pixels: Vec<u32>,
    audio: Vec<[u32; 2]>,
    pc: u32,
    counter: u16,
    joystick: u16,
}
fn field(board: &mut Board) -> Field {
    let before = board.tick_count();
    let initial = board
        .denise()
        .completed_display_field_count(board.agnus().vbl_count);
    let mut audio = Vec::new();
    while board
        .denise()
        .completed_display_field_count(board.agnus().vbl_count)
        == initial
        && board.tick_count() - before < 400_000
    {
        board.tick();
        let (left, right) = board.paula().mix_audio_stereo();
        assert!(left.is_finite() && right.is_finite());
        audio.push([left.to_bits(), right.to_bits()]);
    }
    assert_eq!(
        board
            .denise()
            .completed_display_field_count(board.agnus().vbl_count),
        initial + 1,
        "one native boundary, not a timeout"
    );
    Field {
        ticks: board.tick_count() - before,
        pixels: board.denise().framebuffer().to_vec(),
        audio,
        pc: board.cpu().regs.pc,
        counter: board.memory().read_chip_ram_word(0x1200),
        joystick: board.memory().read_chip_ram_word(0x1202),
    }
}
fn equal(left: &Field, right: &Field) {
    assert_eq!(left.ticks, right.ticks);
    assert!(left.pixels == right.pixels, "every framebuffer pixel");
    assert!(left.audio == right.audio, "every native mixed audio bit");
    assert_eq!(left.pc, right.pc);
    assert_eq!(left.counter, right.counter);
    assert_eq!(left.joystick, right.joystick);
}
fn controls(board: &mut Board, pressed: bool) {
    board.key_event(0x20, pressed);
    board.set_mouse_button_port0("left", pressed);
    assert!(board.set_joystick_control(1, "right", pressed));
    assert!(board.set_joystick_control(1, "fire", pressed));
}
#[test]
fn fixed_identity_region_cpu_clock_and_stock_ram() {
    let board = Board::new(ACTIVE.to_vec());
    assert_eq!(board.machine_id(), EXPECTED_ID);
    assert_eq!(board.region(), EXPECTED_REGION);
    assert_eq!(board.agnus().agnus_id, 0x3300);
    assert_eq!(board.active_cpu().model(), CpuModel::M68EC020);
    assert_eq!(
        (
            board.cpu_clock().numerator(),
            board.cpu_clock().denominator()
        ),
        (2, 1)
    );
    assert_eq!(board.memory().chip_ram_size(), 2 * 1024 * 1024);
}
#[test]
fn guest_reset_executes_through_ram_and_board_ports() {
    let mut board = Board::new(ACTIVE.to_vec());
    assert_eq!(board.cpu().regs.ssp, 0x001F_F000);
    for _ in 0..10_000 {
        board.tick();
    }
    assert!(!board.memory().overlay());
    assert_ne!(board.memory().read_chip_ram_word(0x1200), 0);
    assert_eq!(board.color(0), 0x0F00);
}
#[test]
fn native_fields_retain_the_original_regional_period() {
    let mut board = Board::new(ACTIVE.to_vec());
    field(&mut board);
    for _ in 0..3 {
        assert_eq!(field(&mut board).ticks, ORIGINAL_FIELD_TICKS);
    }
}
#[test]
fn chip_ram_last_word_and_rom_write_protection_are_wired() {
    let mut board = Board::new(ACTIVE.to_vec());
    field(&mut board);
    board.poke_word(0x001F_FFFE, 0xA55A);
    assert_eq!(board.read_word(0x001F_FFFE), 0xA55A);
    let rom = board.read_word(0x00F8_0000);
    board.poke_word(0x00F8_0000, 0xDEAD);
    assert_eq!(board.read_word(0x00F8_0000), rom);
}
#[test]
fn active_guest_picture_and_audio_have_an_inactive_control() {
    let mut active = Board::new(ACTIVE.to_vec());
    let mut inactive = Board::new(INACTIVE.to_vec());
    field(&mut active);
    field(&mut inactive);
    let a = field(&mut active);
    let b = field(&mut inactive);
    assert!(a.pixels.iter().any(|p| p & 0x00FF_FFFF != 0));
    assert!(b.pixels.iter().all(|p| p & 0x00FF_FFFF == 0));
    assert!(
        a.audio
            .iter()
            .flatten()
            .any(|v| f32::from_bits(*v).abs() > 0.00001)
    );
    assert!(
        b.audio
            .iter()
            .flatten()
            .all(|v| f32::from_bits(*v).abs() < 0.00001)
    );
}
#[test]
fn held_inputs_restore_and_release_with_exact_native_output() {
    let mut source = Board::new(ACTIVE.to_vec());
    field(&mut source);
    controls(&mut source, true);
    for _ in 0..137 {
        source.tick();
    }
    let saved = source.snapshot_state();
    let mut restored = Board::new(ACTIVE.to_vec());
    restored
        .restore_snapshot_state(saved)
        .expect("matching board");
    assert_eq!(source.tick_count(), restored.tick_count());
    assert_ne!(source.keyboard().queued_key_count(), 0);
    equal(&field(&mut source), &field(&mut restored));
    assert_ne!(source.memory().read_chip_ram_word(0x1202), 0);
    controls(&mut source, false);
    controls(&mut restored, false);
    equal(&field(&mut source), &field(&mut restored));
    assert_eq!(source.memory().read_chip_ram_word(0x1202), 0);
    assert_ne!(source.read_word(0x00BF_E001) & 0x40, 0);
}
#[derive(Clone, Debug)]
struct Foreign;
impl BoardConfiguration for Foreign {
    const REGION: AgnusRegion = AgnusRegion::Pal;
    const MACHINE_ID: &'static str = "test-foreign-a1200";
}
#[test]
fn foreign_region_restore_is_rejected_without_mutation() {
    let mut board = Board::new(ACTIVE.to_vec());
    field(&mut board);
    controls(&mut board, true);
    let mut expected = Board::new(ACTIVE.to_vec());
    expected
        .restore_snapshot_state(board.snapshot_state())
        .expect("control snapshot");
    let before = board.tick_count();
    let foreign = A1200Core::<Foreign>::new(ACTIVE.to_vec()).snapshot_state();
    assert!(board.restore_snapshot_state(foreign).is_err());
    assert_eq!(board.tick_count(), before);
    assert_eq!(board.region(), EXPECTED_REGION);
    equal(&field(&mut board), &field(&mut expected));
}
