//! Whole-machine tests on a hand-assembled BIOS, P-ROM and M-ROM. No real
//! firmware or game is used.

#![allow(clippy::unwrap_used)]

use super::*;

const RAM: u32 = 0x10_0000;
/// Master clocks per frame: 384 pixels × 264 lines × 4.
const FRAME: u64 = 384 * 264 * 4;

/// A 68000 program builder.
#[derive(Default)]
struct Asm(Vec<u16>);

impl Asm {
    fn long(&mut self, value: u32) -> &mut Self {
        self.0.push((value >> 16) as u16);
        self.0.push(value as u16);
        self
    }
    /// MOVE.B #imm,(abs).L
    fn move_b(&mut self, imm: u8, to: u32) -> &mut Self {
        self.0.extend([0x13FC, u16::from(imm)]);
        self.long(to)
    }
    /// MOVE.W #imm,(abs).L
    fn move_w(&mut self, imm: u16, to: u32) -> &mut Self {
        self.0.extend([0x33FC, imm]);
        self.long(to)
    }
    /// MOVE.B (abs).L,(abs).L
    fn copy_b(&mut self, from: u32, to: u32) -> &mut Self {
        self.0.push(0x13F9);
        self.long(from).long(to)
    }
    /// MOVE.W (abs).L,(abs).L
    fn copy_w(&mut self, from: u32, to: u32) -> &mut Self {
        self.0.push(0x33F9);
        self.long(from).long(to)
    }
    /// Loop until the byte at `at` equals `value`: CMP.B then BNE.S back.
    fn wait_b(&mut self, at: u32, value: u8) -> &mut Self {
        self.0.extend([0x0C39, u16::from(value)]);
        self.long(at);
        self.0.push(0x66F6);
        self
    }
    /// ADDQ.W #1,(abs).L
    fn inc_w(&mut self, at: u32) -> &mut Self {
        self.0.push(0x5279);
        self.long(at)
    }
    /// MOVE.W #imm,SR
    fn set_sr(&mut self, imm: u16) -> &mut Self {
        self.0.extend([0x46FC, imm]);
        self
    }
    /// JMP (abs).L
    fn jmp(&mut self, to: u32) -> &mut Self {
        self.0.push(0x4EF9);
        self.long(to)
    }
    fn rte(&mut self) -> &mut Self {
        self.0.push(0x4E73);
        self
    }
    /// BRA.S *
    fn halt(&mut self) -> &mut Self {
        self.0.push(0x60FE);
        self
    }
}

fn put_words(rom: &mut [u8], at: usize, words: &[u16]) {
    for (i, w) in words.iter().enumerate() {
        rom[at + i * 2..at + i * 2 + 2].copy_from_slice(&w.to_be_bytes());
    }
}

fn put_long(rom: &mut [u8], at: usize, value: u32) {
    rom[at..at + 4].copy_from_slice(&value.to_be_bytes());
}

/// A stand-in BIOS: it acknowledges the reset interrupt, swaps in the
/// cartridge's vectors and jumps to the cartridge at `$000400`.
fn bios() -> Vec<u8> {
    let mut rom = vec![0xFF; BIOS_SIZE];
    put_long(&mut rom, 0, 0x0010_F300);
    put_long(&mut rom, 4, 0x00C0_0400);
    let mut code = Asm::default();
    code.move_w(7, 0x3C_000C).move_b(0, 0x3A_0013).jmp(0x400);
    put_words(&mut rom, 0x400, &code.0);
    rom
}

/// A cartridge: `program` at `$400`, an optional level-1 (vertical blank)
/// handler at `$500`, and `size` bytes of P-ROM in all.
fn cart(program: &Asm, vblank: Option<&Asm>, size: usize) -> Cartridge {
    let mut p = vec![0; size];
    put_long(&mut p, 0, 0x0010_F300);
    put_long(&mut p, 4, 0x400);
    put_words(&mut p, 0x400, &program.0);
    if let Some(handler) = vblank {
        put_long(&mut p, 0x64, 0x500);
        put_words(&mut p, 0x500, &handler.0);
    }
    Cartridge {
        p,
        ..Cartridge::default()
    }
}

fn machine(cartridge: Cartridge) -> NeoGeo {
    NeoGeo::new(cartridge, bios(), Vec::new())
}

