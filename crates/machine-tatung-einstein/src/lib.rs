//! Tatung Einstein TC-01 machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-tatung-einstein`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — memory map, port-`$21`
//! ROM page-out, AY-driven keyboard, I/O port routing — but the
//! wiring is written against [`zilog_z80::Z80`]'s public pin fields
//! and `bus_request()` collapse.
//!
//! # The Tatung Einstein TC-01
//!
//! The Einstein (1984) is a UK-designed Z80-based home computer with
//! built-in floppy drive and CP/M as the primary OS — sold mainly into
//! the UK and German education / small-business markets. Same chip
//! stack as MSX (Z80 + TMS9918 + AY-3-8910) but no PPI: the keyboard
//! row select goes through AY port A instead.
//!
//! - **CPU:** Z80A @ 4 MHz (faster than the 3.58 MHz TMS9918-family
//!   standard)
//! - **VDP:** TMS9918A (16 KB VRAM)
//! - **PSG:** AY-3-8910 @ 2 MHz (CPU ÷ 2) — consumed via our
//!   `gi-ay-3-8910` crate (same silicon)
//! - **RAM:** 64 KB
//! - **ROM:** 8 KB X-TAL MOS at `$0000-$1FFF` (pageable)
//! - **CTC:** Z80 CTC (channel 0 stubbed at port `$28`)
//! - **Floppy:** WD1770 at ports `$18-$1B`, drive select at `$23`
//!
//! # Memory map
//!
//! Page 0 (`$0000-$1FFF`) returns ROM at reset; **every access to port
//! `$24`** (read or write) toggles the ROM in and out, leaving the 64 KB
//! RAM visible underneath — the MOS uses this to copy the ROM into RAM.
//! Writes always land in RAM regardless of the ROM-page state.
//!
//! # I/O map
//!
//! Verified against MAME's `tatung/einstein.cpp`; the donor's map was
//! wrong (it had the AY on `$00-$02` and the keyboard on `$20`).
//!
//! | Port  | R/W   | Function                                       |
//! |-------|-------|------------------------------------------------|
//! | `$02` | read  | AY data read                                   |
//! | `$02` | write | AY register select (address latch)             |
//! | `$03` | write | AY data write                                  |
//! | `$08` | r/w   | VDP data                                       |
//! | `$09` | r/w   | VDP control / status                           |
//! | `$18-$1B` | r/w | WD1770 floppy controller                     |
//! | `$20` | read  | Modifier keys + clears the keyboard interrupt  |
//! | `$20` | write | Keyboard interrupt mask (bit 0: 0 = enabled)   |
//! | `$23` | write | Floppy drive / side select                     |
//! | `$24` | r/w   | ROM-bank toggle (read or write flips it)       |
//! | `$28` | r/w   | Z80 CTC channel 0 (stub)                       |
//!
//! # Keyboard
//!
//! 8 × 8 matrix, active-low, hung off the AY-3-8910's I/O ports: the MOS
//! drives the row-select lines on **port A** (R14, output) and reads the
//! column data back on **port B** (R15, input). The scan runs from a
//! ~50 Hz keyboard interrupt — a dedicated Z80-mode-2 vectored interrupt
//! (vector `$F7`), enabled/masked by `$20` bit 0 and cleared by reading
//! `$20`; that read also returns the GRAPH/CTRL/SHIFT modifier keys on
//! bits 5-7. Without the interrupt the MOS detects a key but never scans
//! the matrix to identify it.
//!
//! # Clock model
//!
//! Adopts the 3:2 VDP-dot-per-T-state phase counter pattern from
//! SG-1000 / MSX. Einstein's CPU runs at 4 MHz (vs the 3.58 MHz
//! TMS9918-family standard); the absolute clock rates differ but the
//! relative phase counter holds because both chips run on their own
//! crystals and we approximate using the ratio. PSG ticks every other
//! T-state for the CPU ÷ 2 = 2 MHz AY clock.

use gi_ay_3_8910::Ay3_8910;
use ti_tms9918::{Tms9918, VdpRegion};
use western_digital_wd1770::{Disk, Wd1770};
use zilog_z80::{BusOp, Z80};

