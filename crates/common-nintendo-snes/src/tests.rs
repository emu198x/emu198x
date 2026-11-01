//! The machine against hand-assembled programs. The 65C816 starts in
//! emulation mode, and these programs stay there: 8-bit registers, the
//! emulation vectors.

use super::*;

/// A little 65C816 assembler: enough to write the test programs without a
/// toolchain.
struct Asm {
    bytes: Vec<u8>,
    /// The CPU address of byte 0.
    base: u16,
}

impl Asm {
    fn new(base: u16) -> Self {
        Self {
            bytes: Vec::new(),
            base,
        }
    }
    fn here(&self) -> u16 {
        self.base + self.bytes.len() as u16
    }
    fn op(&mut self, bytes: &[u8]) -> &mut Self {
        self.bytes.extend_from_slice(bytes);
        self
    }
    fn lda_imm(&mut self, v: u8) -> &mut Self {
        self.op(&[0xA9, v])
    }
    fn lda_abs(&mut self, a: u16) -> &mut Self {
        self.op(&[0xAD, a as u8, (a >> 8) as u8])
    }
    fn lda_abs_x(&mut self, a: u16) -> &mut Self {
        self.op(&[0xBD, a as u8, (a >> 8) as u8])
    }
    fn sta_abs(&mut self, a: u16) -> &mut Self {
        self.op(&[0x8D, a as u8, (a >> 8) as u8])
    }
    fn stz_abs(&mut self, a: u16) -> &mut Self {
        self.op(&[0x9C, a as u8, (a >> 8) as u8])
    }
    fn cmp_imm(&mut self, v: u8) -> &mut Self {
        self.op(&[0xC9, v])
    }
    fn cmp_abs(&mut self, a: u16) -> &mut Self {
        self.op(&[0xCD, a as u8, (a >> 8) as u8])
    }
    fn ldx_imm(&mut self, v: u8) -> &mut Self {
        self.op(&[0xA2, v])
    }
    fn cpx_imm(&mut self, v: u8) -> &mut Self {
        self.op(&[0xE0, v])
    }
    fn inc_abs(&mut self, a: u16) -> &mut Self {
        self.op(&[0xEE, a as u8, (a >> 8) as u8])
    }
    /// A branch back to `target`.
    fn branch(&mut self, opcode: u8, target: u16) -> &mut Self {
        let rel = i32::from(target) - i32::from(self.here() + 2);
        assert!((-128..=127).contains(&rel), "branch out of range");
        self.op(&[opcode, rel as i8 as u8])
    }
    fn bne(&mut self, target: u16) -> &mut Self {
        self.branch(0xD0, target)
    }
    fn bra(&mut self, target: u16) -> &mut Self {
        self.branch(0x80, target)
    }
    /// `BRA *`: spin forever.
    fn halt(&mut self) -> &mut Self {
        let here = self.here();
        self.bra(here)
    }
}

/// A 32 KB LoROM with `code` at `$8000` and the emulation vectors set:
/// reset to `$8000`, NMI and IRQ to `nmi` and `irq`.
fn lorom(code: &[u8], nmi: u16, irq: u16) -> Cartridge {
    let mut rom = vec![0u8; 0x8000];
    rom[..code.len()].copy_from_slice(code);
    let mut vector = |offset: usize, target: u16| {
        rom[offset..offset + 2].copy_from_slice(&target.to_le_bytes());
    };
    vector(0x7FFC, 0x8000);
    vector(0x7FFA, nmi);
    vector(0x7FFE, irq);
    Cartridge::from_rom(rom, Map::LoRom, 0)
}

fn machine(cart: Cartridge) -> Snes {
    Snes::new(cart, Region::Ntsc, None)
}

/// Tick until work RAM byte `address` holds `value`, returning the master
/// clock it happened at.
fn run_until(snes: &mut Snes, address: usize, value: u8, limit: u64) -> u64 {
    let start = snes.mclk();
    while snes.wram()[address] != value {
        assert!(
            snes.mclk() - start < limit,
            "wram[${address:04X}] never became ${value:02X} (it is ${:02X})",
            snes.wram()[address]
        );
        snes.tick();
    }
    snes.mclk()
}

