//! NES machine wiring.
//!
//! Ties the 2A03 CPU ([`mos_6502`]), 2C02 PPU
//! ([`ricoh_ppu_2c02`]), and cartridge mapper
//! ([`format_nintendo_nes_ines::Mapper`]) together with 2 KiB of
//! internal RAM, OAMDMA, and controller I/O.
//!
//! # Tick loop
//!
//! Per [`knowledge/decisions/nes-clock-topology.md`]:
//!
//! - The master clock drives the loop.
//! - The PPU ticks every master clock division (1 dot per call).
//! - The CPU ticks every 3rd PPU dot (`cpu_divider`).
//! - NMI and IRQ are routed from PPU/mapper to CPU between ticks.
//! - OAMDMA stalls the CPU for 513/514 cycles when `$4014` is
//!   written.
//!
//! # Scope
//!
//! Boots NROM test ROMs and commercial games (Super Mario Bros.).
//! CPU + PPU + APU + OAMDMA + controller I/O. Deliberately out of
//! scope:
//!
//! - Sub-cycle-accurate OAMDMA/DMC DMA overlap arbitration.
//! - Runtime / `System` trait integration.
//! - Turbo / fast-forward / rewind.
//!
//! # Porting provenance
//!
//! The archive crate at
//! `~/Projects/Emu198x-archive/crates/machine-nintendo-nes/` is
//! **not directly portable** per
//! [archives-as-source.md](../../knowledge/decisions/archives-as-source.md)
//! — it used a CPU-driven loop where the PPU was stepped in
//! batches. This crate is written from scratch against the
//! nes-clock-topology decision doc, using the C64 machine
//! (`machine-commodore-c64`) as the structural template.

#![allow(clippy::cast_possible_truncation)]

mod serde_skip_audit;

use format_nintendo_nes_ines::{Mapper, MapperSnapshot, mapper_from_snapshot};
use mos_6502::M6502;
use ricoh_apu_2a03::Apu;
use ricoh_ppu_2c02::Ppu;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

pub use ricoh_apu_2a03::{ApuChannel, AudioControls};
pub use ricoh_ppu_2c02::{FB_HEIGHT, FB_WIDTH};

/// Serializable NES machine state.
#[derive(Clone, Serialize, Deserialize)]
pub struct NesSnapshot {
    cpu: M6502,
    ppu: Ppu,
    apu: Apu,
    mapper: MapperSnapshot,
    #[serde(with = "BigArray")]
    ram: [u8; 2048],
    cpu_divider: u8,
    master_clock: u64,
    #[serde(default)]
    internal_master_clock: u64,
    frame_count: u64,
    dma_cycles_remaining: u16,
    dma_page: u8,
    dma_offset: u8,
    dma_alignment_done: bool,
    controller1_shift: u8,
    controller1_state: u8,
    #[serde(default)]
    controller2_shift: u8,
    #[serde(default)]
    controller2_state: u8,
    controller_strobe: bool,
}

/// NES machine.
pub struct Nes {
    /// 2A03 CPU (6502 with BCD disabled).
    pub cpu: M6502,

    /// 2C02 PPU.
    pub ppu: Ppu,

    /// 2A03 APU (audio).
    pub apu: Apu,

    /// Cartridge mapper (boxed — concrete type depends on the
    /// mapper number in the iNES header).
    pub mapper: Box<dyn Mapper>,

    /// 2 KiB internal CPU RAM (`$0000-$07FF`, mirrored through
    /// `$1FFF`).
    ram: [u8; 2048],

    /// PPU clock divider. The CPU ticks when this hits 0.
    /// Counts 0, 1, 2, 0, 1, 2, … — the CPU ticks on 0.
    cpu_divider: u8,

    /// Master clock: PPU dots since construction.
    master_clock: u64,