const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
const VDP_DOT_PHASE_DENOMINATOR: u32 = 2;
const CPU_TSTATES_PER_SCANLINE: u64 = 228;
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
const PAL_SCANLINES_PER_FRAME: u64 = 313;
const NTSC_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
const PAL_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;

const AY_CLOCK_HZ: u32 = 2_000_000;
const AY_SAMPLE_RATE: u32 = 48_000;
const AY_SAMPLES_PER_FRAME: usize = 1024;

/// Number of keyboard matrix rows.
pub const NUM_KEY_ROWS: usize = 8;

/// Einstein region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EinsteinRegion {
    Ntsc,
    Pal,
}

/// ADC0844 4-channel 8-bit ADC — the Einstein's analogue-joystick port at I/O
/// `$38`. A write selects the channel/mode (`data & 0x0f`); a read returns the
/// conversion. Channels 1-4 carry joystick 1 X/Y and joystick 2 X/Y. The
/// single-ended modes (`$04`-`$07`) read one channel; the differential and
/// pseudo-differential modes combine a pair. Decode adapted from MAME's
/// `adc0844` device; the 40 µs conversion is treated as instantaneous.
#[derive(Clone)]
struct Adc0844 {
    /// Channel inputs `[joy1 X, joy1 Y, joy2 X, joy2 Y]`, 8-bit, centre `0x80`.
    channels: [u8; 4],
    /// Selected channel / mode — the low nibble of the last `$38` write.
    channel: u8,
}

impl Adc0844 {
    fn new() -> Self {
        Self {
            channels: [0x80; 4],
            channel: 0x0F,
        }
    }

    fn write(&mut self, data: u8) {
        self.channel = data & 0x0F;
    }

    fn read(&self) -> u8 {
        let ch = |i: usize| i32::from(self.channels[i]);
        let clamp = |v: i32| u8::try_from(v.clamp(0, 0xFF)).unwrap_or(0xFF);
        match self.channel {
            // Differential pairs.
            0x00 | 0x08 => clamp(0xFF - (ch(1) - ch(0))),
            0x01 | 0x09 => clamp(0xFF - (ch(0) - ch(1))),
            0x02 | 0x0A => clamp(0xFF - (ch(3) - ch(2))),
            0x03 | 0x0B => clamp(0xFF - (ch(2) - ch(3))),
            // Single-ended channels 1-4.
            0x04 => self.channels[0],
            0x05 => self.channels[1],
            0x06 => self.channels[2],
            0x07 => self.channels[3],
            // Pseudo-differential (against channel 4).
            0x0C => clamp(0xFF - (ch(3) - ch(0))),
            0x0D => clamp(0xFF - (ch(3) - ch(1))),
            0x0E => clamp(0xFF - (ch(3) - ch(2))),
            _ => 0x00,
        }
    }
}

/// Tatung Einstein TC-01 machine.
pub struct Einstein {
    cpu: Z80,
    vdp: Tms9918,
    psg: Ay3_8910,
    rom: Vec<u8>,
    ram: [u8; 65536],
    /// `$0000-$1FFF` returns ROM at reset; any write to `$21` flips
    /// this `false` and exposes the 64 KB RAM across the full space.
    rom_paged_in: bool,
    /// 8×8 keyboard matrix, active-low (a pressed key clears its bit).
    keyboard: [u8; NUM_KEY_ROWS],
    /// Modifier keys read on port `$20` bits 5-7 (GRAPH/CTRL/SHIFT),
    /// active low. These sit outside the scanned matrix.
    extra_keys: u8,
    /// Whether the keyboard interrupt is enabled ($20 bit 0 = 0 enables).
    /// The MOS scans the matrix from this interrupt's IM 2 handler.
    kbd_int_enabled: bool,
    /// Keyboard interrupt request, raised by the per-frame scan when a key
    /// is down and cleared when the MOS reads $20 in its handler.
    kbd_int_pending: bool,
    /// CTC channel 0 stub.
    ctc_reg: u8,
    /// WD1770 floppy controller at ports $18-$1B (drive select at $23).
    fdc: Wd1770,
    /// ADC0844 analogue joystick port at `$38` (joystick X/Y axes).
    adc: Adc0844,
    /// Joystick fire buttons `[joy1, joy2]`, read on port `$20` bits 0-1
    /// (active low).
    fire: [bool; 2],
    region: EinsteinRegion,
    cpu_tstates: u64,
    tstates_per_frame: u64,
    vdp_phase: u32,
    psg_phase: u8,
    frame_count: u64,
    /// When `Some`, every I/O port access is appended here (debug trace).
    io_trace: Option<Vec<IoEvent>>,
}