#[test]
fn a_program_runs_from_the_reset_vector() {
    let mut a = Asm::new(0x8000);
    a.lda_imm(0x42).sta_abs(0x0010).halt();
    let mut snes = machine(lorom(&a.bytes, 0x8000, 0x8000));
    run_until(&mut snes, 0x10, 0x42, 100_000);
}

#[test]
fn the_multiplier_takes_eight_cpu_cycles() {
    // Product read straight after the write, then again much later.
    let mut a = Asm::new(0x8000);
    a.lda_imm(0x12)
        .sta_abs(0x4202)
        .lda_imm(0x34)
        .sta_abs(0x4203)
        // Too soon: the first read lands two cycles in.
        .lda_abs(0x4216)
        .sta_abs(0x0020)
        .op(&[0xEA, 0xEA, 0xEA, 0xEA])
        .lda_abs(0x4216)
        .sta_abs(0x0010)
        .lda_abs(0x4217)
        .sta_abs(0x0011)
        .lda_imm(1)
        .sta_abs(0x0012)
        .halt();
    let mut snes = machine(lorom(&a.bytes, 0x8000, 0x8000));
    run_until(&mut snes, 0x12, 1, 200_000);
    let product = 0x12u16 * 0x34;
    assert_eq!(
        u16::from(snes.wram()[0x10]) | u16::from(snes.wram()[0x11]) << 8,
        product
    );
    assert_ne!(
        snes.wram()[0x20],
        product as u8,
        "the early read is partial"
    );
}

#[test]
fn the_divider_gives_quotient_and_remainder() {
    let mut a = Asm::new(0x8000);
    a.lda_imm(0x39)
        .sta_abs(0x4204)
        .lda_imm(0x30)
        .sta_abs(0x4205) // 0x3039 = 12345
        .lda_imm(100)
        .sta_abs(0x4206)
        .op(&[0xEA; 10])
        .lda_abs(0x4214)
        .sta_abs(0x0010)
        .lda_abs(0x4215)
        .sta_abs(0x0011)
        .lda_abs(0x4216)
        .sta_abs(0x0012)
        .lda_imm(1)
        .sta_abs(0x0013)
        .halt();
    let mut snes = machine(lorom(&a.bytes, 0x8000, 0x8000));
    run_until(&mut snes, 0x13, 1, 200_000);
    let w = snes.wram();
    assert_eq!(u16::from(w[0x10]) | u16::from(w[0x11]) << 8, 123);
    assert_eq!(w[0x12], 45);
}

/// Configure channel 0 for an A-to-B transfer and start it.
fn dma_setup(a: &mut Asm, mode: u8, b: u8, source: u16, bank: u8, size: u16) {
    a.lda_imm(mode)
        .sta_abs(0x4300)
        .lda_imm(b)
        .sta_abs(0x4301)
        .lda_imm(source as u8)
        .sta_abs(0x4302)
        .lda_imm((source >> 8) as u8)
        .sta_abs(0x4303)
        .lda_imm(bank)
        .sta_abs(0x4304)
        .lda_imm(size as u8)
        .sta_abs(0x4305)
        .lda_imm((size >> 8) as u8)
        .sta_abs(0x4306);
}

#[test]
fn dma_copies_rom_into_vram_eight_clocks_a_byte() {
    const LEN: u16 = 0x200;
    let mut a = Asm::new(0x8000);
    // VRAM increment after the high byte, address 0.
    a.lda_imm(0x80)
        .sta_abs(0x2115)
        .stz_abs(0x2116)
        .stz_abs(0x2117);
    dma_setup(&mut a, 0x01, 0x18, 0xC000, 0x00, LEN);
    a.lda_imm(1).sta_abs(0x0010).sta_abs(0x420B);
    a.lda_imm(2).sta_abs(0x0010).halt();
    let mut code = a.bytes.clone();
    code.resize(0x4000, 0);
    // The source data at $00:C000 (file offset $4000).
    code.extend((0..LEN).map(|i| (i * 7 + 3) as u8));
    let mut snes = machine(lorom(&code, 0x8000, 0x8000));
    let started = run_until(&mut snes, 0x10, 1, 400_000);
    let finished = run_until(&mut snes, 0x10, 2, 400_000);
    for word in 0..LEN / 2 {
        let lo = (word * 2 * 7 + 3) as u8;
        let hi = ((word * 2 + 1) * 7 + 3) as u8;
        assert_eq!(
            snes.ppu.vram(word),
            u16::from(hi) << 8 | u16::from(lo),
            "VRAM word {word}"
        );
    }
    // 8 clocks a byte, plus the channel's setup, alignment, the STA and
    // the following LDA/STA, and a refresh or so.
    let elapsed = finished - started;
    let bytes = u64::from(LEN) * 8;
    assert!(
        (bytes..bytes + 300).contains(&elapsed),
        "{elapsed} clocks for {LEN} bytes"
    );
}

