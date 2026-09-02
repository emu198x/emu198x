//! Atari 800XL machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). Donor at `Emu198x-Oldest/crates/machine-atari-800xl/src/lib.rs`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; the donor is used here as the system spec for the
//! 800XL-specific PORTB-controlled ROM overlay, while the wiring is
//! written against [`emu198x_mos_6502::M6502`]'s public pin fields.
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

pub use cartridge::{Cartridge, CartridgeKind};

use atari_antic::{Antic, AnticRegion, COLOUR_CLOCKS_PER_LINE, CYCLES_HSYNC, cpu_dma_stalled};
use atari_gtia::Gtia;
use atari_pokey::Pokey;
use atari_sio::SioBus;
use emu198x_mos_6502::M6502;
use mos_pia_6520::Pia6520;
use serde::{Deserialize, Serialize};

/// Atari 800XL region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Atari800xlRegion {
    Ntsc,
    Pal,
}

impl Atari800xlRegion {
    fn gtia_region(self) -> atari_gtia::GtiaRegion {
        match self {
            Self::Ntsc => atari_gtia::GtiaRegion::Ntsc,
            Self::Pal => atari_gtia::GtiaRegion::Pal,
        }
    }

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

/// The address space ANTIC fetches through, borrowed from the machine's live
/// fields rather than copied.
///
/// ANTIC reads the display list, screen data, player/missile graphics and —
/// critically — the GR.0 character set through here. That character set lives
/// at `$E000` inside the OS ROM, so bare RAM would make every normal glyph
/// fetch read 0: only the inverse-video cursor cell (`!0 = $FF`) would paint.
/// Overlay precedence mirrors [`Atari800xl::mem_read`]: cart over BASIC at
/// `$A000-$BFFF`.
///
/// `$D000-$D7FF` reads as RAM. ANTIC on real hardware would drive the address
/// bus into the register page like anything else, but no display fetches data
/// from there and modelling the register reads would mean giving ANTIC a path
/// into chips it does not otherwise touch.
struct AnticView<'a> {
    ram: &'a [u8],
    os_rom: Option<&'a [u8]>,
    basic_rom: Option<&'a [u8]>,
    cart: Option<&'a Cartridge>,
    portb: u8,
}

impl atari_antic::AnticMemory for AnticView<'_> {
    fn read(&self, addr: u16) -> u8 {
        let os_on = self.portb & 0x01 != 0;
        let basic_on = self.portb & 0x02 == 0;
        let self_test = self.portb & 0x80 == 0;
        let os_byte = |offset: usize| {
            self.os_rom
                .map_or(0xFF, |os| os.get(offset).copied().unwrap_or(0xFF))
        };

        match addr {
            // Self-test RAM/ROM window, which maps to OS ROM $1000-$17FF.
            0x5000..=0x57FF if self_test && os_on && self.os_rom.is_some() => {
                os_byte(0x1000 + (addr - 0x5000) as usize)
            }
            0x8000..=0xBFFF => {
                if let Some(cart) = self.cart
                    && cart.covers(addr)
                {
                    return cart.read(addr);
                }
                if basic_on
                    && addr >= 0xA000
                    && let Some(basic) = self.basic_rom
                {
                    return basic.get((addr - 0xA000) as usize).copied().unwrap_or(0xFF);
                }
                self.ram[addr as usize]
            }
            0xC000..=0xCFFF if os_on && self.os_rom.is_some() => os_byte((addr - 0xC000) as usize),
            0xD800..=0xFFFF if os_on && self.os_rom.is_some() => os_byte((addr - 0xC000) as usize),
            _ => self.ram[addr as usize],
        }
    }
}

/// Atari 800XL machine.
#[derive(Serialize, Deserialize)]
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
    /// The SIO bus and the disk drives on it. The PIA's CB2 pin is the bus's
    /// command line, POKEY's serial port carries the bytes.
    sio: SioBus,
    /// Which of the current scan line's cycles ANTIC is taking for DMA, from
    /// its own fetch schedule. The CPU runs on the cycles left over.
    dma_mask: u128,
    /// CPU cycle counter within the current scan line, counting from 1.
    line_cycle: u16,
    /// The cycle at which ANTIC reads this line's playfield, once it has
    /// begun a line that has one.
    playfield_fetch_cycle: Option<u16>,
    /// The frame OPTION stops being held down for the OS. The XL OS decides
    /// whether BASIC is in from OPTION during its cold start and writes PORTB
    /// itself, so presetting PORTB is not enough to boot without BASIC: the
    /// key has to be down when the OS looks. AltirraOS reads CONSOL ten
    /// times before deciding and the Atari OS twice, so this is a period
    /// rather than a read count — about the second a person holds the key.
    /// A read from outside the OS ends the hold early so a program never
    /// sees a key nobody is pressing.
    option_held_until_frame: u64,
}

