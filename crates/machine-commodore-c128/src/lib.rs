//! The Commodore 128 in C128 mode, on 40 or 80 columns.
//!
//! Two CPUs share one bus. At power-on the Z80 has it and runs its BIOS,
//! which hands the bus to the 8502 by setting the MMU's MCR bit 0; the 8502
//! then runs its reset sequence and boots BASIC 7.0. Around them: the 8722
//! MMU ([`mmu`]), 128 KB of RAM, the VIC-IIe in its 8566 form, two 6526
//! CIAs, a SID, the 8563 VDC ([`vdc`]) for 80 columns, and 2 KB of colour RAM
//! in two banks the 8502's port selects.
//!
//! The reference is VICE (`src/c128`, `src/vdc`). Not modelled yet: C64
//! mode, 2 MHz mode (`$D030` bit 0 is held but the 8502 stays at 1 MHz),
//! the datasette, IEC drives and cartridges.

mod mmu;
mod vdc;

use common_commodore_c64::{C64Timing, TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use emu198x_mos_6502::M6502;
use emu198x_mos_cia_6526::{Cia6526, CiaModel};
use emu198x_mos_sid_6581::Sid6581;
use emu198x_mos_vic_ii::{Vic, VicMemory, VicModel, VicPhi2Bus};
use emu198x_zilog_z80::{BusOp, Z80};
use machine_commodore_c64::KeyboardMatrix;
use mmu::{Mmu, MmuEvent, Target};
use serde::{Deserialize, Serialize};
use vdc::Vdc;

const RAM_SIZE: usize = 0x2_0000;
/// Where BASIC 7.0's program text starts, and the pointer to its end.
const BASIC_TEXT_START: u16 = 0x1C01;
const BASIC_TEXT_TOP: u16 = 0x1210;
const AUDIO_SAMPLE_RATE: u32 = 48_000;
/// Z80 half-cycles per phi2 cycle: the C128's Z80 runs at an effective
/// 2 MHz, two T-states for each 1 MHz cycle.
const Z80_HALF_CYCLES_PER_PHI2: u32 = 4;

/// The C128's ROMs, as VICE ships them.
pub struct C128Roms<'a> {
    /// `kernal-318020-05.bin`: 16 KB holding the screen editor, the Z80
    /// BIOS and the KERNAL, for `$C000`, the Z80's `$0000` and `$E000`.
    pub kernal: &'a [u8],
    /// `basiclo-318018-04.bin`: BASIC 7.0 for `$4000-$7FFF`.
    pub basic_lo: &'a [u8],
    /// `basichi-318019-04.bin`: BASIC 7.0 for `$8000-$BFFF`.
    pub basic_hi: &'a [u8],
    /// `chargen-390059-01.bin`: 8 KB, the C64's character set then the
    /// C128's.
    pub chargen: &'a [u8],
}

/// Which television standard the machine was built for. The two differ in
/// the VIC-IIe (8566 or 8564), the crystal and so the phi2 clock, and the
/// mains rate the CIAs' time-of-day clocks count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum C128Model {
    /// PAL, 312 lines of 63 cycles at 985,248 Hz.
    #[default]
    Pal,
    /// NTSC, 263 lines of 65 cycles at 1,022,727 Hz.
    Ntsc,
}

impl C128Model {
    /// The board timing for this standard.
    #[must_use]
    pub const fn timing(self) -> C64Timing {
        match self {
            Self::Pal => TIMING_PAL_BREADBIN,
            Self::Ntsc => TIMING_NTSC_BREADBIN,
        }
    }

    const fn vic_model(self) -> VicModel {
        match self {
            Self::Pal => VicModel::Pal8566,
            Self::Ntsc => VicModel::Ntsc8564,
        }
    }
}

/// Why a ROM set was refused.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RomError {
    #[error("{name} is {got} bytes; expected {expected}")]
    WrongSize {
        name: &'static str,
        got: usize,
        expected: usize,
    },
}

fn checked(name: &'static str, rom: &[u8], expected: usize) -> Result<Vec<u8>, RomError> {
    if rom.len() == expected {
        Ok(rom.to_vec())
    } else {
        Err(RomError::WrongSize {
            name,
            got: rom.len(),
            expected,
        })
    }
}

