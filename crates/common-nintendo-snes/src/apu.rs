//! The sound module: the SPC700, the S-DSP and the glue between them.
//!
//! # The clock
//!
//! The module runs from its own 24.576 MHz ceramic resonator, which real
//! consoles measure at about 24.607 MHz; this model uses
//! [`APU_CRYSTAL_HZ`], the 32 040 Hz × 768 ares uses. The glue counts in
//! twelfths of it ("SMP clocks", [`SMP_CLOCK_HZ`]):
//!
//! - An SPC700 cycle lasts 2 SMP clocks at the default wait state, 4, 10 or
//!   20 at the others (TEST bits 4-7). Idle cycles, the I/O registers and the
//!   IPL ROM use the internal wait state; RAM the external one.
//! - The DSP steps every 2 SMP clocks whatever the wait state, which is one
//!   SPC700 cycle at the default.
//! - The timers advance by 2, 4, 8 or 16 per SPC700 cycle, the wait states'
//!   nominal lengths, so the glitched 10 and 20 do not slow them.
//!
//! A read of the CPU ports (`$F4-$F7`) splits its cycle in two and samples
//! the port at the midpoint.
//!
//! # The glue registers
//!
//! `$F0-$FF` follow ares's SMP (ISC licence): TEST, CONTROL, the DSP's
//! address and data ports, the four CPU ports in each direction, two
//! general-purpose latches and three timers. Writes to `$F0-$FF` also land
//! in the RAM beneath.

use emu198x_sony_s_dsp::SDsp;
use emu198x_sony_spc700::{BusCycle, Spc700};
use serde::{Deserialize, Serialize};

/// The sound module's resonator, in Hz.
pub const APU_CRYSTAL_HZ: u64 = 24_606_720;

/// The glue's counting unit: the resonator over 12.
pub const SMP_CLOCK_HZ: u64 = APU_CRYSTAL_HZ / 12;

/// SPC700 cycle lengths in SMP clocks for each wait state. 8 and 16 are the
/// nominal values; hardware glitches to 10 and 20 (ares, `smp/timing.cpp`).
const CYCLE_WAIT: [u32; 4] = [2, 4, 10, 20];
/// What the timers see per cycle at each wait state.
const TIMER_WAIT: [u32; 4] = [2, 4, 8, 16];

/// One of the three timers: a prescaler, a toggling stage, an 8-bit
/// divider against a target, and a 4-bit output counter.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Timer {
    /// Clocks per stage-1 toggle: 128 for timers 0 and 1 (8 kHz), 16 for
    /// timer 2 (64 kHz).
    frequency: u32,
    stage0: u32,
    stage1: bool,
    stage2: u8,
    stage3: u8,
    line: bool,
    pub enable: bool,
    pub target: u8,
}

impl Timer {
    fn new(frequency: u32) -> Self {
        Self {
            frequency,
            ..Self::default()
        }
    }

    fn step(&mut self, clocks: u32, gate: bool) {
        self.stage0 += clocks;
        if self.stage0 < self.frequency {
            return;
        }
        self.stage0 -= self.frequency;
        self.stage1 = !self.stage1;
        self.sync_stage1(gate);
    }

    /// Pulse stage 2 on stage 1's falling edge, gated by TEST.
    fn sync_stage1(&mut self, gate: bool) {
        let level = self.stage1 && gate;
        let falling = self.line && !level;
        self.line = level;
        if !falling || !self.enable {
            return;
        }
        self.stage2 = self.stage2.wrapping_add(1);
        if self.stage2 != self.target {
            return;
        }
        self.stage2 = 0;
        self.stage3 = (self.stage3 + 1) & 0x0F;
    }

    /// The output counter, cleared by the read.
    fn read_out(&mut self) -> u8 {
        let value = self.stage3;
        self.stage3 = 0;
        value
    }
}