impl Einstein {
    /// Create a new Einstein with the given 8 KB X-TAL MOS ROM.
    #[must_use]
    pub fn new(rom: Vec<u8>, region: EinsteinRegion) -> Self {
        let vdp_region = match region {
            EinsteinRegion::Ntsc => VdpRegion::Ntsc,
            EinsteinRegion::Pal => VdpRegion::Pal,
        };
        let tstates_per_frame = match region {
            EinsteinRegion::Ntsc => NTSC_TSTATES_PER_FRAME,
            EinsteinRegion::Pal => PAL_TSTATES_PER_FRAME,
        };
        Self {
            cpu: Z80::new(),
            vdp: Tms9918::new(vdp_region),
            psg: Ay3_8910::new(AY_CLOCK_HZ, AY_SAMPLE_RATE, AY_SAMPLES_PER_FRAME),
            rom,
            ram: [0; 65536],
            rom_paged_in: true,
            keyboard: [0xFF; NUM_KEY_ROWS],
            extra_keys: 0xFF,
            kbd_int_enabled: false,
            kbd_int_pending: false,
            ctc_reg: 0,
            fdc: Wd1770::default(),
            adc: Adc0844::new(),
            fire: [false; 2],
            region,
            cpu_tstates: 0,
            tstates_per_frame,
            vdp_phase: 0,
            psg_phase: 0,
            frame_count: 0,
            io_trace: None,
        }
    }

    /// Run one frame and return T-states consumed.
    pub fn run_frame(&mut self) -> u64 {
        // The keyboard is serviced from a ~50 Hz interrupt: once per frame,
        // if the interrupt is enabled and any matrix key is down, raise it.
        // The MOS's IM 2 handler (vector $F7) then scans the matrix through
        // the AY ports and clears the request by reading $20.
        if self.kbd_int_enabled && self.keyboard.iter().any(|&row| row != 0xFF) {
            self.kbd_int_pending = true;
        }
        let target = self.cpu_tstates + self.tstates_per_frame;
        while self.cpu_tstates < target {
            self.tick_tstate();
        }
        self.frame_count += 1;
        self.tstates_per_frame
    }

    fn tick_tstate(&mut self) {
        self.cpu.tick();
        self.handle_bus();
        self.fdc.tick();

        self.vdp_phase += VDP_DOT_PHASE_NUMERATOR;
        while self.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
            self.vdp.tick();
            self.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
        }

        self.psg_phase ^= 1;
        if self.psg_phase == 0 {
            self.psg.tick();
        }

        // VDP /INT → Z80 /IRQ; CTC stub doesn't generate interrupts.
        self.cpu.irq = self.kbd_int_pending;

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
                // IM 2: the keyboard is the only interrupt source wired, so
                // its Z80-daisy device supplies low vector byte $F7. The
                // Z80 forms the handler address `(I << 8) | $F7`.
                self.cpu.data_in = 0xF7;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        if self.rom_paged_in && addr < 0x2000 {
            self.rom.get(addr as usize).copied().unwrap_or(0xFF)
        } else {
            self.ram[addr as usize]
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        // Writes always go to RAM, even with ROM paged in (so the
        // initial RAM clear can populate $0000-$1FFF before page-out).
        self.ram[addr as usize] = value;
    }

    /// Refresh the AY port B input from the keyboard matrix. The MOS
    /// drives the row-select lines on AY port A (R14, active low) and
    /// reads the column data on port B (R15); the column byte is the AND
    /// of every selected row's bits (a pressed key reads 0).
    fn refresh_keyboard(&mut self) {
        let line = self.psg.port_a_output();
        let mut columns = 0xFFu8;
        for row in 0..NUM_KEY_ROWS {
            if line & (1 << row) == 0 {
                columns &= self.keyboard[row];
            }
        }
        self.psg.set_port_b_input(columns);
    }

