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
//! Einstein's Z80A runs at **4 MHz**, faster than the 3.58 MHz the rest
//! of the TMS9918 family uses. The VDP is fed through an **exact-Hz
//! accumulator** — 5.369318 MHz dot clock against the 4 MHz CPU, ≈1.342
//! dots/T-state — not the integer 3:2 (=1.5) counter the 3.58 MHz
//! machines share. With 3:2 the CPU effectively ran at ~3.58 MHz (≈11 %
//! slow); the exact ratio keeps it at its true 4 MHz. A frame is one
//! VDP raster (the VDP free-runs and is the timing anchor), same model
//! as the Memotech MTX (the other 4 MHz TMS9918 machine). PSG ticks
//! every other T-state for the CPU ÷ 2 = 2 MHz AY clock.

use gi_ay_3_8910::{Ay3_8910, AyWriteRecord, AyWriteWatch};
use ti_tms9918::{Tms9918, VdpRegion};
use western_digital_wd1770::{Disk, Wd1770};
use zilog_z80::{BusOp, Z80};

// Einstein's Z80A runs at 4 MHz; the TMS9918A dot clock is 5.369318 MHz.
// That ratio (≈1.342, *not* 3:2) is what makes the Einstein different from
// the 3.58 MHz TMS9918 machines — feeding the VDP through an exact-Hz
// accumulator instead of the integer 3:2 counter keeps the CPU at its true
// 4 MHz instead of the ~3.58 MHz the 3:2 approximation produced. Same model
// as the Memotech MTX (the other 4 MHz TMS9918 machine in the fleet).
const CPU_CLOCK_HZ: i64 = 4_000_000;
const VDP_CLOCK_HZ: i64 = 5_369_318;

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

/// A modifier key read on `$20` bits 5-7 (active low). The value is the
/// status-byte bit it clears when held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    /// GRAPH — `$20` bit 5.
    Graph = 0x20,
    /// CTRL — `$20` bit 6.
    Control = 0x40,
    /// SHIFT — `$20` bit 7.
    Shift = 0x80,
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
    /// VDP dot-clock accumulator (in CPU-Hz units). Each T-state adds
    /// [`VDP_CLOCK_HZ`]; every [`CPU_CLOCK_HZ`] accumulated ticks the VDP
    /// one dot — an exact 5.369318 MHz : 4 MHz ratio.
    vdp_accum: i64,
    psg_phase: u8,
    frame_count: u64,
    /// When `Some`, every I/O port access is appended here (debug trace).
    io_trace: Option<Vec<IoEvent>>,
    /// When `Some`, every write to the PSG data port ($03) is captured
    /// for the shared `watch_ay_*` tools. Host-side debug only, not
    /// part of the snapshot.
    ay_watch: Option<AyWriteWatch>,
}