fn run(machine: &mut NeoGeo, clocks: u64) {
    for _ in 0..clocks {
        machine.tick();
    }
}

fn ram_word(machine: &NeoGeo, at: u32) -> u16 {
    u16::from_be_bytes([machine.peek(at), machine.peek(at + 1)])
}

#[test]
fn the_bios_starts_and_hands_over_to_the_cartridge() {
    let mut program = Asm::default();
    program.move_w(0x1234, RAM).halt();
    let mut neo = machine(cart(&program, None, 0x1000));
    assert_eq!(neo.peek(4), 0x00, "BIOS vectors at reset");
    assert_eq!(neo.peek(5), 0xC0);
    run(&mut neo, 2000);
    assert_eq!(ram_word(&neo, RAM), 0x1234);
    assert_eq!(neo.peek(5), 0x00, "cartridge vectors after the swap");
    assert_eq!(
        neo.lspc.irq_level(),
        0,
        "the reset interrupt was acknowledged"
    );
}

#[test]
fn reset_brings_back_the_bios_vectors() {
    let mut program = Asm::default();
    program.inc_w(RAM).halt();
    let mut neo = machine(cart(&program, None, 0x1000));
    run(&mut neo, 2000);
    neo.reset();
    assert_eq!(neo.peek(5), 0xC0);
    run(&mut neo, 2000);
    assert_eq!(ram_word(&neo, RAM), 2, "the program ran twice");
}

#[test]
fn work_ram_mirrors_through_its_megabyte() {
    let mut program = Asm::default();
    program
        .move_w(0xBEEF, 0x1F_0010)
        .copy_w(RAM + 0x10, RAM + 0x20)
        .halt();
    let mut neo = machine(cart(&program, None, 0x1000));
    run(&mut neo, 3000);
    assert_eq!(ram_word(&neo, RAM + 0x20), 0xBEEF);
}

#[test]
fn the_p_rom_bank_window_switches_by_megabyte() {
    let mut program = Asm::default();
    program
        .copy_w(0x20_0000, RAM)
        .move_b(1, 0x2F_FFF1)
        .copy_w(0x20_0000, RAM + 2)
        .move_b(5, 0x2F_FFF1) // past the end: bank 0
        .copy_w(0x20_0000, RAM + 4)
        .halt();
    let mut cartridge = cart(&program, None, 0x30_0000);
    put_words(&mut cartridge.p, 0x10_0000, &[0x1111]);
    put_words(&mut cartridge.p, 0x20_0000, &[0x2222]);
    let mut neo = machine(cartridge);
    run(&mut neo, 6000);
    assert_eq!(ram_word(&neo, RAM), 0x1111);
    assert_eq!(ram_word(&neo, RAM + 2), 0x2222);
    assert_eq!(ram_word(&neo, RAM + 4), 0x1111);
}

#[test]
fn vertical_blank_interrupts_once_a_frame() {
    let mut program = Asm::default();
    program.set_sr(0x2000).halt();
    let mut handler = Asm::default();
    handler.inc_w(RAM).move_w(4, 0x3C_000C).rte();
    let mut neo = machine(cart(&program, Some(&handler), 0x1000));
    neo.run_frame();
    let before = ram_word(&neo, RAM);
    for _ in 0..3 {
        neo.run_frame();
    }
    assert_eq!(ram_word(&neo, RAM) - before, 3);
    assert_eq!(neo.run_frame(), FRAME);
}

#[test]
fn the_palette_reaches_the_picture() {
    let mut program = Asm::default();
    // Entry $FFF, the background, in bank 0: full red.
    program.move_w(0x4F00, 0x40_1FFE).halt();
    let mut neo = machine(cart(&program, None, 0x1000));
    neo.run_frame();
    neo.run_frame();
    assert_eq!(neo.framebuffer()[0], 0xFFFF_0000);
    assert_eq!(neo.framebuffer().len(), 320 * 224);
}

#[test]
fn the_system_latch_sets_shadow_and_palette_bank() {
    let mut program = Asm::default();
    program
        .move_b(0, 0x3A_0011) // shadow on
        .move_b(0, 0x3A_001F) // palette bank 1
        .move_w(0x0F00, 0x40_0002) // bank 1, entry 1
        .halt();
    let mut neo = machine(cart(&program, None, 0x1000));
    run(&mut neo, 4000);
    assert!(neo.lspc.shadow);
    assert!(neo.lspc.palette_bank);
    assert_eq!(neo.lspc.palette()[0x1001], 0x0F00);
    assert_eq!(neo.lspc.palette()[0x0001], 0);
}