    fn io_read(&mut self, port: u16) -> u8 {
        match port as u8 {
            0x02 => {
                // AY data read. The keyboard hangs off the AY's I/O ports
                // (port A = row select, port B = column data), so refresh
                // port B from the matrix before the read resolves R15.
                self.refresh_keyboard();
                self.psg.read_data()
            }
            0x08 => self.vdp.read_data(),
            0x09 => self.vdp.read_status(),
            0x18..=0x1B => self.fdc.read((port & 0x03) as u8),
            // $20: reading clears the keyboard interrupt and returns the
            // joystick fire buttons (bits 0-1), printer status (bits 2-4)
            // and the GRAPH/CTRL/SHIFT modifier keys (bits 5-7, active
            // low). No joystick/printer here, so the low five bits read
            // high.
            0x20 => {
                self.kbd_int_pending = false;
                let mut data = 0x1F | (self.extra_keys & 0xE0);
                // Fire buttons are active low: joystick 1 on bit 0, joystick 2
                // on bit 1.
                if self.fire[0] {
                    data &= !0x01;
                }
                if self.fire[1] {
                    data &= !0x02;
                }
                data
            }
            0x23 => 0x00,
            // ADC0844 analogue joystick read ($38) — the selected channel's
            // conversion.
            0x38 => self.adc.read(),
            // Reading $24 toggles the ROM in/out of $0000-$1FFF, exactly like
            // writing it — the MOS uses this to read RAM beneath the ROM.
            0x24 => {
                self.rom_paged_in = !self.rom_paged_in;
                0xFF
            }
            0x28 => self.ctc_reg,
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, value: u8) {
        match port as u8 {
            // AY register select on $02 write, data write on $03 (the AY
            // data *read* is on $02). $00-$01 are the system reset latch.
            0x02 => self.psg.select_register(value),
            0x03 => self.psg.write_data(value),
            0x08 => self.vdp.write_data(value),
            0x09 => self.vdp.write_control(value),
            0x18..=0x1B => self.fdc.write((port & 0x03) as u8, value),
            // $20 bit 0 masks the keyboard interrupt (0 = enabled).
            0x20 => {
                self.kbd_int_enabled = value & 0x01 == 0;
                if !self.kbd_int_enabled {
                    self.kbd_int_pending = false;
                }
            }
            // $24 toggles the ROM bank at $0000-$1FFF between ROM and RAM.
            // (Port $21 is the ADC interrupt mask, not ROM paging.)
            0x24 => self.rom_paged_in = !self.rom_paged_in,
            // Drive/side select latch: bits 0-3 pick a drive, bit 4 the side.
            // (MAME `einstein_state::drsel_w`.)
            0x23 => {
                for d in 0..4 {
                    if value & (1 << d) != 0 {
                        self.fdc.set_drive(d);
                    }
                }
                self.fdc.set_side(value & 0x10);
            }
            0x28 => self.ctc_reg = value,
            // ADC0844 channel/mode select ($38 write).
            0x38 => self.adc.write(value),
            _ => {}
        }
    }

    /// Framebuffer (ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vdp.framebuffer()
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.vdp.framebuffer_width()
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.vdp.framebuffer_height()
    }

    /// Observe one byte on the Z80 bus without side effects.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// Insert a disk image (a flat, side-interleaved sector dump) into a drive
    /// (0-3) for the WD1770 to read, with the given geometry. The MOS boots to
    /// its `Ready` prompt with no disk; supply one to load an OS from it.
    pub fn insert_disk(
        &mut self,
        drive: usize,
        data: Vec<u8>,
        sectors_per_track: usize,
        sector_size: usize,
        sides: usize,
    ) {
        // Derive the track count from the image size and the rest of the
        // geometry (the flat dump is track-major, side-interleaved).
        let bytes_per_track = sectors_per_track * sector_size;
        let tracks = if bytes_per_track == 0 || sides == 0 {
            0
        } else {
            data.len() / (bytes_per_track * sides)
        };
        self.fdc.insert_disk(
            drive,
            Disk::new(data, tracks, sides, sectors_per_track, sector_size),
        );
    }

    /// Press a key at the given (row, column).
    pub fn press_key(&mut self, row: usize, col: u8) {
        if row < self.keyboard.len() && col < 8 {
            self.keyboard[row] &= !(1 << col);
        }
    }

