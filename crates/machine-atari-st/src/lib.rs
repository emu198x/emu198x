//! The Atari ST: the 520ST and 1040ST (STF), PAL and NTSC.
//!
//! A 68000 at a quarter of the master clock, with:
//!
//! - **GLUE and MMU** (`atari-st-glue-mmu`): video timing, the video counter,
//!   RAM mapping and the DMA address.
//! - **Shifter** (`atari-st-shifter`): pixels from the MMU's video words.
//! - **MFP 68901** (`motorola-mfp-68901`): timers and the interrupt
//!   controller, on a 2.4576 MHz crystal of its own.
//! - **Two 6850 ACIAs** (`motorola-acia-6850`): the keyboard link at
//!   7812.5 baud and MIDI at 31250, both from a 500 kHz clock.
//! - **YM2149** (`emu198x-gi-ay-3-8910` in its YM2149 variant): sound at
//!   a quarter of the CPU clock, its port A selecting the floppy drive and side.
//! - **WD1772** (`western-digital-wd1770`) behind the DMA chip ([`dma`]).
//! - **The IKBD** ([`ikbd`]): keyboard, mouse, joysticks and clock, simulated
//!   at the protocol level.
//!
//! # Clocking
//!
//! One [`AtariSt::tick`] is one CPU cycle: four master clocks, 32.084988 MHz
//! on a PAL machine and 32.0424 MHz on NTSC. The CPU, the GLUE, the MMU and
//! the Shifter all step on it; nothing modelled here observes the master clock
//! more finely. The YM2149 steps every fourth cycle and the ACIAs' 500 kHz
//! clock every sixteenth. The MFP's crystal is unrelated, so an integer
//! accumulator steps it 2,457,600 times for every CPU-clock's worth of cycles:
//! exact, with no drift.
//!
//! # Memory map
//!
//! | Range | |
//! |---|---|
//! | `$000000`–`$000007` | ROM's first eight bytes (the reset vectors), read only |
//! | `$000000`–`$3FFFFF` | RAM through the MMU; the first 2 KB is supervisor-only |
//! | `$FA0000`–`$FBFFFF` | Cartridge; reads `$FFFF` with none fitted |
//! | `$FC0000`–`$FEFFFF` | 192 KB TOS (TOS 1.0x) |
//! | `$E00000`–`$E3FFFF` | 256 KB TOS (TOS 1.06 and later) |
//! | `$FF8000`–`$FFFFFF` | I/O, supervisor-only |
//!
//! Anything else is a bus error, as are writes to ROM, user-mode access to
//! the protected ranges, and I/O addresses an STF does not decode. EmuTOS and
//! TOS find out which hardware is present by catching those bus errors.
//!
//! # Interrupts
//!
//! The GLUE raises HBL at level 2 and VBL at level 4, both autovectored; each
//! stays pending until acknowledged. The MFP interrupts at level 6 and
//! supplies its own vector. Its GPIP lines carry, active low: the ACIAs'
//! combined interrupt (bit 4), the floppy controller's INTRQ (bit 5) and the
//! monochrome monitor detect (bit 7). Timer B's input is display enable.
//!
//! # Bus timing
//!
//! The MMU shares RAM with the video fetch on a four-cycle rhythm, so every
//! CPU bus cycle ends on a four-cycle boundary: an access that would end
//! between them waits. This is the ST's familiar "rounding to four cycles".
//!
//! # Not modelled
//!
//! - The extra wait states of the 6800-style ACIA accesses, and the four the
//!   DMA registers add. ACIA and DMA accesses follow the four-cycle rule only.
//! - The CPU cycles DMA bursts steal.
//! - Mid-line video tricks (see `atari-st-glue-mmu`).
//! - The ACSI hard-disk port, the printer port and the RS-232 lines.

#![forbid(unsafe_code)]

pub mod dma;
pub mod ikbd;

pub use atari_st_glue_mmu::Frequency;
use atari_st_glue_mmu::GlueMmu;
use atari_st_shifter::Shifter;
use emu198x_gi_ay_3_8910::{Ay3_8910, Variant};
use motorola_68000::Cpu68000;
use motorola_68000::bus::{BusStatus, FunctionCode, interrupt_acknowledge_level};
use motorola_68000::cpu::State;
use motorola_acia_6850::Acia6850;
use motorola_mfp_68901::{Mfp68901, TimerId};
use serde::{Deserialize, Serialize};
use western_digital_wd1770::{Disk, Wd1770};

pub use dma::Dma;
pub use ikbd::{Ikbd, Stick};

