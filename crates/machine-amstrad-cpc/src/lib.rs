//! Amstrad CPC464 — Z80, Gate Array, 6845 CRTC, AY-3-8912 and 8255 PPI.
//!
//! Scoped in `Emu198x/docs/plans/2026-08-13-amstrad-cpc-plan.md`. Every chip but
//! the Gate Array was already in the workspace and proven in a shipping machine,
//! so this crate is wiring rather than new silicon.
//!
//! # Clock
//!
//! A 16 MHz crystal gives a 4 MHz Z80 and a 1 MHz CRTC — the ratios MAME's
//! `amstrad_base` configures (`Z80(16_MHz_XTAL / 4)`, `HD6845S(16_MHz_XTAL /
//! 16)`). So the CRTC advances one character clock every four T-states, and the
//! AY, at the same 1 MHz, moves with it.
//!
//! **The Z80 is ticked twice per T-state.** `Z80::tick` advances one half-cycle,
//! and a machine that calls it once per T-state runs its CPU at half speed —
//! which is what nine machines in this workspace did until the CPU-rate campaign
//! of 2026-08-13 measured them. `tests/cpu_rate.rs` holds this machine to the
//! same figure from its first commit rather than acquiring the defect and
//! discovering it later. See
//! `knowledge/decisions/z80-validation-surface.md`.
//!
//! # What is not modelled yet
//!
//! **`/WAIT`.** The Gate Array stretches every Z80 M-cycle to a multiple of four
//! T-states, giving an effective ~3.3 MHz rather than 4 — stated outright in the
//! official firmware guide, which is in the reference library:
//!
//! > Accesses to memory are synchronised with the video logic — they are
//! > constrained to occur on microsecond boundaries. This has the effect of
//! > stretching each Z80 M cycle (machine cycle) to be a multiple of 4 T states
//! > (clock cycles). In practice this alters the instruction timing so that the
//! > effective clock rate is approximately 3.3 MHz.
//!
//! `Z80::wait` is a modelled pin the core honours, so the mechanism is
//! available. What is missing is an oracle: none of the three vendored
//! emulators models `/WAIT` as a pin (MAME configures a flat 4 MHz Z80; Arnold
//! folds the stretching into per-instruction cycle counts), so it has to be
//! validated against that ~3.3 MHz figure and observed program timing rather
//! than by reading their source. Until then the CPU runs unstretched, and
//! `cpu_rate` asserts the unstretched figure so the change is visible when it
//! lands.
//!
//! Video rendering is also absent: the Gate Array decodes pixels and holds the
//! palette, but nothing yet walks the CRTC's addresses into a framebuffer.
//!
//! # I/O decode
//!
//! The CPC decodes I/O on the *high* address bits, partially, so one port can
//! reach several devices. From MAME's `amstrad_cpc_io_r` / `amstrad_cpc_io_w`:
//!
//! | Condition | Device |
//! |---|---|
//! | A15 = 0 and A14 = 1 | Gate Array (write only) |
//! | A14 = 0 | 6845 CRTC, function in A9-A8 |
//! | A13 = 0 | ROM select |
//! | A12 = 0 | printer |
//! | A11 = 0 | 8255 PPI, port in A9-A8 |
//! | A10 = 0 | expansion / FDC — absent on a 464 |

use amstrad_gate_array::GateArray;
use gi_ay_3_8912::Ay3_8912;
use intel_8255::Ppi8255;
use motorola_6845::Crtc6845;
use serde::{Deserialize, Serialize};
use zilog_z80::{BusOp, Z80};

/// T-states per CRTC character clock: 4 MHz CPU against a 1 MHz CRTC.
const TSTATES_PER_CRTC_TICK: u32 = 4;

/// T-states in one PAL frame: 64 character clocks per line × 312 lines = 19,968
/// microseconds, and four T-states to the microsecond at 4 MHz. That is
/// ~50.08 Hz, the CPC's actual refresh.
const TSTATES_PER_FRAME: u64 = 64 * 312 * TSTATES_PER_CRTC_TICK as u64;

/// AY-3-8912 clock, 1 MHz — the same divider as the CRTC.
const AY_CLOCK_HZ: u32 = 1_000_000;
const AY_SAMPLE_RATE: u32 = 48_000;
const AY_SAMPLES_PER_FRAME: usize = 1024;