/// How long OPTION is held for the OS after a cold start with BASIC off.
const OPTION_HOLD_FRAMES: u64 = 60;

impl Atari800xl {
    /// Create a new Atari 800XL.
    ///
    /// `os_rom` should be 16 KB (covers `$C000-$FFFF` with a `$D000-$D7FF`
    /// I/O gap baked into the ROM). `basic_rom` is 8 KB. `cart` is a flat 8
    /// or 16 KB image, a banked image with its `CART` header, or a headerless
    /// 32 KB+ XEGS image (see [`Cartridge`]); a cart shadows BASIC at
    /// `$A000-$BFFF`. With no OS ROM, the reset vector is fetched from the
    /// cart entry point.
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
        // bit 7 = 1 self-test off, other bits high. This is what a boot
        // without an OS ROM runs with; the XL OS re-derives it from OPTION.
        let mut portb: u8 = 0xFF;
        if basic_enabled {
            portb &= !0x02;
        }
        pia.write(0x02, portb);
        let option_held_until_frame = if basic_enabled { 0 } else { OPTION_HOLD_FRAMES };

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
            gtia: Gtia::new(region.gtia_region()),
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
            sio: SioBus::new(),
            dma_mask: 0,
            line_cycle: 0,
            playfield_fetch_cycle: None,
            option_held_until_frame,
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
        let ccpl = u64::from(COLOUR_CLOCKS_PER_LINE);

        // At the left edge of a scan line, hand ANTIC the line and prime the
        // GTIA to composite it. Compositing then runs left-to-right through the
        // line (below), so a mid-line COLBK/COLPF write lands at the beam.
        if self.master_clock.is_multiple_of(ccpl) {
            self.start_scan_line();
        }

        self.master_clock += 1;

        // Advance playfield compositing to the beam's new position, sampling
        // the live colour registers as it goes.
        let line_cc = (self.master_clock % ccpl) as u16;
        self.gtia.composite_to_beam(line_cc);

        // When the line completes, overlay players/missiles + collisions.
        if self.master_clock.is_multiple_of(ccpl) {
            self.gtia.finish_scanline();
            // End of the NMI pulse; the CPU latched any rising edge as it
            // ticked through the line.
            self.cpu.nmi = false;
        }