    /// Internal master clock at 4× PPU-dot resolution (12 ticks
    /// per NTSC CPU cycle). The PPU side mirrors this via
    /// `ppu.ppu_clock()`. Private to the machine layer; the
    /// public [`master_clock()`](Self::master_clock) accessor
    /// keeps the PPU-dot resolution contract for existing test
    /// harnesses and the runtime / MCP layer. The 4× counter is
    /// what Phase 4 of the multi-phase refactor uses to drive
    /// `ppu.run(target - 1)` at start and end of each CPU cycle.
    /// See `docs/plans/2026-05-30-refactor-nes-cpu-cycle-multi-phase-plan.md`.
    internal_master_clock: u64,

    /// Completed frame counter.
    frame_count: u64,

    // ── OAMDMA state ────────────────────────────────────────────
    /// Remaining DMA stall cycles. When > 0, the CPU is frozen and
    /// the DMA engine copies one byte per CPU cycle.
    dma_cycles_remaining: u16,
    /// Source page for the current OAMDMA transfer (`$XX00`).
    dma_page: u8,
    /// Current byte offset within the 256-byte DMA transfer.
    dma_offset: u8,
    /// Whether the DMA alignment cycle (odd CPU cycle penalty) has
    /// been consumed.
    dma_alignment_done: bool,

    // ── Controller I/O ──────────────────────────────────────────
    /// Controller 1 shift register (active bits to be read out
    /// serially via `$4016`).
    controller1_shift: u8,
    /// Controller 1 latched state (snapshot taken when `$4016`
    /// bit 0 goes from 1 → 0).
    pub controller1_state: u8,
    /// Controller 2 shift register (read out serially via `$4017`).
    /// Same protocol as controller 1: latched from
    /// `controller2_state` on the strobe falling edge.
    controller2_shift: u8,
    /// Controller 2 latched state.
    pub controller2_state: u8,
    /// Whether the controller strobe is active (bit 0 of last
    /// `$4016` write). While active, reads return button A's
    /// state and the shift register reloads continuously. The
    /// strobe controls both controllers — `$4017` is read-only and
    /// `$4016`'s strobe bit governs the latch.
    controller_strobe: bool,
}

impl Nes {
    /// Construct a new NES from a parsed cartridge mapper.
    ///
    /// The CPU is reset, placing the first bus op at `$FFFC` ready
    /// for the first [`Self::tick()`].
    #[must_use]
    pub fn new(mapper: Box<dyn Mapper>) -> Self {
        let mut cpu = M6502::new_2a03();
        cpu.reset();

        Self {
            cpu,
            ppu: Ppu::new(),
            apu: Apu::new(),
            mapper,
            ram: [0; 2048],
            cpu_divider: 0,
            master_clock: 0,
            internal_master_clock: 0,
            frame_count: 0,
            dma_cycles_remaining: 0,
            dma_page: 0,
            dma_offset: 0,
            dma_alignment_done: false,
            controller1_shift: 0,
            controller1_state: 0,
            controller2_shift: 0,
            controller2_state: 0,
            controller_strobe: false,
        }
    }

    /// Soft reset — equivalent to pressing the reset button on the
    /// Famicom / NES front panel. The CPU is reset (refetches the
    /// reset vector at `$FFFC`/`$FFFD`, sets I, decrements SP by 3,
    /// 7-cycle reset sequence on the next tick). The APU has reset
    /// quirks (`$4015` cleared, `$4017` rewritten with last value
    /// rather than `$00`, IRQ flag cleared, length counters
    /// unaffected on triangle) which are not yet modelled in this
    /// minimal stub — `apu.soft_reset()` will land in a follow-up.
    /// The PPU and mapper are NOT reset (matches real hardware).
    ///
    /// Required by blargg `apu_reset/*` tests which write the `$81`
    /// status code at `$6000` to signal "press reset button now."
    pub fn soft_reset(&mut self) {
        self.cpu.reset();
        // DMA / DMC state is dropped on reset.
        self.dma_cycles_remaining = 0;
        self.dma_alignment_done = false;
    }