/// Amstrad CPC464.
#[derive(Serialize, Deserialize)]
pub struct AmstradCpc {
    cpu: Z80,
    gate_array: GateArray,
    crtc: Crtc6845,
    psg: Ay3_8912,
    ppi: Ppi8255,

    /// 64 KB of RAM, always writable even where a ROM is paged in.
    ram: Vec<u8>,
    /// Lower ROM: the OS, at `$0000-$3FFF` when the Gate Array enables it.
    os_rom: Vec<u8>,
    /// Upper ROM: BASIC, at `$C000-$FFFF` when enabled.
    basic_rom: Vec<u8>,
    /// Selected upper ROM number, from the ROM-select port. Only 0 (BASIC) is
    /// populated on a 464 without expansions.
    selected_upper_rom: u8,

    /// T-states remaining before the next CRTC character clock.
    crtc_phase: u32,
    /// AY register latch, driven through PPI port C.
    psg_control: u8,
    cpu_tstates: u64,
    frame_count: u64,
}

impl AmstradCpc {
    /// Build a CPC464 from its 32 KB firmware image: 16 KB OS followed by
    /// 16 KB BASIC, which is the layout MAME's `cpc464.rom` uses and the one
    /// `~/.emu198x/roms/amstrad-cpc/cpc464.rom` is assembled to.
    ///
    /// # Errors
    ///
    /// Returns an error unless the image is exactly 32 KB.
    pub fn new(firmware: &[u8]) -> Result<Self, String> {
        if firmware.len() != 0x8000 {
            return Err(format!(
                "CPC firmware must be 32 KB (16 KB OS + 16 KB BASIC), got {}",
                firmware.len()
            ));
        }
        Ok(Self {
            cpu: Z80::new(),
            gate_array: GateArray::new(),
            crtc: Crtc6845::new(),
            psg: Ay3_8912::new(AY_CLOCK_HZ, AY_SAMPLE_RATE, AY_SAMPLES_PER_FRAME),
            ppi: Ppi8255::new(),
            ram: vec![0; 0x1_0000],
            os_rom: firmware[..0x4000].to_vec(),
            basic_rom: firmware[0x4000..].to_vec(),
            selected_upper_rom: 0,
            crtc_phase: 0,
            psg_control: 0,
            cpu_tstates: 0,
            frame_count: 0,
        })
    }

    /// CPU T-states since power-on.
    #[must_use]
    pub fn cpu_tstates(&self) -> u64 {
        self.cpu_tstates
    }

    /// Frames completed since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Observe a byte through the CPU's memory map, without side effects.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// The Gate Array, for inspecting video mode, palette and interrupt state.
    #[must_use]
    pub fn gate_array(&self) -> &GateArray {
        &self.gate_array
    }

