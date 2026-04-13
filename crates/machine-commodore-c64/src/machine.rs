//! Board-level C64 machine substrate.

use common_commodore_c64::timing::C64Timing;
use mos_6502::M6502;
use mos_cia_6526::Cia6526;
use mos_vic_ii::{Vic, VicModel};

use crate::config::{C64Config, C64Model};
use crate::keyboard::KeyboardMatrix;
use crate::memory::{C64Memory, MemoryInitError};

const SID_REGISTER_COUNT: usize = 0x20;

/// Fresh-workspace C64 machine substrate.
#[derive(Clone)]
pub struct C64 {
    model: C64Model,
    cpu: M6502,
    vic: Vic,
    cia1: Cia6526,
    cia2: Cia6526,
    memory: C64Memory,
    keyboard: KeyboardMatrix,
    phi2_cycles: u64,
    frame_count: u64,
    sid_registers: [u8; SID_REGISTER_COUNT],
}

impl C64 {
    /// Constructs a new C64 machine substrate from ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if any ROM size is incorrect.
    pub fn new(config: C64Config<'_>) -> Result<Self, MemoryInitError> {
        let memory = C64Memory::new(config.kernal_rom, config.basic_rom, config.character_rom)?;
        let mut cpu = M6502::new();
        cpu.reset();
        let timing = config.model.timing();

        let mut cia1 = Cia6526::new_with_tod(timing.cia_tod_divider);
        cia1.write(0x02, 0xFF);
        cia1.write(0x03, 0x00);
        cia1.write(0x00, 0xFF);

        let mut cia2 = Cia6526::new_with_tod(timing.cia_tod_divider);
        cia2.write(0x02, 0x03);
        cia2.write(0x00, 0x03);

        let vic_model = match config.model {
            C64Model::PalBreadbin => VicModel::Pal6569,
            C64Model::NtscBreadbin => VicModel::Ntsc6567,
        };
        let mut vic = Vic::new(vic_model);
        vic.set_bank(0);

        let mut machine = Self {
            model: config.model,
            cpu,
            vic,
            cia1,
            cia2,
            memory,
            keyboard: KeyboardMatrix::new(),
            phi2_cycles: 0,
            frame_count: 0,
            sid_registers: [0; SID_REGISTER_COUNT],
        };
        machine.refresh_keyboard_scan();
        machine.refresh_vic_bank();
        Ok(machine)
    }

    /// Hardware model.
    #[must_use]
    pub const fn model(&self) -> C64Model {
        self.model
    }

    /// CPU state.
    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    /// VIC-II state.
    #[must_use]
    pub const fn vic(&self) -> &Vic {
        &self.vic
    }

    /// CIA1 state.
    #[must_use]
    pub const fn cia1(&self) -> &Cia6526 {
        &self.cia1
    }

    /// CIA2 state.
    #[must_use]
    pub const fn cia2(&self) -> &Cia6526 {
        &self.cia2
    }

    /// Timing descriptor for the current model.
    #[must_use]
    pub const fn timing(&self) -> C64Timing {
        self.model.timing()
    }

    /// Underlying memory subsystem.
    #[must_use]
    pub const fn memory(&self) -> &C64Memory {
        &self.memory
    }

