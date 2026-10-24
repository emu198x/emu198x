//! The Super NES / Super Famicom.
//!
//! The 5A22 (a 65C816 with DMA, HDMA, a multiplier, a divider, interrupt
//! timers and joypad auto-read on the same die), the S-PPU1/S-PPU2 pair,
//! 128 KB of work RAM, the sound module and a cartridge, wired the way the
//! console wires them.
//!
//! # Clocking
//!
//! Two oscillators drive the console. The main one, 21.477 MHz on NTSC and
//! 21.281 MHz on PAL, clocks the 5A22 and the PPU; the sound module has its
//! own resonator ([`apu::APU_CRYSTAL_HZ`]). One [`Snes::tick`] is two master
//! clocks, the unit every counter on the main side moves in:
//!
//! - The PPU takes one dot every 4 master clocks.
//! - A 65C816 cycle takes 6, 8 or 12 master clocks by address and MEMSEL;
//!   an internal cycle takes 6. A read samples the bus 4 clocks before the
//!   cycle ends; a write lands at its end.
//! - Each line, 40 clocks of DRAM refresh stall whatever holds the bus.
//! - DMA runs in 8-clock units with the CPU halted, aligned to an 8-clock
//!   boundary on the way in and back to the CPU's cycle on the way out.
//!
//! The sound module is stepped from the same loop. Its clock stands in an
//! exact rational ratio to the master clock (NTSC's master clock is
//! 236.25 MHz / 11, PAL's 21 281 370 Hz), so an integer accumulator adds
//! the sound module's clocks per master clock each tick and runs one SMP
//! clock each time it passes a whole one. The two sides never drift, and
//! they interleave at the finer of their two grains, which is where the
//! CPU and the SPC700 meet: the four port latches.
//!
//! # The scheduler
//!
//! The order of work follows ares's S-CPU (ISC licence, `ares/sfc/cpu/`),
//! which runs as a cooperative thread. Here the same sequence is a stack of
//! resumable frames: the CPU's cycle at the bottom, and above it, when they
//! take the bus, the DMA edge check, a DMA or HDMA transfer, or a refresh.
//! Each frame does its work and asks to wait a number of master clocks;
//! when a wait ends, the scheduler runs the checks ares runs at the end of
//! every `step` (refresh, HDMA setup and HDMA triggers) and resumes the top
//! frame. So every chip sees every bus access at the master clock where it
//! happens, and a snapshot can be taken between any two ticks, mid-DMA
//! included.
//!
//! # Interrupts
//!
//! NMI and IRQ are polled every 4 master clocks against the PPU's counters,
//! with the hold and transition logic ares uses, and delivered to the CPU's
//! NMI and IRQ pins at the end of each CPU cycle. A write to NMITIMEN and
//! the end of a DMA hold delivery for that one cycle, as on hardware.
//!
//! # Sound boot
//!
//! The SPC700 boots from a 64-byte IPL ROM. Nintendo's is copyrighted and
//! never shipped: [`Snes::new`] takes it if the user supplies it, and
//! otherwise boots [`ipl::CLEAN_ROOM`], a program written for this project
//! that implements the same upload protocol. See [`ipl`].
//!
//! # Not yet modelled
//!
//! Coprocessor cartridges (DSP-1, SuperFX, SA-1 and the rest), the
//! multitap and the mouse, the 5A22 version 1 timing differences, and the
//! open-bus behaviour of the HDMA "unknown" byte beyond its register.

pub mod apu;
pub mod cart;
pub mod dma;
pub mod ipl;
pub mod pad;

use emu198x_nintendo_s_ppu::{Region as PpuRegion, SPpu};
use emu198x_wdc_65c816::Wdc65c816;
use serde::{Deserialize, Serialize};

pub use apu::Apu;
pub use cart::{Cartridge, CartridgeError, Map};
pub use pad::{Buttons, Pad};

/// The NTSC master clock, rounded: six times the 3.579545 MHz colour
/// subcarrier, exactly 236.25 MHz / 11.
pub const NTSC_MASTER_HZ: u64 = 21_477_273;
/// The PAL master clock: 4.8 times the 4.43361875 MHz subcarrier.
pub const PAL_MASTER_HZ: u64 = 21_281_370;
/// Master clocks per [`Snes::tick`].
pub const MCLKS_PER_TICK: u64 = 2;
/// The rate [`Snes::take_audio`] returns, interleaved stereo.
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
/// The DSP's output rate at this model's resonator: 32 040 Hz.
pub const DSP_SAMPLE_RATE: u32 = (apu::APU_CRYSTAL_HZ / 768) as u32;

/// The console's video standard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Region {
    Ntsc,
    Pal,
}

impl Region {
    #[must_use]
    pub fn master_hz(self) -> u64 {
        match self {
            Self::Ntsc => NTSC_MASTER_HZ,
            Self::Pal => PAL_MASTER_HZ,
        }
    }

    /// SMP clocks per master clock, as numerator and denominator.
    fn apu_ratio(self) -> (u64, u64) {
        match self {
            Self::Ntsc => (apu::SMP_CLOCK_HZ * 11, 236_250_000),
            Self::Pal => (apu::SMP_CLOCK_HZ, PAL_MASTER_HZ),
        }
    }
}

/// What a CPU cycle on the pins is, for timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum CycleKind {
    Read,
    Write,
    Idle,
}

/// Where the CPU is in its current cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum CpuStage {
    Start,
    AfterEdge(CycleKind, u32),
    ReadMid,
    ReadEnd,
    WriteEnd,
    IdleEnd,
}

/// One resumable piece of bus work. See the crate documentation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Frame {
    Cpu(CpuStage),
    /// ares's `dmaEdge`.
    Edge(u8),
    /// ares's `dmaRun`: every enabled channel in turn.
    DmaRun {
        stage: u8,
        ch: u8,
    },
    /// One channel's DMA.
    ChannelDma {
        ch: u8,
        stage: u8,
        index: u8,
    },
    /// One byte between the A-bus and the B-bus.
    Transfer {
        ch: u8,
        address: u32,
        index: u8,
        stage: u8,
        data: u8,
    },
    /// An A-bus read of HDMA's table, into `dma_data`.
    ReadA {
        address: u32,
        stage: u8,
    },
    HdmaSetup {
        stage: u8,
        ch: u8,
    },
    HdmaRun {
        stage: u8,
        ch: u8,
        index: u8,
    },
    /// Read a channel's next HDMA table entry.
    Reload {
        ch: u8,
        stage: u8,
    },
    /// DRAM refresh: five 8-clock units.
    Refresh {
        step: u8,
    },
}