/// RAM, ROMs, colour RAM and the 8502's port: what the CPUs and the VIC read.
#[derive(Clone, Serialize, Deserialize)]
struct Memory {
    ram: Vec<u8>,
    /// BASIC low then high, 32 KB.
    basic: Vec<u8>,
    kernal: Vec<u8>,
    chargen: Vec<u8>,
    /// Two 1 KB banks of 4-bit colour RAM.
    colour: Vec<u8>,
    port_direction: u8,
    port_data: u8,
    /// The MMU's RCR bank for the VIC, copied in each cycle.
    vic_ram_bank: u32,
}

impl Memory {
    /// The port's pins as the 8502 reads them: its outputs, and the inputs
    /// pulled high except where something drives them. Bit 6 is the CAPS
    /// LOCK key, bit 4 the cassette sense line; neither is pressed.
    fn port_pins(&self) -> u8 {
        (self.port_data & self.port_direction) | !self.port_direction
    }

    /// Port bit 0 picks the colour RAM bank the CPU sees, bit 1 the one the
    /// VIC sees (VICE `mem_update_config`, C128 mode).
    fn colour_bank_cpu(&self) -> usize {
        if self.port_pins() & 0x01 != 0 {
            0
        } else {
            0x400
        }
    }

    fn colour_bank_vic(&self) -> usize {
        if self.port_pins() & 0x02 != 0 {
            0
        } else {
            0x400
        }
    }

    /// The C128's own character set, the ROM's second 4 KB, which C128 mode
    /// uses on the international machine (VICE `mem_update_chargen`).
    fn char_rom(&self, offset: u16) -> u8 {
        self.chargen[0x1000 + usize::from(offset & 0x0FFF)]
    }
}

impl VicMemory for Memory {
    fn read_vram(&self, addr: u16) -> u8 {
        if self.is_character_rom(addr) {
            return self.char_rom(addr);
        }
        self.ram[((self.vic_ram_bank as usize) << 16) | usize::from(addr)]
    }

    /// The character ROM shows at `$1000-$1FFF` of VIC banks 0 and 2, unless
    /// port bit 2 hides it.
    fn is_character_rom(&self, addr: u16) -> bool {
        self.port_pins() & 0x04 == 0 && addr & 0x7000 == 0x1000
    }

    fn read_colour(&self, offset: u16) -> u8 {
        self.colour[self.colour_bank_vic() + usize::from(offset & 0x3FF)]
    }
}

/// A Commodore 128.
#[derive(Clone, Serialize, Deserialize)]
pub struct C128 {
    model: C128Model,
    cpu: M6502,
    z80: Z80,
    mmu: Mmu,
    vic: Vic,
    vdc: Vdc,
    cia1: Cia6526,
    cia2: Cia6526,
    sid: Sid6581,
    memory: Memory,
    keyboard: KeyboardMatrix,
    /// The three extra key rows K0-K2 scanned through `$D02F`: one byte per
    /// line, a set bit for each pressed key on that port B line.
    extra_keys: [u8; 3],
    phi2_cycles: u64,
    frame_count: u64,
}

impl C128 {
    /// A PAL C128 with its ROMs, at power-on.
    ///
    /// # Errors
    ///
    /// A ROM of the wrong size.
    pub fn new(roms: &C128Roms<'_>) -> Result<Self, RomError> {
        Self::with_model(roms, C128Model::Pal)
    }

