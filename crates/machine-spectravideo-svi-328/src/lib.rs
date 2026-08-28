//! Spectravideo SVI-328 machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at
//! `Emu198x-Oldest/crates/machine-spectravideo-svi-328` used the
//! deprecated `emu_core::Bus` callback and could not port directly;
//! this file uses it as a system spec — memory map, port-`$97`
//! banking, 8255-driven keyboard, I/O port routing — but the wiring
//! is written against [`emu198x_zilog_z80::Z80`]'s public pin fields and
//! `bus_request()` collapse.
//!
//! # The Spectravideo SVI-328
//!
//! The SVI-328 (1983) is the **MSX precursor** — Spectravideo
//! designed it before MSX standardisation, and the MSX standard later
//! crystallised around essentially the same chip stack: Z80 + TMS9918
//! + AY-3-8910 + Intel 8255. The SVI-328 differs from MSX1 mainly in
//!   its simpler **port-`$97` ROM/RAM banking** rather than MSX's
//!   4-slot system, and in its tighter I/O port window (`$80-$97` vs
//!   MSX's spread across `$98`, `$A0-$AB`).
//!
//! - **CPU:** Z80A @ 3.579545 MHz
//! - **VDP:** TMS9918A (16 KB VRAM)
//! - **PSG:** AY-3-8910 @ 1.789773 MHz (CPU ÷ 2) — consumed via our
//!   `gi-ay-3-8912` crate (same silicon, port B not bonded out)
//! - **PPI:** Intel 8255 (keyboard row select + column read)
//! - **RAM:** 32 KB at `$8000-$FFFF`, expandable to 64 KB in
//!   ROM-overlay mode
//! - **ROM:** 32 KB BASIC + OS at `$0000-$7FFF`
//!
//! # Memory map
//!
//! | Range         | Default                  | Port `$97` overlay  |
//! |---------------|--------------------------|---------------------|
//! | `$0000-$7FFF` | System ROM (BASIC + OS)  | RAM when bit 0 = 1  |
//! | `$8000-$BFFF` | RAM                      | Cart when bit 1 = 1 |
//! | `$C000-$FFFF` | RAM                      | (always RAM)        |
//!
//! Reading port `$96` / `$97` returns the current bank state in bits
//! 0 and 1.
//!
//! # I/O map
//!
//! | Port      | R/W   | Function                                      |
//! |-----------|-------|-----------------------------------------------|
//! | `$80`     | write | VDP data                                       |
//! | `$81`     | write | VDP control                                    |
//! | `$84`     | read  | VDP data                                       |
//! | `$85`     | read  | VDP status (vblank ISR acknowledges here)      |
//! | `$88`     | write | PSG register select                            |
//! | `$8C`     | write | PSG register data (R15 bit 1 banks RAM in)     |
//! | `$90`     | read  | PSG register data (R14 = both joysticks)       |
//! | `$94-$97` | write | PPI ports A / B / C / control                  |
//! | `$98`     | read  | PPI port A (paddles, fire buttons, cassette)   |
//! | `$99`     | read  | PPI port B (keyboard column for selected row)  |
//! | `$9A`     | read  | PPI port C                                     |
//!
//! # Keyboard
//!
//! 11 rows × 8 columns matrix, active-low (1 = released). The CPU
//! writes the row index to PPI port C (low nibble) and reads the
//! column data from port B.
//!
//! # Clock model
//!
//! Adopts SG-1000 / MSX 3:2 VDP-dot-per-T-state phase counter. PSG
//! ticks every other T-state for the CPU ÷ 2 = 1.789 MHz AY clock.

use emu198x_zilog_z80::{BusOp, Z80};
use gi_ay_3_8912::{Ay3_8912, AyWriteRecord, AyWriteWatch};
use intel_8255::Ppi8255;
use serde::{Deserialize, Serialize};
use ti_tms9918::{Tms9918, VdpRegion};

/// VDP dot ticks per CPU half-cycle. Three dots per four half-cycles is the
/// TMS9918A's 3:2 against T-states, interleaved twice as finely.
const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
const VDP_DOT_PHASE_DENOMINATOR: u32 = 4;
const CPU_TSTATES_PER_SCANLINE: u64 = 228;
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
const PAL_SCANLINES_PER_FRAME: u64 = 313;
const NTSC_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
const PAL_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;