/// What a frame wants next.
enum Next {
    /// Wait this many master clocks, then resume this frame.
    Wait(u32),
    /// Run this frame, then resume this one.
    Push(Frame),
    /// Resume this frame straight away.
    Continue,
    /// This frame is done.
    Pop,
}

/// The 5A22's I/O registers and the state behind them.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Io {
    wram_address: u32,
    hirq_enable: bool,
    virq_enable: bool,
    irq_enable: bool,
    nmi_enable: bool,
    auto_joypad_poll: bool,
    pio: u8,
    wrmpya: u8,
    wrmpyb: u8,
    wrdiva: u16,
    wrdivb: u8,
    /// HTIME and VTIME as written (9 bits each).
    htime_raw: u16,
    vtime: u16,
    rom_speed: u32,
    rddiv: u16,
    rdmpy: u16,
    joy: [u16; 4],
}

impl Default for Io {
    fn default() -> Self {
        Self {
            wram_address: 0,
            hirq_enable: false,
            virq_enable: false,
            irq_enable: false,
            nmi_enable: false,
            auto_joypad_poll: false,
            pio: 0xFF,
            wrmpya: 0xFF,
            wrmpyb: 0xFF,
            wrdiva: 0xFFFF,
            wrdivb: 0xFF,
            htime_raw: 0x1FF,
            vtime: 0x1FF,
            rom_speed: 8,
            rddiv: 0,
            rdmpy: 0,
            joy: [0; 4],
        }
    }
}

impl Io {
    /// The H position IRQ compares against, in master clocks.
    fn htime(&self) -> u32 {
        (u32::from(self.htime_raw) + 1) << 2 & 0xFFF
    }
}

/// The multiplier and divider, which work a bit per CPU cycle.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Alu {
    mpyctr: u32,
    divctr: u32,
    shift: u32,
}

/// The 5A22's timing state: interrupt lines, DMA and HDMA triggers, refresh
/// and the auto-read sequence.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Status {
    /// The length of the CPU cycle a DMA started in front of.
    clock_count: u32,
    irq_lock: bool,
    dram_refresh_position: u32,
    dram_refresh: u8,
    hdma_setup_position: u32,
    hdma_setup_triggered: bool,
    hdma_position: u32,
    hdma_triggered: bool,
    nmi_valid: bool,
    nmi_line: bool,
    nmi_transition: bool,
    nmi_hold: bool,
    irq_valid: bool,
    irq_line: bool,
    irq_transition: bool,
    irq_hold: bool,
    dma_active: bool,
    dma_pending: bool,
    hdma_pending: bool,
    /// `false`: setup (start of frame). `true`: a line's transfer.
    hdma_mode: bool,
    auto_joypad_counter: u32,
    auto_joypad_port: [u8; 2],
    cpu_latch: bool,
    auto_joypad_latch: bool,
    /// Master clocks counted by DMA since it took the bus, for realigning.
    counter_dma: u32,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            clock_count: 6,
            irq_lock: false,
            dram_refresh_position: 538,
            dram_refresh: 0,
            hdma_setup_position: 12,
            hdma_setup_triggered: false,
            hdma_position: 1104,
            hdma_triggered: false,
            nmi_valid: false,
            nmi_line: false,
            nmi_transition: false,
            nmi_hold: false,
            irq_valid: false,
            irq_line: false,
            irq_transition: false,
            irq_hold: false,
            dma_active: false,
            dma_pending: false,
            hdma_pending: false,
            hdma_mode: false,
            auto_joypad_counter: 33,
            auto_joypad_port: [0; 2],
            cpu_latch: false,
            auto_joypad_latch: false,
            counter_dma: 0,
        }
    }
}

/// Master clocks the 5A22 waits after reset before its reset sequence.
const RESET_DELAY: u32 = 132;

/// A Super NES with a cartridge in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snes {
    pub cpu: Wdc65c816,
    pub ppu: SPpu,
    pub apu: Apu,
    pub cart: Cartridge,
    pub pads: [Pad; 2],
    region: Region,
    wram: Vec<u8>,
    /// The CPU's memory data register: the open-bus value.
    mdr: u8,
    io: Io,
    alu: Alu,
    status: Status,
    dma: [dma::Channel; 8],
    /// The bus work in progress, the CPU's cycle at the bottom.
    stack: Vec<Frame>,
    /// Master clocks left before the top frame resumes.
    wait: u32,
    /// The last byte a DMA table read returned.
    dma_data: u8,
    /// Master clocks since power-on.
    mclk: u64,
    /// The line before the current one, and its length, for looking back
    /// across a line boundary.
    prev_line: (u32, u32),
    cur_line: (u32, u32),
    /// SMP clocks owed, scaled by the ratio's denominator.
    apu_phase: u64,
    /// The 32 kHz to 48 kHz resampler: the last input sample and the phase.
    resample_prev: [f32; 2],
    resample_phase: u32,
}

impl Snes {
    /// A console of `region`'s with `cart` in the slot, powered on. `ipl` is
    /// the sound module's boot ROM; `None` boots the clean-room program.
    #[must_use]
    pub fn new(cart: Cartridge, region: Region, ipl: Option<[u8; 64]>) -> Self {
        let ppu_region = match region {
            Region::Ntsc => PpuRegion::Ntsc,
            Region::Pal => PpuRegion::Pal,
        };
        let mut snes = Self {
            cpu: Wdc65c816::new(),
            ppu: SPpu::new(ppu_region),
            apu: Apu::new(ipl.unwrap_or(ipl::CLEAN_ROOM)),
            cart,
            pads: [Pad::new(true), Pad::new(true)],
            region,
            wram: vec![0; 0x2_0000],
            mdr: 0,
            io: Io::default(),
            alu: Alu::default(),
            status: Status::default(),
            dma: Default::default(),
            stack: vec![Frame::Cpu(CpuStage::Start)],
            wait: RESET_DELAY,
            dma_data: 0,
            mclk: 0,
            prev_line: (0, 1364),
            cur_line: (0, 1364),
            apu_phase: 0,
            resample_prev: [0.0; 2],
            resample_phase: 0,
        };
        snes.ppu.set_ext_latch(true);
        snes
    }

