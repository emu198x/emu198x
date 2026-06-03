//! Atari 800XL machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). Donor at `Emu198x-Oldest/crates/machine-atari-800xl/src/lib.rs`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; the donor is used here as the system spec for the
//! 800XL-specific PORTB-controlled ROM overlay, while the wiring is
//! written against [`mos_6502::M6502`]'s public pin fields.
//!
//! Scope of this initial port: the **800XL** model — 64 KB RAM with
//! XL-style PORTB ROM banking (OS ROM, BASIC ROM, self-test). The 400 /
//! 800 (no XL banking) and 130XE (extended bank-switched RAM) variants
//! are deliberately deferred to follow-ups so this slice stays atomic.
//!
//! # The Atari 800XL
//!
//! Released in 1983 as Atari's mass-market 8-bit home computer (a slim
//! cost-reduction of the earlier 400 / 800). Shares the ANTIC + GTIA +
//! POKEY chip family with the 5200 console; what differs is the **PIA
//! 6520 at `$D300`** plus the 64 KB RAM with PORTB-controlled ROM
//! overlays: OS ROM at `$C000-$FFFF` (with the `$D000-$D7FF` I/O gap),
//! BASIC ROM at `$A000-$BFFF`, plus an optional self-test ROM at
//! `$5000-$57FF`. Cartridges in the BASIC window shadow BASIC.
//!
//! - **CPU:** MOS 6502C "Sally" (stock 6502 with Atari's HALT pin)
//! - **ANTIC:** display-list processor + DMA controller
//! - **GTIA:** video output + player/missile graphics
//! - **POKEY:** 4-channel audio + serial I/O + paddle pot scanner
//! - **PIA 6520:** joystick + console keys input + PORTB ROM-banking
//!   control
//! - **RAM:** 64 KB
//!
//! # PORTB ROM banking (XL series)
//!
//! Bit 0: OS ROM enabled (1 = ROM, 0 = RAM).
//! Bit 1: BASIC ROM enabled (0 = ROM, 1 = RAM).
//! Bit 7: Self-test ROM at `$5000-$57FF` (0 = ROM, 1 = RAM).
//!
//! # Clock model
//!
//! Master clock = colour clock (3.58 MHz NTSC / 3.55 MHz PAL). CPU +
//! POKEY tick every 2nd colour clock = 1.79 MHz NTSC. ANTIC processes
//! one scan line at every 228-clock boundary and stalls the CPU for
//! its DMA budget.

mod cartridge;

pub use cartridge::Cartridge;

use atari_antic::{Antic, AnticRegion, COLOUR_CLOCKS_PER_LINE};
use atari_gtia::Gtia;
use atari_pokey::Pokey;
use mos_6502::M6502;
use mos_pia_6520::Pia6520;

/// Atari 800XL region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atari800xlRegion {
    Ntsc,
    Pal,
}

impl Atari800xlRegion {
    fn antic_region(self) -> AnticRegion {
        match self {
            Self::Ntsc => AnticRegion::Ntsc,
            Self::Pal => AnticRegion::Pal,
        }
    }

    fn cpu_hz(self) -> u32 {
        match self {
            Self::Ntsc => 1_789_772,
            Self::Pal => 1_773_447,
        }
    }

    fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 262,
            Self::Pal => 312,
        }
    }
}

/// Atari 800XL machine.
pub struct Atari800xl {
    cpu: M6502,
    antic: Antic,
    gtia: Gtia,
    pokey: Pokey,
    pia: Pia6520,
    ram: Vec<u8>,
    os_rom: Option<Vec<u8>>,
    basic_rom: Option<Vec<u8>>,
    cart: Option<Cartridge>,
    region: Atari800xlRegion,
    master_clock: u64,
    clocks_per_frame: u64,
    frame_count: u64,
    dma_budget: u8,
    line_cycle: u16,
}