#[test]
fn hdma_writes_one_table_entry_per_line() {
    // Channel 0 in mode 0 to WMDATA: each line's byte lands in work RAM in
    // turn. Table: three single lines, then a repeat run of three.
    let mut a = Asm::new(0x8000);
    // WRAM address $0100.
    a.stz_abs(0x2181)
        .lda_imm(0x01)
        .sta_abs(0x2182)
        .stz_abs(0x2183)
        .stz_abs(0x4300)
        .lda_imm(0x80)
        .sta_abs(0x4301)
        .stz_abs(0x4302)
        .lda_imm(0xC0)
        .sta_abs(0x4303)
        .stz_abs(0x4304);
    // Enable it in vertical blank, so the first frame starts with the
    // setup at V=0. Enabled mid-frame, HDMA runs from whatever the
    // registers held, as on hardware.
    let wait = a.here();
    a.lda_abs(0x4212)
        .branch(0x10, wait) // BPL: wait for bit 7
        .lda_imm(1)
        .sta_abs(0x420C)
        .halt();
    let mut code = a.bytes.clone();
    code.resize(0x4000, 0);
    code.extend_from_slice(&[
        0x01, 0x11, 0x01, 0x22, 0x01, 0x33, 0x83, 0x44, 0x55, 0x66, 0x00,
    ]);
    let mut snes = machine(lorom(&code, 0x8000, 0x8000));
    for _ in 0..3 {
        snes.run_frame();
    }
    // The table restarts every frame, so the same six bytes repeat.
    let table = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    assert_eq!(&snes.wram()[0x100..0x106], &table);
    assert_eq!(&snes.wram()[0x106..0x10C], &table);
}

#[test]
fn nmi_arrives_once_a_frame() {
    // Main: enable NMI and wait. Handler: count frames in $10.
    let mut main = Asm::new(0x8000);
    main.lda_imm(0x80).sta_abs(0x4200);
    let top = main.here();
    main.op(&[0xCB]).bra(top); // WAI; BRA
    let handler = 0x8100;
    let mut nmi = Asm::new(handler);
    nmi.lda_abs(0x4210).inc_abs(0x0010).op(&[0x40]); // RTI
    let mut code = main.bytes.clone();
    code.resize(0x100, 0);
    code.extend_from_slice(&nmi.bytes);
    let mut snes = machine(lorom(&code, handler, 0x8000));
    snes.run_frame();
    let first = snes.wram()[0x10];
    for _ in 0..5 {
        snes.run_frame();
    }
    assert_eq!(snes.wram()[0x10], first + 5);
}

#[test]
fn a_v_irq_fires_on_its_line() {
    // VTIME = 100, V-IRQ on. The handler latches the counters through
    // $2137 and keeps the line from $213D.
    let mut main = Asm::new(0x8000);
    main.lda_imm(100)
        .sta_abs(0x4209)
        .stz_abs(0x420A)
        .lda_imm(0x20)
        .sta_abs(0x4200)
        .op(&[0x58]); // CLI
    let top = main.here();
    main.bra(top);
    let handler = 0x8100;
    let mut irq = Asm::new(handler);
    irq.lda_abs(0x2137)
        .lda_abs(0x213D)
        .sta_abs(0x0010)
        .lda_abs(0x4211)
        .inc_abs(0x0011)
        .op(&[0x40]);
    let mut code = main.bytes.clone();
    code.resize(0x100, 0);
    code.extend_from_slice(&irq.bytes);
    let mut snes = machine(lorom(&code, 0x8000, handler));
    for _ in 0..3 {
        snes.run_frame();
    }
    let line = snes.wram()[0x10];
    assert!((100..=101).contains(&line), "IRQ handler saw line {line}");
    assert!((2..=3).contains(&snes.wram()[0x11]), "once a frame");
}