#[test]
fn the_lspc2_registers_are_reached_by_word() {
    let mut program = Asm::default();
    program
        .move_w(0x7000, 0x3C_0000)
        .move_w(0x00AB, 0x3C_0002)
        .move_w(0x7000, 0x3C_0000)
        .copy_w(0x3C_0002, RAM)
        .halt();
    let mut neo = machine(cart(&program, None, 0x1000));
    run(&mut neo, 4000);
    assert_eq!(neo.lspc.vram()[0x7000], 0x00AB);
    assert_eq!(ram_word(&neo, RAM), 0x00AB);
}

#[test]
fn controllers_read_active_low() {
    let mut program = Asm::default();
    program
        .copy_b(0x30_0000, RAM)
        .copy_b(0x34_0000, RAM + 1)
        .copy_w(0x38_0000, RAM + 2)
        .halt();
    let mut neo = machine(cart(&program, None, 0x1000));
    neo.buttons[0] = Buttons {
        up: true,
        a: true,
        start: true,
        ..Buttons::default()
    };
    neo.buttons[1] = Buttons {
        d: true,
        select: true,
        ..Buttons::default()
    };
    run(&mut neo, 4000);
    assert_eq!(neo.peek(RAM), 0xEE);
    assert_eq!(neo.peek(RAM + 1), 0x7F);
    // Start 1 held, select 2 held, no memory card, an AES.
    assert_eq!(ram_word(&neo, RAM + 2), 0x76FF);
}

/// A Z80 program: at $0000 enable the NMI and spin; at $0066 read the
/// command, reply with it plus one, and return.
fn echo_m_rom() -> Vec<u8> {
    let mut m = vec![0; 0x2_0000];
    m[..8].copy_from_slice(&[
        0x31, 0x00, 0x00, // LD SP,$0000
        0xD3, 0x08, // OUT ($08),A: NMI on
        0x18, 0xFE, // JR $
        0x00,
    ]);
    m[0x66..0x6D].copy_from_slice(&[
        0xDB, 0x00, // IN A,($00)
        0x3C, // INC A
        0xD3, 0x0C, // OUT ($0C),A
        0xED, 0x45, // RETN
    ]);
    m
}

#[test]
fn a_sound_command_raises_the_z80_nmi_and_gets_a_reply() {
    let mut program = Asm::default();
    program
        .move_b(0x41, 0x32_0000)
        .wait_b(0x32_0000, 0x42)
        .move_w(1, RAM)
        .halt();
    let mut cartridge = cart(&program, None, 0x1000);
    cartridge.m = echo_m_rom();
    let mut neo = machine(cartridge);
    run(&mut neo, 20_000);
    assert_eq!(neo.sound_reply(), 0x42);
    assert_eq!(neo.sound_command(), (0x41, false));
    assert_eq!(ram_word(&neo, RAM), 1);
}

#[test]
fn the_nmi_waits_while_disabled() {
    let mut program = Asm::default();
    program.move_b(0x10, 0x32_0000).halt();
    let mut cartridge = cart(&program, None, 0x1000);
    let mut m = echo_m_rom();
    m[3..5].copy_from_slice(&[0xD3, 0x18]); // OUT ($18),A: NMI off
    cartridge.m = m;
    let mut neo = machine(cartridge);
    run(&mut neo, 20_000);
    assert_eq!(neo.sound_command(), (0x10, true));
    assert_eq!(neo.sound_reply(), 0);
}

