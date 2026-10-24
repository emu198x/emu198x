//! Whole-machine tests on hand-assembled 68000 programs.

use super::*;

const RAM: u32 = 0xFF_0000;

/// A 68000 program builder.
#[derive(Default)]
struct Asm(Vec<u16>);

impl Asm {
    fn long(&mut self, value: u32) -> &mut Self {
        self.0.push((value >> 16) as u16);
        self.0.push(value as u16);
        self
    }
    /// MOVE.W #imm,(abs).L
    fn move_w(&mut self, imm: u16, to: u32) -> &mut Self {
        self.0.extend([0x33FC, imm]);
        self.long(to)
    }
    /// MOVE.L #imm,(abs).L
    fn move_l(&mut self, imm: u32, to: u32) -> &mut Self {
        self.0.push(0x23FC);
        self.long(imm).long(to)
    }
    /// MOVE.B #imm,(abs).L
    fn move_b(&mut self, imm: u8, to: u32) -> &mut Self {
        self.0.extend([0x13FC, u16::from(imm)]);
        self.long(to)
    }
    /// MOVE.B (abs).L,(abs).L
    fn copy_b(&mut self, from: u32, to: u32) -> &mut Self {
        self.0.push(0x13F9);
        self.long(from).long(to)
    }
    /// Wait until bit 0 of the byte at `at` is clear: BTST then BNE.S back.
    fn wait_bit0_clear(&mut self, at: u32) -> &mut Self {
        self.0.extend([0x0839, 0x0000]);
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

/// A cartridge: reset vectors, the program at $200, an optional V-int
/// handler at $300, and `data` at $400.
fn cart(program: &Asm, vint: Option<&Asm>, data: &[u16]) -> Vec<u8> {
    let mut rom = vec![0u8; 0x800];
    let put_long =
        |rom: &mut Vec<u8>, at: usize, v: u32| rom[at..at + 4].copy_from_slice(&v.to_be_bytes());
    put_long(&mut rom, 0, 0x00FF_FE00);
    put_long(&mut rom, 4, 0x200);
    if vint.is_some() {
        put_long(&mut rom, 0x78, 0x300);
    }
    let put = |rom: &mut Vec<u8>, at: usize, words: &[u16]| {
        for (i, w) in words.iter().enumerate() {
            rom[at + i * 2..at + i * 2 + 2].copy_from_slice(&w.to_be_bytes());
        }
    };
    put(&mut rom, 0x200, &program.0);
    if let Some(handler) = vint {
        put(&mut rom, 0x300, &handler.0);
    }
    put(&mut rom, 0x400, data);
    rom
}

fn frames(md: &mut MegaDrive, n: usize) {
    for _ in 0..n {
        md.run_frame();
    }
}

fn ram_word(md: &MegaDrive, at: u32) -> u16 {
    let a = (at & 0xFFFF) as usize;
    u16::from(md.ram()[a]) << 8 | u16::from(md.ram()[a + 1])
}

#[test]
fn the_68000_sets_the_backdrop_through_the_vdp_ports() {
    let mut p = Asm::default();
    p.move_w(0x8144, 0xC0_0004)
        .move_w(0x8C81, 0xC0_0004)
        .move_w(0x8701, 0xC0_0004)
        .move_l(0xC002_0000, 0xC0_0004) // CRAM write, address 2
        .move_w(0x000E, 0xC0_0000)
        .halt();
    let mut md = MegaDrive::new(cart(&p, None, &[]), Region::Americas);
    frames(&mut md, 3);
    assert_eq!(md.vdp.cram()[1], 0x000E);
    let middle = md.vdp.framebuffer()[120 * emu198x_sega_315_5313::render::LINE_WIDTH + 170];
    assert_eq!(middle, 0xFFFF_0000, "red backdrop");
}

#[test]
fn v_int_runs_the_level_6_handler_once_a_frame() {
    let mut p = Asm::default();
    p.move_w(0x8164, 0xC0_0004).set_sr(0x2000).halt();
    let mut handler = Asm::default();
    handler.inc_w(RAM).rte();
    let mut md = MegaDrive::new(cart(&p, Some(&handler), &[]), Region::Americas);
    frames(&mut md, 6);
    let count = ram_word(&md, RAM);
    assert!((5..=6).contains(&count), "{count} V-ints in six frames");
}

#[test]
fn the_68000_loads_and_starts_a_z80_program() {
    // LD A,$42; LD ($1000),A; JR $
    let z80_program = [0x3E, 0x42, 0x32, 0x00, 0x10, 0x18, 0xFE];
    let mut p = Asm::default();
    p.move_w(0x0100, 0xA1_1100).wait_bit0_clear(0xA1_1100);
    for (i, &byte) in z80_program.iter().enumerate() {
        p.move_b(byte, 0xA0_0000 + i as u32);
    }
    p.move_w(0x0000, 0xA1_1100).move_w(0x0100, 0xA1_1200).halt();
    let mut md = MegaDrive::new(cart(&p, None, &[]), Region::Americas);
    frames(&mut md, 2);
    assert!(!md.z80_in_reset());
    assert_eq!(md.z80_ram()[0x1000], 0x42, "the Z80 ran");
}

#[test]
fn busreq_stops_the_z80_at_a_cycle_boundary_and_release_resumes_it() {
    // LD HL,$1000; loop: INC (HL); JR loop
    let mut md = MegaDrive::new(cart(Asm::default().halt(), None, &[]), Region::Americas);
    md.z80_ram[..6].copy_from_slice(&[0x21, 0x00, 0x10, 0x34, 0x18, 0xFD]);
    md.z80_reset = false;
    for _ in 0..20_000 {
        md.tick();
    }
    assert_ne!(md.z80_ram()[0x1000], 0, "running");
    md.z80_busreq = true;
    // The Z80 stops within a machine cycle or two.
    for _ in 0..200 {
        md.tick();
    }
    assert!(md.z80_bus_acknowledged());
    let held = md.z80_ram()[0x1000];
    for _ in 0..20_000 {
        md.tick();
    }
    assert_eq!(md.z80_ram()[0x1000], held, "stopped while the bus is held");
    md.z80_busreq = false;
    for _ in 0..20_000 {
        md.tick();
    }
    assert_ne!(md.z80_ram()[0x1000], held, "resumed");
    assert!(!md.z80_bus_acknowledged());
}

#[test]
fn the_pad_reads_through_the_io_port() {
    let mut p = Asm::default();
    p.move_b(0x40, 0xA1_0009)
        .move_b(0x40, 0xA1_0003)
        .copy_b(0xA1_0003, RAM)
        .move_b(0x00, 0xA1_0003)
        .copy_b(0xA1_0003, RAM + 1)
        .halt();
    let mut md = MegaDrive::new(cart(&p, None, &[]), Region::Americas);
    md.set_buttons(
        0,
        Buttons {
            up: true,
            c: true,
            start: true,
            ..Buttons::default()
        },
    );
    frames(&mut md, 1);
    assert_eq!(md.ram()[0] & 0x3F, 0b01_1110, "TH high: C and Up pressed");
    assert_eq!(
        md.ram()[1] & 0x3F,
        0b01_0010,
        "TH low: Start and Up pressed, 0 0"
    );
}

#[test]
fn the_version_register_reports_the_region() {
    for (region, expected) in [
        (Region::Japan, 0x20),
        (Region::Americas, 0xA0),
        (Region::Europe, 0xE0),
    ] {
        let mut p = Asm::default();
        p.copy_b(0xA1_0001, RAM).halt();
        let mut md = MegaDrive::new(cart(&p, None, &[]), region);
        frames(&mut md, 1);
        assert_eq!(md.ram()[0], expected, "{region:?}");
    }
}

#[test]
fn a_dma_from_rom_fills_cram_while_the_68000_waits() {
    let mut p = Asm::default();
    p.move_w(0x8114, 0xC0_0004) // Mode 5, DMA enabled, display off
        .move_w(0x8F02, 0xC0_0004)
        .move_w(0x9303, 0xC0_0004) // three words
        .move_w(0x9400, 0xC0_0004)
        .move_w(0x9500, 0xC0_0004) // source $400 = word $200
        .move_w(0x9602, 0xC0_0004)
        .move_w(0x9700, 0xC0_0004)
        .move_l(0xC000_0080, 0xC0_0004) // CRAM write + DMA at 0
        .move_w(0x1234, RAM) // runs once the DMA has released the bus
        .halt();
    let mut md = MegaDrive::new(cart(&p, None, &[0x0E00, 0x00E0, 0x000E]), Region::Americas);
    frames(&mut md, 2);
    assert_eq!(&md.vdp.cram()[..3], &[0x0E00, 0x00E0, 0x000E]);
    assert_eq!(ram_word(&md, RAM), 0x1234);
}

#[test]
fn the_z80_reaches_the_ym2612_and_the_68000_window() {
    // LD A,($8400): the 68000's $400 through bank 0. LD ($1000),A.
    // LD A,$22; LD ($4000),A; LD A,$08; LD ($4001),A: LFO on.
    let z80_program = [
        0x3A, 0x00, 0x84, 0x32, 0x00, 0x10, 0x3E, 0x22, 0x32, 0x00, 0x40, 0x3E, 0x08, 0x32, 0x01,
        0x40, 0x18, 0xFE,
    ];
    let mut md = MegaDrive::new(
        cart(Asm::default().halt(), None, &[0xAB00]),
        Region::Americas,
    );
    md.z80_ram[..z80_program.len()].copy_from_slice(&z80_program);
    md.z80_reset = false;
    for _ in 0..100_000 {
        md.tick();
    }
    assert_eq!(md.z80_ram()[0x1000], 0xAB, "read through the bank window");
}

#[test]
fn a_snapshot_resumes_in_lockstep() {
    let mut p = Asm::default();
    p.move_w(0x8164, 0xC0_0004).set_sr(0x2000).halt();
    let mut handler = Asm::default();
    handler.inc_w(RAM).rte();
    let mut md = MegaDrive::new(cart(&p, Some(&handler), &[]), Region::Europe);
    frames(&mut md, 2);
    let bytes = postcard::to_allocvec(&md).expect("encode");
    let mut restored: MegaDrive = postcard::from_bytes(&bytes).expect("decode");
    restored.finish_restore();
    for _ in 0..200_000 {
        md.tick();
        restored.tick();
    }
    assert_eq!(md.ram(), restored.ram());
    assert_eq!(md.cpu.regs.pc, restored.cpu.regs.pc);
    assert_eq!(md.mclk(), restored.mclk());
}

#[test]
fn the_68000_runs_at_a_seventh_of_the_master_clock_less_refresh() {
    // loop: ADDQ.W #1,(abs).L (20 clocks); BRA.S loop (10 clocks).
    let mut p = Asm::default();
    p.inc_w(RAM);
    p.0.push(0x60F8);
    let mut md = MegaDrive::new(cart(&p, None, &[]), Region::Americas);
    md.run_frame();
    let start = ram_word(&md, RAM);
    md.run_frame();
    let per_frame = u32::from(ram_word(&md, RAM).wrapping_sub(start));
    // 262 lines x 3420 master clocks / 7 = 128,005 clocks: 4,267 passes
    // with no refresh. Refresh takes two of every 128 clocks from the
    // ROM and RAM accesses; four of this loop's five accesses are ROM or
    // RAM.
    assert!(
        (4130..=4267).contains(&per_frame),
        "{per_frame} passes a frame"
    );
}

#[test]
fn the_z80_reaches_the_vdp_and_psg_and_the_68000_reads_the_hv_counter() {
    // LD A,$9F; LD ($7F11),A: PSG channel 0 silent. LD A,($7F08): V counter.
    // LD ($1000),A; JR $
    let z80_program = [
        0x3E, 0x9F, 0x32, 0x11, 0x7F, 0x3A, 0x08, 0x7F, 0x32, 0x00, 0x10, 0x18, 0xFE,
    ];
    let mut p = Asm::default();
    p.0.extend([0x33F9]); // MOVE.W (abs).L,(abs).L
    p.long(0xC0_0008).long(RAM);
    p.halt();
    let mut md = MegaDrive::new(cart(&p, None, &[]), Region::Americas);
    md.z80_ram[..z80_program.len()].copy_from_slice(&z80_program);
    md.z80_reset = false;
    frames(&mut md, 1);
    assert!(md.z80_ram()[0x1000] < 0xFF, "a V counter came back");
    assert_ne!(ram_word(&md, RAM), 0, "the 68000 read the HV counter");
}

#[test]
fn the_reset_button_restarts_the_68000_and_holds_the_z80() {
    let mut p = Asm::default();
    p.inc_w(RAM).halt();
    let mut md = MegaDrive::new(cart(&p, None, &[]), Region::Americas);
    frames(&mut md, 1);
    md.z80_reset = false;
    md.reset();
    assert!(md.z80_in_reset());
    frames(&mut md, 1);
    assert_eq!(
        ram_word(&md, RAM),
        2,
        "the program ran again from the vector"
    );
}

#[test]
fn run_frame_returns_a_frames_master_clocks() {
    for (region, lines) in [(Region::Americas, NTSC_LINES), (Region::Europe, PAL_LINES)] {
        let mut md = MegaDrive::new(cart(Asm::default().halt(), None, &[]), region);
        md.run_frame();
        assert_eq!(md.run_frame(), lines * MCLKS_PER_LINE, "{region:?}");
    }
}

#[test]
fn the_framebuffer_passes_the_vdps_through() {
    let mut md = MegaDrive::new(cart(Asm::default().halt(), None, &[]), Region::Americas);
    md.run_frame();
    assert_eq!(md.frame_width() as usize, md.vdp.frame_width());
    assert_eq!(md.frame_height() as usize, md.vdp.frame_height());
    assert!(md.framebuffer().len() >= FRAME_STRIDE * md.frame_height() as usize);
}

#[test]
fn audio_comes_out_as_48_khz_stereo_within_range() {
    // A PSG tone at full volume: channel 0, period $010, attenuation 0.
    let mut p = Asm::default();
    p.move_b(0x80, 0xC0_0011)
        .move_b(0x01, 0xC0_0011)
        .move_b(0x90, 0xC0_0011)
        .halt();
    let mut md = MegaDrive::new(cart(&p, None, &[]), Region::Americas);
    let mut samples = Vec::new();
    for _ in 0..60 {
        md.run_frame();
        samples.extend(md.take_audio());
    }
    assert_eq!(samples.len() % 2, 0, "interleaved pairs");
    // 48 kHz over the clocks run, give or take what is still pending. The
    // first frame is short: it ends at the first V-blank after power-on.
    let pairs = samples.len() as u64 / 2;
    let expected = md.mclk() * u64::from(AUDIO_SAMPLE_RATE) / NTSC_MASTER_HZ;
    assert!(
        pairs.abs_diff(expected) <= 2,
        "{pairs} pairs where {expected} were due"
    );
    assert!(samples.iter().all(|s| s.abs() <= 0.75), "no clipping");
    assert!(samples.iter().any(|s| s.abs() > 0.01), "the PSG is heard");
}

#[test]
fn peek_and_poke_reach_rom_and_both_rams_without_side_effects() {
    let mut md = MegaDrive::new(
        cart(Asm::default().halt(), None, &[0xABCD]),
        Region::Americas,
    );
    assert_eq!(md.peek(0x400), 0xAB);
    assert_eq!(md.peek(0x401), 0xCD);
    md.poke(0xFF_1234, 0x5A);
    md.poke(0xA0_0010, 0x77);
    md.poke(0x400, 0x00);
    assert_eq!(md.peek(0xFF_1234), 0x5A);
    assert_eq!(md.ram()[0x1234], 0x5A);
    assert_eq!(md.peek(0xA0_0010), 0x77);
    assert_eq!(md.z80_ram()[0x10], 0x77);
    assert_eq!(md.peek(0x400), 0xAB, "ROM is read-only");
    assert_eq!(md.peek(0xC0_0004), 0xFF, "ports are not read");
}