/// The PAL master clock.
pub const PAL_MASTER_HZ: u64 = 32_084_988;
/// The NTSC master clock.
pub const NTSC_MASTER_HZ: u64 = 32_042_400;
/// The MFP's crystal.
pub const MFP_HZ: u64 = 2_457_600;
/// The audio output rate.
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// The colour framebuffer: 384 CPU cycles of each line (the 320 the display
/// covers and 32 of border either side) at two pixels a cycle, by the 200
/// displayed lines and 29 of border above and below.
pub const COLOUR_WIDTH: usize = 768;
pub const COLOUR_HEIGHT: usize = 258;
/// The monochrome framebuffer: the SM124's 640 × 400.
pub const MONO_WIDTH: usize = 640;
pub const MONO_HEIGHT: usize = 400;

/// Samples the sound chip holds between [`AtariSt::take_audio`] calls: a
/// second.
const AUDIO_BUFFER: usize = AUDIO_SAMPLE_RATE as usize;
const BORDER_CYCLES: u16 = 32;
/// CPU cycles the Shifter's pixels run behind display enable: a group
/// starts once its last word has come, the fourth fetch in low resolution,
/// 12 cycles after the first. Medium resolution's groups come sooner, so its
/// picture sits 8 cycles further left inside the same border.
const SHIFTER_DELAY: u16 = 12;
const BORDER_LINES: u16 = 29;
/// CPU cycles per byte the floppy delivers at double density (32 µs).
const DMA_BYTE_CYCLES: u16 = 256;
/// CPU cycles between IKBD scans of the mouse and sticks (100 Hz).
const IKBD_SCAN_CYCLES: u32 = 80_000;
/// The CPU cycle counter's mask for the four-cycle bus rhythm.
const BUS_PHASE: u64 = 3;

/// An ST model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Model {
    /// 520ST(F/M): 512 KB in bank 0.
    St520,
    /// 1040ST(F): 512 KB in each bank.
    St1040,
}

impl Model {
    fn banks(self) -> (u32, u32) {
        match self {
            Self::St520 => (512 * 1024, 0),
            Self::St1040 => (512 * 1024, 512 * 1024),
        }
    }
}

/// The machine's video standard: which master clock it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoStandard {
    Pal,
    Ntsc,
}

/// The monitor plugged in. It decides the monochrome-detect line, so TOS
/// picks high resolution for a mono monitor and low for colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Monitor {
    Colour,
    Mono,
}

/// Why a TOS image was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RomError {
    /// Not 192 KB or 256 KB.
    Size(usize),
}

impl std::fmt::Display for RomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Size(size) => write!(f, "a TOS image is 192 KB or 256 KB, not {size} bytes"),
        }
    }
}

impl std::error::Error for RomError {}

/// An Atari ST.
#[derive(Serialize, Deserialize)]
pub struct AtariSt {
    pub cpu: Cpu68000,
    pub glue: GlueMmu,
    pub shifter: Shifter,
    pub mfp: Mfp68901,
    pub acia_keyboard: Acia6850,
    pub acia_midi: Acia6850,
    pub psg: Ay3_8910,
    pub fdc: Wd1770,
    pub dma: Dma,
    pub ikbd: Ikbd,

    model: Model,
    standard: VideoStandard,
    monitor: Monitor,
    ram: Vec<u8>,
    /// TOS is firmware: a snapshot does not carry it; the host supplies it
    /// again before restoring.
    #[serde(skip)]
    rom: Vec<u8>,
    rom_base: u32,

    cycle: u64,
    mfp_phase: u64,
    hbl_pending: bool,
    vbl_pending: bool,
    dma_timer: u16,
    ikbd_scan: u32,
    floppy_side: u8,

    framebuffer: Vec<u32>,
    frame_mono: bool,
    /// The AY's sample clock, mirrored to know how many samples it has
    /// produced: AY clocks times the output rate, against the AY rate.
    audio_error: u64,
    audio_pending: usize,
}

