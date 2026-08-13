//! Memotech MTX500 / MTX512 (1983) — Z80A + TMS9918A + SN76489.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). Donor at `Emu198x-Oldest/crates/machine-memotech-mtx/`
//! used the deprecated `emu_core::Bus` callback; the wiring here goes
//! through [`zilog_z80::Z80`]'s public pin fields and `bus_request()`
//! collapse.
//!
//! # The Memotech MTX
//:
//! UK-built Z80A home computer from Memotech (1983) — aluminium case,
//! pro-grade keyboard, MTX BASIC + Noddy + SuperPascal ROMs.
//! Critically respected; commercially overshadowed by the cheaper
//! Spectrum.
//!
//! - **CPU:** Zilog Z80A at 4 MHz
//! - **VDP:** TI TMS9918A (PAL), interrupt drives Z80 INT
//! - **PSG:** TI SN76489 at 4 MHz (internal ÷16 to 250 kHz)
//! - **RAM:** 32 KB (MTX500) or 64 KB (MTX512)
//! - **ROM:** 8 KB OS plus paged 8 KB ROMs at `$2000-$3FFF` — a stock
//!   machine has BASIC (subpage 0) and ASSEM (subpage 1); the cold-start
//!   path calls into ASSEM, so OS+BASIC alone stops at the first such call.
//!
//! # Memory paging (port `$00`)
//!
//! The paging byte is `RELCPMH(b7) | ROM-subpage(b4-6) | RAM-page P(b0-3)`.
//! In **normal mode** (`RELCPMH=0`) the OS ROM is fixed at `$0000-$1FFF` and
//! BASIC at `$2000-$3FFF` (ROM subpage 0); 16 KB RAM blocks page through
//! `$4000-$7FFF` (block `2P+2`) and `$8000-$BFFF` (block `2P+1`), with
//! `$C000-$FFFF` fixed to block 0. In **CP/M mode** (`RELCPMH=1`) the whole
//! space is RAM. Modelled on MEMU's `mem_set_iobyte` (`src/memu/mem.c`);
//! see [`Mtx::resolve`]. The donor emulator's "bit 0 = page 0 RAM" reading
//! was wrong — it paged the executing OS ROM out during the boot RAM test.
//!
//! # I/O ports
//!
//! | Port      | Direction  | Function                                     |
//! |-----------|------------|----------------------------------------------|
//! | `$00`     | read/write | Paging byte (W) / Centronics status, no printer (R) |
//! | `$01`     | read/write | VDP data                                     |
//! | `$02`     | read/write | VDP status (R) / register (W)                |
//! | `$03`     | read/write | `snd_in3` = `0x03` (R) / cassette out, not fitted (W) |
//! | `$05`     | read/write | Keyboard column drive (W) / sense low (R)    |
//! | `$06`     | read/write | SN76489 PSG (W) / keyboard sense high + country code (R) |
//! | `$08-$0B` | read/write | Z80 CTC (VDP `/INT` → channel 0)             |

pub mod input;
mod keyboard;

pub use input::MtxKey;
pub use keyboard::KeyboardState;
pub use ti_tms9918::Tms9918;

use serde::{Deserialize, Serialize};
use ti_sn76489::{NoiseLfsr, Sn76489};
use ti_tms9918::VdpRegion;
use zilog_z80::z80::{BusOp, Z80};
use zilog_z80_ctc::Ctc;

const VDP_CLOCK_HZ: u64 = 5_369_318;
const CPU_CLOCK_HZ: u64 = 4_000_000;

/// CTC channel whose `CLK/TRG` input the TMS9918A `/INT` line feeds. MEMU's
/// `memu.c` routes the VDP frame interrupt through `ctc_trigger(0)` — channel 0 —
/// and the OS programs that channel (and the rest) at ports `$08-$0B`.
const VDP_INT_CTC_CHANNEL: u8 = 0;

/// MTX model selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MtxModel {
    /// 32 KB RAM.
    Mtx500,
    /// 64 KB RAM.
    Mtx512,
}

impl MtxModel {
    /// Number of 16 KB RAM blocks. MEMU calls these "blocks"; the MTX512's
    /// four are the RAM pages α–δ. MTX500 has two.
    fn blocks(self) -> usize {
        match self {
            Self::Mtx500 => 2,
            Self::Mtx512 => 4,
        }
    }

