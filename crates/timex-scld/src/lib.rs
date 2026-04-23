use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{self, FrameTiming};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::{self, DELAY_TABLE_48K, UlaEngine};

/// Timex SCLD — Semi-Custom Logic Device.
///
/// Used in the TC2048, TC2068, and TS2068. Same contention model as the
/// Ferranti 6C001E (48K pattern) but adds 8 video modes and full I/O decoding.
///
/// Port $FF (SCLD control register):
///   Bits 0-2: Video mode (0-7)
///   Bit 3:    Hi-res ink colour bit 0
///   Bit 4:    Hi-res ink colour bit 1
///   Bit 5:    Hi-res ink colour bit 2
///   Bit 6:    Interrupt disable (1 = disable)
///
/// Video modes:
///   0: Standard Spectrum display (256×192, 8×8 attributes)
///   1: Dual-screen (alternates screen 0 and screen 1)
///   2: Hi-colour (8×1 attribute cells instead of 8×8)
///   3: Hi-colour + dual-screen
///   4: Hi-res monochrome (512×192)
///   5: Hi-res + dual-screen
///   6: Hi-res + hi-colour
///   7: Hi-res + hi-colour + dual-screen
///
/// Currently only Mode 0 is rendered. The mode register is stored for
/// future implementation of the extended video modes.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TimexScld {
    engine: UlaEngine,
    /// Port $FF value (video mode + hi-res colour + interrupt control).
    scld_reg: u8,
}

impl TimexScld {
    pub fn new() -> Self {
        Self {
            engine: UlaEngine::new_hires(&ula_engine::CONFIG_48K),
            scld_reg: 0,
        }
    }

    /// Create with a specific ULA timing config (for TS2068 NTSC).
    pub fn with_config(config: &'static ula_engine::UlaConfig) -> Self {
        Self {
            engine: UlaEngine::new_hires(config),
            scld_reg: 0,
        }
    }

    /// Current video mode (bits 0-2 of port $FF).
    pub fn video_mode(&self) -> u8 {
        self.scld_reg & 0x07
    }

    /// Port $FF write (SCLD control register).
    pub fn write_ff(&mut self, val: u8) {
        self.scld_reg = val;
        self.engine.scld_mode = val & 0x07;
        self.engine.scld_hires_ink = (val >> 3) & 0x07;
    }

    /// Port $FF read.
    pub fn read_ff(&self) -> u8 {
        self.scld_reg
    }
}

impl Default for TimexScld {
    fn default() -> Self {
        Self::new()
    }
}

impl Ula for TimexScld {
    fn tick(
        &mut self,
        memory: &dyn MemoryBus,
        cpu_addr: u16,
        cpu_mreq: bool,
        cpu_iorq: bool,
        framebuffer: &mut [u8],
    ) {
        let e = &mut self.engine;
        let phase = (e.pixel as usize) & 0x0F;

        e.tick_rendering(memory, framebuffer);

        // Same contention as 48K Ferranti (memory + I/O)
        if e.video {
            let contended_addr = memory.is_contended(cpu_addr);
            let mem_contention = contended_addr && e.z80_clock_high && !cpu_mreq;

            let io_even_port = (cpu_addr & 1) == 0;
            let io_contention = (cpu_iorq || e.z80_iorq_prev) && io_even_port && e.z80_clock_high;

            let contention = mem_contention || io_contention;
            e.cpu_clock = !(contention && DELAY_TABLE_48K[phase]);
        } else {
            e.cpu_clock = true;
        }

        e.track_z80_clock(cpu_iorq, cpu_mreq);
    }

    fn cpu_clock_active(&self) -> bool {
        self.engine.cpu_clock
    }

    fn interrupt_active(&self) -> bool {
        // Bit 6 of SCLD register can disable interrupts
        if self.scld_reg & 0x40 != 0 {
            false
        } else {
            self.engine.int_active
        }
    }

    fn floating_bus(&self) -> u8 {
        if self.engine.idle {
            0xFF
        } else {
            self.engine.bus_data
        }
    }

    fn read_fe(&self, port: u16, keyboard: &[u8; 8]) -> u8 {
        self.engine.read_fe(port, keyboard)
    }

    fn write_fe(&mut self, val: u8) {
        self.engine.write_fe(val);
    }

    fn frame_timing(&self) -> &FrameTiming {
        &timing::TIMING_48K
    }

    fn end_frame(&mut self) {
        self.engine.end_frame();
    }
}