    /// Release a key at the given (row, column).
    pub fn release_key(&mut self, row: usize, col: u8) {
        if row < self.keyboard.len() && col < 8 {
            self.keyboard[row] |= 1 << col;
        }
    }

    /// Set an analogue-joystick axis. `channel` 0-3 is joystick 1 X, joystick 1
    /// Y, joystick 2 X, joystick 2 Y; `value` is the 8-bit pot position
    /// (`0x80` = centre). Read back through the ADC0844 at `$38`. Out-of-range
    /// channels are ignored.
    pub fn set_adc_channel(&mut self, channel: u8, value: u8) {
        if let Some(slot) = self.adc.channels.get_mut(channel as usize) {
            *slot = value;
        }
    }

    /// Set a joystick fire button (`port` 1 or 2, `true` = pressed). Read on
    /// port `$20` bit 0 (joystick 1) / bit 1 (joystick 2), active low.
    /// Out-of-range ports clamp to the valid pair.
    pub fn set_fire_button(&mut self, port: u8, pressed: bool) {
        self.fire[usize::from(port.clamp(1, 2) - 1)] = pressed;
    }

    /// The 8-bit pot value latched on an ADC channel (0-3), or 0 for an
    /// out-of-range channel. For inspection and host-side input wiring.
    #[must_use]
    pub fn adc_channel(&self, channel: u8) -> u8 {
        self.adc
            .channels
            .get(channel as usize)
            .copied()
            .unwrap_or(0)
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
    pub fn region(&self) -> EinsteinRegion {
        self.region
    }

    /// Drain accumulated PSG audio samples for the most recent frame.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        let mut out = vec![0.0_f32; AY_SAMPLES_PER_FRAME];
        self.psg.end_frame(&mut out);
        if let Some(last) = out.iter().rposition(|s| *s != 0.0) {
            out.truncate(last + 1);
        } else {
            out.clear();
        }
        out
    }

    /// `true` if the X-TAL MOS ROM is currently visible at
    /// `$0000-$1FFF`.
    #[must_use]
    pub fn rom_paged_in(&self) -> bool {
        self.rom_paged_in
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
}

impl zilog_z80::Z80Stepper for Einstein {
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
        let mut rom = vec![0u8; 0x2000];
        rom[0x0008] = 0x18;
        rom[0x0009] = 0xFE;
        rom
    }