    /// Run one frame's worth of T-states, returning how many were consumed.
    ///
    /// Deliberately a fixed budget rather than "until the CRTC completes a
    /// frame". The CRTC powers up with every register at zero, which makes
    /// `h_total` and `v_total` zero too, so it reports a completed frame every
    /// couple of character clocks — a CRTC-driven loop returns after about five
    /// T-states and the firmware never runs far enough to program the CRTC out
    /// of that state. Frame completion is still available to a video layer
    /// through the CRTC itself; it just cannot be what paces the CPU.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.cpu_tstates;
        while self.cpu_tstates - start < TSTATES_PER_FRAME {
            self.tick_tstate();
        }
        self.frame_count += 1;
        self.cpu_tstates - start
    }

    /// Advance one T-state.
    fn tick_tstate(&mut self) {
        // Two CPU half-cycles per T-state. `Z80::tick` advances one half-cycle,
        // so calling it once here would run the CPU at half speed — the defect
        // the 2026-08-13 campaign found on nine machines. `cpu_rate.rs` holds
        // this to 4 T-states per `NOP`.
        for _ in 0..2 {
            // Pins before the tick: the Z80 samples `/INT` at an instruction
            // boundary during its own tick, so feeding the line afterwards
            // hands it the previous half-cycle's state.
            self.cpu.irq = self.gate_array.interrupt();
            self.cpu.tick();
            self.handle_bus();
        }

        self.crtc_phase += 1;
        if self.crtc_phase >= TSTATES_PER_CRTC_TICK {
            self.crtc_phase = 0;
            self.crtc.tick();
            // The Gate Array counts the CRTC's syncs; this is the whole of the
            // CPC's interrupt source.
            self.gate_array.set_hsync(self.crtc.hsync);
            self.gate_array.set_vsync(self.crtc.vsync);
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
                // Writes always land in RAM, whatever is paged over it.
                self.ram[self.cpu.addr as usize] = self.cpu.data;
            }
            Some(BusOp::IoRead) => {
                self.cpu.data_in = self.io_read(self.cpu.addr);
            }
            Some(BusOp::IoWrite) => {
                self.io_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IntAck) => {
                // IM 1: the firmware uses RST 38h, and the Gate Array drops
                // `/INT` and clears bit 5 of its counter on acknowledge.
                self.cpu.data_in = 0xFF;
                self.gate_array.acknowledge_interrupt();
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF if self.gate_array.lower_rom_enabled() => self.os_rom[addr as usize],
            0xC000..=0xFFFF if self.gate_array.upper_rom_enabled() => {
                let offset = (addr - 0xC000) as usize;
                // Only ROM 0 (BASIC) exists on an unexpanded 464; any other
                // selection reads the open bus, which is $FF.
                if self.selected_upper_rom == 0 {
                    self.basic_rom[offset]
                } else {
                    0xFF
                }
            }
            _ => self.ram[addr as usize],
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        // A14 = 0: the CRTC. A9-A8 pick the function.
        if port & 0x4000 == 0 {
            return match (port >> 8) & 0x03 {
                // A type 0 CRTC (HD6845S, which is what the CPC fits) has no
                // readable status register — see the plan's CRTC-type finding.
                0x02 => 0xFF,
                0x03 => self.crtc.read_data(),
                _ => 0xFF,
            };
        }
        // A11 = 0: the PPI. A9-A8 pick the port.
        if port & 0x0800 == 0 {
            let ppi_port = ((port >> 8) & 0x03) as u8;
            if ppi_port == 0 {
                // Port A is the AY data bus. The PSG only answers when port C's
                // control bits select "read".
                if self.psg_control & 0xC0 == 0x40 {
                    return self.psg.read_data();
                }
            }
            return self.ppi.read(ppi_port);
        }
        0xFF
    }

    fn io_write(&mut self, port: u16, value: u8) {
        // A15 = 0 and A14 = 1: the Gate Array. Write-only.
        if port & 0x8000 == 0 && port & 0x4000 != 0 {
            self.gate_array.write(value);
        }
        // A14 = 0: the CRTC.
        if port & 0x4000 == 0 {
            match (port >> 8) & 0x03 {
                0x00 => self.crtc.write_address(value),
                0x01 => self.crtc.write_data(value),
                _ => {}
            }
        }
        // A13 = 0: upper-ROM select.
        if port & 0x2000 == 0 {
            self.selected_upper_rom = value;
        }
        // A11 = 0: the PPI.
        if port & 0x0800 == 0 {
            let ppi_port = ((port >> 8) & 0x03) as u8;
            self.ppi.write(ppi_port, value);
            if ppi_port == 2 {
                // Port C carries the AY's bus control in bits 7-6 and the
                // keyboard row in bits 3-0.
                self.psg_control = value;
                match value & 0xC0 {
                    0x80 => self.psg.write_data(self.ppi.read(0)),
                    0xC0 => self.psg.select_register(self.ppi.read(0)),
                    _ => {}
                }
            }
        }
    }
}

impl zilog_z80::Z80Stepper for AmstradCpc {
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

    /// 32 KB of firmware: `NOP`s in the OS half, `$C9` (RET) in the BASIC half
    /// so the two are distinguishable through the memory map.
    fn test_firmware() -> Vec<u8> {
        let mut rom = vec![0x00u8; 0x8000];
        rom[0x4000..].fill(0xC9);
        rom
    }

    #[test]
    fn firmware_must_be_32k() {
        assert!(AmstradCpc::new(&[0u8; 0x4000]).is_err());
        assert!(AmstradCpc::new(&test_firmware()).is_ok());
    }