/// The sound module.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Apu {
    pub spc: Spc700,
    pub dsp: SDsp,
    /// The 64-byte boot ROM.
    pub ipl: Vec<u8>,
    /// Bytes the main CPU wrote to `$2140-$2143`, which the SPC700 reads at
    /// `$F4-$F7`.
    pub from_cpu: [u8; 4],
    /// Bytes the SPC700 wrote to `$F4-$F7`, which the main CPU reads.
    pub to_cpu: [u8; 4],
    timers: [Timer; 3],
    timers_disable: bool,
    ram_writable: bool,
    ram_disable: bool,
    timers_enable: bool,
    external_wait: usize,
    internal_wait: usize,
    ipl_enable: bool,
    dsp_address: u8,
    aux: [u8; 2],

    /// The SPC700 cycle in progress: its length and midpoint in SMP clocks,
    /// how far into it the clock is, and whether it is a split port read.
    cycle_len: u32,
    cycle_elapsed: u32,
    split: bool,
    timer_clocks: u32,
    /// Which half of a DSP step the clock is in.
    dsp_half: bool,
}

impl Apu {
    /// A module at power-on, booting from `ipl`.
    #[must_use]
    pub fn new(ipl: [u8; 64]) -> Self {
        let mut apu = Self {
            spc: Spc700::new(),
            dsp: SDsp::new(),
            ipl: ipl.to_vec(),
            from_cpu: [0; 4],
            to_cpu: [0; 4],
            timers: [Timer::new(128), Timer::new(128), Timer::new(16)],
            timers_disable: false,
            ram_writable: true,
            ram_disable: false,
            timers_enable: true,
            external_wait: 0,
            internal_wait: 0,
            ipl_enable: true,
            dsp_address: 0,
            aux: [0; 2],
            cycle_len: 0,
            cycle_elapsed: 0,
            split: false,
            timer_clocks: 0,
            dsp_half: false,
        };
        apu.reset();
        apu
    }

    /// Reset the SPC700 to the IPL ROM's vector and the glue to power-on.
    pub fn reset(&mut self) {
        let pc = u16::from(self.ipl[62]) | u16::from(self.ipl[63]) << 8;
        self.spc.reset_to(pc);
        self.timers = [Timer::new(128), Timer::new(128), Timer::new(16)];
        self.timers_disable = false;
        self.ram_writable = true;
        self.ram_disable = false;
        self.timers_enable = true;
        self.external_wait = 0;
        self.internal_wait = 0;
        self.ipl_enable = true;
        self.dsp_address = 0;
        self.aux = [0; 2];
        self.from_cpu = [0; 4];
        self.to_cpu = [0; 4];
        self.spc.tick();
        self.begin_cycle();
    }

    /// Whether the IPL ROM currently overlays `$FFC0-$FFFF`.
    #[must_use]
    pub fn ipl_enabled(&self) -> bool {
        self.ipl_enable
    }

    fn timer_gate(&self) -> bool {
        self.timers_enable && !self.timers_disable
    }

    fn step_timers(&mut self, clocks: u32) {
        let gate = self.timer_gate();
        for timer in &mut self.timers {
            timer.step(clocks, gate);
        }
    }

    /// Size the cycle the SPC700 has just put on its pins.
    fn begin_cycle(&mut self) {
        let (address, read) = match self.spc.bus_cycle() {
            BusCycle::Idle => (None, false),
            BusCycle::Read(a) => (Some(a), true),
            BusCycle::Write(a, _) => (Some(a), false),
        };
        let wait = match address {
            None => self.internal_wait,
            Some(a) if a & 0xFFF0 == 0x00F0 || (a >= 0xFFC0 && self.ipl_enable) => {
                self.internal_wait
            }
            Some(_) => self.external_wait,
        };
        self.split = read && address.is_some_and(|a| a & 0xFFFC == 0x00F4);
        if self.split {
            // Two half-waits with the read between them.
            self.cycle_len = 2 * (CYCLE_WAIT[wait] >> 1);
            self.timer_clocks = TIMER_WAIT[wait] >> 1;
        } else {
            self.cycle_len = CYCLE_WAIT[wait];
            self.timer_clocks = TIMER_WAIT[wait];
        }
        self.cycle_elapsed = 0;
    }

    /// One SMP clock: a DSP step every other clock, and the SPC700's cycle
    /// served at its end (or, for a split port read, its midpoint).
    pub fn smp_clock(&mut self) {
        self.dsp_half = !self.dsp_half;
        if !self.dsp_half {
            self.dsp.tick();
        }
        self.cycle_elapsed += 1;
        if self.split && self.cycle_elapsed * 2 == self.cycle_len {
            self.step_timers(self.timer_clocks);
            let address = self.spc.addr;
            self.spc.data_in = self.read(address);
            return;
        }
        if self.cycle_elapsed < self.cycle_len {
            return;
        }
        self.step_timers(self.timer_clocks);
        match self.spc.bus_cycle() {
            BusCycle::Read(address) if !self.split => self.spc.data_in = self.read(address),
            BusCycle::Write(address, data) => self.write(address, data),
            _ => {}
        }
        self.spc.tick();
        self.begin_cycle();
    }