impl AtariSt {
    /// An ST of `model` and `standard` with `monitor` attached, running
    /// `tos` (192 KB or 256 KB), powered on.
    ///
    /// # Errors
    ///
    /// [`RomError`] for a TOS image of the wrong size.
    pub fn new(
        model: Model,
        standard: VideoStandard,
        monitor: Monitor,
        tos: Vec<u8>,
    ) -> Result<Self, RomError> {
        let rom_base = rom_base(tos.len())?;
        let (bank0, bank1) = model.banks();
        let cpu_hz = (master_hz(standard) / 4) as u32;
        let mut psg = Ay3_8910::new(cpu_hz / 4, AUDIO_SAMPLE_RATE, AUDIO_BUFFER);
        psg.set_variant(Variant::Ym2149);
        let mut machine = Self {
            cpu: Cpu68000::new(),
            glue: GlueMmu::new(bank0, bank1),
            shifter: Shifter::new(),
            mfp: Mfp68901::new(),
            acia_keyboard: Acia6850::new(),
            acia_midi: Acia6850::new(),
            psg,
            fdc: Wd1770::new(),
            dma: Dma::default(),
            ikbd: Ikbd::new(cpu_hz),
            model,
            standard,
            monitor,
            ram: vec![0; (bank0 + bank1) as usize],
            rom: tos,
            rom_base,
            cycle: 0,
            mfp_phase: 0,
            hbl_pending: false,
            vbl_pending: false,
            dma_timer: 0,
            ikbd_scan: 0,
            floppy_side: 0,
            framebuffer: vec![0; COLOUR_WIDTH * COLOUR_HEIGHT],
            frame_mono: false,
            audio_error: 0,
            audio_pending: 0,
        };
        machine.drive_gpip();
        machine.reset_cpu();
        Ok(machine)
    }

    fn reset_cpu(&mut self) {
        let long = |rom: &[u8], at: usize| {
            u32::from_be_bytes([rom[at], rom[at + 1], rom[at + 2], rom[at + 3]])
        };
        let ssp = long(&self.rom, 0);
        let pc = long(&self.rom, 4);
        self.cpu.reset_to(ssp, pc);
    }

    /// Press the reset button: the CPU restarts and the reset line clears
    /// the MFP, the sound chip and the floppy controller.
    pub fn reset(&mut self) {
        self.reset_peripherals();
        self.reset_cpu();
    }

    fn reset_peripherals(&mut self) {
        self.mfp.reset();
        self.psg = Ay3_8910::new(self.psg_hz(), AUDIO_SAMPLE_RATE, AUDIO_BUFFER);
        self.psg.set_variant(Variant::Ym2149);
        self.audio_error = 0;
        self.audio_pending = 0;
        let disks: Vec<Option<Disk>> = (0..2).map(|d| self.fdc.eject_disk(d)).collect();
        self.fdc = Wd1770::new();
        for (drive, disk) in disks.into_iter().enumerate() {
            if let Some(disk) = disk {
                self.fdc.insert_disk(drive, disk);
            }
        }
        self.dma = Dma::default();
        self.drive_gpip();
    }

    /// Re-apply what a snapshot does not carry: TOS and the sound chip's
    /// variant. Call after deserialising.
    ///
    /// # Errors
    ///
    /// [`RomError`] for a TOS image of the wrong size.
    pub fn finish_restore(&mut self, tos: Vec<u8>) -> Result<(), RomError> {
        self.rom_base = rom_base(tos.len())?;
        self.rom = tos;
        self.psg.set_variant(Variant::Ym2149);
        Ok(())
    }

    // ---- Configuration and media ----

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    #[must_use]
    pub fn standard(&self) -> VideoStandard {
        self.standard
    }

    #[must_use]
    pub fn monitor(&self) -> Monitor {
        self.monitor
    }

    /// The CPU clock: a quarter of the master clock.
    #[must_use]
    pub fn cpu_hz(&self) -> u64 {
        master_hz(self.standard) / 4
    }

    /// The YM2149's clock: a sixteenth of the master clock, about 2 MHz.
    fn psg_hz(&self) -> u32 {
        (self.cpu_hz() / 4) as u32
    }

    /// The master clock.
    #[must_use]
    pub fn master_hz(&self) -> u64 {
        master_hz(self.standard)
    }

    /// Put `disk` in drive `drive` (0 = A, 1 = B).
    pub fn insert_disk(&mut self, drive: usize, disk: Disk) {
        self.fdc.insert_disk(drive, disk);
    }

    /// Take the disk out of `drive`.
    pub fn eject_disk(&mut self, drive: usize) -> Option<Disk> {
        self.fdc.eject_disk(drive)
    }

    /// The disk in `drive`, for writing back.
    #[must_use]
    pub fn disk(&self, drive: usize) -> Option<&Disk> {
        self.fdc.disk(drive)
    }

    // ---- Running ----

    /// CPU cycles since power-on.
    #[must_use]
    pub fn cycles(&self) -> u64 {
        self.cycle
    }