    /// A C128 for one television standard, at power-on. The ROMs are the
    /// same for both: the KERNAL tells them apart by the VIC's line count.
    ///
    /// # Errors
    ///
    /// A ROM of the wrong size.
    pub fn with_model(roms: &C128Roms<'_>, model: C128Model) -> Result<Self, RomError> {
        let mut basic = checked("BASIC low", roms.basic_lo, 0x4000)?;
        basic.extend(checked("BASIC high", roms.basic_hi, 0x4000)?);
        let memory = Memory {
            ram: vec![0; RAM_SIZE],
            basic,
            kernal: checked("KERNAL", roms.kernal, 0x4000)?,
            chargen: checked("character", roms.chargen, 0x2000)?,
            colour: vec![0; 0x800],
            port_direction: 0,
            port_data: 0,
            vic_ram_bank: 0,
        };
        let timing = model.timing();
        let mut cia1 = Cia6526::new_with_tod(timing.cia_tod_divider);
        cia1.set_model(CiaModel::Mos6526A);
        let mut cia2 = Cia6526::new_with_tod(timing.cia_tod_divider);
        cia2.set_model(CiaModel::Mos6526A);
        // Hold the VIC bank lines high so it starts in bank 0, as the C64
        // board does.
        cia2.write(0x02, 0x03);
        cia2.write(0x00, 0x03);
        let mut vic = Vic::new(model.vic_model());
        vic.set_bank(0);
        let mut cpu = M6502::new();
        cpu.reset();
        let mut machine = Self {
            model,
            cpu,
            z80: Z80::new(),
            mmu: Mmu::new(),
            vic,
            vdc: Vdc::new(timing.cpu_hz),
            cia1,
            cia2,
            sid: Sid6581::new(timing.cpu_hz, AUDIO_SAMPLE_RATE),
            memory,
            keyboard: KeyboardMatrix::new(),
            extra_keys: [0; 3],
            phi2_cycles: 0,
            frame_count: 0,
        };
        machine.refresh_keyboard_scan();
        machine.refresh_vic_bank();
        Ok(machine)
    }

    /// Put back what a snapshot cannot carry: the Z80's microcode position.
    pub fn finish_restore(&mut self) {
        self.z80.rehydrate_walker_sequence();
    }

    /// The television standard this machine was built for.
    #[must_use]
    pub const fn model(&self) -> C128Model {
        self.model
    }

    /// The board timing: clock, lines and cycles per line.
    #[must_use]
    pub const fn timing(&self) -> C64Timing {
        self.model.timing()
    }

    /// The 8502.
    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    /// The 8502, mutably, for debuggers.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }

    /// The VIC-IIe.
    #[must_use]
    pub const fn vic(&self) -> &Vic {
        &self.vic
    }

    /// CIA 1: keyboard and joysticks.
    #[must_use]
    pub const fn cia1(&self) -> &Cia6526 {
        &self.cia1
    }

    /// CIA 2: the VIC bank, the serial bus and the user port.
    #[must_use]
    pub const fn cia2(&self) -> &Cia6526 {
        &self.cia2
    }

    /// One VIC register as a debugger sees it, without side effects.
    #[must_use]
    pub fn vic_register(&self, index: u8) -> u8 {
        self.vic.peek(index & 0x3F)
    }

    /// The beam's raster line.
    #[must_use]
    pub fn raster_line(&self) -> u16 {
        self.vic.raster_line()
    }

    /// The cycle within the raster line.
    #[must_use]
    pub fn cycle_in_line(&self) -> u8 {
        self.vic.raster_cycle()
    }

    /// The SID's output sample rate, in Hz.
    #[must_use]
    pub const fn audio_sample_rate(&self) -> u32 {
        AUDIO_SAMPLE_RATE
    }

    /// Run until the 8502 has finished one whole instruction, and return the
    /// phi2 cycles that took. While the Z80 has the bus the 8502 does not
    /// move, so the step gives up after 1024 cycles rather than wait.
    pub fn step_instruction(&mut self) -> u64 {
        let start = self.phi2_cycles;
        let cap = start + 1024;
        while self.cpu.instruction_complete() && self.phi2_cycles < cap {
            self.tick();
        }
        while !self.cpu.instruction_complete() && self.phi2_cycles < cap {
            self.tick();
        }
        self.phi2_cycles - start
    }

    /// The Z80.
    #[must_use]
    pub fn z80(&self) -> &Z80 {
        &self.z80
    }

    /// Whether the 8502 has the bus.
    #[must_use]
    pub fn running_8502(&self) -> bool {
        self.mmu.cpu_8502()
    }