#[test]
fn auto_read_fills_the_joypad_registers() {
    // Main: auto-read and NMI on. Handler: wait for auto-read to finish,
    // copy JOY1.
    let mut main = Asm::new(0x8000);
    main.lda_imm(0x81).sta_abs(0x4200);
    let top = main.here();
    main.bra(top);
    let handler = 0x8100;
    let mut nmi = Asm::new(handler);
    nmi.lda_abs(0x4210);
    let wait = nmi.here();
    nmi.lda_abs(0x4212)
        .op(&[0x29, 0x01]) // AND #1
        .bne(wait)
        .lda_abs(0x4218)
        .sta_abs(0x0010)
        .lda_abs(0x4219)
        .sta_abs(0x0011)
        .op(&[0x40]);
    let mut code = main.bytes.clone();
    code.resize(0x100, 0);
    code.extend_from_slice(&nmi.bytes);
    let mut snes = machine(lorom(&code, handler, 0x8000));
    let buttons = Buttons {
        b: true,
        start: true,
        right: true,
        a: true,
        r: true,
        ..Buttons::default()
    };
    snes.set_buttons(0, buttons);
    for _ in 0..3 {
        snes.run_frame();
    }
    let joy = u16::from(snes.wram()[0x10]) | u16::from(snes.wram()[0x11]) << 8;
    assert_eq!(joy, buttons.word());
    assert_eq!(joy, 0b1001_0001_1001_0000);
}

#[test]
fn the_clean_room_ipl_uploads_and_runs_a_sound_program() {
    // The sound program: write $5A to port 0 forever.
    let spc_program = [0x8F, 0x5A, 0xF4, 0x2F, 0xFE];
    let mut a = Asm::new(0x8000);
    let wait = a.here();
    a.lda_abs(0x2140).cmp_imm(0xAA).bne(wait);
    let wait = a.here();
    a.lda_abs(0x2141).cmp_imm(0xBB).bne(wait);
    // Block at $0200, bytes follow.
    a.stz_abs(0x2142)
        .lda_imm(0x02)
        .sta_abs(0x2143)
        .lda_imm(0x01)
        .sta_abs(0x2141)
        .lda_imm(0xCC)
        .sta_abs(0x2140);
    let wait = a.here();
    a.cmp_abs(0x2140).bne(wait);
    a.ldx_imm(0);
    let byte = a.here();
    a.lda_abs_x(0xC000)
        .sta_abs(0x2141)
        .op(&[0x8A]) // TXA
        .sta_abs(0x2140);
    let wait = a.here();
    a.cmp_abs(0x2140)
        .bne(wait)
        .op(&[0xE8]) // INX
        .cpx_imm(spc_program.len() as u8)
        .bne(byte);
    // Jump to $0200: command 0, a new marker.
    a.stz_abs(0x2142)
        .lda_imm(0x02)
        .sta_abs(0x2143)
        .stz_abs(0x2141)
        .op(&[0x8A, 0x1A]) // TXA; INC A
        .sta_abs(0x2140);
    let wait = a.here();
    a.lda_abs(0x2140).cmp_imm(0x5A).bne(wait);
    a.sta_abs(0x0010).halt();
    let mut code = a.bytes.clone();
    code.resize(0x4000, 0);
    code.extend_from_slice(&spc_program);
    let mut snes = machine(lorom(&code, 0x8000, 0x8000));
    run_until(&mut snes, 0x10, 0x5A, 20_000_000);
    assert_eq!(&snes.apu.dsp.ram()[0x200..0x205], &spc_program);
}

#[test]
fn slowrom_and_fastrom_cycles_take_eight_and_six_clocks() {
    // 200 NOPs (an opcode fetch and an internal cycle each) between two
    // markers, run in bank $00 and again in bank $80 with FastROM on.
    fn timed(fast: bool) -> u64 {
        let mut a = Asm::new(0x8000);
        if fast {
            a.lda_imm(1).sta_abs(0x420D);
            // JML $80:8010
            a.op(&[0x5C, 0x10, 0x80, 0x80]);
            a.bytes.resize(0x10, 0xEA);
        }
        a.lda_imm(1).sta_abs(0x0010);
        a.op(&[0xEA; 200]);
        a.lda_imm(2).sta_abs(0x0010).halt();
        let mut snes = machine(lorom(&a.bytes, 0x8000, 0x8000));
        let start = run_until(&mut snes, 0x10, 1, 1_000_000);
        run_until(&mut snes, 0x10, 2, 1_000_000) - start
    }
    let slow = timed(false);
    let fast = timed(true);
    // NOP: 8 (or 6) for the fetch + 6 internal; LDA #/STA abs follow.
    // Refresh adds 40 clocks a line, and the run crosses two or three.
    let slow_nops = 200 * 14;
    let fast_nops = 200 * 12;
    assert!((slow_nops..slow_nops + 300).contains(&slow), "slow {slow}");
    assert!((fast_nops..fast_nops + 300).contains(&fast), "fast {fast}");
    assert!(
        slow - fast > 350,
        "fast saves 2 clocks a fetch: {slow} vs {fast}"
    );
}