        if self.master_clock.is_multiple_of(2) {
            // ANTIC releases a WSYNC-halted CPU at the start of horizontal
            // blank, not at the next line — so post-WSYNC writes land at the
            // right beam position.
            if self.line_cycle == CYCLES_HSYNC {
                self.antic.clear_wsync();
            }
            // CPU runs unless ANTIC is taking this cycle for a fetch, or it is
            // held by WSYNC.
            if !cpu_dma_stalled(self.line_cycle, self.dma_mask) && !self.antic.wsync_halt() {
                self.cpu.tick();
                if self.cpu.rw {
                    self.cpu.data_in = self.mem_read(self.cpu.addr);
                } else {
                    self.mem_write(self.cpu.addr, self.cpu.data);
                }
            }
            self.pokey.tick();
            self.tick_sio();
            self.cpu.irq = self.pokey.irq_pending() || self.pia.irq_pending();
            self.line_cycle += 1;
            if Some(self.line_cycle) == self.playfield_fetch_cycle {
                self.fetch_playfield();
            }
        }
    }

    /// ANTIC reads the line's playfield and hands it to the GTIA, ahead of
    /// the beam reaching it. Registers the CPU wrote earlier in the line —
    /// CHBASE, CHACTL, HSCROL — shape this line; later writes shape the next.
    fn fetch_playfield(&mut self) {
        let view = AnticView {
            ram: &self.ram,
            os_rom: self.os_rom.as_deref(),
            basic_rom: self.basic_rom.as_deref(),
            cart: self.cart.as_ref(),
            portb: self.pia.port_b_output() | !self.pia.ddr_b(),
        };
        if let Some(fetched) = self.antic.fetch_playfield(&view) {
            self.gtia
                .set_playfield(&fetched.playfield, fetched.playfield_width, fetched.mode);
        }
    }

    /// Carry one machine cycle of the SIO bus.
    ///
    /// The command line is the PIA's CB2 pin, active low, so the bus sees it
    /// asserted when the CPU drives that pin down. Bytes cross whole: whatever
    /// POKEY's output shift register has finished goes to the devices, and a
    /// device's reply goes into POKEY's input register when that is free.
    fn tick_sio(&mut self) {
        let command = self.pia.cb2_output().is_some_and(|level| !level);
        self.sio.set_command_line(command);
        self.sio.tick();
        if let Some(byte) = self.pokey.take_serial_output() {
            self.sio.send(byte);
        }
        if self.pokey.serial_input_idle()
            && let Some(byte) = self.sio.poll_response()
        {
            self.pokey.begin_serial_input(byte);
        }
    }

    /// The SIO bus, to put a disk in a drive.
    #[must_use]
    pub fn sio(&self) -> &SioBus {
        &self.sio
    }

    /// The SIO bus, mutably.
    pub fn sio_mut(&mut self) -> &mut SioBus {
        &mut self.sio
    }

    /// Start a scan line: ANTIC reads the display list and the GTIA begins
    /// beam compositing for it. Player/missile DMA and the DLI/VBI NMI are
    /// applied here, and the line's DMA schedule that gates the CPU is set.
    /// The playfield itself is fetched later in the line (`fetch_playfield`).
    /// The actual pixels are composited incrementally as the beam advances
    /// (`composite_to_beam`), then finished with the PM overlay at line end.
    fn start_scan_line(&mut self) {
        // Borrow the memory ANTIC fetches through directly from the fields it
        // covers, so `self.antic` can be borrowed mutably alongside it.
        let view = AnticView {
            ram: &self.ram,
            os_rom: self.os_rom.as_deref(),
            basic_rom: self.basic_rom.as_deref(),
            cart: self.cart.as_ref(),
            portb: self.pia.port_b_output() | !self.pia.ddr_b(),
        };
        let result = self.antic.begin_line(&view);
        if result.pm_dma {
            // GRACTL decides whether this DMA reaches the graphics registers,
            // and VDELAY whether an object is held back a line; both live in
            // GTIA, so hand the line over rather than poking the registers.
            self.gtia.accept_pm_dma(
                result.player_data,
                result.missile_data,
                result.pm_single_line,
            );
        }
        let line = self.antic.scan_line().saturating_sub(1);
        let visible_line = line.wrapping_sub(8);
        self.gtia.begin_scanline(visible_line);
        self.dma_mask = result.dma_mask;
        self.line_cycle = 0;
        self.playfield_fetch_cycle = self.antic.playfield_fetch_cycle();
        // ANTIC pulses NMI; it does not hold it. See the same wiring in
        // `machine-atari-5200`: holding the line high across two
        // consecutive lines merges a DLI on the last mode line with the
        // VBI on the line after into one edge, and the OS loses the VBI.
        if self.antic.take_vbi() | self.antic.take_dli() {
            self.cpu.nmi = true;
        }
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
            0xD000..=0xD0FF => {
                let value = self.gtia.read(addr as u8);
                if addr & 0x1F == 0x1F && self.frame_count < self.option_held_until_frame {
                    if self.cpu.regs.pc >= 0xC000 {
                        return value & !0x04;
                    }
                    self.option_held_until_frame = 0;
                }
                value
            }
            0xD100..=0xD1FF => 0xFF,
            0xD200..=0xD2FF => self.pokey.read(addr as u8),
            0xD300..=0xD3FF => self.pia.read(Self::bus_to_pia_addr(addr)),
            0xD400..=0xD4FF => self.antic.read(addr as u8),
            // The cartridge control select line: the cartridge decodes the
            // access itself, and nothing drives the data bus.
            0xD500..=0xD5FF => {
                if let Some(ref mut cart) = self.cart {
                    cart.cctl_access(addr);
                }
                0xFF
            }
            0xD600..=0xD7FF => 0xFF,
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
            0xD500..=0xD5FF => {
                if let Some(ref mut cart) = self.cart {
                    cart.cctl_write(addr, value);
                }
            }
            0xD600..=0xD7FF => {}
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

    /// Write one byte through the CPU-visible bus, as an Atari executable
    /// loader would. Unlike debugger `poke`, this preserves writes to the
    /// hardware-register page.
    pub fn load_program_byte(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Call a loaded 6502 subroutine and stop when it returns.
    ///
    /// The synthetic return address is deliberately in page one, matching
    /// Altirra's executable-loader sentinel. Returns whether the routine
    /// reached that sentinel before `max_ticks` elapsed.
    pub fn call_loaded_subroutine(&mut self, entry: u16, max_ticks: u64) -> bool {
        if !self.prepare_loaded_entry(max_ticks) {
            return false;
        }

        const RETURN_PC: u16 = 0x01FE;
        let return_address = RETURN_PC.wrapping_sub(1);
        let sp = self.cpu.regs.sp;
        self.mem_write(0x0100 | u16::from(sp), (return_address >> 8) as u8);
        self.mem_write(0x0100 | u16::from(sp.wrapping_sub(1)), return_address as u8);
        self.cpu.regs.sp = sp.wrapping_sub(2);
        self.cpu.regs.pc = entry;
        self.cpu.addr = entry;
        self.cpu.data_in = self.mem_read(entry);

        self.run_until_pc(RETURN_PC, max_ticks).1
    }

    /// Enter a loaded Atari executable using the register state established
    /// by the DOS loader before it jumps through RUNAD.
    pub fn launch_loaded_program(&mut self, entry: u16, max_ticks: u64) -> bool {
        if !self.prepare_loaded_entry(max_ticks) {
            return false;
        }
        self.cpu.regs.x = 0x20;
        self.cpu.regs.y = 0x03;
        self.cpu.regs.p = 0x03;
        self.cpu.regs.pc = entry;
        self.cpu.addr = entry;
        self.cpu.data_in = self.mem_read(entry);
        true
    }

    fn prepare_loaded_entry(&mut self, max_ticks: u64) -> bool {
        for _ in 0..max_ticks {
            if self.cpu.instruction_complete() && self.cpu.sync {
                return true;
            }
            self.tick_colour_clock();
        }
        self.cpu.instruction_complete() && self.cpu.sync
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

    /// A cart that changes COLBK twice per scan line: colour A from the line
    /// start, colour B from mid-line. With beam compositing this paints a
    /// horizontal split on every visible line — the whole-line renderer could
    /// only ever show the final colour (B) across the entire row.
    fn colbk_split_cart() -> Vec<u8> {
        let mut rom = vec![0xEAu8; 8192]; // NOP fill; entry at $A000
        let prog: [u8; 26] = [
            0xA9, 0x00, // $A000 LDA #$00
            0x8D, 0x0E, 0xD4, // $A002 STA $D40E  (NMIEN = 0: no DLI/VBI NMI)
            0x8D, 0x0A, 0xD4, // $A005 STA $D40A  (WSYNC — wait for line start)  [loop]
            0xA9, 0x0F, // $A008 LDA #$0F  (white)
            0x8D, 0x1A, 0xD0, // $A00A STA $D01A  (COLBK = A, near line start → left)
            0xA2, 0x08, // $A00D LDX #$08
            0xCA, // $A00F DEX                                            [wait]
            0xD0, 0xFD, // $A010 BNE $A00F  (burn cycles → beam advances mid-line)
            0xA9, 0x46, // $A012 LDA #$46  (red)
            0x8D, 0x1A, 0xD0, // $A014 STA $D01A  (COLBK = B, mid-line → right)
            0x4C, 0x05, 0xA0, // $A017 JMP $A005  (back to WSYNC)
        ];
        rom[..prog.len()].copy_from_slice(&prog);
        rom
    }

    #[test]
    fn mid_line_colbk_write_splits_the_scanline() {
        // Beam compositing made observable end-to-end: a program that rewrites
        // COLBK partway across each line must produce a horizontal split. The
        // old whole-line renderer sampled COLBK once at line end, so every row
        // was a single colour; a non-uniform active row proves the beam path.
        let mut sys = Atari800xl::new(
            None,
            None,
            Some(colbk_split_cart()),
            Atari800xlRegion::Ntsc,
            false,
        )
        .expect("init");
        for _ in 0..40 {
            sys.run_frame();
        }
        let fb = sys.framebuffer();
        let w = sys.framebuffer_width() as usize;
        // A visible line well inside the active region (active starts at
        // BORDER_TOP; pick row 100 of the 240 active lines).
        let row = atari_gtia::GtiaRegion::Pal.border_top() as usize + 100;
        let base = row * w + atari_gtia::GtiaRegion::Pal.border_left() as usize;
        let active: std::collections::BTreeSet<u32> = (0..atari_gtia::ACTIVE_WIDTH as usize)
            .map(|x| fb[base + x])
            .collect();
        assert!(
            active.len() >= 2,
            "scan line should show two background colours (a beam split), got {}",
            active.len()
        );
        // And the split runs left-to-right: a clearly-left pixel differs from a
        // clearly-right one.
        assert_ne!(
            fb[base + 10],
            fb[base + atari_gtia::ACTIVE_WIDTH as usize - 10],
            "left and right of the line should be different COLBK colours"
        );
    }

    /// A cart that puts one scrolled playfield on screen and nothing else.
    ///
    /// The display list is 24 blank lines, then 21 mode lines of `first`
    /// (which carries LMS) and `rest`, then JVB. Screen memory is cleared and
    /// `fill_len` bytes from `$3000 + fill_start` are set to `fill_value`, so
    /// the only lit pixels on the frame are the ones under test — whole-frame
    /// minimum x and maximum y then locate them without guessing at border
    /// geometry.
    fn scroll_cart(
        first: u8,
        rest: u8,
        hscrol: u8,
        vscrol: u8,
        fill_start: u8,
        fill_len: u8,
        fill_value: u8,
    ) -> Vec<u8> {
        let mut p: Vec<u8> = Vec::new();

        p.extend_from_slice(&[0xA9, 0x00, 0x8D, 0x0E, 0xD4]); // NMIEN = 0
        p.extend_from_slice(&[0xA9, 0x70]); // 8 blank lines
        for addr in 0x2000u16..=0x2002 {
            p.extend_from_slice(&[0x8D, addr as u8, (addr >> 8) as u8]);
        }
        p.extend_from_slice(&[0xA9, first, 0x8D, 0x03, 0x20]);
        p.extend_from_slice(&[0xA9, 0x00, 0x8D, 0x04, 0x20]); // LMS low
        p.extend_from_slice(&[0xA9, 0x30, 0x8D, 0x05, 0x20]); // LMS high → $3000

        p.push(0xA2); // LDX #$00
        p.push(0x00);
        let dl_loop = p.len();
        p.extend_from_slice(&[0xA9, rest]); // LDA #rest
        p.extend_from_slice(&[0x9D, 0x06, 0x20]); // STA $2006,X
        p.push(0xE8); // INX
        p.extend_from_slice(&[0xE0, 0x14]); // CPX #$14
        p.push(0xD0); // BNE dl_loop
        p.push(((dl_loop as isize - (p.len() as isize + 1)) as i8) as u8);

        p.extend_from_slice(&[0xA9, 0x41, 0x8D, 0x1A, 0x20]); // JVB
        p.extend_from_slice(&[0xA9, 0x00, 0x8D, 0x1B, 0x20]);
        p.extend_from_slice(&[0xA9, 0x20, 0x8D, 0x1C, 0x20]); // → $2000

        p.extend_from_slice(&[0xA9, 0x00, 0xA2, 0x00]); // clear screen memory
        let clear_loop = p.len();
        p.extend_from_slice(&[0x9D, 0x00, 0x30]); // STA $3000,X
        p.push(0xE8);
        p.extend_from_slice(&[0xE0, 0x40]); // CPX #$40
        p.push(0xD0);
        p.push(((clear_loop as isize - (p.len() as isize + 1)) as i8) as u8);

        p.extend_from_slice(&[0xA9, fill_value, 0xA2, 0x00]);
        let fill_loop = p.len();
        p.extend_from_slice(&[0x9D, fill_start, 0x30]); // STA $30xx,X
        p.push(0xE8);
        p.extend_from_slice(&[0xE0, fill_len]);
        p.push(0xD0);
        p.push(((fill_loop as isize - (p.len() as isize + 1)) as i8) as u8);

        p.extend_from_slice(&[0xA9, 0x0F, 0x8D, 0x16, 0xD0]); // COLPF0 = white
        p.extend_from_slice(&[0xA9, 0x0F, 0x8D, 0x17, 0xD0]); // COLPF1 = white
        p.extend_from_slice(&[0xA9, 0x00, 0x8D, 0x18, 0xD0]); // COLPF2 = black
        p.extend_from_slice(&[0xA9, 0x00, 0x8D, 0x1A, 0xD0]); // COLBK = black

        p.extend_from_slice(&[0xA9, hscrol, 0x8D, 0x04, 0xD4]);
        p.extend_from_slice(&[0xA9, vscrol, 0x8D, 0x05, 0xD4]);
        p.extend_from_slice(&[0xA9, 0x00, 0x8D, 0x02, 0xD4]); // DLISTL
        p.extend_from_slice(&[0xA9, 0x20, 0x8D, 0x03, 0xD4]); // DLISTH → $2000
        p.extend_from_slice(&[0xA9, 0x22, 0x8D, 0x00, 0xD4]); // DMACTL: normal + DL DMA

        let here = 0xA000u16 + p.len() as u16;
        p.extend_from_slice(&[0x4C, here as u8, (here >> 8) as u8]);

        let mut rom = vec![0xEAu8; 8192];
        rom[..p.len()].copy_from_slice(&p);
        rom
    }

    /// Every lit pixel on the frame, as (x, y).
    fn lit_pixels(sys: &Atari800xl) -> Vec<(usize, usize)> {
        let w = sys.framebuffer_width() as usize;
        let fb = sys.framebuffer();
        let black = fb[0];
        fb.iter()
            .enumerate()
            .filter(|&(_, &px)| px != black)
            .map(|(i, _)| (i % w, i / w))
            .collect()
    }

    fn run_scroll_cart(cart: Vec<u8>) -> Atari800xl {
        let mut sys =
            Atari800xl::new(None, None, Some(cart), Atari800xlRegion::Ntsc, false).expect("init");
        for _ in 0..3 {
            sys.run_frame();
        }
        sys
    }

    /// A cart that counts a tight loop into `$0600/$0601` behind a screenful
    /// of mode 2 lines at the requested playfield width.
    fn cpu_speed_cart(dmactl: u8) -> Vec<u8> {
        let mut p: Vec<u8> = Vec::new();

        p.extend_from_slice(&[0xA9, 0x00, 0x8D, 0x0E, 0xD4]); // NMIEN = 0
        p.extend_from_slice(&[0xA9, 0x42, 0x8D, 0x00, 0x20]); // mode 2 + LMS
        p.extend_from_slice(&[0xA9, 0x00, 0x8D, 0x01, 0x20]);
        p.extend_from_slice(&[0xA9, 0x30, 0x8D, 0x02, 0x20]); // screen $3000

        p.extend_from_slice(&[0xA2, 0x00]); // LDX #$00
        let dl_loop = p.len();
        p.extend_from_slice(&[0xA9, 0x02]); // LDA #$02 — mode 2
        p.extend_from_slice(&[0x9D, 0x03, 0x20]); // STA $2003,X
        p.push(0xE8);
        p.extend_from_slice(&[0xE0, 0x17]); // CPX #$17 — 23 more mode lines
        p.push(0xD0);
        p.push(((dl_loop as isize - (p.len() as isize + 1)) as i8) as u8);

        p.extend_from_slice(&[0xA9, 0x41, 0x8D, 0x1A, 0x20]); // JVB → $2000
        p.extend_from_slice(&[0xA9, 0x00, 0x8D, 0x1B, 0x20]);
        p.extend_from_slice(&[0xA9, 0x20, 0x8D, 0x1C, 0x20]);

        p.extend_from_slice(&[0xA9, 0x00, 0x8D, 0x00, 0x06]); // counter = 0
        p.extend_from_slice(&[0x8D, 0x01, 0x06]);
        p.extend_from_slice(&[0xA9, 0x00, 0x8D, 0x02, 0xD4]); // DLISTL
        p.extend_from_slice(&[0xA9, 0x20, 0x8D, 0x03, 0xD4]); // DLISTH → $2000
        p.extend_from_slice(&[0xA9, dmactl, 0x8D, 0x00, 0xD4]); // DMACTL

        let count_loop = 0xA000u16 + p.len() as u16;
        p.extend_from_slice(&[0xEE, 0x00, 0x06]); // INC $0600
        p.extend_from_slice(&[0xD0, 0x03]); // BNE +3
        p.extend_from_slice(&[0xEE, 0x01, 0x06]); // INC $0601
        p.extend_from_slice(&[0x4C, count_loop as u8, (count_loop >> 8) as u8]);

        let mut rom = vec![0xEAu8; 8192];
        rom[..p.len()].copy_from_slice(&p);
        rom
    }

    /// ANTIC's DMA reaches the CPU as the cycles it actually takes, so a wider
    /// playfield visibly slows the CPU down. Mode 2 fetches a character name
    /// every two cycles and its data two cycles after that, so a wide line
    /// leaves the CPU almost nothing while a narrow one leaves a third of the
    /// fetch window free.
    #[test]
    fn playfield_width_changes_how_many_cycles_the_cpu_gets() {
        let count = |dmactl| {
            let mut sys = Atari800xl::new(
                None,
                None,
                Some(cpu_speed_cart(dmactl)),
                Atari800xlRegion::Ntsc,
                false,
            )
            .expect("init");
            for _ in 0..4 {
                sys.run_frame();
            }
            u32::from(sys.ram[0x0600]) | u32::from(sys.ram[0x0601]) << 8
        };

        let narrow = count(0x21);
        let normal = count(0x22);
        let wide = count(0x23);

        assert!(
            narrow > normal && normal > wide,
            "each width step should cost the CPU cycles: narrow {narrow}, normal {normal}, wide {wide}"
        );
    }

    /// ANTIC fetches through the machine's live memory, so a program can
    /// build a display list and switch display DMA on in the same frame and
    /// see the result on that frame. ANTIC used to read a copy of RAM taken
    /// once at frame start: the display list was still zeros in that copy, so
    /// ANTIC walked blank instructions to the bottom of the screen, never
    /// reached the JVB that resets the pointer, and lost its place for good.
    #[test]
    fn a_display_list_built_this_frame_is_visible_on_this_frame() {
        let mut sys = Atari800xl::new(
            None,
            None,
            Some(scroll_cart(0x4F, 0x0F, 0, 0, 0x04, 0x01, 0xFF)),
            Atari800xlRegion::Ntsc,
            false,
        )
        .expect("init");

        sys.run_frame();

        assert!(
            !lit_pixels(&sys).is_empty(),
            "the first frame should already show the playfield"
        );
    }

    /// HSCROL wired all the way through: one lit byte of a scrolled mode F
    /// line moves right by exactly the register's value in colour clocks, two
    /// framebuffer pixels each.
    #[test]
    fn hscrol_moves_the_playfield_right_through_the_machine_bus() {
        let x_at = |hscrol| {
            // Mode F + LMS + HSCROL, then mode F + HSCROL. Screen byte 4 is
            // the only lit one, far enough in to stay visible at HSCROL 0.
            let sys = run_scroll_cart(scroll_cart(0x5F, 0x1F, hscrol, 0, 0x04, 0x01, 0xFF));
            let lit = lit_pixels(&sys);
            assert!(!lit.is_empty(), "HSCROL {hscrol} lit nothing");
            lit.iter().map(|&(x, _)| x).min().expect("lit")
        };

        let base = x_at(0);
        for hscrol in [1u8, 4, 9, 15] {
            assert_eq!(
                x_at(hscrol),
                base + 2 * usize::from(hscrol),
                "HSCROL {hscrol} should move the playfield {hscrol} colour clocks right"
            );
        }
    }

    /// VSCROL wired all the way through: the first mode line of a scrolling
    /// region starts partway down its rows, so it is that many scan lines
    /// shorter and everything below it moves up.
    #[test]
    fn vscrol_shortens_the_first_line_of_a_region_through_the_machine_bus() {
        let bottom_at = |vscrol| {
            // Mode 8 + LMS + VSCROL, then mode 8 + VSCROL. Mode 8 is eight
            // scan lines per row and ten bytes per line, so filling the first
            // ten lights exactly the first mode line.
            let sys = run_scroll_cart(scroll_cart(0x68, 0x28, 0, vscrol, 0x00, 0x0A, 0x55));
            let lit = lit_pixels(&sys);
            assert!(!lit.is_empty(), "VSCROL {vscrol} lit nothing");
            lit.iter().map(|&(_, y)| y).max().expect("lit")
        };

        let base = bottom_at(0);
        for vscrol in [1u8, 3, 7] {
            assert_eq!(
                bottom_at(vscrol),
                base - usize::from(vscrol),
                "VSCROL {vscrol} should shorten the region's first line by {vscrol} scan lines"
            );
        }
    }

    #[test]
    fn prior_playfield_front_scheme_is_wired_through_the_machine_bus() {
        let mut sys = Atari800xl::new(None, None, Some(trap_cart()), Atari800xlRegion::Ntsc, false)
            .expect("init");

        // Put player 0 over a PF0 pixel, configuring every GTIA register
        // through the 800XL's real $D000 bus window.
        sys.mem_write(0xD000, 60); // HPOSP0
        sys.mem_write(0xD00D, 0x80); // GRAFP0: leftmost bit
        sys.mem_write(0xD012, 0x38); // COLPM0
        sys.mem_write(0xD016, 0x94); // COLPF0
        let mut playfield = vec![0u8; 160];
        playfield[12] = 1;

        sys.gtia
            .render_line(0, &playfield, 160, atari_gtia::AnticMode::ModeD);
        sys.mem_write(0xD01B, 0x04); // PRIOR: all playfields over all players
        sys.gtia
            .render_line(1, &playfield, 160, atari_gtia::AnticMode::ModeD);

        let x = sys.gtia.border_left() as usize + ((60 - 48) * 2) as usize;
        let width = sys.framebuffer_width() as usize;
        let player_colour = atari_gtia::palette::NTSC_PALETTE[0x38];
        let playfield_colour = atari_gtia::palette::NTSC_PALETTE[0x94];
        assert_eq!(sys.framebuffer()[x], player_colour, "default PRIOR");
        assert_eq!(
            sys.framebuffer()[width + x],
            playfield_colour,
            "PRIOR=$04 should occlude player 0 with PF0"
        );
    }

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

    /// Minimal cartridge program that configures one POKEY tone through the
    /// 800XL's real `$D200` bus window, then spins.  This deliberately avoids
    /// reaching into the chip object from the test: the captured samples prove
    /// the CPU, machine bus, POKEY clocks, and host audio drain work together.
    fn pokey_tone_cart(audf1: u8, audf2: u8, audc_register: u8, audc: u8, audctl: u8) -> Vec<u8> {
        let mut rom = vec![0xEAu8; 8192];
        let prog = [
            0x78, // SEI
            0xA9,
            0x00, // LDA #0
            0x8D,
            0x0E,
            0xD4, // STA NMIEN
            0xA9,
            audf1, // LDA #AUDF1
            0x8D,
            0x00,
            0xD2, // STA AUDF1
            0xA9,
            audf2, // LDA #AUDF2
            0x8D,
            0x02,
            0xD2, // STA AUDF2
            0xA9,
            audc, // LDA #AUDC2
            0x8D,
            audc_register,
            0xD2, // STA AUDC1 or AUDC2
            0xA9,
            audctl, // LDA #AUDCTL
            0x8D,
            0x08,
            0xD2, // STA AUDCTL
            0x8D,
            0x09,
            0xD2, // STA STIMER (value ignored)
            0x4C,
            0x1D,
            0xA0, // JMP $A01D
        ];
        rom[..prog.len()].copy_from_slice(&prog);
        rom
    }

    fn captured_pokey_tone(
        audf1: u8,
        audf2: u8,
        audc_register: u8,
        audc: u8,
        audctl: u8,
    ) -> Vec<f32> {
        let mut sys = Atari800xl::new(
            None,
            None,
            Some(pokey_tone_cart(audf1, audf2, audc_register, audc, audctl)),
            Atari800xlRegion::Ntsc,
            false,
        )
        .expect("init");
        // Discard startup and DC-filter settling, then capture a stable window.
        for _ in 0..2 {
            sys.run_frame();
        }
        sys.take_audio_buffer();
        for _ in 0..8 {
            sys.run_frame();
        }
        sys.take_audio_buffer()
    }

    #[test]
    fn linked_pokey_channels_are_audible_at_the_reference_pitch() {
        // Atari800's reference POKEY implementation specifies the 1.79 MHz
        // linked divider as AUDF2*256 + AUDF1 + 7.  For $010A that is 273
        // source clocks per toggle, or about 3.278 kHz for the full wave.
        // AUDCTL $50: channel 1+2 link (D4) plus channel 1's fast clock (D6).
        // This read $12 while `atari-pokey` had six of the eight AUDCTL bits
        // transposed — a value that asks the real chip for the link and
        // channel 2's high-pass filter, and never reaches 1.79 MHz at all.
        let samples = captured_pokey_tone(10, 1, 0x03, 0xAF, 0x50);
        let crossings = samples
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count();
        let measured_hz = crossings as f32 * 48_000.0 / samples.len() as f32;
        let reference_hz = 1_789_772.0 / (2.0 * 273.0);
        assert!(
            (measured_hz - reference_hz).abs() < 35.0,
            "linked POKEY tone measured {measured_hz:.1} Hz, expected {reference_hz:.1} Hz"
        );
    }

    #[test]
    fn pokey_distortions_have_distinct_audible_signatures() {
        fn rms(samples: &[f32]) -> f32 {
            (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32)
                .sqrt()
        }

        let poly5 = rms(&captured_pokey_tone(31, 0, 0x01, 0x2F, 0x02));
        let poly5_and_poly4 = rms(&captured_pokey_tone(31, 0, 0x01, 0x4F, 0x02));
        let poly4 = rms(&captured_pokey_tone(31, 0, 0x01, 0xCF, 0x02));
        let pure = rms(&captured_pokey_tone(31, 0, 0x01, 0xEF, 0x02));
        // Atari800's gate ordering predicts fewer transitions for P5&P4 than
        // P5 alone, while P4 noise remains clearly distinct from an ungated
        // pure tone.  Generous margins tolerate downsampling/filter changes
        // but reject the former swapped/ungated distortion implementations.
        assert!(
            poly5 > poly5_and_poly4 * 1.15,
            "P5 ({poly5}) should be audibly stronger than P5&P4 ({poly5_and_poly4})"
        );
        assert!(
            poly4 < pure * 0.85,
            "P4 ({poly4}) should remain gated relative to pure tone ({pure})"
        );
    }

    /// Save-state must capture LIVE machine state (6502C + ANTIC + GTIA +
    /// POKEY + PIA + 64 KB RAM), not cold-boot from ROM. Serialise, advance
    /// (so the state differs), then deserialise the first snapshot and confirm
    /// re-serialising it is byte-identical — every stateful field across all
    /// chips round-trips, including the 64 KB RAM and the cartridge.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = Atari800xl::new(None, None, Some(trap_cart()), Atari800xlRegion::Ntsc, false)
            .expect("init");
        sys.run_frame();
        sys.poke(0x0600, 0xA5); // a low work-RAM byte to carry across the snapshot
        assert_eq!(sys.peek(0x0600), 0xA5, "poke landed in RAM");
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: Atari800xl = postcard::from_bytes(&s1).expect("decode snapshot");
        assert_eq!(
            restored.peek(0x0600),
            0xA5,
            "poked RAM byte survives restore"
        );
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
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
    fn accepts_up_to_8k_cart_but_rejects_odd_sizes() {
        // Any 1..=8192-byte image is accepted as an 8 KB cartridge, so 4097
        // bytes is fine; a size no scheme uses (here 24 KB) is rejected.
        let small = vec![0u8; 4097];
        assert!(Atari800xl::new(None, None, Some(small), Atari800xlRegion::Ntsc, false).is_ok());
        let odd = vec![0u8; 24576];
        assert!(Atari800xl::new(None, None, Some(odd), Atari800xlRegion::Ntsc, false).is_err());
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