    fn read_ram(&self, address: u16) -> u8 {
        if address >= 0xFFC0 && self.ipl_enable {
            return self.ipl[usize::from(address & 0x3F)];
        }
        if self.ram_disable {
            return 0x5A;
        }
        self.dsp.ram()[usize::from(address)]
    }

    /// A read by the SPC700, with the glue's side effects.
    pub fn read(&mut self, address: u16) -> u8 {
        if address & 0xFFF0 != 0x00F0 {
            return self.read_ram(address);
        }
        match address {
            0xF2 => self.dsp_address,
            0xF3 => self.dsp.read(self.dsp_address),
            0xF4..=0xF7 => self.from_cpu[usize::from(address - 0xF4)],
            0xF8 | 0xF9 => self.aux[usize::from(address - 0xF8)],
            0xFD..=0xFF => self.timers[usize::from(address - 0xFD)].read_out(),
            // TEST, CONTROL and the timer targets are write-only.
            _ => 0,
        }
    }

    /// A read with no side effects, for debuggers.
    #[must_use]
    pub fn peek(&self, address: u16) -> u8 {
        if address & 0xFFF0 == 0x00F0 {
            return match address {
                0xF2 => self.dsp_address,
                0xF3 => self.dsp.read(self.dsp_address),
                0xF4..=0xF7 => self.from_cpu[usize::from(address - 0xF4)],
                0xF8 | 0xF9 => self.aux[usize::from(address - 0xF8)],
                0xFD..=0xFF => self.timers[usize::from(address - 0xFD)].stage3,
                _ => 0,
            };
        }
        self.read_ram(address)
    }

    /// A write by the SPC700. Every write reaches RAM, I/O included.
    pub fn write(&mut self, address: u16, data: u8) {
        if self.ram_writable && !self.ram_disable {
            self.dsp.ram_mut()[usize::from(address)] = data;
        }
        if address & 0xFFF0 != 0x00F0 {
            return;
        }
        match address {
            0xF0 => {
                // TEST only takes effect with the P flag clear.
                if self.spc.regs.p.p {
                    return;
                }
                self.timers_disable = data & 0x01 != 0;
                self.ram_writable = data & 0x02 != 0;
                self.ram_disable = data & 0x04 != 0;
                self.timers_enable = data & 0x08 != 0;
                self.external_wait = usize::from(data >> 4 & 3);
                self.internal_wait = usize::from(data >> 6 & 3);
                let gate = self.timer_gate();
                for timer in &mut self.timers {
                    timer.sync_stage1(gate);
                }
            }
            0xF1 => {
                for (i, timer) in self.timers.iter_mut().enumerate() {
                    let enable = data >> i & 1 != 0;
                    let rising = !timer.enable && enable;
                    timer.enable = enable;
                    // Timers 0 and 1 reset on a 0-to-1 enable. Timer 2
                    // resets on every write that is not one: ares and bsnes
                    // both invert the test for it.
                    let reset = if i < 2 { rising } else { !rising };
                    if reset {
                        timer.stage2 = 0;
                        timer.stage3 = 0;
                    }
                }
                if data & 0x10 != 0 {
                    self.from_cpu[0] = 0;
                    self.from_cpu[1] = 0;
                }
                if data & 0x20 != 0 {
                    self.from_cpu[2] = 0;
                    self.from_cpu[3] = 0;
                }
                self.ipl_enable = data & 0x80 != 0;
            }
            0xF2 => self.dsp_address = data,
            0xF3 => {
                // $80-$FF are read-only mirrors.
                if self.dsp_address & 0x80 == 0 {
                    self.dsp.write(self.dsp_address, data);
                }
            }
            0xF4..=0xF7 => self.to_cpu[usize::from(address - 0xF4)] = data,
            0xF8 | 0xF9 => self.aux[usize::from(address - 0xF8)] = data,
            0xFA..=0xFC => self.timers[usize::from(address - 0xFA)].target = data,
            _ => {}
        }
    }
}