    /// Run until the GLUE finishes a frame. Returns the CPU cycles it took.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.glue.frames();
        let start_cycle = self.cycle;
        while self.glue.frames() == start {
            self.tick();
        }
        self.cycle - start_cycle
    }

    /// One CPU cycle.
    pub fn tick(&mut self) {
        let (line, line_cycle) = self.glue.position();
        if line == 0 && line_cycle == 0 {
            self.start_frame();
        }
        self.glue.tick();
        if let Some(address) = self.glue.take_fetch() {
            let word = self.ram_word(address);
            self.shifter.load(word);
        }
        let pixels = self.shifter.tick(self.glue.display_enable());
        self.draw(line, line_cycle, pixels.as_slice());
        if self.glue.take_hbl() {
            self.hbl_pending = true;
        }
        if self.glue.take_vbl() {
            self.vbl_pending = true;
        }
        self.mfp
            .set_timer_input(TimerId::B, self.glue.timer_b_input());

        // The MFP's own crystal.
        self.mfp_phase += MFP_HZ;
        let cpu_hz = self.cpu_hz();
        while self.mfp_phase >= cpu_hz {
            self.mfp_phase -= cpu_hz;
            self.mfp.tick();
        }

        if self.cycle & 0x0F == 0 {
            self.tick_serial();
        }
        if self.cycle & 0x03 == 0 {
            self.psg.tick();
            self.audio_error += u64::from(AUDIO_SAMPLE_RATE);
            let psg_hz = u64::from(self.psg_hz());
            if self.audio_error >= psg_hz {
                self.audio_error -= psg_hz;
                self.audio_pending += 1;
            }
        }
        self.tick_ikbd();
        self.fdc.tick();
        self.tick_dma();
        self.drive_gpip();

        self.cpu.ipl = self.interrupt_level();
        self.service_bus();
        self.cpu.tick();
        if self.cpu.reset_out {
            self.cpu.reset_out = false;
            self.reset_peripherals();
        }
        self.cycle += 1;
    }

    fn interrupt_level(&self) -> u8 {
        if self.mfp.irq() {
            6
        } else if self.vbl_pending {
            4
        } else if self.hbl_pending {
            2
        } else {
            0
        }
    }

    fn tick_serial(&mut self) {
        self.acia_keyboard.tick();
        self.acia_midi.tick();
        for byte in self.acia_keyboard.take_transmitted() {
            self.ikbd.receive(byte);
        }
        // MIDI out goes nowhere; MIDI in has nothing plugged in.
        self.acia_midi.take_transmitted();
    }

    fn tick_ikbd(&mut self) {
        self.ikbd.tick();
        self.ikbd_scan += 1;
        if self.ikbd_scan >= IKBD_SCAN_CYCLES {
            self.ikbd_scan = 0;
            self.ikbd.scan();
        }
        // The IKBD sends at the line rate: one byte whenever the ACIA has
        // finished taking the last.
        if !self.acia_keyboard.receiving()
            && let Some(byte) = self.ikbd.next_output()
        {
            self.acia_keyboard.receive(byte);
        }
    }

    fn tick_dma(&mut self) {
        self.dma_timer += 1;
        if self.dma_timer < DMA_BYTE_CYCLES {
            return;
        }
        self.dma_timer = 0;
        if !self.fdc.drq() || !self.dma.floppy() {
            return;
        }
        if self.dma.writing() {
            if self.dma.needs_fill() {
                let address = self.glue.dma_address();
                let burst: Vec<u8> = (0..dma::FIFO_SIZE as u32)
                    .map(|i| self.ram_byte(address + i))
                    .collect();
                self.dma.fill(&burst);
                self.glue.advance_dma(dma::FIFO_SIZE as u32);
            }
            if let Some(byte) = self.dma.pull() {
                self.fdc.write(3, byte);
            }
        } else {
            let byte = self.fdc.read(3);
            if let Some(burst) = self.dma.push(byte) {
                let address = self.glue.dma_address();
                for (i, byte) in burst.into_iter().enumerate() {
                    self.write_ram_byte(address + i as u32, byte);
                }
                self.glue.advance_dma(dma::FIFO_SIZE as u32);
            }
        }
    }

    /// Drive the MFP's GPIP inputs from the lines wired to them.
    fn drive_gpip(&mut self) {
        let acia_irq = self.acia_keyboard.irq() || self.acia_midi.irq();
        let mut gpip = 0xFF;
        if acia_irq {
            gpip &= !0x10;
        }
        if self.fdc.intrq() {
            gpip &= !0x20;
        }
        if self.monitor == Monitor::Mono {
            gpip &= !0x80;
        }
        self.mfp.set_gpip_inputs(gpip);
    }

    // ---- Video output ----

    fn start_frame(&mut self) {
        let mono =
            self.glue.frequency() == Frequency::Hz71 || (self.shifter.read_resolution() == 2);
        if mono != self.frame_mono {
            self.frame_mono = mono;
            let size = if mono {
                MONO_WIDTH * MONO_HEIGHT
            } else {
                COLOUR_WIDTH * COLOUR_HEIGHT
            };
            self.framebuffer = vec![0; size];
        }
    }

    fn draw(&mut self, line: u16, cycle: u16, pixels: &[u32]) {
        let frequency = self.glue.frequency();
        let (de_start, _, first_line, _) = frequency.display_window();
        if self.frame_mono {
            if frequency != Frequency::Hz71 {
                return;
            }
            let (Some(y), Some(x)) = (
                line.checked_sub(first_line),
                // One word is a whole group in high resolution: no delay.
                cycle.checked_sub(de_start),
            ) else {
                return;
            };
            let x = usize::from(x) * 4;
            if usize::from(y) >= MONO_HEIGHT || x + 4 > MONO_WIDTH || pixels.len() != 4 {
                return;
            }
            let row = usize::from(y) * MONO_WIDTH;
            self.framebuffer[row + x..row + x + 4].copy_from_slice(pixels);
            return;
        }
        if frequency == Frequency::Hz71 {
            // A mono frame on a colour monitor: nothing it can show.
            return;
        }
        let (Some(y), Some(x)) = (
            line.checked_sub(first_line - BORDER_LINES),
            cycle.checked_sub(de_start + SHIFTER_DELAY - BORDER_CYCLES),
        ) else {
            return;
        };
        let (y, x) = (usize::from(y), usize::from(x) * 2);
        if y >= COLOUR_HEIGHT || x + 2 > COLOUR_WIDTH {
            return;
        }
        let row = y * COLOUR_WIDTH + x;
        match pixels {
            [one] => {
                self.framebuffer[row] = *one;
                self.framebuffer[row + 1] = *one;
            }
            [a, b] => {
                self.framebuffer[row] = *a;
                self.framebuffer[row + 1] = *b;
            }
            // High resolution on a colour monitor: halve it.
            [a, _, c, _] => {
                self.framebuffer[row] = *a;
                self.framebuffer[row + 1] = *c;
            }
            _ => {}
        }
    }

    /// The picture, `0x00RRGGBB`, [`AtariSt::frame_width`] pixels a row.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    #[must_use]
    pub fn frame_width(&self) -> usize {
        if self.frame_mono {
            MONO_WIDTH
        } else {
            COLOUR_WIDTH
        }
    }

    #[must_use]
    pub fn frame_height(&self) -> usize {
        if self.frame_mono {
            MONO_HEIGHT
        } else {
            COLOUR_HEIGHT
        }
    }

    // ---- Audio ----

    /// The sound since the last call: mono samples at
    /// [`AUDIO_SAMPLE_RATE`], `0.0`–`1.0`.
    pub fn take_audio(&mut self) -> Vec<f32> {
        let mut out = vec![0.0; self.audio_pending.min(AUDIO_BUFFER)];
        self.audio_pending = 0;
        self.psg.end_frame(&mut out);
        out
    }

    // ---- Input ----

    /// A key went down or up; `scancode` is the ST's.
    pub fn key(&mut self, scancode: u8, pressed: bool) {
        self.ikbd.key(scancode, pressed);
    }

    /// The mouse moved by `dx`, `dy`.
    pub fn mouse_move(&mut self, dx: i32, dy: i32) {
        self.ikbd.mouse_move(dx, dy);
    }

    /// The mouse buttons.
    pub fn mouse_buttons(&mut self, left: bool, right: bool) {
        self.ikbd.mouse_buttons(left, right);
    }

    /// One mouse button went down or up.
    pub fn mouse_button(&mut self, left: bool, pressed: bool) {
        self.ikbd.mouse_button(left, pressed);
    }

    /// Joystick `port`'s state.
    pub fn joystick(&mut self, port: usize, stick: Stick) {
        self.ikbd.joystick(port, stick);
    }

    // ---- The 68000's bus ----

    fn service_bus(&mut self) {
        let State::BusCycle {
            addr,
            fc,
            is_read,
            is_word,
            data,
            cycle_count,
            ..
        } = self.cpu.state.clone()
        else {
            self.cpu.bus_status = BusStatus::Wait;
            return;
        };
        if cycle_count < 3 || self.cycle & BUS_PHASE != BUS_PHASE {
            self.cpu.bus_status = BusStatus::Wait;
            return;
        }
        if fc == FunctionCode::InterruptAck {
            self.cpu.bus_status = self.acknowledge(interrupt_acknowledge_level(addr));
            return;
        }
        let supervisor = matches!(
            fc,
            FunctionCode::SupervisorData | FunctionCode::SupervisorProgram
        );
        let address = addr & 0x00FF_FFFF;
        let result = if is_read {
            self.read(address, is_word, supervisor)
                .map(BusStatus::Ready)
        } else {
            self.write(address, is_word, data.unwrap_or(0), supervisor)
                .map(|()| BusStatus::Ready(0))
        };
        self.cpu.bus_status = result.unwrap_or(BusStatus::Error);
    }

    fn acknowledge(&mut self, level: u8) -> BusStatus {
        match level {
            6 => self.mfp.acknowledge().map_or(BusStatus::Error, |vector| {
                BusStatus::Ready(u16::from(vector))
            }),
            4 => {
                self.vbl_pending = false;
                BusStatus::Ready(24 + 4)
            }
            2 => {
                self.hbl_pending = false;
                BusStatus::Ready(24 + 2)
            }
            _ => BusStatus::Error,
        }
    }

    /// A CPU read. `None` is a bus error.
    fn read(&mut self, address: u32, is_word: bool, supervisor: bool) -> Option<u16> {
        let even = address & !1;
        let word = match address {
            0x00_0000..=0x00_0007 => self.rom_word(even)?,
            0x00_0008..=0x3F_FFFF => {
                if address < 0x800 && !supervisor {
                    return None;
                }
                self.ram_word(even)
            }
            0xFA_0000..=0xFB_FFFF => 0xFFFF,
            0xE0_0000..=0xEF_FFFF | 0xFC_0000..=0xFE_FFFF => self.rom_word(even)?,
            0xFF_8000..=0xFF_FFFF => {
                if !supervisor {
                    return None;
                }
                self.io_read(address, is_word)?
            }
            _ => return None,
        };
        Some(if is_word {
            word
        } else if address & 1 == 0 {
            word >> 8
        } else {
            word & 0xFF
        })
    }

    /// A CPU write. `None` is a bus error.
    fn write(&mut self, address: u32, is_word: bool, value: u16, supervisor: bool) -> Option<()> {
        match address {
            0x00_0000..=0x3F_FFFF => {
                if address < 0x800 && !supervisor {
                    return None;
                }
                if is_word {
                    self.write_ram_byte(address & !1, (value >> 8) as u8);
                    self.write_ram_byte(address | 1, value as u8);
                } else {
                    self.write_ram_byte(address, value as u8);
                }
                Some(())
            }
            // An empty cartridge port ignores writes.
            0xFA_0000..=0xFB_FFFF => Some(()),
            0xFF_8000..=0xFF_FFFF => {
                if !supervisor {
                    return None;
                }
                self.io_write(address, is_word, value)
            }
            _ => None,
        }
    }

    fn rom_word(&self, address: u32) -> Option<u16> {
        let offset = if address < 8 {
            address
        } else {
            address.checked_sub(self.rom_base)?
        } as usize;
        let hi = *self.rom.get(offset)?;
        let lo = *self.rom.get(offset + 1)?;
        Some(u16::from(hi) << 8 | u16::from(lo))
    }

    fn ram_byte(&self, address: u32) -> u8 {
        self.glue
            .translate(address)
            .and_then(|offset| self.ram.get(offset).copied())
            .unwrap_or(0xFF)
    }

    fn ram_word(&self, address: u32) -> u16 {
        u16::from(self.ram_byte(address)) << 8 | u16::from(self.ram_byte(address | 1))
    }

    fn write_ram_byte(&mut self, address: u32, value: u8) {
        if let Some(offset) = self.glue.translate(address)
            && let Some(byte) = self.ram.get_mut(offset)
        {
            *byte = value;
        }
    }

    // ---- I/O ----

    /// Read the I/O at `address`, as a word whose byte lanes follow the
    /// address. `None` is a bus error.
    fn io_read(&mut self, address: u32, is_word: bool) -> Option<u16> {
        let even = address & !1;
        let offset = (even & 0x7FFF) as u16;
        match even {
            0xFF_8240..=0xFF_825F => {
                return Some(
                    self.shifter
                        .read_palette(((address - 0xFF_8240) / 2) as usize),
                );
            }
            0xFF_8604 => {
                let value = match self.dma.target() {
                    dma::Target::SectorCount => self.dma.recent(),
                    dma::Target::Floppy(register) => {
                        let value = u16::from(self.fdc.read(register)) | 0xFF00;
                        self.dma.note(value);
                        value
                    }
                    dma::Target::HardDisk => 0xFFFF,
                };
                return Some(value);
            }
            0xFF_8606 => return Some(self.dma.status(self.fdc.drq())),
            0xFF_8800..=0xFF_88FF => {
                // Register select reads the selected register; data does too.
                let value = if address & 2 == 0 {
                    self.psg.read_data()
                } else {
                    0xFF
                };
                return Some(u16::from(value) << 8 | 0xFF);
            }
            0xFF_FA00..=0xFF_FA3F => {
                // The MFP sits on the low byte lane; its even bytes are
                // undecoded and fault.
                if !is_word && address & 1 == 0 {
                    return None;
                }
                let register = ((address & 0x3F) >> 1) as u8;
                let value = if register <= 23 {
                    self.mfp.read(register)
                } else {
                    0xFF
                };
                return Some(0xFF00 | u16::from(value));
            }
            0xFF_FC00..=0xFF_FC1F => {
                let value = match address & 0x1E {
                    0x00 => self.acia_keyboard.read(false),
                    0x02 => self.acia_keyboard.read(true),
                    0x04 => self.acia_midi.read(false),
                    0x06 => self.acia_midi.read(true),
                    _ => 0xFF,
                };
                // The ACIAs answer on the high byte lane.
                return Some(u16::from(value) << 8 | 0xFF);
            }
            _ => {}
        }
        // The GLUE/MMU's byte registers and the void ones around them. A
        // byte access touches only its own lane.
        let hi = if is_word || address & 1 == 0 {
            self.glue_or_void(offset)?
        } else {
            0xFF
        };
        let lo = if is_word || address & 1 == 1 {
            self.glue_or_void(offset | 1)?
        } else {
            0xFF
        };
        Some(u16::from(hi) << 8 | u16::from(lo))
    }

    /// A GLUE/MMU byte register, the Shifter's resolution, or one of the
    /// STF's undecoded-but-harmless bytes.
    fn glue_or_void(&self, offset: u16) -> Option<u8> {
        if let Some(value) = self.glue.read(offset) {
            return Some(value);
        }
        match offset {
            0x260 => Some(self.shifter.read_resolution()),
            0x261 | 0x20B | 0x20D | 0x20F | 0x60F | 0x262..=0x27F => Some(0xFF),
            _ => None,
        }
    }

    /// Write the I/O at `address`. `None` is a bus error.
    fn io_write(&mut self, address: u32, is_word: bool, value: u16) -> Option<()> {
        let even = address & !1;
        let offset = (even & 0x7FFF) as u16;
        // A byte write carries its byte in the low eight bits whichever
        // lane it is for; a word carries both.
        let byte_hi = (value >> 8) as u8;
        let byte_lo = value as u8;
        let byte = if is_word { byte_hi } else { byte_lo };
        match even {
            0xFF_8240..=0xFF_825F => {
                let index = ((even - 0xFF_8240) / 2) as usize;
                if is_word {
                    self.shifter.write_palette(index, value);
                } else {
                    self.shifter
                        .write_palette_byte(index, address & 1 == 0, byte_lo);
                }
                return Some(());
            }
            0xFF_8604 => {
                let word = if is_word { value } else { u16::from(byte_lo) };
                self.dma.note(word);
                match self.dma.target() {
                    dma::Target::SectorCount => self.dma.set_sector_count(word),
                    dma::Target::Floppy(register) => self.fdc.write(register, word as u8),
                    dma::Target::HardDisk => {}
                }
                return Some(());
            }
            0xFF_8606 => {
                let word = if is_word { value } else { u16::from(byte_lo) };
                self.dma.write_mode(word);
                return Some(());
            }
            0xFF_8800..=0xFF_88FF => {
                if address & 2 == 0 {
                    self.psg.select_register(byte & 0x0F);
                } else {
                    self.psg.write_data(byte);
                    if self.psg.selected_register() == 14 {
                        self.floppy_select(byte);
                    }
                }
                return Some(());
            }
            0xFF_FA00..=0xFF_FA3F => {
                if !is_word && address & 1 == 0 {
                    return None;
                }
                let register = ((address & 0x3F) >> 1) as u8;
                if register <= 23 {
                    self.mfp.write(register, byte_lo);
                }
                return Some(());
            }
            0xFF_FC00..=0xFF_FC1F => {
                match address & 0x1E {
                    0x00 => self.acia_keyboard.write(false, byte),
                    0x02 => self.acia_keyboard.write(true, byte),
                    0x04 => self.acia_midi.write(false, byte),
                    0x06 => self.acia_midi.write(true, byte),
                    _ => {}
                }
                return Some(());
            }
            _ => {}
        }
        let lanes: &[(u16, u8)] = if is_word {
            &[(offset, byte_hi), (offset | 1, byte_lo)]
        } else if address & 1 == 0 {
            &[(offset, byte_lo)]
        } else {
            &[(offset | 1, byte_lo)]
        };
        for &(offset, byte) in lanes {
            if offset == 0x260 {
                self.shifter.write_resolution(byte);
                self.glue.set_resolution(byte);
            } else if !self.glue.write(offset, byte) && self.glue_or_void(offset).is_none() {
                return None;
            }
        }
        Some(())
    }

    /// The YM2149's port A: bit 0 the floppy side (inverted), bits 1 and 2
    /// the drive selects (active low).
    fn floppy_select(&mut self, port_a: u8) {
        self.floppy_side = u8::from(port_a & 1 == 0);
        self.fdc.set_side(self.floppy_side);
        if port_a & 0x02 == 0 {
            self.fdc.set_drive(0);
        } else if port_a & 0x04 == 0 {
            self.fdc.set_drive(1);
        }
    }

    // ---- Debugging ----

    /// Read a byte as the CPU would in supervisor mode, without side effects
    /// on I/O (I/O reads `$FF`).
    #[must_use]
    pub fn peek(&self, address: u32) -> u8 {
        let address = address & 0x00FF_FFFF;
        let word = match address {
            0x00_0000..=0x00_0007 => self.rom_word(address & !1),
            0x00_0008..=0x3F_FFFF => return self.ram_byte(address),
            0xE0_0000..=0xEF_FFFF | 0xFC_0000..=0xFE_FFFF => self.rom_word(address & !1),
            _ => None,
        }
        .unwrap_or(0xFFFF);
        if address & 1 == 0 {
            (word >> 8) as u8
        } else {
            word as u8
        }
    }

    /// Write a byte of RAM directly, without bus timing.
    pub fn poke(&mut self, address: u32, value: u8) {
        let address = address & 0x00FF_FFFF;
        if address < 0x40_0000 {
            self.write_ram_byte(address, value);
        }
    }

    /// The RAM, as the chips see it (physical order).
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// The loaded TOS image.
    #[must_use]
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }
}