impl Atari800xl {
    /// Create a new Atari 800XL.
    ///
    /// `os_rom` should be 16 KB (covers `$C000-$FFFF` with a `$D000-$D7FF`
    /// I/O gap baked into the ROM). `basic_rom` is 8 KB. `cart` may be 8 or
    /// 16 KB; an 8 KB cart shadows BASIC at `$A000-$BFFF`. With no OS ROM,
    /// the reset vector is fetched from the cart entry point.
    pub fn new(
        os_rom: Option<Vec<u8>>,
        basic_rom: Option<Vec<u8>>,
        cart: Option<Vec<u8>>,
        region: Atari800xlRegion,
        basic_enabled: bool,
    ) -> Result<Self, String> {
        let cart = match cart {
            Some(rom) => Some(Cartridge::from_rom(&rom)?),
            None => None,
        };
        let mut cpu = M6502::new();
        cpu.reset();
        let mut pokey = Pokey::new(region.cpu_hz());
        pokey.set_pot(0, 114);
        pokey.set_pot(1, 114);
        let mut pia = Pia6520::new();
        // CRB bit 2 = 0 → next $D302 write addresses DDR_B.
        pia.write(0x02, 0xFF);
        // CRB bit 2 = 1 → future $D302 writes hit data register.
        pia.write(0x03, 0x04);
        // PORTB: bit 0 = 1 OS ROM on, bit 1 = 0 BASIC on (or 1 off),
        // bit 7 = 1 self-test off, other bits high.
        let mut portb: u8 = 0xFF;
        if basic_enabled {
            portb &= !0x02;
        }
        pia.write(0x02, portb);

        let mut ram = vec![0u8; 65536];

        // With no OS ROM, fake a reset vector pointing at the cart entry.
        if os_rom.is_none()
            && let Some(ref c) = cart
        {
            let base = c.base();
            ram[0xFFFC] = (base & 0xFF) as u8;
            ram[0xFFFD] = (base >> 8) as u8;
            ram[0x0000] = 0x40;
            ram[0xFFFA] = 0x00;
            ram[0xFFFB] = 0x00;
            ram[0xFFFE] = 0x00;
            ram[0xFFFF] = 0x00;
        }

        let clocks_per_frame =
            u64::from(region.lines_per_frame()) * u64::from(COLOUR_CLOCKS_PER_LINE);

        let mut sys = Self {
            cpu,
            antic: Antic::new(region.antic_region()),
            gtia: Gtia::new(),
            pokey,
            pia,
            ram,
            os_rom,
            basic_rom,
            cart,
            region,
            master_clock: 0,
            clocks_per_frame,
            frame_count: 0,
            dma_budget: 0,
            line_cycle: 0,
        };

        // Prime CPU's PC from reset vector via our memory map.
        let lo = sys.mem_read(0xFFFC);
        let hi = sys.mem_read(0xFFFD);
        sys.cpu.regs.pc = u16::from(lo) | (u16::from(hi) << 8);

        Ok(sys)
    }

    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        let target = start + self.clocks_per_frame;
        // Paint the canonical TV-visible border (COLBK) at frame start.
        self.gtia.fill_border();
        while self.master_clock < target {
            self.tick_colour_clock();
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    /// Run until the CPU is about to execute `target_pc`, or until `max_ticks`
    /// colour clocks elapse — for a debugger / MCP `run_until_pc`. Returns the
    /// number of colour clocks actually run and whether the target was hit.
    ///
    /// Steps a whole CPU instruction at a time and checks PC after each, so
    /// the starting position never counts as an immediate hit and the target
    /// is recognised at the instruction boundary where it becomes the next
    /// instruction to fetch. Frame rendering and the TV-border fill continue
    /// as in `run_frame`, so a screenshot taken afterwards is current.
    pub fn run_until_pc(&mut self, target_pc: u16, max_ticks: u64) -> (u64, bool) {
        let start = self.master_clock;
        while self.master_clock - start < max_ticks {
            // Leave the current instruction-complete state (begin the next
            // instruction's fetch), then run until it completes again.
            while self.cpu.instruction_complete() && self.master_clock - start < max_ticks {
                self.tick_render(start);
            }
            while !self.cpu.instruction_complete() && self.master_clock - start < max_ticks {
                self.tick_render(start);
            }
            if self.cpu.regs.pc == target_pc {
                return (self.master_clock - start, true);
            }
        }
        (self.master_clock - start, false)
    }

    /// One colour clock, repainting the TV border at frame boundaries (as
    /// `run_frame` does) so stepped runs still render.
    fn tick_render(&mut self, start_clock: u64) {
        if (self.master_clock - start_clock).is_multiple_of(self.clocks_per_frame) {
            self.gtia.fill_border();
        }
        self.tick_colour_clock();
    }

    fn tick_colour_clock(&mut self) {
        self.master_clock += 1;

        if self
            .master_clock
            .is_multiple_of(u64::from(COLOUR_CLOCKS_PER_LINE))
        {
            self.process_scan_line();
        }

        if self.master_clock.is_multiple_of(2) {
            self.line_cycle += 1;
            if self.line_cycle > u16::from(self.dma_budget) && !self.antic.wsync_halt() {
                self.cpu.tick();
                if self.cpu.rw {
                    self.cpu.data_in = self.mem_read(self.cpu.addr);
                } else {
                    self.mem_write(self.cpu.addr, self.cpu.data);
                }
            }
            self.pokey.tick();
            self.cpu.irq = self.pokey.irq_pending() || self.pia.irq_pending();
        }
    }

    fn process_scan_line(&mut self) {
        let result = self.antic.process_line(&self.ram);
        if result.pm_dma {
            for i in 0..4 {
                self.gtia.write(0x0D + i as u8, result.player_data[i]);
            }
            self.gtia.write(0x11, result.missile_data);
        }
        let line = self.antic.scan_line().saturating_sub(1);
        let visible_line = line.wrapping_sub(8);
        self.gtia.render_line(
            visible_line,
            &result.playfield,
            result.playfield_width,
            result.mode,
        );
        self.dma_budget = result.dma_cycles;
        self.line_cycle = 0;
        self.antic.clear_wsync();
        self.cpu.nmi = self.antic.take_vbi() || self.antic.take_dli();
    }

    fn effective_portb(&self) -> u8 {
        self.pia.port_b_output() | !self.pia.ddr_b()
    }

    /// Translate a CPU bus address ($D300/01/02/03) to the PIA's RS pins.
    /// The Atari board cross-wires CPU A0↔A1 into PIA RS0/RS1, so:
    ///
    /// | bus addr | bus bits A1 A0 | PIA RS1 RS0 | register |
    /// |----------|----------------|-------------|----------|
    /// | $D300    | 0 0            | 0 0         | PORTA    |
    /// | $D301    | 0 1            | 1 0         | PORTB    |
    /// | $D302    | 1 0            | 0 1         | CRA      |
    /// | $D303    | 1 1            | 1 1         | CRB      |
    ///
    /// Our `Pia6520` follows the raw MOS 6520 datasheet layout —
    /// `addr 0/1/2/3 = PORTA / CRA / PORTB / CRB` — so we swap A0 and A1
    /// here to match the OS's expectations.
    const fn bus_to_pia_addr(addr: u16) -> u8 {
        let bus = (addr & 0x03) as u8;
        ((bus & 0x01) << 1) | ((bus >> 1) & 0x01)
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        let portb = self.effective_portb();
        let os_on = portb & 0x01 != 0;
        let basic_on = portb & 0x02 == 0;
        let self_test = portb & 0x80 == 0;

        match addr {
            0x0000..=0x3FFF => self.ram[addr as usize],
            0x4000..=0x4FFF => self.ram[addr as usize],
            0x5000..=0x57FF => {
                if self_test
                    && os_on
                    && let Some(ref os) = self.os_rom
                {
                    let offset = (addr - 0x5000 + 0x1000) as usize;
                    return os.get(offset).copied().unwrap_or(0xFF);
                }
                self.ram[addr as usize]
            }
            0x5800..=0x7FFF => self.ram[addr as usize],
            0x8000..=0x9FFF => {
                if let Some(ref cart) = self.cart
                    && cart.covers(addr)
                {
                    return cart.read(addr);
                }
                self.ram[addr as usize]
            }
            0xA000..=0xBFFF => {
                if let Some(ref cart) = self.cart
                    && cart.covers(addr)
                {
                    return cart.read(addr);
                }
                if basic_on && let Some(ref basic) = self.basic_rom {
                    let offset = (addr - 0xA000) as usize;
                    return basic.get(offset).copied().unwrap_or(0xFF);
                }
                self.ram[addr as usize]
            }
            0xC000..=0xCFFF => {
                if os_on && let Some(ref os) = self.os_rom {
                    let offset = (addr - 0xC000) as usize;
                    return os.get(offset).copied().unwrap_or(0xFF);
                }
                self.ram[addr as usize]
            }
            0xD000..=0xD0FF => self.gtia.read(addr as u8),
            0xD100..=0xD1FF => 0xFF,
            0xD200..=0xD2FF => self.pokey.read(addr as u8),
            0xD300..=0xD3FF => self.pia.read(Self::bus_to_pia_addr(addr)),
            0xD400..=0xD4FF => self.antic.read(addr as u8),
            0xD500..=0xD7FF => 0xFF,
            0xD800..=0xFFFF => {
                if os_on && let Some(ref os) = self.os_rom {
                    let offset = (addr - 0xC000) as usize;
                    return os.get(offset).copied().unwrap_or(0xFF);
                }
                self.ram[addr as usize]
            }
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0xCFFF => self.ram[addr as usize] = value,
            0xD000..=0xD0FF => self.gtia.write(addr as u8, value),
            0xD100..=0xD1FF => {}
            0xD200..=0xD2FF => self.pokey.write(addr as u8, value),
            0xD300..=0xD3FF => self.pia.write(Self::bus_to_pia_addr(addr), value),
            0xD400..=0xD4FF => self.antic.write(addr as u8, value),
            0xD500..=0xD7FF => {}
            // Writes under OS ROM go to underlying RAM.
            0xD800..=0xFFFF => self.ram[addr as usize] = value,
        }
    }

    /// Read a byte as the CPU would see it through the current PORTB banking,
    /// without side effects — for debugger / MCP `memory_read`. RAM, OS ROM,
    /// BASIC ROM, self-test ROM and cartridge windows resolve exactly as the
    /// bus does. The `$D000-$D7FF` hardware-register page returns `$FF`
    /// (open bus) rather than reading the chips, since a real register read
    /// has side effects (clearing collisions, latching status, etc.) — use
    /// the `query_*` chip tools to inspect those.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        let portb = self.effective_portb();
        let os_on = portb & 0x01 != 0;
        let basic_on = portb & 0x02 == 0;
        let self_test = portb & 0x80 == 0;

        match addr {
            0x5000..=0x57FF if self_test && os_on => self
                .os_rom
                .as_ref()
                .and_then(|os| os.get((addr - 0x5000 + 0x1000) as usize).copied())
                .unwrap_or(0xFF),
            0x8000..=0xBFFF if self.cart.as_ref().is_some_and(|c| c.covers(addr)) => {
                self.cart.as_ref().map_or(0xFF, |c| c.read(addr))
            }
            0xA000..=0xBFFF if basic_on => self
                .basic_rom
                .as_ref()
                .and_then(|b| b.get((addr - 0xA000) as usize).copied())
                .unwrap_or(0xFF),
            0xC000..=0xCFFF | 0xD800..=0xFFFF if os_on => self
                .os_rom
                .as_ref()
                .and_then(|os| os.get((addr - 0xC000) as usize).copied())
                .unwrap_or(0xFF),
            0xD000..=0xD7FF => 0xFF,
            _ => self.ram[addr as usize],
        }
    }