#[test]
fn a_snapshot_resumes_in_lockstep() {
    let mut a = Asm::new(0x8000);
    a.lda_imm(0x81).sta_abs(0x4200);
    let top = a.here();
    a.inc_abs(0x0010).bra(top);
    let mut snes = machine(lorom(&a.bytes, 0x8000, 0x8000));
    for _ in 0..12_345 {
        snes.tick();
    }
    let bytes = postcard::to_allocvec(&snes).expect("serialises");
    let mut copy: Snes = postcard::from_bytes(&bytes).expect("deserialises");
    for _ in 0..100_000 {
        snes.tick();
        copy.tick();
    }
    assert_eq!(snes.mclk(), copy.mclk());
    assert_eq!(snes.wram(), copy.wram());
    assert_eq!(snes.cpu.regs, copy.cpu.regs);
}

#[test]
fn frames_have_the_ntsc_and_pal_lengths() {
    let mut a = Asm::new(0x8000);
    a.halt();
    for (region, lines) in [(Region::Ntsc, 262u64), (Region::Pal, 312u64)] {
        let mut snes = Snes::new(lorom(&a.bytes, 0x8000, 0x8000), region, None);
        snes.run_frame();
        let clocks = snes.run_frame();
        // 1364 a line, give or take the short or long line.
        assert!(
            (lines * 1364 - 8..=lines * 1364 + 8).contains(&clocks),
            "{region:?}: {clocks}"
        );
    }
}

#[test]
fn sound_comes_out_at_48_khz() {
    let mut a = Asm::new(0x8000);
    a.halt();
    let mut snes = machine(lorom(&a.bytes, 0x8000, 0x8000));
    let clocks = snes.run_frame() + snes.run_frame();
    let samples = snes.take_audio().len() / 2;
    let expected = clocks as f64 * 48_000.0 / 21_477_272.7;
    assert!(
        (samples as f64 - expected).abs() < 4.0,
        "{samples} samples for {expected:.1}"
    );
}

#[test]
fn reset_restarts_the_cpu_and_keeps_wram() {
    let mut a = Asm::new(0x8000);
    a.inc_abs(0x0010).halt();
    let mut snes = machine(lorom(&a.bytes, 0x8000, 0x8000));
    snes.run_frame();
    assert_eq!(snes.wram()[0x10], 1);
    snes.reset();
    snes.run_frame();
    assert_eq!(snes.wram()[0x10], 2, "ran again, from the RAM it left");
}

#[test]
fn peek_and_poke_reach_ram_without_side_effects() {
    let mut snes = machine(Cartridge::from_rom(vec![0x11; 0x8000], Map::LoRom, 0x2000));
    snes.poke(0x7E_1234, 0xAB);
    assert_eq!(snes.peek(0x7E_1234), 0xAB);
    assert_eq!(snes.peek(0x00_1234), 0xAB, "low RAM mirrors bank $7E");
    snes.poke(0x70_0010, 0xCD);
    assert_eq!(snes.peek(0x70_0010), 0xCD);
    assert_eq!(snes.cart.ram()[0x10], 0xCD);
    assert_eq!(snes.peek(0x00_8000), 0x11);
}

mod cartridge {
    use crate::cart::{Cartridge, Map, Target, mirror};