/// Why a disk image could not be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageError {
    St(format_atari_st_st::StError),
    Msa(format_atari_st_msa::MsaError),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::St(error) => error.fmt(f),
            Self::Msa(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ImageError {}

/// A floppy from a `.st` image.
///
/// # Errors
///
/// [`ImageError::St`] when no geometry fits the image.
pub fn disk_from_st(bytes: &[u8]) -> Result<Disk, ImageError> {
    let image = format_atari_st_st::parse(bytes).map_err(ImageError::St)?;
    let g = image.geometry;
    Ok(Disk::new(
        image.data,
        g.tracks,
        g.sides,
        g.sectors_per_track,
        format_atari_st_st::SECTOR_SIZE,
    ))
}

/// A floppy from a `.msa` archive.
///
/// # Errors
///
/// [`ImageError::Msa`] for a malformed archive.
pub fn disk_from_msa(bytes: &[u8]) -> Result<Disk, ImageError> {
    let image = format_atari_st_msa::parse(bytes).map_err(ImageError::Msa)?;
    let tracks = image.tracks();
    Ok(Disk::new(
        image.data,
        tracks,
        image.sides,
        image.sectors_per_track,
        format_atari_st_msa::SECTOR_SIZE,
    ))
}

/// A floppy's sectors as a `.msa` archive, its geometry read the way a
/// `.st` image's is. `None` when no geometry fits.
#[must_use]
pub fn msa_from_disk(disk: &Disk) -> Option<Vec<u8>> {
    let image = format_atari_st_st::parse(disk.data()).ok()?;
    let g = image.geometry;
    Some(format_atari_st_msa::write(
        g.sectors_per_track,
        g.sides,
        &image.data,
    ))
}

/// A floppy from an image of either kind: `.msa` by its magic, else `.st`.
///
/// # Errors
///
/// [`ImageError`] when the image cannot be read.
pub fn disk_from_image(bytes: &[u8]) -> Result<Disk, ImageError> {
    if bytes.starts_with(&[0x0E, 0x0F]) {
        disk_from_msa(bytes)
    } else {
        disk_from_st(bytes)
    }
}

fn master_hz(standard: VideoStandard) -> u64 {
    match standard {
        VideoStandard::Pal => PAL_MASTER_HZ,
        VideoStandard::Ntsc => NTSC_MASTER_HZ,
    }
}

fn rom_base(size: usize) -> Result<u32, RomError> {
    match size {
        0x3_0000 => Ok(0xFC_0000),
        0x4_0000 => Ok(0xE0_0000),
        _ => Err(RomError::Size(size)),
    }
}

#[cfg(test)]
mod tests;