    /// Write a byte into the underlying RAM — for debugger / MCP `poke`.
    /// Mirrors the bus: writes to the `$0000-$CFFF` and `$D800-$FFFF` ranges
    /// land in RAM even where ROM is banked over them (the ROM is read-only,
    /// the RAM beneath it still takes the write). Writes to the `$D000-$D7FF`
    /// register page are ignored here — drive chips through their own tools.
    pub fn poke(&mut self, addr: u16, value: u8) {
        if !(0xD000..=0xD7FF).contains(&addr) {
            self.ram[addr as usize] = value;
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.gtia.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.gtia.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.gtia.framebuffer_height()
    }

    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.pokey.take_buffer()
    }

    /// Set joystick direction via PIA PORTA (active-low bits 0-3).
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn set_joystick(&mut self, up: bool, down: bool, left: bool, right: bool) {
        let mut value: u8 = 0xFF;
        if up {
            value &= !0x01;
        }
        if down {
            value &= !0x02;
        }
        if left {
            value &= !0x04;
        }
        if right {
            value &= !0x08;
        }
        self.pia.set_port_a_input(value);
    }

    /// Set fire button (GTIA TRIG0).
    pub fn set_fire(&mut self, pressed: bool) {
        self.gtia.set_trigger(0, pressed);
    }