    /// Press the reset button. The 5A22 and the sound module restart; work
    /// RAM, VRAM and the cartridge's RAM keep what they had.
    pub fn reset(&mut self) {
        self.cpu.reset();
        self.apu.reset();
        self.io = Io::default();
        self.alu = Alu::default();
        self.status = Status::default();
        self.dma = Default::default();
        self.stack = vec![Frame::Cpu(CpuStage::Start)];
        self.wait = RESET_DELAY;
        self.ppu.set_ext_latch(true);
    }

    #[must_use]
    pub fn region(&self) -> Region {
        self.region
    }

    #[must_use]
    pub fn master_hz(&self) -> u64 {
        self.region.master_hz()
    }

    /// Master clocks since power-on.
    #[must_use]
    pub fn mclk(&self) -> u64 {
        self.mclk
    }

    /// Two master clocks.
    pub fn tick(&mut self) {
        self.advance();
        self.wait -= MCLKS_PER_TICK as u32;
        if self.wait == 0 {
            self.segment_end();
            self.resume();
        }
    }

    /// Run until the PPU finishes a frame, returning the master clocks it
    /// took.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.ppu.frames();
        let start_mclk = self.mclk;
        while self.ppu.frames() == start {
            self.tick();
        }
        self.mclk - start_mclk
    }

    /// The picture: 512 × 480 `0xAARRGGBB` (see the PPU crate).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.ppu.framebuffer()
    }

    /// The sound made since the last call, interleaved 48 kHz stereo.
    pub fn take_audio(&mut self) -> Vec<f32> {
        let input = self.apu.dsp.take_samples();
        let mut out = Vec::with_capacity(input.len() * 3 + 4);
        for (left, right) in input {
            let current = [f32::from(left) / 32768.0, f32::from(right) / 32768.0];
            while self.resample_phase < AUDIO_SAMPLE_RATE {
                let t = self.resample_phase as f32 / AUDIO_SAMPLE_RATE as f32;
                for (prev, now) in self.resample_prev.iter().zip(current) {
                    out.push(prev + (now - prev) * t);
                }
                self.resample_phase += DSP_SAMPLE_RATE;
            }
            self.resample_phase -= AUDIO_SAMPLE_RATE;
            self.resample_prev = current;
        }
        out
    }

    /// Set a pad's buttons; `port` is 0 or 1.
    pub fn set_buttons(&mut self, port: usize, buttons: Buttons) {
        if let Some(pad) = self.pads.get_mut(port) {
            pad.buttons = buttons;
        }
    }

    #[must_use]
    pub fn wram(&self) -> &[u8] {
        &self.wram
    }

    /// A read with no side effects, for debuggers: work RAM and the
    /// cartridge; the I/O registers read as the open bus.
    #[must_use]
    pub fn peek(&self, address: u32) -> u8 {
        let bank = (address >> 16) as u8;
        let offset = address as u16;
        if bank & 0xFE == 0x7E {
            return self.wram[usize::from(bank & 1) << 16 | usize::from(offset)];
        }
        if bank & 0x40 == 0 && offset < 0x2000 {
            return self.wram[usize::from(offset)];
        }
        if bank & 0x40 == 0 && (0x2000..0x6000).contains(&offset) {
            return self.mdr;
        }
        self.cart.read(address).unwrap_or(self.mdr)
    }

    /// Write work RAM or cartridge RAM with no side effects.
    pub fn poke(&mut self, address: u32, value: u8) {
        let bank = (address >> 16) as u8;
        let offset = address as u16;
        if bank & 0xFE == 0x7E {
            self.wram[usize::from(bank & 1) << 16 | usize::from(offset)] = value;
        } else if bank & 0x40 == 0 && offset < 0x2000 {
            self.wram[usize::from(offset)] = value;
        } else {
            self.cart.write(address, value);
        }
    }

    // -- The master clock --------------------------------------------------

    /// The H counter at master-clock grain: the PPU's dot counter plus the
    /// half-dot the clock is into.
    fn h(&self) -> u32 {
        self.ppu.hcounter() + (self.mclk & 2) as u32
    }

    /// The counters `n` master clocks ago.
    fn hv_ago(&self, n: u32) -> (u32, u32) {
        let (v, h) = (self.ppu.vcounter(), self.h());
        if h >= n {
            (v, h - n)
        } else {
            (self.prev_line.0, self.prev_line.1 + h - n)
        }
    }

    /// Advance the world by two master clocks: the PPU, the interrupt
    /// polls, auto-read and the sound module.
    fn advance(&mut self) {
        self.mclk += MCLKS_PER_TICK;
        if self.mclk & 3 == 0 {
            self.ppu.tick();
            if self.ppu.hcounter() == 0 {
                self.scanline();
            }
        }
        if self.h() & 2 != 0 {
            self.nmi_poll();
            self.irq_poll();
        }
        if self.mclk & 127 == 0 {
            self.joypad_edge();
        }
        let (num, den) = self.region.apu_ratio();
        self.apu_phase += MCLKS_PER_TICK * num;
        while self.apu_phase >= den {
            self.apu_phase -= den;
            self.apu.smp_clock();
        }
    }

    fn dma_counter(&self) -> u32 {
        (self.mclk & 7) as u32
    }

    /// The top of a line.
    fn scanline(&mut self) {
        self.prev_line = self.cur_line;
        self.cur_line = (self.ppu.vcounter(), self.ppu.hperiod());
        if self.ppu.vcounter() == 0 {
            self.status.hdma_setup_position = 12 + self.dma_counter();
            self.status.hdma_setup_triggered = false;
            self.status.auto_joypad_counter = 33;
        }
        self.status.dram_refresh_position = 530 + 8 - self.dma_counter();
        self.status.dram_refresh = 0;
        if self.ppu.vcounter() < self.ppu.vdisp() {
            self.status.hdma_position = 1104;
            self.status.hdma_triggered = false;
        }
    }

    /// The checks ares makes at the end of every `step`.
    fn segment_end(&mut self) {
        let h = self.h();
        if self.status.dram_refresh == 0 && h >= self.status.dram_refresh_position {
            self.status.dram_refresh = 1;
            self.stack.push(Frame::Refresh { step: 0 });
        }
        if !self.status.hdma_setup_triggered && h >= self.status.hdma_setup_position {
            self.status.hdma_setup_triggered = true;
            for channel in &mut self.dma {
                channel.hdma_completed = false;
                channel.hdma_do_transfer = false;
            }
            if self.dma.iter().any(|c| c.hdma_enable) {
                self.status.hdma_pending = true;
                self.status.hdma_mode = false;
            }
        }
        if !self.status.hdma_triggered && h >= self.status.hdma_position {
            self.status.hdma_triggered = true;
            if self.dma.iter().any(dma::Channel::hdma_active) {
                self.status.hdma_pending = true;
                self.status.hdma_mode = true;
            }
        }
    }

    /// Run frames until one asks to wait.
    fn resume(&mut self) {
        loop {
            let mut frame = self.stack.pop().expect("the CPU frame never finishes");
            match self.run(&mut frame) {
                Next::Wait(clocks) => {
                    self.stack.push(frame);
                    self.status.irq_lock = false;
                    self.wait = clocks;
                    return;
                }
                Next::Push(child) => {
                    self.stack.push(frame);
                    self.stack.push(child);
                }
                Next::Continue => self.stack.push(frame),
                Next::Pop => {}
            }
        }
    }

    fn run(&mut self, frame: &mut Frame) -> Next {
        match frame {
            Frame::Cpu(stage) => self.run_cpu(stage),
            Frame::Edge(stage) => self.run_edge(stage),
            Frame::DmaRun { stage, ch } => self.run_dma(stage, ch),
            Frame::ChannelDma { ch, stage, index } => self.run_channel(*ch, stage, index),
            Frame::Transfer {
                ch,
                address,
                index,
                stage,
                data,
            } => self.run_transfer(*ch, *address, *index, stage, data),
            Frame::ReadA { address, stage } => self.run_read_a(*address, stage),
            Frame::HdmaSetup { stage, ch } => self.run_hdma_setup(stage, ch),
            Frame::HdmaRun { stage, ch, index } => self.run_hdma(stage, ch, index),
            Frame::Reload { ch, stage } => self.run_reload(*ch, stage),
            Frame::Refresh { step } => self.run_refresh(step),
        }
    }

    // -- The CPU's cycle ---------------------------------------------------

    /// The 5A22's wait states for a bus cycle at `address`.
    fn wait_states(&self, address: u32) -> u32 {
        if address & 0x40_8000 != 0 {
            return if address & 0x80_0000 != 0 {
                self.io.rom_speed
            } else {
                8
            };
        }
        if (address.wrapping_add(0x6000)) & 0x4000 != 0 {
            return 8;
        }
        if (address.wrapping_sub(0x4000)) & 0x7E00 != 0 {
            return 6;
        }
        12
    }

    fn run_cpu(&mut self, stage: &mut CpuStage) -> Next {
        match *stage {
            CpuStage::Start => {
                let internal = !self.cpu.bus_active() || (!self.cpu.vda && !self.cpu.vpa);
                let (kind, len) = if internal {
                    (CycleKind::Idle, 6)
                } else if self.cpu.rw {
                    (CycleKind::Read, self.wait_states(self.cpu.addr))
                } else {
                    (CycleKind::Write, self.wait_states(self.cpu.addr))
                };
                if kind == CycleKind::Write {
                    self.alu_edge();
                }
                self.status.clock_count = len;
                *stage = CpuStage::AfterEdge(kind, len);
                Next::Push(Frame::Edge(0))
            }
            CpuStage::AfterEdge(kind, len) => match kind {
                CycleKind::Read => {
                    *stage = CpuStage::ReadMid;
                    Next::Wait(len - 4)
                }
                CycleKind::Write => {
                    *stage = CpuStage::WriteEnd;
                    Next::Wait(len)
                }
                CycleKind::Idle => {
                    *stage = CpuStage::IdleEnd;
                    Next::Wait(6)
                }
            },
            CpuStage::ReadMid => {
                let address = self.cpu.addr;
                let data = self.bus_read(address);
                // The 5A22's own registers do not drive the external bus.
                if address & 0x40_FC00 != 0x4000 {
                    self.mdr = data;
                }
                self.cpu.data_in = data;
                *stage = CpuStage::ReadEnd;
                Next::Wait(4)
            }
            CpuStage::ReadEnd | CpuStage::IdleEnd => {
                self.finish_cpu_cycle();
                self.alu_edge();
                *stage = CpuStage::Start;
                Next::Continue
            }
            CpuStage::WriteEnd => {
                let (address, data) = (self.cpu.addr, self.cpu.data);
                self.mdr = data;
                self.bus_write(address, data);
                self.finish_cpu_cycle();
                *stage = CpuStage::Start;
                Next::Continue
            }
        }
    }

    /// Deliver interrupts to the pins, then let the CPU finish its cycle.
    fn finish_cpu_cycle(&mut self) {
        self.cpu.nmi = false;
        if !self.status.irq_lock {
            if self.status.nmi_transition {
                self.status.nmi_transition = false;
                self.cpu.nmi = true;
            }
            self.cpu.irq =
                self.status.irq_transition || (self.status.irq_line && self.io.irq_enable);
        }
        self.cpu.tick();
        if self.cpu.instruction_complete() {
            self.status.irq_transition = false;
        }
    }

    // -- DMA ---------------------------------------------------------------

    fn dma_enabled(&self) -> bool {
        self.dma.iter().any(|c| c.dma_enable)
    }

    fn hdma_enabled(&self) -> bool {
        self.dma.iter().any(|c| c.hdma_enable)
    }

    /// The realignment back to the CPU's clock after DMA.
    fn realign(&self) -> u32 {
        let count = self.status.clock_count;
        count - self.status.counter_dma % count
    }

    /// ares's `dmaEdge`: if H/DMA is pending and the CPU has run a cycle
    /// since, take the bus.
    fn run_edge(&mut self, stage: &mut u8) -> Next {
        loop {
            match *stage {
                0 => {
                    if !self.status.dma_active {
                        *stage = 5;
                        continue;
                    }
                    *stage = 3;
                    if self.status.hdma_pending {
                        self.status.hdma_pending = false;
                        if self.hdma_enabled() {
                            *stage = 1;
                            if !self.dma_enabled() {
                                self.status.counter_dma = 8 - self.dma_counter();
                                return Next::Wait(self.status.counter_dma);
                            }
                        }
                    }
                }
                1 => {
                    *stage = 2;
                    return Next::Push(if self.status.hdma_mode {
                        Frame::HdmaRun {
                            stage: 0,
                            ch: 0,
                            index: 0,
                        }
                    } else {
                        Frame::HdmaSetup { stage: 0, ch: 0 }
                    });
                }
                2 => {
                    *stage = 3;
                    if !self.dma_enabled() {
                        self.status.dma_active = false;
                        return Next::Wait(self.realign());
                    }
                }
                3 => {
                    *stage = 5;
                    if self.status.dma_pending {
                        self.status.dma_pending = false;
                        if self.dma_enabled() {
                            self.status.counter_dma = 8 - self.dma_counter();
                            *stage = 4;
                            return Next::Wait(self.status.counter_dma);
                        }
                    }
                }
                4 => {
                    *stage = 6;
                    return Next::Push(Frame::DmaRun { stage: 0, ch: 0 });
                }
                6 => {
                    *stage = 5;
                    self.status.dma_active = false;
                    return Next::Wait(self.realign());
                }
                _ => {
                    if !self.status.dma_active
                        && (self.status.dma_pending || self.status.hdma_pending)
                    {
                        self.status.dma_active = true;
                    }
                    return Next::Pop;
                }
            }
        }
    }

    fn run_dma(&mut self, stage: &mut u8, ch: &mut u8) -> Next {
        match *stage {
            0 => {
                self.status.counter_dma += 8;
                *stage = 1;
                Next::Wait(8)
            }
            1 => {
                *stage = 2;
                Next::Push(Frame::Edge(0))
            }
            _ => {
                if *ch == 8 {
                    self.status.irq_lock = true;
                    return Next::Pop;
                }
                let child = Frame::ChannelDma {
                    ch: *ch,
                    stage: 0,
                    index: 0,
                };
                *ch += 1;
                Next::Push(child)
            }
        }
    }

    fn run_channel(&mut self, ch: u8, stage: &mut u8, index: &mut u8) -> Next {
        let c = usize::from(ch);
        match *stage {
            0 => {
                if !self.dma[c].dma_enable {
                    return Next::Pop;
                }
                self.status.counter_dma += 8;
                *stage = 1;
                Next::Wait(8)
            }
            1 => {
                *stage = 2;
                Next::Push(Frame::Edge(0))
            }
            2 => {
                let channel = &self.dma[c];
                let address =
                    u32::from(channel.source_bank) << 16 | u32::from(channel.source_address);
                let transfer = Frame::Transfer {
                    ch,
                    address,
                    index: *index,
                    stage: 0,
                    data: 0,
                };
                *index = (*index + 1) & 3;
                *stage = 3;
                Next::Push(transfer)
            }
            3 => {
                let channel = &mut self.dma[c];
                if !channel.fixed_transfer {
                    channel.source_address = if channel.reverse_transfer {
                        channel.source_address.wrapping_sub(1)
                    } else {
                        channel.source_address.wrapping_add(1)
                    };
                }
                *stage = 4;
                Next::Push(Frame::Edge(0))
            }
            _ => {
                let channel = &mut self.dma[c];
                channel.transfer_size = channel.transfer_size.wrapping_sub(1);
                if channel.dma_enable && channel.transfer_size != 0 {
                    *stage = 2;
                    Next::Continue
                } else {
                    channel.dma_enable = false;
                    Next::Pop
                }
            }
        }
    }

    fn run_transfer(
        &mut self,
        ch: u8,
        address: u32,
        index: u8,
        stage: &mut u8,
        data: &mut u8,
    ) -> Next {
        let channel = &self.dma[usize::from(ch)];
        let b = channel.b_address(index);
        let valid = dma::valid_b(address, b);
        let direction = channel.direction;
        match *stage {
            0 => {
                self.status.counter_dma += 4;
                *stage = 1;
                Next::Wait(4)
            }
            1 => {
                *data = if !direction {
                    if dma::valid_a(address) {
                        self.bus_read(address)
                    } else {
                        0
                    }
                } else if valid {
                    self.b_read(b)
                } else {
                    0
                };
                self.mdr = *data;
                self.status.counter_dma += 4;
                *stage = 2;
                Next::Wait(4)
            }
            _ => {
                if !direction {
                    if valid {
                        self.b_write(b, *data);
                    }
                } else if dma::valid_a(address) {
                    self.bus_write(address, *data);
                }
                Next::Pop
            }
        }
    }

    fn run_read_a(&mut self, address: u32, stage: &mut u8) -> Next {
        match *stage {
            0 => {
                self.status.counter_dma += 4;
                *stage = 1;
                Next::Wait(4)
            }
            1 => {
                self.dma_data = if dma::valid_a(address) {
                    self.bus_read(address)
                } else {
                    0
                };
                self.mdr = self.dma_data;
                self.status.counter_dma += 4;
                *stage = 2;
                Next::Wait(4)
            }
            _ => Next::Pop,
        }
    }

    fn run_hdma_setup(&mut self, stage: &mut u8, ch: &mut u8) -> Next {
        if *stage == 0 {
            self.status.counter_dma += 8;
            *stage = 1;
            return Next::Wait(8);
        }
        while *ch < 8 {
            let c = usize::from(*ch);
            *ch += 1;
            let channel = &mut self.dma[c];
            channel.hdma_do_transfer = true;
            if !channel.hdma_enable {
                continue;
            }
            channel.dma_enable = false;
            channel.hdma_address = channel.source_address;
            channel.line_counter = 0;
            return Next::Push(Frame::Reload {
                ch: c as u8,
                stage: 0,
            });
        }
        self.status.irq_lock = true;
        Next::Pop
    }

    fn run_hdma(&mut self, stage: &mut u8, ch: &mut u8, index: &mut u8) -> Next {
        match *stage {
            0 => {
                self.status.counter_dma += 8;
                *stage = 1;
                Next::Wait(8)
            }
            1 => {
                while *ch < 8 {
                    let c = usize::from(*ch);
                    let channel = &mut self.dma[c];
                    if !channel.hdma_active() {
                        *ch += 1;
                        continue;
                    }
                    channel.dma_enable = false;
                    if !channel.hdma_do_transfer
                        || *index >= dma::HDMA_LENGTHS[usize::from(channel.transfer_mode)]
                    {
                        *ch += 1;
                        *index = 0;
                        continue;
                    }
                    let address = if channel.indirect {
                        let a = u32::from(channel.indirect_bank) << 16
                            | u32::from(channel.transfer_size);
                        channel.transfer_size = channel.transfer_size.wrapping_add(1);
                        a
                    } else {
                        let a =
                            u32::from(channel.source_bank) << 16 | u32::from(channel.hdma_address);
                        channel.hdma_address = channel.hdma_address.wrapping_add(1);
                        a
                    };
                    let transfer = Frame::Transfer {
                        ch: c as u8,
                        address,
                        index: *index,
                        stage: 0,
                        data: 0,
                    };
                    *index += 1;
                    return Next::Push(transfer);
                }
                *stage = 2;
                *ch = 0;
                Next::Continue
            }
            _ => {
                while *ch < 8 {
                    let c = usize::from(*ch);
                    *ch += 1;
                    let channel = &mut self.dma[c];
                    if !channel.hdma_active() {
                        continue;
                    }
                    channel.line_counter = channel.line_counter.wrapping_sub(1);
                    channel.hdma_do_transfer = channel.line_counter & 0x80 != 0;
                    return Next::Push(Frame::Reload {
                        ch: c as u8,
                        stage: 0,
                    });
                }
                self.status.irq_lock = true;
                Next::Pop
            }
        }
    }

    /// Whether no channel after `ch` still has HDMA work this frame.
    fn hdma_finished(&self, ch: usize) -> bool {
        self.dma[ch + 1..].iter().all(|c| !c.hdma_active())
    }

    fn run_reload(&mut self, ch: u8, stage: &mut u8) -> Next {
        let c = usize::from(ch);
        match *stage {
            0 => {
                let channel = &self.dma[c];
                let address =
                    u32::from(channel.source_bank) << 16 | u32::from(channel.hdma_address);
                *stage = 1;
                Next::Push(Frame::ReadA { address, stage: 0 })
            }
            1 => {
                let data = self.dma_data;
                let channel = &mut self.dma[c];
                if channel.line_counter & 0x7F != 0 {
                    return Next::Pop;
                }
                channel.line_counter = data;
                channel.hdma_address = channel.hdma_address.wrapping_add(1);
                channel.hdma_completed = channel.line_counter == 0;
                channel.hdma_do_transfer = !channel.hdma_completed;
                if !channel.indirect {
                    return Next::Pop;
                }
                let address =
                    u32::from(channel.source_bank) << 16 | u32::from(channel.hdma_address);
                channel.hdma_address = channel.hdma_address.wrapping_add(1);
                *stage = 2;
                Next::Push(Frame::ReadA { address, stage: 0 })
            }
            2 => {
                let data = self.dma_data;
                self.dma[c].transfer_size = u16::from(data) << 8;
                if self.dma[c].hdma_completed && self.hdma_finished(c) {
                    return Next::Pop;
                }
                let channel = &mut self.dma[c];
                let address =
                    u32::from(channel.source_bank) << 16 | u32::from(channel.hdma_address);
                channel.hdma_address = channel.hdma_address.wrapping_add(1);
                *stage = 3;
                Next::Push(Frame::ReadA { address, stage: 0 })
            }
            _ => {
                let data = self.dma_data;
                let channel = &mut self.dma[c];
                channel.transfer_size = u16::from(data) << 8 | channel.transfer_size >> 8;
                Next::Pop
            }
        }
    }

    /// Five units of 6 clocks refreshing and 2 not, the ALU working a step
    /// after each.
    fn run_refresh(&mut self, step: &mut u8) -> Next {
        let even = step.is_multiple_of(2);
        if *step > 0 && even {
            self.alu_edge();
        }
        if *step == 10 {
            return Next::Pop;
        }
        *step += 1;
        if even {
            self.status.dram_refresh = 1;
            Next::Wait(6)
        } else {
            self.status.dram_refresh = 2;
            Next::Wait(2)
        }
    }

    // -- The ALU -----------------------------------------------------------

    fn alu_edge(&mut self) {
        if self.alu.mpyctr > 0 {
            self.alu.mpyctr -= 1;
            if self.io.rddiv & 1 != 0 {
                self.io.rdmpy = self.io.rdmpy.wrapping_add(self.alu.shift as u16);
            }
            self.io.rddiv >>= 1;
            self.alu.shift <<= 1;
        }
        if self.alu.divctr > 0 {
            self.alu.divctr -= 1;
            self.io.rddiv <<= 1;
            self.alu.shift >>= 1;
            if u32::from(self.io.rdmpy) >= self.alu.shift {
                self.io.rdmpy = (u32::from(self.io.rdmpy) - self.alu.shift) as u16;
                self.io.rddiv |= 1;
            }
        }
    }

    // -- Interrupts --------------------------------------------------------

    fn nmi_poll(&mut self) {
        if std::mem::take(&mut self.status.nmi_hold) && self.io.nmi_enable {
            self.status.nmi_transition = true;
        }
        let valid = self.hv_ago(2).0 >= self.ppu.vdisp();
        if self.status.nmi_valid != valid {
            self.status.nmi_valid = valid;
            self.status.nmi_line = valid;
            if valid {
                self.status.nmi_hold = true;
            }
        }
    }

    fn irq_poll(&mut self) {
        self.status.irq_hold = false;
        if self.status.irq_line && self.io.irq_enable {
            self.status.irq_transition = true;
        }
        let (v10, h10) = self.hv_ago(10);
        let (v6, h6) = self.hv_ago(6);
        let condition = self.io.irq_enable
            && (!self.io.virq_enable || v10 == u32::from(self.io.vtime))
            && (!self.io.hirq_enable || h10 == self.io.htime())
            // IRQs cannot trigger on the last dot of a field.
            && (v6 != 0 || h6 != 0);
        let raised = !self.status.irq_valid && condition;
        self.status.irq_valid = condition;
        if raised {
            self.status.irq_line = true;
            self.status.irq_hold = true;
        }
    }

    fn nmitimen_update(&mut self, data: u8) {
        self.io.hirq_enable = data & 0x10 != 0;
        self.io.virq_enable = data & 0x20 != 0;
        self.io.irq_enable = self.io.hirq_enable || self.io.virq_enable;
        if self.io.virq_enable && !self.io.hirq_enable && self.status.irq_line {
            self.status.irq_transition = true;
        } else if !self.io.irq_enable {
            self.status.irq_line = false;
            self.status.irq_transition = false;
        }
        let nmi_enable = data & 0x80 != 0;
        if !self.io.nmi_enable && nmi_enable && self.status.nmi_line {
            self.status.nmi_transition = true;
        }
        self.io.nmi_enable = nmi_enable;
        self.status.irq_lock = true;
    }

    fn rdnmi(&mut self) -> bool {
        let result = self.status.nmi_line;
        if !self.status.nmi_hold {
            self.status.nmi_line = false;
        }
        result
    }

    fn timeup(&mut self) -> bool {
        let result = self.status.irq_line;
        if !self.status.irq_hold {
            self.status.irq_line = false;
            self.status.irq_transition = false;
        }
        result
    }

    // -- Joypad auto-read --------------------------------------------------

    fn set_pad_latches(&mut self) {
        let level = self.status.auto_joypad_latch || self.status.cpu_latch;
        for pad in &mut self.pads {
            pad.latch(level);
        }
    }

    /// Called every 128 master clocks: ares's `joypadEdge`.
    fn joypad_edge(&mut self) {
        let h = self.h();
        if self.ppu.vcounter() == self.ppu.vdisp()
            && self.mclk & 255 == 0
            && (130..=384).contains(&h)
        {
            self.status.auto_joypad_counter = 0;
        } else {
            if self.status.auto_joypad_counter >= 33 {
                return;
            }
            self.status.auto_joypad_counter += 1;
        }
        let counter = self.status.auto_joypad_counter;
        if counter == 0 {
            self.status.auto_joypad_latch = self.io.auto_joypad_poll;
            self.set_pad_latches();
            if self.io.auto_joypad_poll {
                self.io.joy = [0; 4];
            }
        }
        if counter == 1 {
            self.status.auto_joypad_latch = false;
            self.set_pad_latches();
        }
        if counter != 1 && !self.io.auto_joypad_poll {
            self.status.auto_joypad_counter = 33;
            return;
        }
        if counter >= 2 {
            if counter & 1 == 0 {
                self.status.auto_joypad_port = [self.pads[0].data(), self.pads[1].data()];
            } else {
                let [p1, p2] = self.status.auto_joypad_port;
                self.io.joy[0] = self.io.joy[0] << 1 | u16::from(p1 & 1);
                self.io.joy[1] = self.io.joy[1] << 1 | u16::from(p2 & 1);
                self.io.joy[2] = self.io.joy[2] << 1 | u16::from(p1 >> 1 & 1);
                self.io.joy[3] = self.io.joy[3] << 1 | u16::from(p2 >> 1 & 1);
            }
        }
    }

    // -- The buses ---------------------------------------------------------

    /// An A-bus read.
    fn bus_read(&mut self, address: u32) -> u8 {
        let bank = (address >> 16) as u8;
        let offset = address as u16;
        if bank & 0xFE == 0x7E {
            return self.wram[usize::from(bank & 1) << 16 | usize::from(offset)];
        }
        if bank & 0x40 == 0 {
            match offset {
                0x0000..=0x1FFF => return self.wram[usize::from(offset)],
                0x2100..=0x21FF => return self.b_read(offset as u8),
                0x4016 | 0x4017 | 0x4200..=0x421F => return self.read_cpu_register(offset),
                0x4300..=0x437F => {
                    let channel = &self.dma[usize::from(offset >> 4 & 7)];
                    return channel.read((offset & 0xF) as u8, self.mdr);
                }
                0x2000..=0x5FFF => return self.mdr,
                _ => {}
            }
        }
        self.cart.read(address).unwrap_or(self.mdr)
    }

    /// An A-bus write.
    fn bus_write(&mut self, address: u32, data: u8) {
        let bank = (address >> 16) as u8;
        let offset = address as u16;
        if bank & 0xFE == 0x7E {
            self.wram[usize::from(bank & 1) << 16 | usize::from(offset)] = data;
            return;
        }
        if bank & 0x40 == 0 {
            match offset {
                0x0000..=0x1FFF => {
                    self.wram[usize::from(offset)] = data;
                    return;
                }
                0x2100..=0x21FF => {
                    self.b_write(offset as u8, data);
                    return;
                }
                0x4016 | 0x4200..=0x421F => {
                    self.write_cpu_register(offset, data);
                    return;
                }
                0x4300..=0x437F => {
                    self.dma[usize::from(offset >> 4 & 7)].write((offset & 0xF) as u8, data);
                    return;
                }
                0x2000..=0x5FFF => return,
                _ => {}
            }
        }
        self.cart.write(address, data);
    }

    /// A B-bus read: `$21xx`.
    fn b_read(&mut self, address: u8) -> u8 {
        match address {
            0x00..=0x3F => self.ppu.read(address, self.mdr),
            0x40..=0x7F => self.apu.to_cpu[usize::from(address & 3)],
            0x80 => {
                let data = self.wram[self.io.wram_address as usize];
                self.io.wram_address = (self.io.wram_address + 1) & 0x1_FFFF;
                data
            }
            _ => self.mdr,
        }
    }

    /// A B-bus write.
    fn b_write(&mut self, address: u8, data: u8) {
        match address {
            0x00..=0x3F => self.ppu.write(address, data),
            0x40..=0x7F => self.apu.from_cpu[usize::from(address & 3)] = data,
            0x80 => {
                self.wram[self.io.wram_address as usize] = data;
                self.io.wram_address = (self.io.wram_address + 1) & 0x1_FFFF;
            }
            0x81 => self.io.wram_address = self.io.wram_address & 0x1_FF00 | u32::from(data),
            0x82 => {
                self.io.wram_address = self.io.wram_address & 0x1_00FF | u32::from(data) << 8;
            }
            0x83 => {
                self.io.wram_address = self.io.wram_address & 0xFFFF | u32::from(data & 1) << 16;
            }
            _ => {}
        }
    }

    fn read_cpu_register(&mut self, address: u16) -> u8 {
        let data = self.mdr;
        match address {
            0x4016 => data & 0xFC | self.pads[0].data() & 3,
            0x4017 => data & 0xE0 | 0x1C | self.pads[1].data() & 3,
            // Version 2 of the 5A22 in the low nibble.
            0x4210 => data & 0x70 | u8::from(self.rdnmi()) << 7 | 0x02,
            0x4211 => data & 0x7F | u8::from(self.timeup()) << 7,
            0x4212 => {
                let h = self.h();
                data & 0x3E
                    | u8::from(self.io.auto_joypad_poll && self.status.auto_joypad_counter < 33)
                    | u8::from(h <= 2 || h >= 1096) << 6
                    | u8::from(self.ppu.vcounter() >= self.ppu.vdisp()) << 7
            }
            0x4213 => self.io.pio,
            0x4214 => self.io.rddiv as u8,
            0x4215 => (self.io.rddiv >> 8) as u8,
            0x4216 => self.io.rdmpy as u8,
            0x4217 => (self.io.rdmpy >> 8) as u8,
            0x4218..=0x421F => {
                let word = self.io.joy[usize::from((address - 0x4218) >> 1)];
                if address & 1 == 0 {
                    word as u8
                } else {
                    (word >> 8) as u8
                }
            }
            _ => data,
        }
    }

    fn write_cpu_register(&mut self, address: u16, data: u8) {
        match address {
            0x4016 => {
                // Bit 0 latches both ports.
                self.status.cpu_latch = data & 1 != 0;
                self.set_pad_latches();
            }
            0x4200 => {
                self.io.auto_joypad_poll = data & 1 != 0;
                if self.status.auto_joypad_counter == 0 {
                    self.status.auto_joypad_latch = self.io.auto_joypad_poll;
                    self.set_pad_latches();
                } else if !self.io.auto_joypad_poll && self.status.auto_joypad_counter >= 2 {
                    self.status.auto_joypad_counter = 33;
                }
                self.nmitimen_update(data);
            }
            0x4201 => {
                self.ppu.set_ext_latch(data & 0x80 != 0);
                self.io.pio = data;
            }
            0x4202 => self.io.wrmpya = data,
            0x4203 => {
                self.io.rdmpy = 0;
                if self.alu.mpyctr > 0 || self.alu.divctr > 0 {
                    return;
                }
                self.io.wrmpyb = data;
                self.io.rddiv = u16::from(self.io.wrmpyb) << 8 | u16::from(self.io.wrmpya);
                self.alu.mpyctr = 8;
                self.alu.shift = u32::from(self.io.wrmpyb);
            }
            0x4204 => self.io.wrdiva = self.io.wrdiva & 0xFF00 | u16::from(data),
            0x4205 => self.io.wrdiva = self.io.wrdiva & 0x00FF | u16::from(data) << 8,
            0x4206 => {
                self.io.rdmpy = self.io.wrdiva;
                if self.alu.mpyctr > 0 || self.alu.divctr > 0 {
                    return;
                }
                self.io.wrdivb = data;
                self.alu.divctr = 16;
                self.alu.shift = u32::from(self.io.wrdivb) << 16;
            }
            0x4207 => {
                self.io.htime_raw = self.io.htime_raw & 0x100 | u16::from(data);
                self.irq_poll();
            }
            0x4208 => {
                self.io.htime_raw = self.io.htime_raw & 0xFF | u16::from(data & 1) << 8;
                self.irq_poll();
            }
            0x4209 => {
                self.io.vtime = self.io.vtime & 0x100 | u16::from(data);
                self.irq_poll();
            }
            0x420A => {
                self.io.vtime = self.io.vtime & 0xFF | u16::from(data & 1) << 8;
                self.irq_poll();
            }
            0x420B => {
                for (n, channel) in self.dma.iter_mut().enumerate() {
                    channel.dma_enable = data >> n & 1 != 0;
                }
                if data != 0 {
                    self.status.dma_pending = true;
                }
            }
            0x420C => {
                for (n, channel) in self.dma.iter_mut().enumerate() {
                    channel.hdma_enable = data >> n & 1 != 0;
                }
            }
            0x420D => self.io.rom_speed = if data & 1 != 0 { 6 } else { 8 },
            _ => {}
        }
    }

    /// Run until the 65C816 finishes the instruction under way and the next
    /// one's opcode fetch is on the bus, returning the master clocks that
    /// took. Stops after `limit` clocks, for a CPU that is waiting or
    /// stopped.
    pub fn step_instruction(&mut self, limit: u64) -> u64 {
        let start = self.mclk;
        let mut left = !self.cpu.instruction_complete();
        while self.mclk - start < limit {
            self.tick();
            if !self.cpu.instruction_complete() {
                left = true;
            } else if left {
                break;
            }
        }
        self.mclk - start
    }
}

#[cfg(test)]
mod tests;