impl Einstein {
    /// Create a new Einstein with the given 8 KB X-TAL MOS ROM.
    #[must_use]
    pub fn new(rom: Vec<u8>, region: EinsteinRegion) -> Self {
        let vdp_region = match region {
            EinsteinRegion::Ntsc => VdpRegion::Ntsc,
            EinsteinRegion::Pal => VdpRegion::Pal,
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
            vdp_accum: 0,
            psg_phase: 0,
            frame_count: 0,
            io_trace: None,
            ay_watch: None,
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
        // Run until the VDP completes a frame. The VDP is the timing anchor
        // (it free-runs its scanline counter), so a frame is exactly one
        // TMS9918 raster — and the CPU fits its true 4 MHz worth of T-states
        // into it, not the ~3.58 MHz the old fixed T-state budget implied.
        // Same VDP-frame-driven loop as the Memotech MTX.
        let start = self.cpu_tstates;
        let target_frame = self.frame_count + 1;
        // Defensive cap (~2× a PAL frame at 4 MHz) so a misbehaving VDP can't
        // spin forever.
        let cap = start + 160_000;
        while self.vdp.frame_count < target_frame && self.cpu_tstates < cap {
            self.tick_tstate();
        }
        self.frame_count = target_frame;
        self.cpu_tstates - start
    }

    fn tick_tstate(&mut self) {
        self.cpu.tick();
        self.handle_bus();
        self.fdc.tick();

        self.vdp_accum += VDP_CLOCK_HZ;
        while self.vdp_accum >= CPU_CLOCK_HZ {
            self.vdp_accum -= CPU_CLOCK_HZ;
            self.vdp.tick();
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
            0x03 => {
                if let Some(w) = &mut self.ay_watch {
                    w.record(self.cpu.regs.pc, self.psg.selected_register(), value);
                }
                self.psg.write_data(value);
            }
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

    /// Insert a disk from a CPCEMU standard or extended `.DSK` image (the
    /// container the Einstein TOSEC / MAME `einstein_flop` disks ship in) into
    /// a drive (0-3). Returns an error if the bytes are not a recognised DSK.
    ///
    /// The DSK stores sectors in physical (skewed) order with explicit IDs; we
    /// flatten by **sector ID** into the drive's geometry, so the relaxed
    /// (non-bit-timing) controller serves each ID correctly. Einstein disks use
    /// sector IDs 0-9, 512-byte sectors, 40 tracks, single sided.
    pub fn insert_cpc_dsk(&mut self, drive: usize, dsk: &[u8]) -> Result<(), String> {
        let disk = parse_cpc_dsk(dsk)?;
        self.fdc.insert_disk(drive, disk);
        Ok(())
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

    /// Hold or release a modifier key read on `$20` bits 5-7 (active low):
    /// GRAPH (bit 5), CTRL (bit 6), SHIFT (bit 7). The BREAK key itself is in
    /// the scanned matrix at row 0, column 0 (`press_key(0, 0)`), so a
    /// Ctrl-BREAK disk boot is `set_control(true)` + `press_key(0, 0)`.
    pub fn set_modifier(&mut self, modifier: Modifier, held: bool) {
        let bit = modifier as u8;
        if held {
            self.extra_keys &= !bit;
        } else {
            self.extra_keys |= bit;
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

impl zilog_z80::Z80Stepper for Einstein {
    fn z80_instructions_retired(&self) -> u64 {
        self.cpu.instructions_retired()
    }

    fn step_tick(&mut self) {
        self.tick_tstate();
    }
}

/// Parse a CPCEMU standard or extended `.DSK` image into a flat,
/// ID-addressable [`Disk`].
///
/// Layout: a 256-byte disk-information block, then for each track a 256-byte
/// track-information block followed by its sector data in physical order. Each
/// sector carries its own ID (`R`) in the track block's sector list; we place
/// the data by ID so the controller — which the host addresses by ID — serves
/// the right bytes regardless of the on-disk skew. Geometry is taken to be
/// uniform across tracks (true for the Einstein `einstein_flop` set: 40 tracks,
/// 1 side, 10 sectors of 512 bytes, IDs 0-9).
///
/// Reference: the CPCEMU DSK / "EXTENDED CPC DSK" format (the standard 3" disk
/// container, shared with Amstrad CPC / PCW / Spectrum +3).
fn parse_cpc_dsk(dsk: &[u8]) -> Result<Disk, String> {
    if dsk.len() < 0x100 {
        return Err("DSK too small for a disk-info block".into());
    }
    let extended = dsk.starts_with(b"EXTENDED");
    let standard = dsk.starts_with(b"MV - CPC");
    if !extended && !standard {
        return Err("not a CPCEMU .DSK (bad disk-info signature)".into());
    }

    let tracks = dsk[0x30] as usize;
    let sides = (dsk[0x31] as usize).max(1);
    if tracks == 0 {
        return Err("DSK declares zero tracks".into());
    }

    // Per-track byte size: a table of (size / 256) in the extended format, or a
    // single uniform word in the standard format.
    let track_size = |i: usize| -> usize {
        if extended {
            dsk.get(0x34 + i).map_or(0, |&b| b as usize * 256)
        } else {
            u16::from_le_bytes([dsk[0x32], dsk[0x33]]) as usize
        }
    };

    // Read the uniform geometry (sector count, size, lowest ID) from the first
    // present track, so we can size the flat buffer before placing sectors.
    let mut geometry: Option<(usize, usize, u8)> = None;
    let scan = 0x100;
    for ti in 0..tracks * sides {
        let tsize = track_size(ti);
        if tsize == 0 {
            continue;
        }
        let tib = dsk
            .get(scan..scan + 256)
            .ok_or("DSK truncated in the first track-info block")?;
        let nsec = tib[0x15] as usize;
        let mut min_id = u8::MAX;
        for s in 0..nsec {
            min_id = min_id.min(tib[0x18 + s * 8 + 2]);
        }
        geometry = Some((nsec, 128usize << tib[0x14], min_id));
        break;
    }
    let (sectors_per_track, sector_size, first_id) = geometry.ok_or("DSK has no present tracks")?;
    if sectors_per_track == 0 || sector_size == 0 {
        return Err("DSK track has no sectors".into());
    }

    // Single pass: copy each sector into the flat buffer at its ID slot.
    let mut flat = vec![0u8; tracks * sides * sectors_per_track * sector_size];
    let mut off = 0x100;
    for ti in 0..tracks * sides {
        let tsize = track_size(ti);
        if tsize == 0 {
            continue;
        }
        let tib = dsk
            .get(off..off + 256)
            .ok_or("DSK truncated in a track-info block")?;
        if !tib.starts_with(b"Track-Info") {
            // Some images carry a zeroed/blank track block (an unformatted or
            // deliberately-skipped track). Skip its sectors but keep the
            // file offset aligned by the declared track size.
            off += tsize;
            continue;
        }
        let track = tib[0x10] as usize;
        let side = tib[0x11] as usize;
        let nsec = tib[0x15] as usize;

        let mut data_off = off + 256;
        for s in 0..nsec {
            let entry = &tib[0x18 + s * 8..0x18 + s * 8 + 8];
            let id = entry[2];
            // Extended images record the real stored length per sector; the
            // standard format uses the track's N code.
            let stored = if extended {
                u16::from_le_bytes([entry[6], entry[7]]) as usize
            } else {
                sector_size
            };
            if let Some(index) = id.checked_sub(first_id).map(usize::from)
                && track < tracks
                && side < sides
                && index < sectors_per_track
            {
                let dst = ((track * sides + side) * sectors_per_track + index) * sector_size;
                let len = sector_size.min(dsk.len().saturating_sub(data_off));
                if dst + len <= flat.len() {
                    flat[dst..dst + len].copy_from_slice(&dsk[data_off..data_off + len]);
                }
            }
            data_off += stored;
        }
        off += tsize;
    }

    Ok(
        Disk::new(flat, tracks, sides, sectors_per_track, sector_size)
            .with_first_sector_id(first_id),
    )
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
    fn ntsc_frame_runs_one_vdp_frame_of_4mhz_tstates() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        // The VDP increments frame_count at vblank (scanline 192), so the
        // first run_frame is a short 193-line startup frame; measure a
        // steady-state one.
        sys.run_frame();
        let t = sys.run_frame();
        // One full NTSC VDP frame is 342 × 262 dots; at 4 MHz : 5.369318 MHz
        // that is ≈66 754 T-states — ~1.5× the ~44 730 the old 3.58 MHz-
        // equivalent budget produced.
        let expected = 342u64 * 262 * 4_000_000 / 5_369_318;
        assert!(
            t.abs_diff(expected) <= 400,
            "got {t} T-states, expected ≈{expected} for one 4 MHz NTSC frame"
        );
    }

    #[test]
    fn pal_frame_runs_one_vdp_frame_of_4mhz_tstates() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Pal);
        sys.run_frame(); // discard the short startup frame
        let t = sys.run_frame();
        // One full PAL VDP frame is 342 × 313 dots; at 4 MHz : 5.369318 MHz
        // that is ≈79 760 T-states (4 MHz / 50.16 Hz).
        let expected = 342u64 * 313 * 4_000_000 / 5_369_318;
        assert!(
            t.abs_diff(expected) <= 400,
            "got {t} T-states, expected ≈{expected} for one 4 MHz PAL frame"
        );
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
    fn vdp_dot_ratio_is_exact_5_369_to_4_mhz() {
        // The VDP advances 5_369_318 dots per 4_000_000 T-states — the true
        // 4 MHz Einstein ratio (≈1.342 dots/T-state), not the 3:2 (=1.5) the
        // 3.58 MHz TMS9918 machines use. The accumulator yields exactly
        // VDP_CLOCK_HZ dots over CPU_CLOCK_HZ T-states with no drift, the
        // same recurrence `tick_tstate` runs.
        let mut accum = 0i64;
        let mut dots = 0i64;
        for _ in 0..CPU_CLOCK_HZ {
            accum += VDP_CLOCK_HZ;
            while accum >= CPU_CLOCK_HZ {
                accum -= CPU_CLOCK_HZ;
                dots += 1;
            }
        }
        assert_eq!(dots, VDP_CLOCK_HZ);
        assert_eq!(accum, 0);
    }

    #[test]
    fn ay_watch_captures_psg_data_writes() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        assert!(sys.ay_write_watch_records().is_none());
        let cap = sys.start_ay_write_watch();
        assert!(cap > 0);
        sys.io_write(0x02, 7); // select R7
        sys.io_write(0x03, 0x38); // data
        sys.io_write(0x02, 8); // select R8
        sys.io_write(0x03, 0x0F); // data
        let records = sys.ay_write_watch_records().expect("armed");
        assert_eq!(records.len(), 2);
        assert_eq!((records[0].register, records[0].value), (7, 0x38));
        assert_eq!((records[1].register, records[1].value), (8, 0x0F));
        sys.clear_ay_write_watch_records();
        assert_eq!(sys.ay_write_watch_records().expect("armed").len(), 0);
        sys.stop_ay_write_watch();
        assert!(sys.ay_write_watch_records().is_none());
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

    /// Build a minimal extended CPC `.DSK`: `tracks` tracks, 1 side, `nsec`
    /// sectors of 512 bytes with IDs `0..nsec`, where sector (t, id) is filled
    /// with the byte `t ^ (id << 4) ^ 0x5A` so each sector is identifiable.
    fn synthetic_dsk(tracks: u8, nsec: u8) -> Vec<u8> {
        const SS: usize = 512;
        let track_len = 256 + nsec as usize * SS;
        let mut dsk = vec![0u8; 256 + tracks as usize * track_len];
        dsk[..23].copy_from_slice(b"EXTENDED CPC DSK File\r\n");
        dsk[0x30] = tracks;
        dsk[0x31] = 1;
        for t in 0..tracks as usize {
            dsk[0x34 + t] = (track_len / 256) as u8;
        }
        let mut off = 0x100;
        for t in 0..tracks {
            dsk[off..off + 12].copy_from_slice(b"Track-Info\r\n");
            dsk[off + 0x10] = t;
            dsk[off + 0x11] = 0;
            dsk[off + 0x14] = 2; // N=2 → 512
            dsk[off + 0x15] = nsec;
            for s in 0..nsec {
                let e = off + 0x18 + s as usize * 8;
                dsk[e] = t; // C
                dsk[e + 1] = 0; // H
                dsk[e + 2] = s; // R (sector ID)
                dsk[e + 3] = 2; // N
                dsk[e + 6] = (SS & 0xFF) as u8;
                dsk[e + 7] = (SS >> 8) as u8;
                let data = off + 256 + s as usize * SS;
                dsk[data..data + SS].fill(t ^ (s << 4) ^ 0x5A);
            }
            off += track_len;
        }
        dsk
    }

    #[test]
    fn cpc_dsk_round_trips_through_the_fdc() {
        let mut sys = Einstein::new(vec![0u8; 0x2000], EinsteinRegion::Pal);
        sys.insert_cpc_dsk(0, &synthetic_dsk(5, 10))
            .expect("synthetic DSK parses");

        // Read track 3, sector 7 through the ports and check the marker byte.
        sys.io_write(0x23, 0x01); // drive 0, side 0
        sys.io_write(0x1B, 3); // data register = target track
        sys.io_write(0x18, 0x10); // seek
        for _ in 0..128 {
            sys.fdc.tick();
        }
        assert_eq!(sys.io_read(0x19), 3, "seeked to track 3");
        sys.io_write(0x1A, 7); // sector register = ID 7
        sys.io_write(0x18, 0x80); // read sector
        for _ in 0..128 {
            sys.fdc.tick();
        }
        let expected = 3u8 ^ (7 << 4) ^ 0x5A;
        for i in 0..512 {
            assert_eq!(sys.io_read(0x1B), expected, "byte {i} of track 3 sector 7");
        }
    }

    #[test]
    fn cpc_dsk_rejects_non_dsk() {
        let mut sys = Einstein::new(vec![0u8; 0x2000], EinsteinRegion::Pal);
        assert!(sys.insert_cpc_dsk(0, b"not a disk image at all").is_err());
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