    #[test]
    fn both_roms_are_paged_in_at_reset() {
        // Without the OS at $0000 the Z80 has nothing to boot from.
        let cpc = AmstradCpc::new(&test_firmware()).expect("build");
        assert_eq!(cpc.peek(0x0000), 0x00, "OS ROM");
        assert_eq!(cpc.peek(0xC000), 0xC9, "BASIC ROM");
    }

    #[test]
    fn the_gate_array_can_page_either_rom_out() {
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.ram[0x0000] = 0x11;
        cpc.ram[0xC000] = 0x22;

        cpc.io_write(0x7F00, 0b1000_0100); // RMR: lower ROM disabled
        assert_eq!(cpc.peek(0x0000), 0x11, "RAM shows through");
        assert_eq!(cpc.peek(0xC000), 0xC9, "upper still paged in");

        cpc.io_write(0x7F00, 0b1000_1100); // both disabled
        assert_eq!(cpc.peek(0xC000), 0x22);
    }

    #[test]
    fn writes_reach_ram_under_a_paged_in_rom() {
        // The CPC has no write-protect: ROM covers reads only.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.cpu.addr = 0x0100;
        cpc.cpu.data = 0xAB;
        cpc.ram[0x0100] = 0xAB;
        assert_eq!(cpc.peek(0x0100), 0x00, "ROM still answers the read");
        cpc.io_write(0x7F00, 0b1000_0100); // page the OS out
        assert_eq!(cpc.peek(0x0100), 0xAB, "the write was there all along");
    }

    #[test]
    fn an_unselected_upper_rom_reads_as_open_bus() {
        // An unexpanded 464 has only ROM 0.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.io_write(0xDF00, 7);
        assert_eq!(cpc.peek(0xC000), 0xFF);
        cpc.io_write(0xDF00, 0);
        assert_eq!(cpc.peek(0xC000), 0xC9);
    }

    #[test]
    fn the_crtc_has_no_readable_status_on_a_type_0() {
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        assert_eq!(cpc.io_read(0xBE00), 0xFF);
    }

    #[test]
    fn crtc_registers_round_trip_through_their_ports() {
        // R14 is one of the few a 6845 lets you read back; R0-R13 are
        // write-only, which is the chip's behaviour and not a gap here.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.io_write(0xBC00, 14);
        cpc.io_write(0xBD00, 0x2A);
        assert_eq!(cpc.io_read(0xBF00), 0x2A);
    }

    #[test]
    fn the_gate_array_only_answers_when_a15_is_low_and_a14_high() {
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.io_write(0x7F00, 0b1000_0010); // mode 2
        assert_eq!(
            cpc.gate_array().mode(),
            amstrad_gate_array::VideoMode::Mode2
        );
        // A15 high: not the Gate Array, so the mode must not move.
        cpc.io_write(0xFF00, 0b1000_0001);
        assert_eq!(
            cpc.gate_array().mode(),
            amstrad_gate_array::VideoMode::Mode2
        );
    }

    #[test]
    fn the_crtc_advances_once_every_four_tstates() {
        // 4 MHz CPU, 1 MHz CRTC. If this ratio is wrong every raster timing
        // downstream of it is wrong too.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        // The address output only advances while the display is enabled, so
        // give the CRTC a displayed area to walk: R1 = 40 characters across,
        // R6 = 25 rows down, which is roughly the CPC's own setup.
        for (reg, value) in [(0u8, 63u8), (1, 40), (6, 25)] {
            cpc.io_write(0xBC00, reg);
            cpc.io_write(0xBD00, value);
        }

        // Prime past the first CRTC tick: the address output latches the
        // counter *before* incrementing it, so it still reads zero after one.
        for _ in 0..4 {
            cpc.tick_tstate();
        }

        let before = cpc.crtc.memory_address();
        for _ in 0..3 {
            cpc.tick_tstate();
        }
        assert_eq!(cpc.crtc.memory_address(), before, "no tick yet at 3");
        cpc.tick_tstate();
        assert_eq!(
            cpc.crtc.memory_address(),
            before + 1,
            "the CRTC advances exactly one character on the fourth T-state"
        );
    }

    #[test]
    fn the_machine_runs_frames_without_panicking() {
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        for _ in 0..3 {
            cpc.run_frame();
        }
        assert_eq!(cpc.frame_count(), 3);
        assert!(cpc.cpu_tstates() > 0);
    }
}