#[test]
fn z80_bank_windows_select_by_reading_their_port() {
    let mut cartridge = cart(&Asm(vec![0x60FE]), None, 0x1000);
    let mut m = vec![0; 0x4_0000];
    m[..19].copy_from_slice(&[
        0x01, 0x0B, 0x05, // LD BC,$050B
        0xED, 0x78, // IN A,(C): 16 KB window to bank 5
        0x3A, 0x00, 0x80, // LD A,($8000)
        0xD3, 0x0C, // OUT ($0C),A
        0x01, 0x08, 0x47, // LD BC,$4708
        0xED, 0x78, // IN A,(C): 2 KB window to bank $47
        0x3A, 0x00, 0xF0, // LD A,($F000)
        0x32,
    ]);
    m[19..23].copy_from_slice(&[0x00, 0xF8, 0x18, 0xFE]); // LD ($F800),A; JR $
    m[5 * 0x4000] = 0x5A;
    m[0x47 * 0x800] = 0xA5;
    // At power-on the windows show the M-ROM straight through.
    m[0x8000] = 0x80;
    m[0xF000] = 0xF0;
    cartridge.m = m;
    let mut neo = machine(cartridge);
    assert_eq!(neo.read_z80(0x8000), 0x80);
    assert_eq!(neo.read_z80(0xF000), 0xF0);
    run(&mut neo, 5000);
    assert_eq!(neo.sound_reply(), 0x5A);
    assert_eq!(neo.z80_ram()[0], 0xA5);
}

#[test]
fn the_z80_reaches_the_ym2610() {
    let mut cartridge = cart(&Asm(vec![0x60FE]), None, 0x1000);
    let mut m = vec![0; 0x1_0000];
    m[..14].copy_from_slice(&[
        0x3E, 0x01, // LD A,$01
        0xD3, 0x06, // OUT ($06),A: upper address $101
        0x3E, 0x3F, // LD A,$3F
        0xD3, 0x07, // OUT ($07),A: ADPCM-A total level
        0xDB, 0x04, // IN A,($04): status
        0x32, 0x00, 0xF8, // LD ($F800),A
        0x76, // HALT
    ]);
    cartridge.m = m;
    let mut neo = machine(cartridge);
    run(&mut neo, 5000);
    assert_eq!(neo.ym2610.adpcm_a.registers()[1], 0x3F);
    assert_eq!(neo.z80_ram()[0] & 0x03, 0, "no timer has fired");
}

#[test]
fn the_memory_card_reads_writes_and_locks() {
    let mut program = Asm::default();
    program
        .copy_w(0x38_0000, RAM)
        .move_b(0, 0x3A_0017) // unlock 2
        .move_b(0x5A, 0x80_0004) // card byte 2
        .move_b(0, 0x3A_0007) // lock 2
        .move_b(0xA5, 0x80_0006) // ignored
        .copy_w(0x80_0004, RAM + 2)
        .halt();
    let mut neo = machine(cart(&program, None, 0x1000));
    neo.set_memory_card(Some(vec![0; MEMORY_CARD_SIZE]));
    run(&mut neo, 8000);
    assert_eq!(ram_word(&neo, RAM) & 0x7000, 0, "a card is present");
    let card = neo.memory_card().unwrap();
    assert_eq!(card[2], 0x5A);
    assert_eq!(card[3], 0);
    assert_eq!(ram_word(&neo, RAM + 2), 0xFF5A);
}

#[test]
fn audio_comes_out_at_48_khz() {
    let mut neo = machine(cart(&Asm(vec![0x60FE]), None, 0x1000));
    run(&mut neo, MASTER_HZ / 10);
    let audio = neo.take_audio();
    let frames = audio.len() / 2;
    assert!((4799..=4801).contains(&frames), "{frames} frames");
    assert!(neo.take_audio().is_empty());
}

#[test]
fn state_survives_a_snapshot() {
    let mut program = Asm::default();
    program.set_sr(0x2000).halt();
    let mut handler = Asm::default();
    handler.inc_w(RAM).move_w(4, 0x3C_000C).rte();
    let cartridge = cart(&program, Some(&handler), 0x1000);
    let mut neo = machine(cartridge.clone());
    run(&mut neo, FRAME + 12_345);
    // The frame-complete signal is for the host and is not saved.
    neo.lspc.take_frame_complete();

    let bytes = postcard::to_allocvec(&neo).unwrap();
    let mut copy: NeoGeo = postcard::from_bytes(&bytes).unwrap();
    copy.attach_media(cartridge, bios(), Vec::new());
    for _ in 0..2 {
        neo.run_frame();
        copy.run_frame();
    }
    assert_eq!(neo.ram(), copy.ram());
    assert_eq!(neo.mclk(), copy.mclk());
    assert_eq!(neo.framebuffer(), copy.framebuffer());
}