    fn ram_size(self) -> usize {
        self.blocks() * 0x4000
    }
}

/// Memotech MTX machine.
#[derive(Serialize, Deserialize)]
pub struct Mtx {
    cpu: Z80,
    rom: Vec<u8>,
    /// Number of 8 KB paged-ROM subpages at `$2000-$3FFF` after the OS:
    /// 1 = BASIC only, 2 = BASIC + ASSEM, …
    rom_subpages: usize,
    ram: Vec<u8>,
    blocks: usize,
    page_reg: u8,
    vdp: Tms9918,
    psg: Sn76489,
    /// Z80 CTC at ports `$08-$0B`. The VDP `/INT` line drives channel
    /// [`VDP_INT_CTC_CHANNEL`]'s `CLK/TRG`; the CTC's own INT output (not the raw
    /// VDP line) is what reaches the Z80 IRQ pin.
    ctc: Ctc,
    keyboard: KeyboardState,
    kbd_drive: u8,
    vdp_accum: i64,
    master_clock: u64,
    frame_count: u64,
    /// Tracks whether the previous opcode-fetch byte was the `$ED` prefix, so a
    /// following `$4D` (RETI) can release the CTC daisy chain's in-service channel.
    prev_opcode_ed: bool,
    /// When `Some`, every I/O port access is appended here (debug trace).
    #[serde(skip)]
    io_trace: Option<Vec<IoEvent>>,
}

/// Where a Z80 address resolves under the current paging byte.
#[derive(Debug, Clone, Copy)]
enum Cell {
    /// OS ROM byte (`rom[i]`, `i` in `$0000-$1FFF`).
    RomOs(u16),
    /// Paged ROM byte at this flat index into the ROM (an `$2000-$3FFF`
    /// subpage — BASIC, ASSEM, …).
    RomPaged(usize),
    /// RAM byte at this flat index into the block array.
    Ram(usize),
    /// No chip selected — reads float high (`$FF`), writes drop.
    Unmapped,
}