    /// The keyboard matrix shared with the C64.
    pub fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        &mut self.keyboard
    }

    /// Hold the 40/80 DISPLAY key down (80 columns) or let it up (40).
    pub fn set_80_column_key(&mut self, down: bool) {
        self.mmu.set_80_column_key(down);
    }

    /// The VIC-II's picture.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vic.framebuffer()
    }

    /// The 8563 VDC's 80-column picture, [`Self::VDC_WIDTH`] by
    /// [`Self::VDC_HEIGHT`].
    #[must_use]
    pub fn vdc_framebuffer(&self) -> &[u32] {
        self.vdc.framebuffer()
    }

    /// The VDC picture's width in pixels.
    pub const VDC_WIDTH: usize = vdc::VDC_FB_WIDTH;
    /// The VDC picture's height in lines.
    pub const VDC_HEIGHT: usize = vdc::VDC_FB_HEIGHT;

    /// The 8563's video RAM.
    #[must_use]
    pub fn vdc_ram(&self) -> &[u8] {
        self.vdc.vram()
    }

    /// Drain the SID's samples.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.sid.take_buffer()
    }

    /// Phi2 cycles since power-on.
    #[must_use]
    pub fn phi2_cycles(&self) -> u64 {
        self.phi2_cycles
    }

    /// Frames completed since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// RAM by its 17-bit address: bank 1 is `0x1_0000` up.
    #[must_use]
    pub fn ram(&self, address: u32) -> u8 {
        self.memory.ram[address as usize % RAM_SIZE]
    }

    /// What the 8502 would read at `addr`, without side effects.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0 => self.memory.port_direction,
            1 => self.memory.port_pins(),
            _ => match self.mmu.target_8502(addr) {
                Target::Io => self.io_peek(addr),
                target => self.read_target(target),
            },
        }
    }

    /// A debugger write to what the 8502 sees at `addr`. It lands in the RAM
    /// under any ROM or I/O, so it never sets off a chip; an MMU register
    /// at `$FF00-$FF04` is written, since that is all the address holds.
    pub fn poke(&mut self, addr: u16, value: u8) {
        match self.mmu.target_8502(addr) {
            Target::MmuMirror(index) => self.mmu.write_mirror(index, value),
            target => self.write_ram_under(target, addr, value),
        }
    }

    /// Put a PRG file's bytes into bank 0 RAM at the address its first two
    /// bytes give, and return that address. A program loaded at `$1C01`, the
    /// start of BASIC 7.0's text, also moves BASIC's end-of-program pointer
    /// at `$1210` to follow it, so `LIST` and `RUN` see it.
    ///
    /// # Errors
    ///
    /// A file shorter than its two-byte header, or one running past `$FFFF`.
    pub fn load_prg(&mut self, data: &[u8]) -> Result<u16, String> {
        let [lo, hi, body @ ..] = data else {
            return Err("PRG file is shorter than its two-byte load address".to_owned());
        };
        let start = u16::from_le_bytes([*lo, *hi]);
        let end = usize::from(start) + body.len();
        if end > 0x1_0000 {
            return Err(format!(
                "PRG of {} bytes at ${start:04X} runs past $FFFF",
                body.len()
            ));
        }
        self.memory.ram[usize::from(start)..end].copy_from_slice(body);
        if start == BASIC_TEXT_START {
            let [end_lo, end_hi] = (end as u16).to_le_bytes();
            self.memory.ram[usize::from(BASIC_TEXT_TOP)] = end_lo;
            self.memory.ram[usize::from(BASIC_TEXT_TOP) + 1] = end_hi;
        }
        Ok(start)
    }

    /// The 40-column screen as text: 25 lines of the screen RAM at `$0400`
    /// in bank 0, with screen codes turned into ASCII where they have one.
    #[must_use]
    pub fn screen_text(&self) -> Vec<String> {
        (0..25)
            .map(|row| {
                (0..40)
                    .map(|col| screen_code_char(self.memory.ram[0x0400 + row * 40 + col]))
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    /// Run to the end of the next frame and return the cycles it took.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.phi2_cycles;
        while !self.tick() {}
        self.phi2_cycles - start
    }

    /// One phi2 cycle. Returns `true` when it finished a frame.
    pub fn tick(&mut self) -> bool {
        self.phi2_cycles += 1;
        self.memory.vic_ram_bank = self.mmu.vic_bank();
        let phi2_bus = VicPhi2Bus {
            cpu_data: if self.cpu.rw {
                self.peek(self.cpu.addr)
            } else {
                self.cpu.data
            },
        };
        self.vic.tick(&self.memory, phi2_bus);
        self.vdc.tick();
        self.refresh_keyboard_scan();
        self.cia1.tick();
        self.cia2.tick();
        self.refresh_vic_bank();
        let irq = self.vic.irq || self.cia1.irq;
        if self.mmu.cpu_8502() {
            self.cpu.irq = irq;
            self.cpu.nmi = self.cia2.irq;
            self.cpu.rdy = !self.vic.ba_low || !self.cpu.rw;
            if self.cpu.rdy {
                if self.cpu.rw {
                    self.cpu.data_in = self.read_8502(self.cpu.addr);
                } else {
                    self.write_8502(self.cpu.addr, self.cpu.data);
                }
                self.cpu.tick();
            }
        } else if !self.vic.ba_low {
            // The VIC's DMA stops the Z80's clock as well as the 8502's.
            self.z80.irq = irq;
            for _ in 0..Z80_HALF_CYCLES_PER_PHI2 {
                self.z80.tick();
                self.z80_bus();
                if self.mmu.cpu_8502() {
                    break;
                }
            }
        }
        self.sid.tick();
        if self.vic.take_frame_complete() {
            self.frame_count += 1;
            return true;
        }
        false
    }

    fn z80_bus(&mut self) {
        match self.z80.bus_request() {
            Some(BusOp::MemRead) => {
                let target = self.mmu.target_z80(self.z80.addr, false);
                self.z80.data_in = self.read_target(target);
            }
            Some(BusOp::MemWrite) => {
                let (addr, value) = (self.z80.addr, self.z80.data);
                match self.mmu.target_z80(addr, true) {
                    Target::MmuMirror(index) => self.mmu.write_mirror(index, value),
                    Target::ColourRam(offset) => self.write_colour(offset, value),
                    target => self.write_ram_under(target, addr, value),
                }
            }
            // The Z80 reaches the chips with IN and OUT at $D000-$DFFF.
            Some(BusOp::IoRead) => {
                let port = self.z80.addr;
                self.z80.data_in = if (0xD000..=0xDFFF).contains(&port) {
                    self.io_read(port)
                } else {
                    0xFF
                };
            }
            Some(BusOp::IoWrite) => {
                let (port, value) = (self.z80.addr, self.z80.data);
                if (0xD000..=0xDFFF).contains(&port) {
                    self.io_write(port, value);
                }
            }
            Some(BusOp::IntAck) => self.z80.data_in = 0xFF,
            None => {}
        }
    }

    fn read_8502(&mut self, addr: u16) -> u8 {
        match addr {
            0 => self.memory.port_direction,
            1 => self.memory.port_pins(),
            _ => match self.mmu.target_8502(addr) {
                Target::Io => self.io_read(addr),
                target => self.read_target(target),
            },
        }
    }

    fn write_8502(&mut self, addr: u16, value: u8) {
        match addr {
            // The port's registers, with the RAM under them written too.
            0 => self.memory.port_direction = value,
            1 => self.memory.port_data = value,
            _ => {}
        }
        match self.mmu.target_8502(addr) {
            Target::Io => self.io_write(addr, value),
            Target::MmuMirror(index) => self.mmu.write_mirror(index, value),
            target => self.write_ram_under(target, addr, value),
        }
    }

    /// Read whatever `target` names. I/O goes through [`Self::io_read`].
    fn read_target(&self, target: Target) -> u8 {
        let memory = &self.memory;
        match target {
            Target::Ram(address) => memory.ram[address as usize],
            Target::Basic(offset) => memory.basic[usize::from(offset)],
            Target::Editor(offset) => memory.kernal[usize::from(offset)],
            Target::Z80Bios(offset) => memory.kernal[0x1000 + usize::from(offset)],
            Target::Kernal(offset) => memory.kernal[0x2000 + usize::from(offset)],
            Target::CharRom(offset) => memory.char_rom(offset),
            Target::ColourRam(offset) => {
                memory.colour[memory.colour_bank_cpu() + usize::from(offset & 0x3FF)]
            }
            Target::MmuMirror(index) => self.mmu.read_mirror(index),
            Target::FunctionRom | Target::Io => 0xFF,
        }
    }

    /// A write to ROM or an empty socket reaches the RAM underneath.
    fn write_ram_under(&mut self, target: Target, addr: u16, value: u8) {
        let address = match target {
            Target::Ram(address) => address,
            _ => self.mmu.ram_address(addr),
        };
        self.memory.ram[address as usize] = value;
    }

    fn write_colour(&mut self, offset: u16, value: u8) {
        let index = self.memory.colour_bank_cpu() + usize::from(offset & 0x3FF);
        self.memory.colour[index] = value & 0x0F;
    }

    fn io_read(&mut self, addr: u16) -> u8 {
        match addr {
            0xD000..=0xD3FF => self.vic.read((addr & 0x3F) as u8),
            0xD400..=0xD4FF => self.sid.cpu_read((addr & 0x1F) as u8),
            0xD500..=0xD5FF => self.mmu.read((addr & 0xFF) as u8),
            0xD600..=0xD6FF => self.vdc.read(addr),
            0xD800..=0xDBFF => self.colour_read(addr),
            0xDC00..=0xDCFF => {
                self.refresh_keyboard_scan();
                self.cia1.read((addr & 0x0F) as u8)
            }
            0xDD00..=0xDDFF => self.cia2.read((addr & 0x0F) as u8),
            _ => 0xFF,
        }
    }

    fn io_peek(&self, addr: u16) -> u8 {
        match addr {
            0xD000..=0xD3FF => self.vic.peek((addr & 0x3F) as u8),
            0xD500..=0xD5FF => self.mmu.read((addr & 0xFF) as u8),
            0xD600..=0xD6FF => self.vdc.peek(addr),
            0xD800..=0xDBFF => self.colour_read(addr),
            _ => 0xFF,
        }
    }

    /// Colour RAM keeps four bits; the top four are whatever the VIC last
    /// drove onto the bus.
    fn colour_read(&self, addr: u16) -> u8 {
        let index = self.memory.colour_bank_cpu() + usize::from(addr & 0x3FF);
        self.memory.colour[index] | (self.vic.last_bus_data() & 0xF0)
    }

    fn io_write(&mut self, addr: u16, value: u8) {
        match addr {
            0xD000..=0xD3FF => self.vic.write((addr & 0x3F) as u8, value),
            0xD400..=0xD4FF => self.sid.write((addr & 0x1F) as u8, value),
            0xD500..=0xD5FF => {
                if self.mmu.write((addr & 0xFF) as u8, value) == Some(MmuEvent::CpuSwitched) {
                    self.cpu_switched();
                }
            }
            0xD600..=0xD6FF => self.vdc.write(addr, value),
            0xD800..=0xDBFF => self.write_colour(addr, value),
            0xDC00..=0xDCFF => {
                self.cia1.write((addr & 0x0F) as u8, value);
                self.refresh_keyboard_scan();
            }
            0xDD00..=0xDDFF => {
                self.cia2.write((addr & 0x0F) as u8, value);
                self.refresh_vic_bank();
            }
            _ => {}
        }
    }

    /// The other CPU has the bus. The one giving it up stops where it is
    /// and resumes from there when it gets the bus back; the 8502's first
    /// turn is the reset sequence it has held since power-on.
    fn cpu_switched(&mut self) {}

    fn refresh_vic_bank(&mut self) {
        self.vic.set_bank((!self.cia2.pa) & 0x03);
    }

    /// Scan the keyboard: the C64's matrix through CIA 1, plus the C128's
    /// extra rows, which pull port B low on the lines `$D02F` drives low.
    fn refresh_keyboard_scan(&mut self) {
        let (pa, mut pb) = self.keyboard.resolve_ports(
            self.cia1.port_a_drive_state(),
            self.cia1
                .timer_port_b_override(self.cia1.port_b_drive_state()),
            self.cia1.ddr_a() & !self.cia1.port_a_latch(),
            self.cia1.ddr_b() & self.cia1.port_b_latch(),
            0xFF,
            0xFF,
        );
        let lines = self.vic.keyboard_lines();
        for (line, keys) in self.extra_keys.iter().enumerate() {
            if lines & (1 << line) == 0 {
                pb &= !keys;
            }
        }
        self.cia1.pa_in = pa;
        self.cia1.pb_in = pb;
    }
}

/// A screen code as the character it shows, for the upper-case set.
fn screen_code_char(code: u8) -> char {
    match code & 0x7F {
        0 => '@',
        c @ 1..=26 => char::from(b'A' + c - 1),
        c @ 27..=31 => char::from(b"[\\]^_"[usize::from(c - 27)]),
        c @ 32..=63 => char::from(c),
        _ => ' ',
    }
}

#[cfg(test)]
mod tests;