const AY_CLOCK_HZ: u32 = 1_789_773;
const AY_SAMPLE_RATE: u32 = 48_000;
const AY_SAMPLES_PER_FRAME: usize = 1024;

/// Number of keyboard matrix rows.
pub const NUM_KEY_ROWS: usize = 11;

/// SVI-328 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SviRegion {
    Ntsc,
    Pal,
}

/// Spectravideo SVI-328 machine.
///
/// Fully serialisable for save-states: the Z80, the TMS9918 VDP, the
/// AY-3-8910 PSG, the 8255 PPI, ROM, RAM, banking, keyboard, and joystick
/// state all carry live state. `io_trace` and `ay_watch` are host-side debug
/// buffers, not machine state, so they are skipped and default on restore.
#[derive(Serialize, Deserialize)]
pub struct Svi328 {
    cpu: Z80,
    vdp: Tms9918,
    psg: Ay3_8912,
    ppi: Ppi8255,
    rom: Vec<u8>,
    /// 64 KB RAM; the lower 32 KB is exposed in place of the BASIC ROM when
    /// the PSG port B bank register selects it.
    ram: Vec<u8>,
    cart_rom: Vec<u8>,
    /// `true` → RAM replaces ROM at `$0000-$7FFF`. Driven by AY-3-8910 port B
    /// (R15) bit 1 (`bk21`): low banks RAM in. ROM is visible at reset.
    bank_ram_low: bool,
    /// `true` → cart ROM replaces RAM at `$8000-$BFFF`.
    bank_cart: bool,
    /// The PSG register last selected via the address port ($88); used to spot
    /// writes to R15, the memory-bank register.
    psg_reg_select: u8,
    /// 11×8 keyboard matrix, active-low.
    keyboard: [u8; NUM_KEY_ROWS],
    /// Joystick direction lines, presented on PSG port A (register 14):
    /// player 1 in the low nibble, player 2 in the high nibble, active low
    /// (bit 0/4 up, 1/5 down, 2/6 left, 3/7 right). Idle is `0xFF`.
    joy_dirs: u8,
    /// Joystick fire buttons `[player1, player2]`, read on PPI port A bits
    /// 4-5 (active low). See [`Self::io_read`] `$98`.
    joy_fire: [bool; 2],
    region: SviRegion,
    cpu_tstates: u64,
    tstates_per_frame: u64,
    vdp_phase: u32,
    psg_phase: u8,
    frame_count: u64,
    /// When `Some`, every I/O port access is appended here (debug trace).
    #[serde(skip)]
    io_trace: Option<Vec<IoEvent>>,
    /// When `Some`, every write to the PSG data port ($8C) is captured
    /// for the shared `watch_ay_*` tools. Host-side debug only, not
    /// part of the snapshot.
    #[serde(skip)]
    ay_watch: Option<AyWriteWatch>,
}

impl Svi328 {
    /// Create a new SVI-328 with the given system ROM (32 KB).
    #[must_use]
    pub fn new(rom: Vec<u8>, region: SviRegion) -> Self {
        let vdp_region = match region {
            SviRegion::Ntsc => VdpRegion::Ntsc,
            SviRegion::Pal => VdpRegion::Pal,
        };
        let tstates_per_frame = match region {
            SviRegion::Ntsc => NTSC_TSTATES_PER_FRAME,
            SviRegion::Pal => PAL_TSTATES_PER_FRAME,
        };
        Self {
            cpu: Z80::new(),
            vdp: Tms9918::new(vdp_region),
            psg: Ay3_8912::new(AY_CLOCK_HZ, AY_SAMPLE_RATE, AY_SAMPLES_PER_FRAME),
            ppi: Ppi8255::new(),
            rom,
            ram: vec![0u8; 65536],
            cart_rom: Vec::new(),
            bank_ram_low: false,
            bank_cart: false,
            psg_reg_select: 0,
            keyboard: [0xFF; NUM_KEY_ROWS],
            joy_dirs: 0xFF,
            joy_fire: [false; 2],
            region,
            cpu_tstates: 0,
            tstates_per_frame,
            vdp_phase: 0,
            psg_phase: 0,
            frame_count: 0,
            io_trace: None,
            ay_watch: None,
        }
    }