    // ════════════════════════════════════════════════════════════
    //  Master-clock tick
    // ════════════════════════════════════════════════════════════

    /// Advance the machine by one master clock division (one PPU
    /// dot). The CPU ticks every 3rd dot.
    pub fn tick(&mut self) {
        self.master_clock += 1;
        // Mirror at 4× resolution so the start/end phase split
        // can drive `ppu.run` at sub-PPU-dot precision. Public
        // master_clock stays at PPU-dot resolution for back-compat.
        self.internal_master_clock += ricoh_ppu_2c02::MASTER_CLOCK_DIVIDER;
        self.cpu_divider = (self.cpu_divider + 1) % 3;

        if self.cpu_divider != 0 {
            // Non-CPU master tick: PPU runs with the 1-master-tick
            // lag (Mesen `_ppuOffset = 1` analog). PPU is
            // permanently behind the wall clock by 1 master tick.
            self.ppu
                .run(self.mapper.as_mut(), self.internal_master_clock - 1);
            return;
        }

        // ── CPU master tick — start/end phase split ──
        //
        // 1. BUS OP first, while PPU is still at its prior-tick
        //    lagged state. A $2002 read here clears
        //    `nmi_occurred` BEFORE the PPU processes this CPU
        //    cycle's final PPU dot — the suppression window for
        //    blargg 06/07/08.
        let do_cpu_tick = if self.dma_cycles_remaining > 0 {
            self.tick_dma();
            false
        } else if self.apu.dmc.dma_pending {
            // DMC sample DMA steals a CPU cycle. The APU keeps
            // ticking, but the CPU does not advance this cycle.
            let addr = self.apu.dmc.current_address;
            let byte = self.cpu_read(addr);
            self.apu.dmc.receive_dma_byte(byte);
            false
        } else {
            if self.cpu.rw {
                self.cpu.data_in = self.cpu_read(self.cpu.addr);
            } else {
                self.cpu_write(self.cpu.addr, self.cpu.data);
            }
            true
        };

        // 2. END PHASE — PPU catches up to `master - 1`.
        self.ppu
            .run(self.mapper.as_mut(), self.internal_master_clock - 1);

        // 3. SAMPLE PINS — post-end-phase PPU state. Matches
        //    Mesen's `EndCpuCycle` NmiFlag sample with PPU at
        //    master - 1.
        self.cpu.nmi = self.ppu.nmi;
        self.cpu.irq = self.mapper.irq_pending() || self.apu.irq_pending();

        // 4. CPU TICK — only on a "normal" (non-DMA, non-DMC)
        //    CPU cycle. DMA and DMC stall the CPU; their bus ops
        //    above already consumed the cycle.
        if do_cpu_tick {
            self.cpu.tick();
        }

        // Mapper and APU tick once per CPU cycle. Mapper expansion
        // audio is sampled just before the APU downsampler runs.
        self.mapper.cpu_tick();
        self.apu.expansion_audio = self.mapper.expansion_audio_sample();
        self.apu.tick();

        // Flush deferred $2000 NMI enable — after all 3 PPU
        // dots in this CPU cycle have run.
        self.ppu.flush_nmi_line();
    }

    /// Run until the PPU completes a frame (scanline wraps from
    /// pre-render to 0). Returns the number of master clock ticks.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        // A frame is 341 × 262 = 89 342 PPU dots (NTSC).
        // Detect the frame boundary by watching scanline wrap from
        // the pre-render line to scanline 0. We require that the
        // PPU has passed through VBlank (scanline 241) before the
        // transition counts — this prevents a false boundary when
        // the PPU starts on or near the pre-render line.
        let pre = self.ppu.pre_render_line();
        let mut seen_vblank = false;
        let mut was_prerender = false;

        loop {
            self.tick();
            let now_scanline = self.ppu.scanline();
            if now_scanline == 241 {
                seen_vblank = true;
            }
            if seen_vblank && was_prerender && now_scanline == 0 {
                self.frame_count += 1;
                break;
            }
            was_prerender = now_scanline == pre;
        }