impl Mtx {
    /// Create a new MTX. `rom` is the 8 KB OS followed by one or more 8 KB
    /// paged ROMs for `$2000-$3FFF` (subpage 0 = BASIC, 1 = ASSEM, …). A
    /// stock machine is OS + BASIC + ASSEM (24 KB); OS + BASIC (16 KB) boots
    /// only as far as the first ASSEM system call.
    pub fn new(rom: Vec<u8>, model: MtxModel) -> Result<Self, String> {
        if rom.len() < 0x4000 || !rom.len().is_multiple_of(0x2000) {
            return Err(format!(
                "MTX ROM must be the 8 KB OS plus at least one 8 KB paged ROM \
                 (a multiple of 8192, ≥ 16384 bytes); got {}",
                rom.len()
            ));
        }
        let rom_subpages = rom.len() / 0x2000 - 1;
        Ok(Self {
            cpu: Z80::new(),
            rom,
            rom_subpages,
            ram: vec![0; model.ram_size()],
            blocks: model.blocks(),
            page_reg: 0,
            vdp: Tms9918::new(VdpRegion::Pal),
            psg: Sn76489::new(4_000_000, NoiseLfsr::Tms15),
            ctc: Ctc::new(),
            keyboard: KeyboardState::new(),
            kbd_drive: 0,
            vdp_accum: 0,
            master_clock: 0,
            frame_count: 0,
            prev_opcode_ed: false,
            io_trace: None,
        })
    }

    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        while !self.tick_cpu() {
            // The TMS9918A asserts VBlank after active line 192 but does not
            // finish the raster until line 313. A machine frame ends at that
            // raster wrap, not at the earlier interrupt edge.
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    /// Advance one Z80 T-state and report whether the VDP raster wrapped.
    fn tick_cpu(&mut self) -> bool {
        self.master_clock += 1;
        self.psg.tick();

        let mut frame_complete = false;
        self.vdp_accum += VDP_CLOCK_HZ as i64;
        while self.vdp_accum >= CPU_CLOCK_HZ as i64 {
            self.vdp_accum -= CPU_CLOCK_HZ as i64;
            frame_complete |= self.vdp.tick();
        }

        // VDP /INT feeds CTC channel 0's CLK/TRG input; the CTC edge-counts
        // those frame interrupts and its own INT output — vectored through the
        // OS's interrupt mode — is what drives the Z80 IRQ pin, not the raw VDP
        // line. (MEMU `memu.c` `LoopZ80` → `ctc_trigger(0)`.)
        self.ctc.set_trg(VDP_INT_CTC_CHANNEL, self.vdp.interrupt);
        self.ctc.tick();

        // Two CPU half-cycles per T-state. `Z80::tick` advances one
        // half-cycle — `T1Rise` then `T1Fall` — so calling it once per
        // T-state ran the CPU at half speed: a `NOP` cost 8 T-states against
        // the Z80's 4.
        //
        // The CTC's INT output is fed before each tick, as it already was:
        // the Z80 samples `/INT` at an instruction boundary during its own
        // tick. The VDP is not re-denominated to half-cycles, because it does
        // not reach the CPU directly — the CTC stands between them and ticks
        // once per T-state, so a finer VDP interleave could not change when
        // the interrupt arrives.
        for _ in 0..2 {
            self.cpu.irq = self.ctc.interrupt();
            self.cpu.tick();
            self.handle_bus();
        }

        frame_complete
    }

    fn handle_bus(&mut self) {
        match self.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                let byte = self.mem_read(self.cpu.addr);
                self.cpu.data_in = byte;
                // Watch the opcode stream for RETI (`ED 4D`) so the CTC daisy
                // chain releases its in-service channel. M1 marks an opcode
                // fetch; the `ED` prefix and its `4D` second byte are
                // consecutive M1 fetches.
                if self.cpu.m1 {
                    if self.prev_opcode_ed && byte == 0x4D {
                        self.ctc.reti();
                    }
                    self.prev_opcode_ed = byte == 0xED;
                }
            }
            Some(BusOp::MemWrite) => {
                self.mem_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IoRead) => {
                let io_port = (self.cpu.addr & 0xFF) as u8;
                let io_pc = self.cpu.regs.pc;
                let io_val = self.io_read(self.cpu.addr);
                self.cpu.data_in = io_val;
                if let Some(trace) = &mut self.io_trace {
                    trace.push(IoEvent {
                        pc: io_pc,
                        port: io_port,
                        value: io_val,
                        write: false,
                    });
                }
            }
            Some(BusOp::IoWrite) => {
                if let Some(trace) = &mut self.io_trace {
                    trace.push(IoEvent {
                        pc: self.cpu.regs.pc,
                        port: (self.cpu.addr & 0xFF) as u8,
                        value: self.cpu.data,
                        write: true,
                    });
                }
                self.io_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IntAck) => {
                // Acknowledge to the CTC: it supplies the low vector byte for
                // the requesting channel (used in IM 2; ignored by the Z80 in
                // IM 1, where it RST-38s) and clears its pending INT / marks the
                // channel in-service for the daisy chain.
                self.cpu.data_in = self.ctc.acknowledge();
            }
            None => {}
        }
    }

    /// Resolve a Z80 address through the paging byte (port `$00`) into a
    /// concrete storage cell. Models MEMU's `mem_set_iobyte` decode
    /// (`src/memu/mem.c`): the paging byte is `RELCPMH(b7) | ROM(b4-6) |
    /// RAM-page P(b0-3)`, and 16 KB RAM blocks page through three windows.
    fn resolve(&self, addr: u16) -> Cell {
        let p = (self.page_reg & 0x0F) as usize;

        // Common page: $C000-$FFFF is always RAM block 0 in both modes.
        if (0xC000..=0xFFFF).contains(&addr) {
            return self.ram_cell(0, addr);
        }

        if self.page_reg & 0x80 != 0 {
            // RELCPMH=1 — all-RAM (CP/M) mode. ipage 0/1/2 cover
            // $0000-$3FFF / $4000-$7FFF / $8000-$BFFF.
            let ipage = (addr >> 14) as usize; // 0,1,2 for the three windows
            let block = if p != 0 {
                3 * p + 1 + ipage
            } else {
                // P=0 fills the windows in descending order: ipage0←3,1←2,2←1.
                [3, 2, 1][ipage]
            };
            return self.ram_cell(block, addr);
        }

        // RELCPMH=0 — normal / ROM mode.
        match addr {
            0x0000..=0x1FFF => Cell::RomOs(addr),
            0x2000..=0x3FFF => {
                let irom = ((self.page_reg >> 4) & 0x07) as usize;
                if irom < self.rom_subpages {
                    // OS occupies the first 8 KB; subpage `irom` follows.
                    Cell::RomPaged((1 + irom) * 0x2000 + (addr as usize & 0x1FFF))
                } else {
                    Cell::Unmapped // that paged ROM is not fitted
                }
            }
            // P=$0F with RELCPMH=0 unmaps this window (MEMU's $8F mask quirk).
            0x4000..=0x7FFF if self.page_reg & 0x8F == 0x0F => Cell::Unmapped,
            0x4000..=0x7FFF => self.ram_cell(2 * p + 2, addr),
            0x8000..=0xBFFF => self.ram_cell(2 * p + 1, addr),
            _ => unreachable!("$C000+ handled above"),
        }
    }

    /// A 16 KB RAM block at the window containing `addr`, or `Unmapped` when
    /// the block is past the fitted RAM (reads float high, writes drop).
    fn ram_cell(&self, block: usize, addr: u16) -> Cell {
        if block < self.blocks {
            Cell::Ram(block * 0x4000 + (addr as usize & 0x3FFF))
        } else {
            Cell::Unmapped
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match self.resolve(addr) {
            Cell::RomOs(i) => self.rom[i as usize],
            Cell::RomPaged(idx) => self.rom[idx],
            Cell::Ram(idx) => self.ram[idx],
            Cell::Unmapped => 0xFF,
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        // ROM and unmapped writes drop. (In normal mode a write to
        // $0000-$1FFF selects a ROM subpage on real hardware; we fit only
        // subpage 0, so there is nothing to switch.)
        if let Cell::Ram(idx) = self.resolve(addr) {
            self.ram[idx] = value;
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        match (port & 0xFF) as u8 {
            0x00 => 0xFF, // Centronics status (no printer)
            0x01 => self.vdp.read_data(),
            0x02 => self.vdp.read_status(),
            0x03 => 0x03,                                      // snd_in3 — constant
            0x05 => self.keyboard.in5(self.kbd_drive),         // keyboard sense low
            0x06 => self.keyboard.in6(self.kbd_drive),         // keyboard sense high + country
            0x08..=0x0B => self.ctc.read((port & 0x03) as u8), // Z80 CTC
            _ => 0xFF,                                         // PIO/DART reads: open bus for now
        }
    }

    fn io_write(&mut self, port: u16, value: u8) {
        match (port & 0xFF) as u8 {
            0x00 => self.page_reg = value,
            0x01 => self.vdp.write_data(value),
            0x02 => self.vdp.write_control(value),
            0x03 => {}                      // cassette out — not fitted
            0x05 => self.kbd_drive = value, // keyboard column drive
            0x06 => self.psg.write(value),  // SN76489 sound
            0x08..=0x0B => self.ctc.write((port & 0x03) as u8, value), // Z80 CTC
            _ => {}                         // PIO/DART writes: ignore for now
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vdp.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.vdp.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.vdp.framebuffer_height()
    }

    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.psg.take_buffer()
    }

    pub fn press_key(&mut self, key: MtxKey) {
        let (row, bit) = key.matrix();
        self.keyboard.set_key(row, bit, true);
    }

    pub fn release_key(&mut self, key: MtxKey) {
        let (row, bit) = key.matrix();
        self.keyboard.set_key(row, bit, false);
    }

    pub fn release_all_keys(&mut self) {
        self.keyboard.release_all();
    }

    /// Read the keyboard/joystick sense lines for a `drive` byte (a zero bit
    /// drives that column): returns `(port $05 low byte, port $06 high byte)`.
    /// For inspection and host-side input wiring.
    #[must_use]
    pub fn sense(&self, drive: u8) -> (u8, u8) {
        (self.keyboard.in5(drive), self.keyboard.in6(drive))
    }

    /// Set the digital joystick state for `port` (1 or 2). The MTX joysticks
    /// share the keyboard matrix sense lines, so each control maps to a fixed
    /// `(column, sense bit)` position read back through `$05`/`$06`:
    /// player 1 (the "Right" port) on columns 2-6 sense bit 7, player 2 (the
    /// "Left" port) on column 7. Active low. Out-of-range ports clamp to the
    /// pair. (MAME `mtx` `JOY2`-`JOY7` matrix positions.)
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn set_joystick(
        &mut self,
        port: u8,
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        fire: bool,
    ) {
        if port.clamp(1, 2) == 1 {
            self.keyboard.set_joystick_bit(2, 7, up);
            self.keyboard.set_joystick_bit(3, 7, left);
            self.keyboard.set_joystick_bit(4, 7, right);
            self.keyboard.set_joystick_bit(5, 7, fire);
            self.keyboard.set_joystick_bit(6, 7, down);
        } else {
            self.keyboard.set_joystick_bit(7, 0, left);
            self.keyboard.set_joystick_bit(7, 1, right);
            self.keyboard.set_joystick_bit(7, 2, up);
            self.keyboard.set_joystick_bit(7, 3, down);
            self.keyboard.set_joystick_bit(7, 8, fire);
        }
    }

    #[must_use]
    pub fn peek_memory(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut Z80 {
        &mut self.cpu
    }

    #[must_use]
    pub fn vdp(&self) -> &Tms9918 {
        &self.vdp
    }

    /// The Z80 CTC at ports `$08-$0B`.
    #[must_use]
    pub fn ctc(&self) -> &Ctc {
        &self.ctc
    }

    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

impl zilog_z80::Z80Stepper for Mtx {
    fn z80_instructions_retired(&self) -> u64 {
        self.cpu.instructions_retired()
    }

    fn step_tick(&mut self) {
        let _ = self.tick_cpu();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom() -> Vec<u8> {
        let mut rom = vec![0xEAu8; 0x4000];
        rom[0] = 0xF3;
        rom[1] = 0x76;
        rom
    }

    #[test]
    fn rom_size_validated() {
        assert!(Mtx::new(vec![0u8; 1024], MtxModel::Mtx500).is_err());
    }

    /// Save-state must capture LIVE machine state (Z80 + TMS9918 VDP + SN76489
    /// PSG + Z80 CTC + RAM + keyboard), not cold-boot from the ROM. Serialise,
    /// advance (so the state differs), then deserialise the first snapshot and
    /// confirm re-serialising it is byte-identical — every stateful field across
    /// all chips round-trips, including the VDP's 16 KB VRAM and the CTC channels.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx500).expect("init");
        sys.run_frame();
        // $C000-$FFFF is always RAM block 0 (the common page) in every paging
        // mode, so this byte is guaranteed to land in RAM and carry across.
        sys.poke(0xC000, 0xA5);
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: Mtx = postcard::from_bytes(&s1).expect("decode snapshot");
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
    }

    #[test]
    fn each_pal_frame_runs_one_complete_vdp_raster() {
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx500).expect("init");
        let first = sys.run_frame();
        let second = sys.run_frame();
        let expected = 342u64 * 313 * CPU_CLOCK_HZ / VDP_CLOCK_HZ;

        assert_eq!(sys.frame_count(), 2);
        for ticks in [first, second] {
            assert!(
                ticks.abs_diff(expected) <= 1,
                "got {ticks} T-states, expected {expected} or {}",
                expected + 1
            );
        }
    }

    #[test]
    fn normal_mode_keeps_os_rom_under_paging() {
        // The boot RAM test writes RAM-page numbers to port $00. In normal
        // mode (RELCPMH=0) that must never page the OS ROM out of $0000 —
        // the donor's bug, which derailed the boot.
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx512).expect("init");
        assert_eq!(sys.mem_read(0x0000), 0xF3);
        for page in 0u8..=7 {
            sys.page_reg = page; // RELCPMH=0, RAM page = `page`
            assert_eq!(sys.mem_read(0x0000), 0xF3, "OS ROM at page {page}");
            sys.mem_write(0x0000, 0x99); // write to ROM drops
            assert_eq!(sys.mem_read(0x0000), 0xF3, "ROM unwritable at page {page}");
        }
    }

    #[test]
    fn relcpmh_maps_ram_over_low_memory() {
        // CP/M mode (bit 7) replaces the low ROM with RAM.
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx512).expect("init");
        sys.page_reg = 0x80; // RELCPMH=1, P=0
        sys.mem_write(0x0000, 0x42);
        assert_eq!(sys.mem_read(0x0000), 0x42);
    }

    #[test]
    fn ram_page_selects_distinct_blocks_at_4000() {
        // $4000-$7FFF pages block 2P+2: page 0 → block 2, and on an MTX512
        // page 1 → block 4 which is absent, so it floats high. That absence
        // is exactly how the boot ROM sizes RAM.
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx512).expect("init");
        sys.page_reg = 0x00;
        sys.mem_write(0x4000, 0x11); // block 2
        sys.page_reg = 0x01;
        assert_eq!(sys.mem_read(0x4000), 0xFF, "block 4 absent on MTX512");
        sys.page_reg = 0x00;
        assert_eq!(sys.mem_read(0x4000), 0x11, "block 2 retained");
    }

    #[test]
    fn common_page_is_block_zero() {
        // $C000-$FFFF is fixed to RAM block 0 regardless of page register —
        // the OS workspace lives here.
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx512).expect("init");
        sys.page_reg = 0x00;
        sys.mem_write(0xC000, 0x55);
        sys.page_reg = 0x03;
        assert_eq!(sys.mem_read(0xC000), 0x55);
    }

    #[test]
    fn keyboard_via_io() {
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx500).expect("init");
        // Drive only column 3 (active low) and read sense low.
        sys.io_write(0x05, !(1 << 3));
        assert_eq!(sys.io_read(0x05), 0xFF); // nothing held
        sys.keyboard.set_key(3, 1, true);
        assert_eq!(sys.io_read(0x05) & 0x02, 0x00); // key on column 3 sensed
        // Port $06 reports the country code (English = 0) with no keys.
        assert_eq!(sys.io_read(0x06) & 0x0C, 0x00);
    }

    #[test]
    fn sound_writes_go_to_port_6() {
        // Regression: the donor wired the SN76489 to $03; it is $06.
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx500).expect("init");
        sys.io_write(0x06, 0x9F); // SN76489 latch/volume — must not touch paging
        assert_eq!(sys.page_reg, 0x00);
    }

    #[test]
    fn joystick_reads_through_the_keyboard_matrix() {
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx500).expect("init");

        // Player 1 up sits at column 2, sense bit 7. Drive column 2, read $05.
        sys.set_joystick(1, true, false, false, false, false);
        sys.io_write(0x05, !(1 << 2));
        assert_eq!(sys.io_read(0x05) & 0x80, 0x00, "P1 up → col 2 bit 7 low");

        // Player 1 fire is column 5, bit 7.
        sys.set_joystick(1, false, false, false, false, true);
        sys.io_write(0x05, !(1 << 5));
        assert_eq!(sys.io_read(0x05) & 0x80, 0x00, "P1 fire → col 5 bit 7 low");

        // Player 2 right is column 7, sense bit 1 (low byte via $05).
        sys.set_joystick(2, false, false, false, true, false);
        sys.io_write(0x05, !(1 << 7));
        assert_eq!(sys.io_read(0x05) & 0x02, 0x00, "P2 right → col 7 bit 1 low");

        // Player 2 fire is column 7, sense bit 8 (high byte via $06).
        sys.set_joystick(2, false, false, false, false, true);
        sys.io_write(0x05, !(1 << 7));
        assert_eq!(sys.io_read(0x06) & 0x01, 0x00, "P2 fire → col 7 bit 8 low");
    }
}

/// One captured I/O port access, for the debug trace.
#[derive(Debug, Clone, Copy)]
pub struct IoEvent {
    /// CPU program counter at the time of the access.
    pub pc: u16,
    /// I/O port (low 8 bits of the address bus).
    pub port: u8,
    /// Byte written, or byte returned on a read.
    pub value: u8,
    /// `true` for `OUT`, `false` for `IN`.
    pub write: bool,
}

impl Mtx {
    /// Observe one byte on the bus without side effects.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Start (or restart) the I/O port-access trace.
    pub fn start_io_trace(&mut self) {
        self.io_trace = Some(Vec::new());
    }

    /// Stop tracing and return the captured I/O events.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.io_trace.take().unwrap_or_default()
    }
}