    /// Insert a cartridge ROM and enable the cart bank overlay.
    pub fn insert_cart(&mut self, rom: Vec<u8>) {
        self.cart_rom = rom;
        self.bank_cart = true;
    }

    /// Run one frame and return T-states consumed.
    pub fn run_frame(&mut self) -> u64 {
        let target = self.cpu_tstates + self.tstates_per_frame;
        while self.cpu_tstates < target {
            self.tick_tstate();
        }
        self.frame_count += 1;
        self.tstates_per_frame
    }

    fn tick_tstate(&mut self) {
        // Two CPU half-cycles per T-state. `Z80::tick` advances one
        // half-cycle — `T1Rise` then `T1Fall` — so calling it once per
        // T-state ran the CPU at half speed: a `NOP` cost 8 T-states
        // against the Z80's 4.
        for _ in 0..2 {
            // VDP INT → Z80 IRQ, fed before the tick. The Z80 samples
            // `/INT` at an instruction boundary during its own tick, so
            // setting the line afterwards hands it the previous
            // half-cycle's state.
            self.cpu.irq = self.vdp.interrupt;

            self.cpu.tick();
            self.handle_bus();

            self.vdp_phase += VDP_DOT_PHASE_NUMERATOR;
            while self.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
                self.vdp.tick();
                self.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
            }
        }

        self.psg_phase ^= 1;
        if self.psg_phase == 0 {
            self.psg.tick();
        }