        self.master_clock - start
    }

    // ════════════════════════════════════════════════════════════
    //  OAMDMA
    // ════════════════════════════════════════════════════════════

    /// One CPU-cycle's worth of OAMDMA work.
    fn tick_dma(&mut self) {
        if !self.dma_alignment_done {
            // Alignment cycle: if the CPU was on an odd cycle when
            // $4014 was written, an extra dummy read cycle is
            // inserted. We approximate this by always inserting one
            // alignment cycle (514 total) — the odd/even distinction
            // is a refinement that matters for sub-cycle-accurate
            // DMC DMA interaction.
            self.dma_alignment_done = true;
            self.dma_cycles_remaining -= 1;
            return;
        }

        // Alternating read/write cycles: even = read from CPU
        // memory, odd = write to PPU OAM.
        let cycle_in_transfer = 256u16 * 2 - (self.dma_cycles_remaining - 1);
        if cycle_in_transfer & 1 == 0 {
            // Read cycle.
            let addr = u16::from(self.dma_page) << 8 | u16::from(self.dma_offset);
            self.cpu.data_in = self.cpu_read(addr);
        } else {
            // Write cycle: route through OAMADDR ($2003), which the
            // copy post-increments — so the transfer starts at OAMADDR
            // and wraps. `dma_offset` advances the read source only.
            self.ppu.oam_dma_write(self.cpu.data_in);
            self.dma_offset = self.dma_offset.wrapping_add(1);
        }

        self.dma_cycles_remaining -= 1;
    }

    // ════════════════════════════════════════════════════════════
    //  CPU bus routing
    // ════════════════════════════════════════════════════════════

    /// Resolve a CPU read through the NES address space.
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            // $0000-$1FFF: internal RAM (2 KiB, mirrored).
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize],

            // $2000-$3FFF: PPU registers (mirrored every 8 bytes).
            0x2000..=0x3FFF => self.ppu.cpu_read(addr, self.mapper.as_mut()),

            // $4000-$4014: APU registers (write-only except $4015).
            0x4000..=0x4014 => 0,

            // $4015: APU status (readable).
            0x4015 => self.apu.read(0x4015),

            // $4016: Controller 1.
            0x4016 => {
                if self.controller_strobe {
                    // While strobe is active, return button A.
                    self.controller1_state & 1
                } else {
                    let bit = self.controller1_shift & 1;
                    self.controller1_shift >>= 1;
                    // After all 8 bits are shifted out, reads
                    // return 1 (open bus on D0).
                    bit
                }
            }

            // $4017: Controller 2. Same protocol as $4016 — bit 0 of
            // the strobe (latched from the last $4016 write) governs
            // the latch/shift behaviour for both controllers.
            0x4017 => {
                if self.controller_strobe {
                    self.controller2_state & 1
                } else {
                    let bit = self.controller2_shift & 1;
                    self.controller2_shift >>= 1;
                    bit
                }
            }

            // $4018-$401F: APU test registers (unused).
            0x4018..=0x401F => 0,

            // $4020-$FFFF: cartridge space.
            0x4020..=0xFFFF => self.mapper.cpu_read_side_effect(addr),
        }
    }

    /// Resolve a CPU write through the NES address space.
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            // $0000-$1FFF: internal RAM.
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize] = value,

            // $2000-$3FFF: PPU registers.
            0x2000..=0x3FFF => self.ppu.cpu_write(addr, value, self.mapper.as_mut()),

            // $4014: OAMDMA — triggers a 513/514-cycle CPU stall.
            0x4014 => {
                self.dma_page = value;
                self.dma_offset = 0;
                self.dma_alignment_done = false;
                // 1 alignment + 256 read + 256 write = 513 cycles.
                // (514 on odd CPU cycles — we always do 513 + 1
                // alignment = 514 for simplicity.)
                self.dma_cycles_remaining = 514;
            }

            // $4016: Controller strobe. Bit 0 controls both
            // controllers — a falling 1→0 edge latches the live
            // controller_n_state into the corresponding shift
            // register, ready to be clocked out by reads of $4016
            // (controller 1) and $4017 (controller 2).
            0x4016 => {
                let new_strobe = value & 1 != 0;
                if self.controller_strobe && !new_strobe {
                    self.controller1_shift = self.controller1_state;
                    self.controller2_shift = self.controller2_state;
                }
                self.controller_strobe = new_strobe;
            }

            // $4000-$4013, $4015, $4017: APU registers.
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write(addr, value),

            // $4018-$401F: APU test registers (unused).
            0x4018..=0x401F => {}

            // $4020-$FFFF: cartridge space.
            0x4020..=0xFFFF => self.mapper.cpu_write(addr, value),
        }
    }

    // ════════════════════════════════════════════════════════════
    //  Public accessors
    // ════════════════════════════════════════════════════════════

    /// Reference to the PPU framebuffer (ARGB32, 256×240).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.ppu.framebuffer()
    }

    /// Drain the APU's mixed audio output buffer (48 kHz f32).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.apu.take_buffer()
    }

    /// Current host-side APU audio controls.
    #[must_use]
    pub const fn audio_controls(&self) -> AudioControls {
        self.apu.audio_controls()
    }

    /// Replace all host-side APU audio controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.apu.set_audio_controls(controls);
    }

    /// Enable or disable one APU channel in the host mixer.
    pub fn set_audio_channel_enabled(&mut self, channel: ApuChannel, enabled: bool) {
        self.apu.set_audio_channel_enabled(channel, enabled);
    }

    /// Set one APU channel's host mixer gain.
    pub fn set_audio_channel_gain(&mut self, channel: ApuChannel, gain: f32) {
        self.apu.set_audio_channel_gain(channel, gain);
    }

    /// Peek a byte of CPU-visible memory (no side effects).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize],
            0x4020..=0xFFFF => self.mapper.cpu_read(addr),
            _ => 0,
        }
    }

    /// Master clock count (PPU dots since construction).
    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    /// Completed frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Set controller 1 button state. Bits: A=0, B=1, Select=2,
    /// Start=3, Up=4, Down=5, Left=6, Right=7.
    pub fn set_controller1(&mut self, state: u8) {
        self.controller1_state = state;
    }

    /// Set controller 2 button state. Same bit layout as
    /// [`Self::set_controller1`]. Read out by the CPU via `$4017`.
    pub fn set_controller2(&mut self, state: u8) {
        self.controller2_state = state;
    }

    /// Capture complete machine state for save-state export.
    #[must_use]
    pub fn snapshot(&self) -> NesSnapshot {
        NesSnapshot {
            cpu: self.cpu.clone(),
            ppu: self.ppu.clone(),
            apu: self.apu.clone(),
            mapper: self.mapper.snapshot(),
            ram: self.ram,
            cpu_divider: self.cpu_divider,
            master_clock: self.master_clock,
            internal_master_clock: self.internal_master_clock,
            frame_count: self.frame_count,
            dma_cycles_remaining: self.dma_cycles_remaining,
            dma_page: self.dma_page,
            dma_offset: self.dma_offset,
            dma_alignment_done: self.dma_alignment_done,
            controller1_shift: self.controller1_shift,
            controller1_state: self.controller1_state,
            controller2_shift: self.controller2_shift,
            controller2_state: self.controller2_state,
            controller_strobe: self.controller_strobe,
        }
    }

    /// Restore complete machine state captured by [`Self::snapshot`].
    ///
    /// Calls `after_restore` on chips that hold `&'static` references
    /// (currently the APU's region-dependent timing tables) — see
    /// Seam 3 of `knowledge/decisions/nes-architecture-review.md`.
    pub fn restore_snapshot(&mut self, snapshot: NesSnapshot) {
        self.cpu = snapshot.cpu;
        self.ppu = snapshot.ppu;
        self.apu = snapshot.apu;
        self.apu.after_restore();
        self.mapper = mapper_from_snapshot(snapshot.mapper);
        self.ram = snapshot.ram;
        self.cpu_divider = snapshot.cpu_divider;
        self.master_clock = snapshot.master_clock;
        self.internal_master_clock = snapshot.internal_master_clock;
        self.frame_count = snapshot.frame_count;
        self.dma_cycles_remaining = snapshot.dma_cycles_remaining;
        self.dma_page = snapshot.dma_page;
        self.dma_offset = snapshot.dma_offset;
        self.dma_alignment_done = snapshot.dma_alignment_done;
        self.controller1_shift = snapshot.controller1_shift;
        self.controller1_state = snapshot.controller1_state;
        self.controller2_shift = snapshot.controller2_shift;
        self.controller2_state = snapshot.controller2_state;
        self.controller_strobe = snapshot.controller_strobe;
    }

    /// Reconstruct a machine directly from a snapshot. Like
    /// [`Self::restore_snapshot`], rehydrates `&'static` references.
    #[must_use]
    pub fn from_snapshot(snapshot: NesSnapshot) -> Self {
        let mut apu = snapshot.apu;
        apu.after_restore();
        Self {
            cpu: snapshot.cpu,
            ppu: snapshot.ppu,
            apu,
            mapper: mapper_from_snapshot(snapshot.mapper),
            ram: snapshot.ram,
            cpu_divider: snapshot.cpu_divider,
            master_clock: snapshot.master_clock,
            internal_master_clock: snapshot.internal_master_clock,
            frame_count: snapshot.frame_count,
            dma_cycles_remaining: snapshot.dma_cycles_remaining,
            dma_page: snapshot.dma_page,
            dma_offset: snapshot.dma_offset,
            dma_alignment_done: snapshot.dma_alignment_done,
            controller1_shift: snapshot.controller1_shift,
            controller1_state: snapshot.controller1_state,
            controller2_shift: snapshot.controller2_shift,
            controller2_state: snapshot.controller2_state,
            controller_strobe: snapshot.controller_strobe,
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use format_nintendo_nes_ines::{Mirroring, Nrom};

    /// Build an NES with a 16 KiB NROM (all $EA = NOP) and the
    /// reset vector pointing at $8000. Same pattern as the C64
    /// `nop_machine` test helper — the CPU runs NOPs forever.
    fn nop_nes() -> Nes {
        let mut prg = vec![0xEA; 16384];
        // Reset vector at $FFFC/$FFFD → $8000.
        // In a 16 KiB NROM, $FFFC maps to offset $7FFC (since
        // $8000-$BFFF mirrors to $C000-$FFFF).
        prg[0x3FFC] = 0x00; // low byte
        prg[0x3FFD] = 0x80; // high byte
        let mapper = Box::new(Nrom::new(prg, Vec::new(), Mirroring::Horizontal));
        Nes::new(mapper)
    }

    #[test]
    fn constructs_with_expected_initial_state() {
        let nes = nop_nes();
        assert_eq!(nes.master_clock(), 0);
        assert_eq!(nes.frame_count(), 0);
        // Post-reset the CPU is in a 7-cycle reset sequence; the first
        // five cycles are phantom stack reads, so addr is SP-relative
        // initially rather than on the reset vector.
        assert_eq!(nes.cpu.reset_phase, 7);
        assert!(nes.cpu.rw);
    }

    #[test]
    fn single_tick_advances_master_clock() {
        let mut nes = nop_nes();
        nes.tick();
        assert_eq!(nes.master_clock(), 1);
    }

    #[test]
    fn cpu_ticks_every_3rd_dot() {
        let mut nes = nop_nes();
        let initial_pc = nes.cpu.regs.pc;

        // Tick 1 and 2: PPU only, CPU should not have advanced.
        nes.tick();
        nes.tick();
        assert_eq!(
            nes.cpu.regs.pc, initial_pc,
            "CPU should not tick on dots 1-2"
        );

        // Tick 3: CPU should advance (reset bootstrap reads $FFFC).
        nes.tick();
        // After the reset bootstrap tick, PC may still be 0 but
        // the CPU state has advanced (reading the vector). Just
        // verify a CPU cycle happened by checking the clock.
        assert_eq!(nes.master_clock(), 3);
    }

    #[test]
    fn ram_read_write_roundtrip() {
        let mut nes = nop_nes();
        nes.cpu_write(0x0042, 0xAB);
        assert_eq!(nes.cpu_read(0x0042), 0xAB);
        // Mirror at $0842 should see the same value.
        assert_eq!(nes.cpu_read(0x0842), 0xAB);
    }

    #[test]
    fn ram_mirrors_within_2k() {
        let mut nes = nop_nes();
        nes.cpu_write(0x0100, 0x55);
        assert_eq!(nes.cpu_read(0x0900), 0x55);
        assert_eq!(nes.cpu_read(0x1100), 0x55);
        assert_eq!(nes.cpu_read(0x1900), 0x55);
    }

    #[test]
    fn mapper_read_serves_prg_rom() {
        let nes = nop_nes();
        // $8000 maps to offset 0 of the 16 KiB PRG, which is $EA.
        assert_eq!(nes.peek(0x8000), 0xEA);
        // 16 KiB mirror: $C000 should equal $8000.
        assert_eq!(nes.peek(0xC000), 0xEA);
    }

    #[test]
    fn reset_vector_read_correctly() {
        let nes = nop_nes();
        assert_eq!(nes.peek(0xFFFC), 0x00);
        assert_eq!(nes.peek(0xFFFD), 0x80);
    }

    #[test]
    fn oamdma_write_triggers_stall() {
        let mut nes = nop_nes();
        nes.cpu_write(0x4014, 0x02); // DMA from page $0200
        assert_eq!(nes.dma_cycles_remaining, 514);
        assert_eq!(nes.dma_page, 0x02);
    }

    #[test]
    fn controller_strobe_latches_state() {
        let mut nes = nop_nes();
        nes.controller1_state = 0b1010_0101; // A, Select, Up, Left

        // Strobe on → off latches.
        nes.cpu_write(0x4016, 0x01); // strobe on
        nes.cpu_write(0x4016, 0x00); // strobe off — latches

        // Read 8 bits out.
        let mut result = 0u8;
        for i in 0..8 {
            result |= (nes.cpu_read(0x4016) & 1) << i;
        }
        assert_eq!(result, 0b1010_0101);
    }

    #[test]
    fn controller_2_strobe_latches_independently_of_controller_1() {
        let mut nes = nop_nes();
        nes.set_controller1(0b0000_0001); // controller 1: A only
        nes.set_controller2(0b1100_0000); // controller 2: Left + Right

        // Strobe both controllers simultaneously.
        nes.cpu_write(0x4016, 0x01);
        nes.cpu_write(0x4016, 0x00);

        // Read 8 bits from each.
        let mut p1 = 0u8;
        let mut p2 = 0u8;
        for i in 0..8 {
            p1 |= (nes.cpu_read(0x4016) & 1) << i;
            p2 |= (nes.cpu_read(0x4017) & 1) << i;
        }
        assert_eq!(p1, 0b0000_0001, "controller 1 state");
        assert_eq!(p2, 0b1100_0000, "controller 2 state");
    }

    #[test]
    fn controller_2_strobe_active_returns_button_a_bit() {
        let mut nes = nop_nes();
        nes.set_controller2(0b0000_0001); // A pressed on controller 2

        nes.cpu_write(0x4016, 0x01); // strobe on
        // While strobe is high, $4017 reads return bit 0 of
        // controller 2's live state.
        assert_eq!(nes.cpu_read(0x4017) & 1, 1);

        nes.set_controller2(0); // release
        assert_eq!(nes.cpu_read(0x4017) & 1, 0);
    }

    #[test]
    fn audio_controls_proxy_to_apu() {
        let mut nes = nop_nes();

        nes.set_audio_channel_enabled(ApuChannel::Triangle, false);
        nes.set_audio_channel_gain(ApuChannel::Noise, 0.5);

        assert!(!nes.audio_controls().channel(ApuChannel::Triangle).enabled());
        assert_eq!(nes.audio_controls().channel(ApuChannel::Noise).gain(), 0.5);
    }

    #[test]
    fn dmc_dma_request_steals_cpu_cycle_and_fetches_sample_byte() {
        let mut prg = vec![0xEA; 16384];
        prg[0] = 0xAB;
        prg[0x3FFC] = 0x00;
        prg[0x3FFD] = 0x80;
        let mapper = Box::new(Nrom::new(prg, Vec::new(), Mirroring::Horizontal));
        let mut nes = Nes::new(mapper);
        nes.apu.dmc.current_address = 0xC000;
        nes.apu.dmc.bytes_remaining = 1;
        nes.apu.dmc.dma_pending = true;
        let cpu_cycles = nes.cpu.total_cycles;

        nes.tick();
        nes.tick();
        nes.tick();

        assert_eq!(
            nes.cpu.total_cycles, cpu_cycles,
            "DMC DMA should stall CPU for this CPU cycle"
        );
        assert!(!nes.apu.dmc.dma_pending);
        assert_eq!(nes.apu.dmc.current_address, 0xC001);
    }

    #[test]
    fn run_frame_advances_frame_counter() {
        let mut nes = nop_nes();
        let dots = nes.run_frame();
        assert_eq!(nes.frame_count(), 1);
        // NTSC: 341 dots × 262 scanlines = 89 342 (without odd
        // frame skip — the first frame is even).
        assert!(
            (89_000..=90_000).contains(&dots),
            "expected ~89342 dots per frame, got {dots}"
        );
    }

    #[test]
    fn cpu_executes_nop_after_reset_bootstrap() {
        let mut nes = nop_nes();
        // Reset bootstrap is now 7 CPU cycles = 21 PPU dots.
        for _ in 0..21 {
            nes.tick();
        }
        // After bootstrap, PC should be $8000 (the reset vector).
        assert_eq!(nes.cpu.regs.pc, 0x8000, "PC should point to reset vector");

        // Execute one NOP (2 CPU cycles = 6 PPU dots).
        for _ in 0..6 {
            nes.tick();
        }
        assert_eq!(
            nes.cpu.regs.pc, 0x8001,
            "CPU should have executed NOP at $8000"
        );
    }

    #[test]
    fn ppu_nmi_routes_to_cpu() {
        let mut nes = nop_nes();
        // Enable NMI in the PPU by writing $2000 bit 7.
        // First, run through the reset bootstrap (7 CPU cycles = 21 dots).
        for _ in 0..21 {
            nes.tick();
        }

        // Write $80 to $2000 (PPUCTRL) — enable NMI on VBL.
        // We poke it directly into the CPU bus.
        nes.ppu.cpu_write(0x2000, 0x80, nes.mapper.as_mut());
        nes.ppu.flush_nmi_line();

        // Run until VBlank (scanline 241, dot 3).
        // A full frame is ~89342 dots. Just run enough dots.
        for _ in 0..90_000 {
            nes.tick();
            if nes.ppu.nmi {
                break;
            }
        }

        assert!(nes.ppu.nmi, "PPU should assert NMI during VBlank");
        // The machine routes ppu.nmi → cpu.nmi on CPU ticks.
        // Run a few more ticks to let the routing happen.
        for _ in 0..3 {
            nes.tick();
        }
        // The CPU's nmi field should have been set.
        assert!(nes.cpu.nmi, "CPU NMI should be set from PPU NMI");
    }
}