    /// Set console keys (START / SELECT / OPTION) via GTIA CONSOL.
    pub fn set_console_keys(&mut self, start: bool, select: bool, option: bool) {
        let mut consol: u8 = 0x07;
        if start {
            consol &= !0x01;
        }
        if select {
            consol &= !0x02;
        }
        if option {
            consol &= !0x04;
        }
        self.gtia.set_console_switches(consol);
    }

    /// Press a keyboard key. `scancode` is the POKEY keyboard code (bits 0-5),
    /// with bit 6 = Ctrl and bit 7 = Shift. Raises the keyboard interrupt; the
    /// OS handler reads KBCODE and converts it to ATASCII in `CH` ($02FC).
    pub fn press_key(&mut self, scancode: u8) {
        self.pokey.press_key(scancode);
    }

    /// Release the currently held keyboard key (clears the POKEY "key down"
    /// status so the OS stops auto-repeating and accepts the next key).
    pub fn release_key(&mut self) {
        self.pokey.release_key();
    }

    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }
    #[must_use]
    pub fn antic(&self) -> &Antic {
        &self.antic
    }
    #[must_use]
    pub fn gtia(&self) -> &Gtia {
        &self.gtia
    }
    #[must_use]
    pub fn pokey(&self) -> &Pokey {
        &self.pokey
    }
    #[must_use]
    pub fn pia(&self) -> &Pia6520 {
        &self.pia
    }
    /// Raw RAM, for diagnostics. The 64KB array is fully resident; OS/ROM
    /// banking shadows portions of it through `mem_read`, but this view
    /// returns the underlying bytes regardless of which bank is selected.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }
    #[must_use]
    pub fn region(&self) -> Atari800xlRegion {
        self.region
    }
    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }
    #[must_use]
    pub fn clocks_per_frame(&self) -> u64 {
        self.clocks_per_frame
    }
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