        self.cpu_tstates += 1;
    }

    fn handle_bus(&mut self) {
        match self.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                self.cpu.data_in = self.mem_read(self.cpu.addr);
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
                // SVI-328 system ROM sets IM 1 — INT fetches RST 38h
                // via the floating bus.
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                if self.bank_ram_low {
                    self.ram[addr as usize]
                } else {
                    self.rom.get(addr as usize).copied().unwrap_or(0xFF)
                }
            }
            0x8000..=0xBFFF => {
                if self.bank_cart && !self.cart_rom.is_empty() {
                    self.cart_rom
                        .get((addr - 0x8000) as usize)
                        .copied()
                        .unwrap_or(0xFF)
                } else {
                    self.ram[addr as usize]
                }
            }
            0xC000..=0xFFFF => self.ram[addr as usize],
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => {
                if self.bank_ram_low {
                    self.ram[addr as usize] = value;
                }
            }
            0x8000..=0xBFFF => {
                if !self.bank_cart {
                    self.ram[addr as usize] = value;
                }
            }
            0xC000..=0xFFFF => self.ram[addr as usize] = value,
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        match port as u8 {
            // The VDP is written at $80/$81 but *read* at $84/$85. This split
            // matters: the vblank ISR reads the status at $85 to acknowledge
            // and clear the interrupt. (MAME `svi318` io map.)
            0x84 => self.vdp.read_data(),
            0x85 => self.vdp.read_status(),
            // PSG data read at $90. Register 14 (port A) carries the two
            // joysticks' directions, so present them before the read: P1 in
            // the low nibble, P2 in the high nibble (active low). Unlike the
            // MSX, the SVI reads both sticks at once — no port select.
            0x90 => {
                if self.psg.selected_register() == 14 {
                    self.psg.set_port_a_input_mask(self.joy_dirs);
                }
                self.psg.read_data()
            }
            // PPI port A read ($98): paddle (0-3), joystick fire buttons (4-5,
            // active low), cassette (6-7). With no cassette attached the
            // no-cassette bit reads high and the cassette input low, so the
            // idle byte is 0x7F; a held fire button pulls its bit (PB1 = bit 4,
            // PB2 = bit 5) low. (MAME `svi318` `ppi_port_a_r`.)
            0x98 => {
                let mut data = 0x7F;
                if self.joy_fire[0] {
                    data &= !0x10;
                }
                if self.joy_fire[1] {
                    data &= !0x20;
                }
                data
            }
            // PPI port B read ($99): keyboard column data for the row selected
            // via port C.
            0x99 => {
                let row = self.ppi.keyboard_row() as usize;
                self.keyboard.get(row).copied().unwrap_or(0xFF)
            }
            // PPI port C read ($9A).
            0x9A => self.ppi.read(2),
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, value: u8) {
        match port as u8 {
            0x80 => self.vdp.write_data(value),
            0x81 => self.vdp.write_control(value),
            // PSG: address latch at $88, data at $8C.
            0x88 => {
                self.psg_reg_select = value & 0x0F;
                self.psg.select_register(value);
            }
            0x8C => {
                if let Some(w) = &mut self.ay_watch {
                    w.record(self.cpu.regs.pc, self.psg.selected_register(), value);
                }
                self.psg.write_data(value);
                // The AY-3-8910's port B (R15) is the memory-bank register.
                // Bit 1 (`bk21`) low banks the lower 32 KB of RAM in over the
                // BASIC ROM; otherwise the ROM stays visible. (The cart and
                // expansion-ROM bits are not modelled on the base machine.)
                if self.psg_reg_select == 0x0F {
                    self.bank_ram_low = value & 0x02 == 0;
                }
            }
            // PPI ports: A $94, B $95, C $96, control $97. Port C's low nibble
            // selects the keyboard row.
            0x94 => self.ppi.write(0, value),
            0x95 => self.ppi.write(1, value),
            0x96 => self.ppi.write(2, value),
            0x97 => self.ppi.write(3, value),
            _ => {}
        }
    }

    /// Framebuffer (256×192 ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vdp.framebuffer()
    }

    /// Finish the current PSG frame and return its mono host samples.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        let mut out = vec![0.0; AY_SAMPLES_PER_FRAME];
        self.psg.end_frame(&mut out);
        out
    }

    /// Press a key at the given (row, bit).
    pub fn press_key(&mut self, row: usize, bit: u8) {
        if row < self.keyboard.len() && bit < 8 {
            self.keyboard[row] &= !(1 << bit);
        }
    }

    /// Release a key at the given (row, bit).
    pub fn release_key(&mut self, row: usize, bit: u8) {
        if row < self.keyboard.len() && bit < 8 {
            self.keyboard[row] |= 1 << bit;
        }
    }

    /// Set the digital joystick state for `port` (1 or 2). The directions are
    /// read on PSG port A — player 1 in the low nibble, player 2 in the high
    /// nibble (active low: bit 0/4 up, 1/5 down, 2/6 left, 3/7 right) — and the
    /// fire button on PPI port A bit 4 (player 1) / bit 5 (player 2). Both
    /// joysticks are read independently. Out-of-range ports clamp to the pair.
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
        let idx = usize::from(port.clamp(1, 2) - 1);
        let mut nibble = 0x0Fu8;
        for (pressed, bit) in [(up, 0x01), (down, 0x02), (left, 0x04), (right, 0x08)] {
            if pressed {
                nibble &= !bit;
            }
        }
        let shift = idx * 4;
        self.joy_dirs = (self.joy_dirs & !(0x0F << shift)) | (nibble << shift);
        self.joy_fire[idx] = fire;
    }

    /// The active-low PSG port-A byte the joysticks present (both players'
    /// directions). For inspection and host-side input wiring.
    #[must_use]
    pub fn joystick_dirs(&self) -> u8 {
        self.joy_dirs
    }

    /// Observe the column bits for a keyboard matrix row (active-low:
    /// a `0` bit is a pressed key). Returns `0xFF` for an out-of-range
    /// row.
    #[must_use]
    pub fn key_row(&self, row: usize) -> u8 {
        self.keyboard.get(row).copied().unwrap_or(0xFF)
    }

    /// CPU reference.
    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    /// CPU mutable reference.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        &mut self.cpu
    }

    /// VDP reference.
    #[must_use]
    pub fn vdp(&self) -> &Tms9918 {
        &self.vdp
    }

    /// Region.
    #[must_use]
    pub fn region(&self) -> SviRegion {
        self.region
    }

    /// CPU T-states executed since power-on.
    #[must_use]
    pub fn cpu_tstates(&self) -> u64 {
        self.cpu_tstates
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Memory-control register state — `(ram_low, cart)`.
    #[must_use]
    pub fn memory_control(&self) -> (bool, bool) {
        (self.bank_ram_low, self.bank_cart)
    }
}

impl emu198x_zilog_z80::Z80Stepper for Svi328 {
    fn z80_instructions_retired(&self) -> u64 {
        self.cpu.instructions_retired()
    }