    #[test]
    fn lorom_maps_32k_banks_and_its_ram() {
        let cart = Cartridge::from_rom(vec![0; 0x10_0000], Map::LoRom, 0x8000);
        assert_eq!(cart.decode(0x00_8000), Target::Rom(0));
        assert_eq!(cart.decode(0x01_8000), Target::Rom(0x8000));
        assert_eq!(cart.decode(0x81_FFFF), Target::Rom(0xFFFF));
        assert_eq!(
            cart.decode(0x41_0000),
            Target::Rom(0x8000),
            "low-half mirror"
        );
        assert_eq!(cart.decode(0x70_0000), Target::Ram(0));
        assert_eq!(cart.decode(0xF0_7FFF), Target::Ram(0x7FFF));
        assert_eq!(cart.decode(0x00_6000), Target::None);
    }

    #[test]
    fn hirom_maps_64k_banks_and_its_ram() {
        let cart = Cartridge::from_rom(vec![0; 0x20_0000], Map::HiRom, 0x2000);
        assert_eq!(cart.decode(0xC0_0000), Target::Rom(0));
        assert_eq!(cart.decode(0x01_8000), Target::Rom(0x1_8000));
        assert_eq!(cart.decode(0x41_1234), Target::Rom(0x1_1234));
        assert_eq!(cart.decode(0x20_6000), Target::Ram(0));
        assert_eq!(cart.decode(0xBF_7FFF), Target::Ram(0x1FFF));
        assert_eq!(cart.decode(0x00_1000), Target::None);
    }

    #[test]
    fn exhirom_puts_the_second_four_megabytes_low() {
        let cart = Cartridge::from_rom(vec![0; 0x60_0000], Map::ExHiRom, 0);
        assert_eq!(cart.decode(0xC0_0000), Target::Rom(0));
        assert_eq!(cart.decode(0x40_0000), Target::Rom(0x40_0000));
    }

    #[test]
    fn non_power_of_two_roms_mirror_their_tail() {
        // 3 MB: the last MB repeats in the fourth.
        assert_eq!(mirror(0x10_0000, 0x30_0000), 0x10_0000);
        assert_eq!(mirror(0x30_0000, 0x30_0000), 0x20_0000);
        assert_eq!(mirror(0x38_0000, 0x30_0000), 0x28_0000);
        assert_eq!(mirror(0x12_3456, 0x10_0000), 0x02_3456);
        assert_eq!(mirror(5, 0), 0);
    }

    #[test]
    fn images_go_through_the_header() {
        let mut rom = vec![0u8; 0x8000];
        let base = 0x7FC0;
        rom[base..base + 21].copy_from_slice(b"HEADER TEST          ");
        rom[base + 0x15] = 0x20;
        rom[base + 0x16] = 0x02;
        rom[base + 0x17] = 0x05;
        rom[base + 0x18] = 0x01;
        rom[base + 0x19] = 0x02;
        rom[base + 0x3C..base + 0x3E].copy_from_slice(&0x8000u16.to_le_bytes());
        let checksum = format_nintendo_snes_sfc::compute_checksum(&rom);
        rom[base + 0x1C..base + 0x1E].copy_from_slice(&(!checksum).to_le_bytes());
        rom[base + 0x1E..base + 0x20].copy_from_slice(&checksum.to_le_bytes());
        let cart = Cartridge::from_image(&rom).expect("valid");
        assert_eq!(cart.map, Map::LoRom);
        assert_eq!(cart.title, "HEADER TEST");
        assert!(cart.battery && cart.pal);
        assert_eq!(cart.ram().len(), 2048);
        rom[base + 0x16] = 0x13; // SuperFX
        assert!(matches!(
            Cartridge::from_image(&rom),
            Err(crate::CartridgeError::UnsupportedCoprocessor(_))
        ));
        assert!(Cartridge::from_image(&[0; 16]).is_err());
    }
}

mod pad {
    use crate::pad::{Buttons, Pad};

    #[test]
    fn the_shift_register_reads_b_first_then_ones() {
        let mut pad = Pad::new(true);
        pad.buttons = Buttons {
            b: true,
            r: true,
            ..Buttons::default()
        };
        pad.latch(true);
        assert_eq!(pad.data(), 1, "latched: B");
        pad.latch(false);
        let bits: Vec<u8> = (0..17).map(|_| pad.data()).collect();
        assert_eq!(bits[0], 1);
        assert_eq!(bits[11], 1, "R is the twelfth");
        assert_eq!(&bits[12..16], &[0, 0, 0, 0], "the standard pad's id");
        assert_eq!(bits[16], 1);
        assert_eq!(Pad::new(false).data(), 0);
    }
}