    #[test]
    fn ntsc_frame_returns_expected_tstates() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        let t = sys.run_frame();
        assert_eq!(t, NTSC_TSTATES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_frame_returns_expected_tstates() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Pal);
        let t = sys.run_frame();
        assert_eq!(t, PAL_TSTATES_PER_FRAME);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        for _ in 0..60 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 60);
    }

    #[test]
    fn rom_visible_at_reset_toggles_on_port_24() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        assert!(sys.rom_paged_in());
        assert_eq!(sys.mem_read(0x0008), 0x18);
        // $24 toggles the bank: write pages RAM in, $0008 then reads RAM.
        sys.io_write(0x24, 0x00);
        assert!(!sys.rom_paged_in());
        assert_eq!(sys.mem_read(0x0008), 0x00);
        // Toggling again brings the ROM back.
        sys.io_write(0x24, 0x00);
        assert!(sys.rom_paged_in());
        assert_eq!(sys.mem_read(0x0008), 0x18);
    }

    #[test]
    fn reading_port_24_also_toggles_the_rom() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        assert!(sys.rom_paged_in());
        let _ = sys.io_read(0x24);
        assert!(!sys.rom_paged_in());
    }

    #[test]
    fn writes_always_land_in_ram_even_with_rom_paged_in() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        // ROM still in view but write to RAM underneath.
        sys.mem_write(0x0100, 0x42);
        // Toggle the ROM out and re-read.
        sys.io_write(0x24, 0x00);
        assert_eq!(sys.mem_read(0x0100), 0x42);
    }

    #[test]
    fn keyboard_row_selected_via_ay_port_a() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        sys.keyboard[5] = 0xAB;
        // The MOS drives the row-select lines on AY port A (R14) and reads
        // the columns back on port B (R15). Select R14, drive row 5 low
        // (active low), then read R15 — it returns that row's columns.
        sys.io_write(0x02, 14); // select R14 (port A)
        sys.io_write(0x03, !(1 << 5)); // 0xDF: row 5 selected
        sys.io_write(0x02, 15); // select R15 (port B)
        assert_eq!(sys.io_read(0x02), 0xAB);
    }

    #[test]
    fn vdp_dot_ratio_is_three_per_two_tstates() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        let start = sys.vdp.scanline();
        for _ in 0..4 {
            sys.tick_tstate();
        }
        assert_eq!(sys.vdp.scanline(), start);
        assert_eq!(sys.vdp_phase, 0);
    }

    #[test]
    fn key_press_and_release() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        sys.press_key(2, 5);
        assert_eq!(sys.keyboard[2] & (1 << 5), 0);
        sys.release_key(2, 5);
        assert_eq!(sys.keyboard[2] & (1 << 5), 1 << 5);
    }

    #[test]
    fn analogue_joystick_reads_through_the_adc0844() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        sys.set_adc_channel(0, 0xC0); // joy1 X
        sys.set_adc_channel(1, 0x30); // joy1 Y

        // Single-ended channel 1 ($04) reads joy1 X back directly.
        sys.io_write(0x38, 0x04);
        assert_eq!(sys.io_read(0x38), 0xC0, "channel 1 = joy1 X");
        // Channel 2 ($05) reads joy1 Y.
        sys.io_write(0x38, 0x05);
        assert_eq!(sys.io_read(0x38), 0x30, "channel 2 = joy1 Y");
        // Idle joy2 X (channel 3) reads centre.
        sys.io_write(0x38, 0x06);
        assert_eq!(sys.io_read(0x38), 0x80, "channel 3 idle = centre");
    }

    #[test]
    fn joystick_fire_buttons_read_on_port_20_active_low() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        // Idle: bits 0-1 high (no fire).
        assert_eq!(sys.io_read(0x20) & 0x03, 0x03);
        sys.set_fire_button(1, true);
        assert_eq!(sys.io_read(0x20) & 0x01, 0, "joy1 fire → bit 0 low");
        assert_eq!(sys.io_read(0x20) & 0x02, 0x02, "joy2 idle → bit 1 high");
        sys.set_fire_button(2, true);
        assert_eq!(sys.io_read(0x20) & 0x03, 0, "both fire → bits 0-1 low");
    }

    #[test]
    fn fdc_reads_a_sector_from_an_inserted_disk() {
        // The WD1770 command engine is unit-tested in `western-digital-wd1770`;
        // here we only check the machine-level wiring: a disk inserted through
        // the public API reads back through ports $18-$1B after a $23 select.
        let bios = vec![0u8; 0x2000];
        let mut sys = Einstein::new(bios, EinsteinRegion::Pal);
        let mut data = vec![0u8; 10 * 512];
        data[0] = 0xAA; // track 0, sector 1, first byte
        data[511] = 0xBB; // last byte of that sector
        sys.insert_disk(0, data, 10, 512, 1);

        sys.io_write(0x23, 0x01); // drive 0, side 0
        sys.io_write(0x18, 0x00); // restore
        for _ in 0..128 {
            sys.fdc.tick();
        }
        sys.io_write(0x1A, 1); // sector register = 1
        sys.io_write(0x18, 0x80); // read sector
        for _ in 0..128 {
            sys.fdc.tick();
        }
        assert_ne!(sys.io_read(0x18) & 0x02, 0, "DRQ should be raised");
        assert_eq!(sys.io_read(0x1B), 0xAA, "first sector byte");
        let mut last = 0;
        for _ in 1..512 {
            last = sys.io_read(0x1B);
        }
        assert_eq!(last, 0xBB, "last sector byte");
        assert_eq!(
            sys.io_read(0x18) & 0x01,
            0,
            "BUSY clears after the transfer"
        );
    }

    #[test]
    fn fdc_read_with_no_disk_reports_record_not_found() {
        let bios = vec![0u8; 0x2000];
        let mut sys = Einstein::new(bios, EinsteinRegion::Pal);
        sys.io_write(0x1A, 1);
        sys.io_write(0x18, 0x80); // read sector, no disk inserted
        for _ in 0..128 {
            sys.fdc.tick();
        }
        let status = sys.io_read(0x18);
        assert_ne!(status & 0x10, 0, "record-not-found should be set");
        assert_eq!(status & 0x01, 0, "command should have finished");
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

impl Einstein {
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