    fn step_tick(&mut self) {
        self.tick_tstate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom() -> Vec<u8> {
        // 32 KB ROM: NOPs with JR -2 trap at $0008.
        let mut rom = vec![0u8; 0x8000];
        rom[0x0008] = 0x18;
        rom[0x0009] = 0xFE;
        rom
    }

    /// Save-state must capture LIVE machine state (Z80 + TMS9918 VDP +
    /// AY-3-8910 PSG + 8255 PPI + RAM + banking), not cold-boot from the ROM.
    /// Serialise, advance (so the state differs), then deserialise the first
    /// snapshot and confirm re-serialising it is byte-identical — every
    /// stateful field round-trips, including the VDP's 16 KB VRAM.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        sys.run_frame();
        sys.poke(0xC100, 0xA5); // a work-RAM byte to carry across the snapshot
        assert_eq!(sys.peek(0xC100), 0xA5, "0xC100 is RAM and accepts the poke");
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: Svi328 = postcard::from_bytes(&s1).expect("decode snapshot");
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
    }

    #[test]
    fn ntsc_frame_returns_expected_tstates() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        let t = sys.run_frame();
        assert_eq!(t, NTSC_TSTATES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_frame_returns_expected_tstates() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Pal);
        let t = sys.run_frame();
        assert_eq!(t, PAL_TSTATES_PER_FRAME);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        for _ in 0..60 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 60);
    }

    #[test]
    fn joystick_directions_read_through_psg_port_a() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        sys.io_write(0x88, 14); // select PSG register 14 (port A)

        // Idle: both sticks read all-high.
        assert_eq!(sys.io_read(0x90), 0xFF);

        // Player 1 up+left → low-nibble bits 0 and 2 low; P2 nibble idle high.
        sys.set_joystick(1, true, false, true, false, false);
        let v = sys.io_read(0x90);
        assert_eq!(v & 0x01, 0, "P1 up → bit 0 low");
        assert_eq!(v & 0x04, 0, "P1 left → bit 2 low");
        assert_eq!(v & 0x0A, 0x0A, "P1 down/right idle high");
        assert_eq!(v & 0xF0, 0xF0, "P2 nibble untouched");

        // Player 2 down → high-nibble bit 5 low, independent of P1.
        sys.set_joystick(2, false, true, false, false, false);
        let v = sys.io_read(0x90);
        assert_eq!(v & 0x20, 0, "P2 down → bit 5 low");
        assert_eq!(v & 0x01, 0, "P1 up still held");
    }

    #[test]
    fn joystick_fire_buttons_read_on_ppi_port_a() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        // Idle PPI port A: buttons high, no-cassette bit set → 0x7F.
        assert_eq!(sys.io_read(0x98), 0x7F);

        sys.set_joystick(1, false, false, false, false, true); // P1 fire
        assert_eq!(sys.io_read(0x98) & 0x10, 0, "P1 fire → PPI PA4 low");
        assert_eq!(sys.io_read(0x98) & 0x20, 0x20, "P2 fire idle high");

        sys.set_joystick(2, false, false, false, false, true); // P2 fire
        assert_eq!(sys.io_read(0x98) & 0x30, 0, "both fire → PA4+PA5 low");
    }

    #[test]
    fn rom_visible_at_reset() {
        let sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        // BIOS-trap byte at $0008.
        assert_eq!(sys.mem_read(0x0008), 0x18);
        // Read past ROM end → 0xFF from the unmapped fall-through.
        assert_eq!(sys.mem_read(0xFFFE), 0x00); // RAM defaults zero.
    }

    #[test]
    fn ram_overlay_replaces_rom_at_low_window() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        // Bank the lower RAM in via PSG R15 (port B), bit 1 low.
        sys.io_write(0x88, 0x0F); // select R15
        sys.io_write(0x8C, 0x00); // bk21 = 0 → RAM low
        assert_eq!(sys.memory_control(), (true, false));
        sys.mem_write(0x0100, 0x42);
        assert_eq!(sys.mem_read(0x0100), 0x42);
    }

    #[test]
    fn cart_overlay_replaces_ram_at_8000() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        sys.insert_cart(vec![0xAA; 0x4000]);
        // insert_cart auto-enables bank_cart.
        assert_eq!(sys.memory_control(), (false, true));
        assert_eq!(sys.mem_read(0x8000), 0xAA);
        assert_eq!(sys.mem_read(0xBFFF), 0xAA);
        // $C000 is always RAM.
        assert_eq!(sys.mem_read(0xC000), 0x00);
    }

    #[test]
    fn psg_port_b_drives_lower_ram_bank() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        sys.io_write(0x88, 0x0F); // select R15
        sys.io_write(0x8C, 0x00); // bk21 = 0 → RAM low
        assert!(sys.memory_control().0, "bk21 low should bank RAM in");
        sys.io_write(0x88, 0x0F);
        sys.io_write(0x8C, 0x02); // bk21 = 1 → ROM low
        assert!(!sys.memory_control().0, "bk21 high should restore the ROM");
    }

    #[test]
    fn keyboard_io_returns_selected_row() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        sys.keyboard[5] = 0xCD;
        // Select row 5 via PPI port C ($96), read the column data at port B ($99).
        sys.io_write(0x96, 0x05);
        assert_eq!(sys.io_read(0x99), 0xCD);
    }

    #[test]
    fn vdp_dot_ratio_is_three_per_two_tstates() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        let start = sys.vdp.scanline();
        for _ in 0..4 {
            sys.tick_tstate();
        }
        assert_eq!(sys.vdp.scanline(), start);
        assert_eq!(sys.vdp_phase, 0);
    }

    #[test]
    fn key_press_and_release_round_trip() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        sys.press_key(3, 2);
        assert_eq!(sys.keyboard[3] & 0b0000_0100, 0);
        sys.release_key(3, 2);
        assert_eq!(sys.keyboard[3] & 0b0000_0100, 0b0000_0100);
    }

    #[test]
    fn ay_watch_captures_psg_data_writes() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        assert!(sys.ay_write_watch_records().is_none());
        let cap = sys.start_ay_write_watch();
        assert!(cap > 0);
        sys.io_write(0x88, 7); // select R7
        sys.io_write(0x8C, 0x38); // data
        sys.io_write(0x88, 8); // select R8
        sys.io_write(0x8C, 0x0F); // data
        let records = sys.ay_write_watch_records().expect("armed");
        assert_eq!(records.len(), 2);
        assert_eq!((records[0].register, records[0].value), (7, 0x38));
        assert_eq!((records[1].register, records[1].value), (8, 0x0F));
        sys.clear_ay_write_watch_records();
        assert_eq!(sys.ay_write_watch_records().expect("armed").len(), 0);
        sys.stop_ay_write_watch();
        assert!(sys.ay_write_watch_records().is_none());
    }

    /// #254: the PSG was ticking, but its generated samples had no path out
    /// of the machine. Exercise the real SVI I/O ports so this covers both the
    /// register wiring and the host-facing buffer.
    #[test]
    fn psg_tone_reaches_the_audio_buffer() {
        let mut sys = Svi328::new(trap_rom(), SviRegion::Ntsc);
        for (register, value) in [(0, 100), (1, 0), (7, 0x3E), (8, 15)] {
            sys.io_write(0x88, register);
            sys.io_write(0x8C, value);
        }

        sys.run_frame();
        let audio = sys.take_audio_buffer();
        let (minimum, maximum) = audio.iter().copied().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), sample| (minimum.min(sample), maximum.max(sample)),
        );

        assert_eq!(audio.len(), AY_SAMPLES_PER_FRAME);
        assert!(
            maximum - minimum > 0.01,
            "an enabled tone should produce a varying waveform"
        );
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

impl Svi328 {
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

    /// Start (or restart) capturing PSG register writes for `watch_ay_*`.
    /// Returns the log capacity (max records before writes are dropped).
    pub fn start_ay_write_watch(&mut self) -> u32 {
        let watch = AyWriteWatch::new();
        let cap = watch.cap() as u32;
        self.ay_watch = Some(watch);
        cap
    }

    /// Stop capturing PSG writes and drop the log.
    pub fn stop_ay_write_watch(&mut self) {
        self.ay_watch = None;
    }

    /// Captured PSG writes since the last `start_ay_write_watch`, or
    /// `None` when the watch is disarmed.
    #[must_use]
    pub fn ay_write_watch_records(&self) -> Option<&[AyWriteRecord]> {
        self.ay_watch.as_ref().map(AyWriteWatch::records)
    }

    /// Drop captured PSG writes while leaving the watch armed.
    pub fn clear_ay_write_watch_records(&mut self) {
        if let Some(w) = &mut self.ay_watch {
            w.clear();
        }
    }
}