impl Atari800xl {
    /// Run exactly one whole 6502C instruction, returning the colour clocks
    /// it consumed. A safety cap prevents an unbounded spin.
    pub fn step_instruction(&mut self) -> u64 {
        let mut ticks = 0u64;
        while self.cpu.instruction_complete() && ticks < 4096 {
            self.tick_colour_clock();
            ticks += 1;
        }
        while !self.cpu.instruction_complete() && ticks < 4096 {
            self.tick_colour_clock();
            ticks += 1;
        }
        ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_cart() -> Vec<u8> {
        let mut rom = vec![0xEAu8; 8192];
        rom[0x0000] = 0x4C;
        rom[0x0001] = 0x00;
        rom[0x0002] = 0xA0;
        rom[0x1FFC] = 0x00;
        rom[0x1FFD] = 0xA0;
        rom[0x1FFE] = 0x00;
        rom[0x1FFF] = 0xA0;
        rom
    }

    #[test]
    fn frame_advances_master_clock_and_count() {
        let mut sys = Atari800xl::new(None, None, Some(trap_cart()), Atari800xlRegion::Ntsc, false)
            .expect("init");
        let clocks = sys.run_frame();
        assert_eq!(clocks, 228 * 262);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_runs_more_clocks_than_ntsc() {
        let mut ntsc =
            Atari800xl::new(None, None, Some(trap_cart()), Atari800xlRegion::Ntsc, false)
                .expect("init");
        let mut pal = Atari800xl::new(None, None, Some(trap_cart()), Atari800xlRegion::Pal, false)
            .expect("init");
        assert!(pal.run_frame() > ntsc.run_frame());
    }

    #[test]
    fn cpu_starts_at_cart_entry_without_os_rom() {
        let sys = Atari800xl::new(None, None, Some(trap_cart()), Atari800xlRegion::Ntsc, false)
            .expect("init");
        assert_eq!(sys.cpu().regs.pc, 0xA000);
    }

    #[test]
    fn os_rom_visible_at_reset_vector() {
        let mut os = vec![0u8; 16384];
        os[0x3FFC] = 0x00;
        os[0x3FFD] = 0xE0;
        let sys =
            Atari800xl::new(Some(os), None, None, Atari800xlRegion::Ntsc, false).expect("init");
        assert_eq!(sys.cpu().regs.pc, 0xE000);
    }

    #[test]
    fn basic_rom_visible_when_enabled() {
        let basic = vec![0xAA_u8; 8192];
        let mut sys =
            Atari800xl::new(None, Some(basic), None, Atari800xlRegion::Ntsc, true).expect("init");
        assert_eq!(sys.mem_read(0xA000), 0xAA);
    }

    #[test]
    fn basic_rom_hidden_when_disabled() {
        let basic = vec![0xAA_u8; 8192];
        let mut sys =
            Atari800xl::new(None, Some(basic), None, Atari800xlRegion::Ntsc, false).expect("init");
        assert_eq!(sys.mem_read(0xA000), 0x00);
    }

    #[test]
    fn cartridge_overrides_basic() {
        let basic = vec![0xAA_u8; 8192];
        let mut cart = vec![0xCC_u8; 8192];
        cart[0x1FFC] = 0x00;
        cart[0x1FFD] = 0xA0;
        let mut sys = Atari800xl::new(None, Some(basic), Some(cart), Atari800xlRegion::Ntsc, true)
            .expect("init");
        assert_eq!(sys.mem_read(0xA000), 0xCC);
    }

    #[test]
    fn write_under_os_rom_goes_to_ram_but_read_returns_rom() {
        let os = vec![0xBB_u8; 16384];
        let mut sys =
            Atari800xl::new(Some(os), None, None, Atari800xlRegion::Ntsc, false).expect("init");
        sys.mem_write(0xC000, 0x42);
        assert_eq!(sys.ram[0xC000], 0x42);
        assert_eq!(sys.mem_read(0xC000), 0xBB);
    }

    #[test]
    fn rejects_invalid_rom_size() {
        let bad = vec![0u8; 4097];
        assert!(Atari800xl::new(None, None, Some(bad), Atari800xlRegion::Ntsc, false).is_ok());
        let bad = vec![0u8; 32768];
        assert!(Atari800xl::new(None, None, Some(bad), Atari800xlRegion::Ntsc, false).is_err());
    }

    #[test]
    fn joystick_drives_pia_port_a() {
        let mut sys = Atari800xl::new(None, None, Some(trap_cart()), Atari800xlRegion::Ntsc, false)
            .expect("init");
        sys.set_joystick(true, false, false, false);
        assert_eq!(sys.pia.input_a & 0x01, 0);
        sys.set_joystick(false, false, false, false);
        assert_eq!(sys.pia.input_a & 0x0F, 0x0F);
    }
}