    /// Mutable access to the keyboard matrix.
    #[must_use]
    pub fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        &mut self.keyboard
    }

    /// `phi2` cycles elapsed since construction.
    #[must_use]
    pub const fn phi2_cycles(&self) -> u64 {
        self.phi2_cycles
    }

    /// Completed video frames.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Current raster line within the frame.
    #[must_use]
    pub fn raster_line(&self) -> u16 {
        self.vic.raster_line()
    }

    /// Current `phi2` cycle within the raster line.
    #[must_use]
    pub fn cycle_in_line(&self) -> u8 {
        self.vic.raster_cycle()
    }

    /// Current VIC bank selected by CIA2 port A bits 0-1, inverted.
    #[must_use]
    pub fn vic_bank(&self) -> u8 {
        self.vic.bank()
    }

    /// Current CIA1 Port B input value after keyboard scan.
    #[must_use]
    pub const fn cia1_port_b_input(&self) -> u8 {
        self.cia1.pb_in
    }

    /// Reads one live VIC-II register without side effects.
    #[must_use]
    pub fn vic_register(&self, index: u8) -> u8 {
        self.vic.peek(index)
    }

    /// Reads one shadowed SID register.
    #[must_use]
    pub fn sid_register(&self, index: u8) -> u8 {
        self.sid_registers[usize::from(index & 0x1F)]
    }

    /// Borrow the VIC-II framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vic.framebuffer()
    }

    /// Advances the board by one `phi2` cycle.
    ///
    /// Returns `true` when this tick completed a frame.
    pub fn tick(&mut self) -> bool {
        self.phi2_cycles = self.phi2_cycles.saturating_add(1);
        let _cpu_stalled = self.vic.tick(&self.memory);
        self.refresh_keyboard_scan();
        self.cia1.tick();
        self.cia2.tick();
        self.refresh_vic_bank();
        self.cpu.irq = self.vic.irq || self.cia1.irq;
        self.cpu.nmi = self.cia2.irq;
        self.cpu.rdy = !self.vic.ba_low || !self.cpu.rw;

        if self.cpu.rdy {
            if self.cpu.rw {
                self.cpu.data_in = self.cpu_read(self.cpu.addr);
            } else {
                self.cpu_write(self.cpu.addr, self.cpu.data);
            }
            self.cpu.tick();
        }

        if self.vic.take_frame_complete() {
            self.frame_count = self.frame_count.saturating_add(1);
            return true;
        }

        false
    }

    /// Advances the board by a fixed number of `phi2` cycles.
    pub fn advance_phi2_cycles(&mut self, cycles: u64) {
        for _ in 0..cycles {
            self.tick();
        }
    }

    /// Advances exactly one frame and returns the number of cycles executed.
    pub fn run_frame(&mut self) -> u32 {
        let start = self.phi2_cycles;
        while !self.tick() {}
        (self.phi2_cycles - start) as u32
    }

    /// CPU-visible read through banked memory and the current board I/O state.
    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        if (0xD000..=0xDFFF).contains(&addr) && self.memory.is_io_visible() {
            return self.io_read(addr);
        }
        self.memory.cpu_read(addr)
    }

    /// CPU-visible write through banked memory and the current board I/O state.
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        self.memory.cpu_write(addr, value);
        if (0xD000..=0xDFFF).contains(&addr) && self.memory.is_io_visible() {
            self.io_write(addr, value);
        }
    }

    /// Reads the current VIC-visible byte from the active bank.
    #[must_use]
    pub fn vic_read(&self, offset: u16) -> u8 {
        self.memory.vic_read(self.vic.bank(), offset)
    }

    fn io_read(&mut self, addr: u16) -> u8 {
        match addr {
            0xD000..=0xD3FF => self.vic.read((addr & 0x3F) as u8),
            0xD400..=0xD7FF => self.sid_registers[usize::from(addr & 0x1F)],
            0xD800..=0xDBFF => self.memory.colour_ram_read(addr - 0xD800),
            0xDC00..=0xDCFF => {
                self.refresh_keyboard_scan();
                self.cia1.read((addr & 0x0F) as u8)
            }
            0xDD00..=0xDDFF => self.cia2.read((addr & 0x0F) as u8),
            0xDE00..=0xDFFF => 0xFF,
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, addr: u16, value: u8) {
        match addr {
            0xD000..=0xD3FF => self.vic.write((addr & 0x3F) as u8, value),
            0xD400..=0xD7FF => self.sid_registers[usize::from(addr & 0x1F)] = value,
            0xD800..=0xDBFF => self.memory.colour_ram_write(addr - 0xD800, value),
            0xDC00..=0xDCFF => {
                self.cia1.write((addr & 0x0F) as u8, value);
                self.refresh_keyboard_scan();
            }
            0xDD00..=0xDDFF => {
                self.cia2.write((addr & 0x0F) as u8, value);
                self.refresh_vic_bank();
            }
            0xDE00..=0xDFFF => {}
            _ => {}
        }
    }

    fn refresh_keyboard_scan(&mut self) {
        self.cia1.pb_in = self.keyboard.scan(self.cia1.pa);
    }

    fn refresh_vic_bank(&mut self) {
        self.vic.set_bank((!self.cia2.pa) & 0x03);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_machine(model: C64Model) -> C64 {
        stub_machine_with_reset_vector(model, 0xE000)
    }

    fn stub_machine_with_reset_vector(model: C64Model, start_pc: u16) -> C64 {
        let mut kernal = [0xEA; 0x2000];
        kernal[0x1FFC] = start_pc as u8;
        kernal[0x1FFD] = (start_pc >> 8) as u8;
        C64::new(C64Config {
            model,
            kernal_rom: &kernal,
            basic_rom: &[0xBB; 0x2000],
            character_rom: &[0xCC; 0x1000],
        })
        .expect("stub ROM sizes should be valid")
    }

    #[test]
    fn constructs_with_expected_initial_state() {
        let machine = stub_machine(C64Model::PalBreadbin);
        assert_eq!(machine.phi2_cycles(), 0);
        assert_eq!(machine.frame_count(), 0);
        assert_eq!(machine.raster_line(), 0);
        assert_eq!(machine.cycle_in_line(), 0);
        assert_eq!(machine.vic_bank(), 0);
        assert_eq!(machine.cpu().addr, 0xFFFC);
        assert!(machine.cpu().rw);
        assert!(!machine.cpu().sync);
    }

    #[test]
    fn one_tick_advances_cycle_position() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        assert!(!machine.tick());
        assert_eq!(machine.phi2_cycles(), 1);
        assert_eq!(machine.cycle_in_line(), 1);
        assert_eq!(machine.raster_line(), 0);
    }

    #[test]
    fn pal_line_wraps_after_sixty_three_cycles() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.advance_phi2_cycles(63);
        assert_eq!(machine.cycle_in_line(), 0);
        assert_eq!(machine.raster_line(), 1);
    }

    #[test]
    fn run_frame_matches_pal_geometry() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let cycles = machine.run_frame();
        assert_eq!(cycles, 19_656);
        assert_eq!(machine.frame_count(), 1);
        assert_eq!(machine.raster_line(), 0);
        assert_eq!(machine.cycle_in_line(), 0);
    }

    #[test]
    fn ntsc_frame_geometry_is_honoured() {
        let mut machine = stub_machine(C64Model::NtscBreadbin);
        machine.advance_phi2_cycles(17_095);
        assert_eq!(machine.frame_count(), 1);
        assert_eq!(machine.raster_line(), 0);
        assert_eq!(machine.cycle_in_line(), 0);
    }

    #[test]
    fn cpu_reset_bootstrap_reaches_first_opcode_fetch() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        assert!(!machine.tick());
        assert!(machine.cpu().rw);
        assert_eq!(machine.cpu().addr, 0xFFFD);

        assert!(!machine.tick());
        assert!(machine.cpu().sync);
        assert_eq!(machine.cpu().addr, 0xE000);
        assert_eq!(machine.cpu().regs.pc, 0xE000);
        assert!(machine.cpu().instruction_complete());
    }

    #[test]
    fn cpu_can_execute_load_and_store_through_board_bus() {
        let mut machine = stub_machine_with_reset_vector(C64Model::PalBreadbin, 0x0400);
        machine.memory.ram_write(0x0400, 0xA9);
        machine.memory.ram_write(0x0401, 0x42);
        machine.memory.ram_write(0x0402, 0x8D);
        machine.memory.ram_write(0x0403, 0x00);
        machine.memory.ram_write(0x0404, 0x02);

        machine.tick();
        machine.tick();
        for _ in 0..6 {
            machine.tick();
        }

        assert_eq!(machine.cpu().regs.a, 0x42);
        assert_eq!(machine.memory().ram_read(0x0200), 0x42);
        assert_eq!(machine.cpu().regs.pc, 0x0405);
    }

    #[test]
    fn keyboard_scan_flows_through_dc01() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.keyboard_mut().set_key(1, 1, true);
        machine.cpu_write(0xDC00, 0xFD);
        assert_eq!(machine.cpu_read(0xDC01) & 0x02, 0x00);
        assert_eq!(machine.cia1_port_b_input() & 0x02, 0x00);
    }

    #[test]
    fn cia2_port_a_selects_vic_bank() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDD02, 0x03);
        machine.cpu_write(0xDD00, 0x01);
        assert_eq!(machine.vic_bank(), 2);
    }

    #[test]
    fn visible_io_writes_hit_vic_and_underlying_ram() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xD020, 0x06);
        assert_eq!(machine.vic_register(0x20) & 0x0F, 0x06);
        assert_eq!(machine.memory().ram_read(0xD020), 0x06);
    }

    #[test]
    fn hidden_io_reads_and_writes_fall_back_to_ram() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0x0000, 0xFF);
        machine.cpu_write(0x0001, 0x00);
        machine.cpu_write(0xD020, 0x44);
        assert_eq!(machine.memory().ram_read(0xD020), 0x44);
        assert_eq!(machine.vic_register(0x20), 0x00);
        assert_eq!(machine.cpu_read(0xD020), 0x44);
    }

    #[test]
    fn active_vic_bank_feeds_vic_reads() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.memory.ram_write(0x5000, 0xAA);
        machine.cpu_write(0xDD02, 0x03);
        machine.cpu_write(0xDD00, 0x02);
        assert_eq!(machine.vic_bank(), 1);
        assert_eq!(machine.vic_read(0x1000), 0xAA);
    }

    #[test]
    fn cia1_timer_irq_reaches_cpu_irq_line() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDC04, 0x00);
        machine.cpu_write(0xDC05, 0x00);
        machine.cpu_write(0xDC0D, 0x81);
        machine.cpu_write(0xDC0E, 0x01);
        assert!(!machine.cpu().irq);
        machine.tick();
        assert!(machine.cia1().irq);
        assert!(machine.cpu().irq);
    }

    #[test]
    fn cia2_timer_irq_reaches_cpu_nmi_line() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDD04, 0x00);
        machine.cpu_write(0xDD05, 0x00);
        machine.cpu_write(0xDD0D, 0x81);
        machine.cpu_write(0xDD0E, 0x01);
        assert!(!machine.cpu().nmi);
        machine.tick();
        assert!(machine.cia2().irq);
        assert!(machine.cpu().nmi);
    }

    #[test]
    fn vic_raster_irq_reaches_cpu_irq_line() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xD012, 0x01);
        machine.cpu_write(0xD01A, 0x01);
        for _ in 0..63 {
            machine.tick();
        }
        assert!(machine.vic().irq);
        assert!(machine.cpu().irq);
    }

    #[test]
    fn badline_ba_stalls_cpu_reads() {
        let mut machine = stub_machine_with_reset_vector(C64Model::PalBreadbin, 0x0400);
        machine.memory.ram_write(0x0400, 0xEA);
        machine.memory.ram_write(0x0401, 0xEA);
        machine.memory.ram_write(0x0402, 0xEA);
        machine.cpu_write(0xD011, 0x1B);

        machine.tick();
        machine.tick();
        let target_cycles = (0x33u64 * 63) + 13;
        while machine.phi2_cycles() < target_cycles {
            machine.tick();
        }

        let pc_before = machine.cpu().regs.pc;
        assert!(machine.vic().ba_low);
        assert!(machine.cpu().rw);
        machine.tick();
        assert_eq!(machine.cpu().regs.pc, pc_before);
    }
}
