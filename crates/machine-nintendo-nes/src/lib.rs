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
//! - The CPU ticks every 3rd PPU dot on NTSC, every 3.2 on PAL
//!   (`cpu_phase`, driven by [`Region`]).
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
//! - Runtime / `System` trait integration.
//! - Turbo / fast-forward / rewind.
//!
//! ⚠ "Sub-cycle-accurate OAMDMA/DMC DMA overlap arbitration" was listed here as
//! out of scope until it was measured. Every DMA read cycle of all sixteen OAM
//! transfers in `sprdma_and_dmc_dma` matches Mesen2 exactly, address for
//! address, so the arbitration is in scope and correct. Both ROMs now pass; the
//! remaining defect was in the DMC's `$4015` transfer-start delay, not the
//! arbitration. See
//! [nes-accuracy-closure-campaign.md](../../knowledge/decisions/nes-accuracy-closure-campaign.md).
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
pub use ricoh_ppu_2c02::{
    FB_HEIGHT, FB_WIDTH, TV_CROP_BOTTOM, TV_CROP_TOP, TV_VISIBLE_HEIGHT, TV_VISIBLE_WIDTH,
};

/// Console region. Selects the clock divider and frame geometry.
///
/// ⚠ The master oscillator drives the loop in both regions — only the
/// dividers change. NTSC runs 1 CPU cycle per 3 dots; PAL runs 1 per
/// **3.2** (dot = 5 master units, CPU = 16), giving a 3, 3, 3, 3, 4
/// pattern. See `knowledge/decisions/nes-clock-topology.md`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Region {
    /// 21.477 272 MHz master, 262 scanlines, 1 CPU cycle per 3 dots.
    #[default]
    Ntsc,
    /// 26.601 712 MHz master, 312 scanlines, 1 CPU cycle per 3.2 dots.
    Pal,
}

impl Region {
    /// Internal master-clock units in one PPU dot.
    #[must_use]
    pub const fn dot_units(self) -> u64 {
        match self {
            Self::Ntsc => 4,
            Self::Pal => 5,
        }
    }

    /// Internal master-clock units in one CPU cycle.
    #[must_use]
    pub const fn cpu_units(self) -> u64 {
        match self {
            Self::Ntsc => 12,
            Self::Pal => 16,
        }
    }

    /// Pre-render scanline: the last line of the frame.
    #[must_use]
    pub const fn pre_render_line(self) -> u16 {
        match self {
            Self::Ntsc => 261,
            Self::Pal => 311,
        }
    }

    /// Matching APU region, for the frame-counter and DMC rate tables.
    #[must_use]
    pub const fn apu_region(self) -> ricoh_apu_2a03::ApuRegion {
        match self {
            Self::Ntsc => ricoh_apu_2a03::ApuRegion::Ntsc,
            Self::Pal => ricoh_apu_2a03::ApuRegion::Pal,
        }
    }
}

/// Serializable NES machine state.
#[derive(Clone, Serialize, Deserialize)]
pub struct NesSnapshot {
    cpu: M6502,
    ppu: Ppu,
    apu: Apu,
    mapper: MapperSnapshot,
    #[serde(with = "BigArray")]
    ram: [u8; 2048],
    #[serde(default)]
    cpu_phase: u64,
    #[serde(default)]
    region: Region,
    master_clock: u64,
    #[serde(default)]
    internal_master_clock: u64,
    frame_count: u64,
    #[serde(default)]
    cpu_cycle_count: u64,
    dma_page: u8,
    dma_offset: u8,
    #[serde(default)]
    sprite_dma_active: bool,
    #[serde(default)]
    sprite_dma_counter: u16,
    #[serde(default)]
    dma_read_value: u8,
    #[serde(default)]
    dmc_dma_active: bool,
    #[serde(default)]
    dma_need_halt: bool,
    #[serde(default)]
    dma_need_dummy: bool,
    /// DMC DMA aborted after its halt cycle. `serde(default)` so snapshots
    /// written before this field existed still load as "not aborting".
    #[serde(default)]
    dma_abort_dmc: bool,
    #[serde(default)]
    dma_halt_done: bool,
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

    /// Master-clock units accumulated since the last CPU cycle.
    ///
    /// ⚠ Replaces a `% 3` counter because the CPU:PPU ratio is 1:3 on
    /// NTSC but **1:3.2** on PAL — 3, 3, 3, 3, 4 dots repeating. Both
    /// regions keep the master oscillator driving the loop; only the
    /// divider changes. See `knowledge/decisions/nes-clock-topology.md`.
    ///
    /// NTSC is bit-identical to the old counter: 4 units per dot, 12
    /// per CPU cycle, so the CPU still ticks on every third dot.
    cpu_phase: u64,
    /// NTSC or PAL. Fixed at construction.
    region: Region,

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

    /// CPU-cycle counter (advances every CPU cycle, *including* DMA
    /// cycles — unlike `cpu.total_cycles`, which freezes while the CPU
    /// is halted). Its parity drives the get/put alignment of DMA, the
    /// 2A03's continuous read(get)/write(put) cycle phase.
    cpu_cycle_count: u64,

    // ── DMA state (OAM sprite DMA + DMC sample DMA) ─────────────
    // Modelled on Mesen2's `NesCpu::ProcessPendingDma` (see
    // `emulators/nes/Mesen2/Core/NES/NesCpu.cpp`): a get/put-parity
    // state machine in which a DMC fetch interleaves with an in-flight
    // OAM transfer, stealing an aligned get cycle.
    /// Source page for the current OAM (sprite) DMA transfer (`$XX00`).
    dma_page: u8,
    /// Read offset within the 256-byte OAM DMA source page.
    dma_offset: u8,
    /// OAM (sprite) DMA in progress (`$4014` write → 256 read/write pairs).
    sprite_dma_active: bool,
    /// Cycle counter within the OAM transfer (0..=0x200); even = read
    /// (get), odd = write (put).
    sprite_dma_counter: u16,
    /// Byte latched by the last OAM read, to be written on the next put.
    dma_read_value: u8,
    /// DMC sample DMA in progress (the DMC channel needs a byte).
    dmc_dma_active: bool,
    /// A halt cycle is pending (the cycle that stalls the CPU).
    dma_need_halt: bool,
    /// A dummy cycle is pending (DMC requires a dummy before its fetch).
    dma_need_dummy: bool,
    /// DMC DMA aborted after its halt cycle: the fetch is skipped, but the
    /// cycles already spent are not refunded. Mesen2's `_abortDmcDma`.
    dma_abort_dmc: bool,
    /// The initial halt cycle has run and the DMA loop is in progress.
    dma_halt_done: bool,
    /// Diagnostic bus-op trace of DMA read cycles, armed by
    /// [`Nes::start_dma_trace`] and `None` otherwise, so the cost in a normal
    /// build is one predictable branch per DMA read cycle and nothing else.
    ///
    /// Exists to be diffed against a reference emulator. `sprdma_and_dmc_dma`
    /// reports its failure as a table of totals that cannot say WHICH cycle is
    /// wrong; the sequence of DMA read addresses can, because each address
    /// identifies its cycle's role — the OAM source page for a transfer read,
    /// the DMC sample address for a steal, the CPU's pending address for a
    /// halt, dummy or alignment cycle.
    dma_trace: Option<Vec<(u64, u16, bool)>>,
    /// CPU cycles at which the program wrote `$4015`, recorded alongside
    /// [`Self::dma_trace`]. Kept separate because a DMA halt read can land on
    /// `$4015` too, and the two are not distinguishable by address.
    reg_4015_trace: Option<Vec<u64>>,

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

    /// Last value driven onto the CPU data bus. Read from any
    /// write-only or unallocated APU register (`$4000-$4014`,
    /// `$4018-$401F`) returns this, and bits 5-7 of `$4016`/`$4017`
    /// reads are sourced from it (controller data lives in bits 0-4).
    /// Updated by [`Self::cpu_read`] after each successful resolve.
    /// Required by blargg `cpu_exec_space/test_cpu_exec_space_apu`.
    open_bus: u8,
}

impl Nes {
    /// Construct a new NES from a parsed cartridge mapper.
    ///
    /// The CPU is reset, placing the first bus op at `$FFFC` ready
    /// for the first [`Self::tick()`].
    #[must_use]
    pub fn new(mapper: Box<dyn Mapper>) -> Self {
        Self::new_with_region(mapper, Region::Ntsc)
    }

    /// Build a machine for an explicit [`Region`].
    ///
    /// ⚠ The region is fixed at construction: it selects the clock
    /// dividers, the PPU's frame geometry and the APU's timing tables,
    /// all of which are read on every tick. Nothing supports changing
    /// it on a running machine, and nothing should — a mid-run change
    /// would leave the PPU's dot counter and the CPU phase accumulator
    /// referring to different clocks.
    #[must_use]
    pub fn new_with_region(mapper: Box<dyn Mapper>, region: Region) -> Self {
        let mut cpu = M6502::new_2a03();
        cpu.reset();

        // Power-on: the PPU ignores PPUCTRL/MASK/SCROLL/ADDR writes for the
        // first ~frame until it reaches the pre-render line (nesdev power-up
        // state). Games wait for two VBLANKs before touching these, so the
        // lockout is invisible to correct code and silently drops the writes
        // of code that does not wait.
        let mut ppu = Ppu::new_with_timing(region.pre_render_line(), region.dot_units());
        // The 2C07 runs every frame at the full 341 dots.
        ppu.set_odd_frame_dot_skip(region == Region::Ntsc);
        ppu.arm_reset_write_lockout();

        Self {
            cpu,
            ppu,
            apu: Apu::new_with_region(region.apu_region()),
            mapper,
            ram: [0; 2048],
            cpu_phase: 0,
            region,
            master_clock: 0,
            internal_master_clock: 0,
            frame_count: 0,
            cpu_cycle_count: 0,
            dma_page: 0,
            dma_offset: 0,
            sprite_dma_active: false,
            sprite_dma_counter: 0,
            dma_read_value: 0,
            dmc_dma_active: false,
            dma_need_halt: false,
            dma_need_dummy: false,
            dma_abort_dmc: false,
            dma_halt_done: false,
            dma_trace: None,
            reg_4015_trace: None,
            controller1_shift: 0,
            controller1_state: 0,
            controller2_shift: 0,
            controller2_state: 0,
            controller_strobe: false,
            open_bus: 0,
        }
    }

    /// Soft reset — equivalent to pressing the reset button on the
    /// Famicom / NES front panel. CPU refetches the reset vector
    /// at `$FFFC`/`$FFFD` (sets I, SP -= 3, 7-cycle reset on next
    /// tick). APU clears `$4015`, rewrites `$4017` with the last
    /// value written (preserves frame counter mode — distinct from
    /// power-on which writes `$00`), and clears the frame IRQ
    /// flag. PPU and mapper retain state (matches real hardware).
    ///
    /// Required by blargg `apu_reset/*` tests which write the `$81`
    /// status code at `$6000` to signal "press reset button now."
    pub fn soft_reset(&mut self) {
        self.cpu.reset();
        self.apu.soft_reset();
        // Reset re-arms the PPU register write-lockout (Mesen arms it on both
        // power-on and soft reset); the rest of the PPU keeps its state.
        self.ppu.arm_reset_write_lockout();
        // DMA / DMC state is dropped on reset.
        self.sprite_dma_active = false;
        self.sprite_dma_counter = 0;
        self.dmc_dma_active = false;
        self.dma_need_halt = false;
        self.dma_need_dummy = false;
        self.dma_abort_dmc = false;
        self.dma_halt_done = false;
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
        self.internal_master_clock += self.region.dot_units();
        self.cpu_phase += self.region.dot_units();

        // The CPU cycle boundary. On NTSC this is exactly every third
        // dot, identical to the `% 3` counter it replaces; on PAL the
        // accumulator produces the 3, 3, 3, 3, 4 pattern that 16 master
        // units per CPU cycle against 5 per dot demands.
        let cpu_due = self.cpu_phase >= self.region.cpu_units();
        if cpu_due {
            self.cpu_phase -= self.region.cpu_units();
        }

        if !cpu_due {
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
        // This is a CPU cycle (one of every three master ticks). Advance
        // the get/put parity counter — it ticks during DMA cycles too,
        // where `cpu.total_cycles` would freeze.
        self.cpu_cycle_count += 1;

        // Publish the get/put phase to the APU. The DMC's transfer-start
        // delay is chosen from it, and it must be the same counter the DMA
        // arbiter aligns on (see `dma_cycle`) rather than the APU's own.
        self.apu.cpu_cycle_odd = self.cpu_cycle_count & 1 != 0;

        // Take a newly-pending DMC sample DMA under machine control; the
        // machine then owns its halt → dummy → fetch sequence and can
        // interleave it with an in-flight OAM transfer.
        if self.apu.dmc.dma_pending && !self.dmc_dma_active {
            self.apu.dmc.dma_pending = false;
            self.dmc_dma_active = true;
            self.dma_need_halt = true;
            self.dma_need_dummy = true;
        }

        // ⚠⚠ A `$4015` write that clears DMC enable cancels the transfer, and
        // WHERE it lands changes the cycle cost — which is what
        // `sprdma_and_dmc_dma` measures across 16 alignments.
        //
        // Mesen2 splits this in `StopDmcTransfer()`:
        //   * still waiting on the halt cycle → cancel outright, no cycles taken
        //   * already halted → it can only be ABORTED, and the halt/dummy cycles
        //     already spent are not refunded
        //
        // Collapsing the two (or ignoring the write entirely, as before) makes
        // the transfer cost whole cycles too many at some alignments.
        if std::mem::take(&mut self.apu.dmc.dma_cancelled) && self.dmc_dma_active {
            if self.dma_need_halt {
                // Pre-halt: nothing has been spent, so drop the whole request.
                self.dmc_dma_active = false;
                self.dma_need_halt = false;
                self.dma_need_dummy = false;
            } else {
                // Post-halt: abort. The in-flight cycles stand; the fetch does
                // not happen. An OAM transfer sharing these cycles continues.
                self.dma_abort_dmc = true;
            }
        }

        let dma_pending = self.sprite_dma_active || self.dmc_dma_active;
        // DMA halts the CPU only on a read cycle (RDY gates reads, not
        // writes); once the halt cycle has run the transfer runs to
        // completion regardless of the (now frozen) CPU bus direction.
        let do_cpu_tick = if dma_pending && (self.dma_halt_done || self.cpu.rw) {
            self.dma_cycle();
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

    /// One CPU cycle of DMA work — OAM (sprite) DMA and/or DMC sample
    /// DMA. The CPU is halted; this performs the cycle's bus op.
    ///
    /// Modelled on Mesen2's `NesCpu::ProcessPendingDma`. Get/put
    /// alignment is keyed off [`Self::cpu_cycle_count`] (a get cycle is
    /// even), since the 2A03 alternates read(get)/write(put) cycles
    /// continuously. The first cycle is always a halt (a dummy read that
    /// stalls the CPU); thereafter, on a get cycle the DMC fetches if
    /// ready (stealing the cycle), else OAM reads a byte, else a dummy
    /// read; on a put cycle OAM writes the latched byte, else an
    /// alignment dummy. A DMC request raised mid-OAM has its halt/dummy
    /// absorbed by the OAM cycles, then steals the next aligned get.
    fn dma_cycle(&mut self) {
        // Halt cycle: the first DMA cycle stalls the CPU with a dummy
        // read at its pending address (side effects intended — this is
        // the source of the DMC/controller read glitch).
        if !self.dma_halt_done {
            self.dma_halt_done = true;
            self.dma_need_halt = false;
            let _ = self.dma_read_halt(self.cpu.addr);
            return;
        }

        let get_cycle = self.cpu_cycle_count & 1 == 0;
        if get_cycle {
            if self.dma_abort_dmc {
                // Aborted after halt: this cycle is still consumed, but no
                // sample fetch happens and the DMC request is discarded.
                self.dma_abort_dmc = false;
                self.dmc_dma_active = false;
                self.dma_consume_flag();
                let _ = self.dma_read(self.cpu.addr);
            } else if self.dmc_dma_active && !self.dma_need_halt && !self.dma_need_dummy {
                // DMC ready — fetch the sample byte, stealing this get
                // cycle from any in-flight OAM transfer.
                self.dma_consume_flag();
                let addr = self.apu.dmc.current_address;
                let byte = self.dma_read(addr);
                self.apu.dmc.receive_dma_byte(byte);
                self.dmc_dma_active = false;
            } else if self.sprite_dma_active {
                // OAM read.
                self.dma_consume_flag();
                let addr = u16::from(self.dma_page) << 8 | u16::from(self.dma_offset);
                self.dma_read_value = self.dma_read(addr);
                self.dma_offset = self.dma_offset.wrapping_add(1);
                self.sprite_dma_counter += 1;
            } else {
                // DMC running but not yet ready (halt/dummy pending),
                // no OAM transfer: a dummy read.
                self.dma_consume_flag();
                let _ = self.dma_read(self.cpu.addr);
            }
        } else if self.sprite_dma_active && self.sprite_dma_counter & 1 == 1 {
            // OAM write — route through OAMADDR ($2003), which the PPU
            // post-increments, so the copy starts at OAMADDR and wraps.
            self.dma_consume_flag();
            self.ppu.oam_dma_write(self.dma_read_value);
            self.sprite_dma_counter += 1;
            if self.sprite_dma_counter == 0x200 {
                self.sprite_dma_active = false;
            }
        } else {
            // Put cycle with no OAM write due: alignment dummy read.
            self.dma_consume_flag();
            let _ = self.dma_read(self.cpu.addr);
        }

        if !self.sprite_dma_active && !self.dmc_dma_active {
            self.dma_halt_done = false;
        }
    }

    /// One DMA read cycle's bus op, recording the address when a trace is
    /// armed. Every read in [`Self::dma_cycle`] goes through here so a trace
    /// cannot silently miss a cycle.
    fn dma_read(&mut self, addr: u16) -> u8 {
        self.dma_read_tagged(addr, false)
    }

    /// As [`Self::dma_read`], but marks this cycle as the halt that opens a DMA
    /// episode, so a trace can be split into episodes for comparison.
    fn dma_read_halt(&mut self, addr: u16) -> u8 {
        self.dma_read_tagged(addr, true)
    }

    fn dma_read_tagged(&mut self, addr: u16, is_halt: bool) -> u8 {
        if let Some(trace) = self.dma_trace.as_mut() {
            trace.push((self.cpu_cycle_count, addr, is_halt));
        }
        self.cpu_read(addr)
    }

    /// Arm the diagnostic DMA bus-op trace, discarding anything already
    /// recorded. See [`Self::dma_trace`].
    pub fn start_dma_trace(&mut self) {
        self.dma_trace = Some(Vec::new());
        self.reg_4015_trace = Some(Vec::new());
    }

    /// Number of DMA episodes recorded so far, so a caller can stop ticking
    /// once it has seen enough without draining the trace.
    pub fn dma_trace_episodes(&self) -> usize {
        self.dma_trace
            .as_ref()
            .map_or(0, |t| t.iter().filter(|(_, _, halt)| *halt).count())
    }

    /// Take the recorded `$4015` write cycles. See [`Self::reg_4015_trace`].
    pub fn take_reg_4015_trace(&mut self) -> Vec<u64> {
        self.reg_4015_trace.take().unwrap_or_default()
    }

    /// Take the recorded DMA read addresses and disarm the trace.
    pub fn take_dma_trace(&mut self) -> Vec<(u64, u16, bool)> {
        self.dma_trace.take().unwrap_or_default()
    }

    /// Clear one pending DMA halt/dummy flag per cycle (Mesen2's
    /// `processCycle`): OAM cycles double as the DMC's halt/dummy when
    /// both DMAs run together, so the DMC adds no extra cycles beyond the
    /// one get cycle it steals.
    fn dma_consume_flag(&mut self) {
        if self.dma_need_halt {
            self.dma_need_halt = false;
        } else if self.dma_need_dummy {
            self.dma_need_dummy = false;
        }
    }

    // ════════════════════════════════════════════════════════════
    //  CPU bus routing
    // ════════════════════════════════════════════════════════════

    /// Resolve a CPU read through the NES address space.
    ///
    /// Wraps [`Self::cpu_read_resolve`] and latches the returned
    /// value into `self.open_bus`, so the next read from any
    /// write-only / unallocated APU register sees whatever was last
    /// on the data bus — the behaviour blargg's `cpu_exec_space`
    /// suite relies on.
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let value = self.cpu_read_resolve(addr);
        self.open_bus = value;
        value
    }

    fn cpu_read_resolve(&mut self, addr: u16) -> u8 {
        match addr {
            // $0000-$1FFF: internal RAM (2 KiB, mirrored).
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize],

            // $2000-$3FFF: PPU registers (mirrored every 8 bytes).
            0x2000..=0x3FFF => self.ppu.cpu_read(addr, self.mapper.as_mut()),

            // $4000-$4014: APU registers (write-only except $4015).
            // Reads return whatever was last on the data bus.
            0x4000..=0x4014 => self.open_bus,

            // $4015: APU status (readable).
            0x4015 => self.apu.read(0x4015),

            // $4016: Controller 1. Bits 0-4 are real data, bits
            // 5-7 are open bus (per nesdev).
            0x4016 => {
                let data = if self.controller_strobe {
                    self.controller1_state & 1
                } else {
                    let bit = self.controller1_shift & 1;
                    self.controller1_shift >>= 1;
                    bit
                };
                (self.open_bus & 0xE0) | (data & 0x1F)
            }

            // $4017: Controller 2. Same protocol as $4016 — bit 0 of
            // the strobe (latched from the last $4016 write) governs
            // the latch/shift behaviour for both controllers.
            0x4017 => {
                let data = if self.controller_strobe {
                    self.controller2_state & 1
                } else {
                    let bit = self.controller2_shift & 1;
                    self.controller2_shift >>= 1;
                    bit
                };
                (self.open_bus & 0xE0) | (data & 0x1F)
            }

            // $4018-$401F: APU test registers (unused). Open bus.
            0x4018..=0x401F => self.open_bus,

            // $4020-$5FFF: cartridge expansion area. Most mappers
            // leave it floating, so it reads as open bus; MMC5 claims
            // it for its IRQ-status, multiplier and ExRAM registers.
            // The mapper returns `None` when it leaves the bus
            // floating, and the open-bus value stands.
            0x4020..=0x5FFF => self
                .mapper
                .cpu_read_expansion(addr)
                .unwrap_or(self.open_bus),

            // $6000-$FFFF: PRG-RAM + PRG-ROM.
            0x6000..=0xFFFF => self.mapper.cpu_read_side_effect(addr),
        }
    }

    /// Resolve a CPU write through the NES address space.
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            // $0000-$1FFF: internal RAM.
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize] = value,

            // $2000-$3FFF: PPU registers.
            0x2000..=0x3FFF => self.ppu.cpu_write(addr, value, self.mapper.as_mut()),

            // $4015: APU status. Recorded in the DMA trace when armed, so a
            // diagnostic can see where the ROM re-arms the DMC relative to the
            // sample fetches. ⚠ A DMA halt read at $4015 would be
            // indistinguishable here; that is acceptable for a diagnostic.
            0x4015 => {
                if let Some(trace) = self.reg_4015_trace.as_mut() {
                    trace.push(self.cpu_cycle_count);
                }
                self.apu.write(addr, value);
            }

            // $4014: OAMDMA — halts the CPU and copies 256 bytes from
            // page $XX00 to OAM. A halt cycle + 256 read/write pairs =
            // 513 or 514 cycles by get/put alignment; see `dma_cycle`.
            0x4014 => {
                self.dma_page = value;
                self.dma_offset = 0;
                self.sprite_dma_counter = 0;
                self.sprite_dma_active = true;
                self.dma_need_halt = true;
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
            0x4000..=0x4013 | 0x4017 => self.apu.write(addr, value),

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

    /// PPU framebuffer width (256).
    #[must_use]
    pub const fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// PPU framebuffer height (240).
    #[must_use]
    pub const fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    /// Copy the TV-visible region of the framebuffer (256 × 224 — the
    /// PPU's full 256 × 240 with the top 8 and bottom 8 overscan lines
    /// removed). Allocates a fresh `Vec<u32>`; for high-frequency
    /// snapshotting prefer reading the raw `framebuffer()` slice and
    /// indexing from `TV_CROP_TOP * FB_WIDTH`.
    #[must_use]
    pub fn framebuffer_tv_visible(&self) -> Vec<u32> {
        let fb = self.ppu.framebuffer();
        let start = (TV_CROP_TOP * FB_WIDTH) as usize;
        let end = ((FB_HEIGHT - TV_CROP_BOTTOM) * FB_WIDTH) as usize;
        fb[start..end].to_vec()
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

    /// The 2 KiB of nametable the PPU actually fetches, mapper included.
    ///
    /// ⚠⚠ Use this, not `ppu.nametable_ram()`, to read what is on screen.
    /// A mapper may serve `$2000-$2FFF` from its own memory: MMC5 keeps
    /// its nametable RAM inside the mapper and can map ExRAM or a fill
    /// tile into any of the four slots, so the console's CIRAM stays
    /// **entirely empty** for every MMC5 ROM. Three of them were briefly
    /// recorded as rendering nothing on exactly that mistake — the ROMs
    /// were fine, and the framebuffer proved it.
    ///
    /// Side-effect free: goes through [`Mapper::nametable_peek`] rather
    /// than `nametable_read`, which would clock MMC5's scanline detector.
    #[must_use]
    pub fn effective_nametable(&self) -> [u8; 2048] {
        let ciram = self.ppu.nametable_ram();
        let mut out = [0u8; 2048];
        for (i, slot) in out.iter_mut().enumerate() {
            let addr = 0x2000 + u16::try_from(i).expect("2048 fits in u16");
            *slot = self
                .mapper
                .nametable_peek(addr)
                .unwrap_or_else(|| ciram[i & 0x07FF]);
        }
        out
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

    /// Borrow the 6502 CPU. Companion accessor to the public `cpu`
    /// field — the shared `impl_6502_debug_primitives!` macro reaches
    /// the register file through `cpu().regs`, matching every other
    /// 6502 machine in the fleet.
    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    /// Debug write into the CPU address space, mirroring [`Self::peek`]:
    /// internal RAM and cartridge space only. Deliberately *not* the
    /// full bus path — a debug poke must not trigger OAMDMA, PPU/APU
    /// register side effects, or the controller strobe.
    pub fn poke(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize] = value,
            0x4020..=0xFFFF => self.mapper.cpu_write(addr, value),
            _ => {}
        }
    }

    /// Advance until the CPU retires one instruction, returning the
    /// master-clock ticks consumed. The CPU runs on every third master
    /// tick; this ticks until the CPU leaves its current instruction
    /// boundary and arrives at a new one with a different PC — the same
    /// boundary detection the MCP `step` tool open-coded. Bounded so a
    /// wedged CPU (e.g. a `KIL`/jam opcode) can't spin forever.
    pub fn step_instruction(&mut self) -> u64 {
        const MAX_TICKS: u64 = 100_000;
        let start_pc = self.cpu.regs.pc;
        let mut left_boundary = false;
        let mut ticks = 0u64;
        while ticks < MAX_TICKS {
            self.tick();
            ticks += 1;
            let complete = self.cpu.instruction_complete();
            if !complete {
                left_boundary = true;
            }
            if left_boundary && complete && self.cpu.regs.pc != start_pc {
                break;
            }
        }
        ticks
    }

    /// Master clock count (PPU dots since construction).
    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    /// CPU cycles since construction, DMA stalls included.
    ///
    /// Differs from `cpu.total_cycles`, which freezes while the CPU is
    /// halted for DMA. This one is the get/put phase counter.
    #[must_use]
    pub fn cpu_cycle_count(&self) -> u64 {
        self.cpu_cycle_count
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
            cpu_phase: self.cpu_phase,
            region: self.region,
            master_clock: self.master_clock,
            internal_master_clock: self.internal_master_clock,
            frame_count: self.frame_count,
            cpu_cycle_count: self.cpu_cycle_count,
            dma_page: self.dma_page,
            dma_offset: self.dma_offset,
            sprite_dma_active: self.sprite_dma_active,
            sprite_dma_counter: self.sprite_dma_counter,
            dma_read_value: self.dma_read_value,
            dmc_dma_active: self.dmc_dma_active,
            dma_need_halt: self.dma_need_halt,
            dma_need_dummy: self.dma_need_dummy,
            dma_abort_dmc: self.dma_abort_dmc,
            dma_halt_done: self.dma_halt_done,
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
        self.cpu_phase = snapshot.cpu_phase;
        self.region = snapshot.region;
        self.master_clock = snapshot.master_clock;
        self.internal_master_clock = snapshot.internal_master_clock;
        self.frame_count = snapshot.frame_count;
        self.cpu_cycle_count = snapshot.cpu_cycle_count;
        self.dma_page = snapshot.dma_page;
        self.dma_offset = snapshot.dma_offset;
        self.sprite_dma_active = snapshot.sprite_dma_active;
        self.sprite_dma_counter = snapshot.sprite_dma_counter;
        self.dma_read_value = snapshot.dma_read_value;
        self.dmc_dma_active = snapshot.dmc_dma_active;
        self.dma_need_halt = snapshot.dma_need_halt;
        self.dma_need_dummy = snapshot.dma_need_dummy;
        self.dma_abort_dmc = snapshot.dma_abort_dmc;
        self.dma_halt_done = snapshot.dma_halt_done;
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
            cpu_phase: snapshot.cpu_phase,
            region: snapshot.region,
            master_clock: snapshot.master_clock,
            internal_master_clock: snapshot.internal_master_clock,
            frame_count: snapshot.frame_count,
            cpu_cycle_count: snapshot.cpu_cycle_count,
            dma_page: snapshot.dma_page,
            dma_offset: snapshot.dma_offset,
            sprite_dma_active: snapshot.sprite_dma_active,
            sprite_dma_counter: snapshot.sprite_dma_counter,
            dma_read_value: snapshot.dma_read_value,
            dmc_dma_active: snapshot.dmc_dma_active,
            dma_need_halt: snapshot.dma_need_halt,
            dma_need_dummy: snapshot.dma_need_dummy,
            dma_abort_dmc: snapshot.dma_abort_dmc,
            dma_halt_done: snapshot.dma_halt_done,
            dma_trace: None,
            reg_4015_trace: None,
            controller1_shift: snapshot.controller1_shift,
            controller1_state: snapshot.controller1_state,
            controller2_shift: snapshot.controller2_shift,
            controller2_state: snapshot.controller2_state,
            controller_strobe: snapshot.controller_strobe,
            // open_bus isn't snapshotted — the next CPU read will
            // refresh it before any code can observe the value.
            open_bus: 0,
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use format_nintendo_nes_ines::{Mirroring, Nrom, parse_ines};

    /// Run a blargg-style NES test ROM (result block at `$6000`,
    /// signature `DE B0 61` at `$6001-3`, `$6000`=`0x80` running else
    /// result; `0`=pass). Returns the result code, or `None` on timeout.
    #[cfg(test)]
    fn run_blargg_nes(rom: &[u8], max_frames: u64) -> Option<u8> {
        let cart = parse_ines(rom).ok()?;
        let mut nes = Nes::new(cart.mapper);
        let mut needs_reset_at: Option<u64> = None;
        for frame in 0..max_frames {
            nes.run_frame();
            if nes.peek(0x6001) == 0xDE && nes.peek(0x6002) == 0xB0 && nes.peek(0x6003) == 0x61 {
                let status = nes.peek(0x6000);
                match status {
                    0x80 => {} // running
                    0x81 => {
                        // Test requests a soft reset ~100ms later.
                        if needs_reset_at.is_none() {
                            needs_reset_at = Some(frame + 7);
                        }
                        if needs_reset_at == Some(frame) {
                            nes.soft_reset();
                            needs_reset_at = None;
                        }
                    }
                    other => return Some(other),
                }
            }
        }
        None
    }

    #[test]
    #[ignore = "diagnostic: run a directory of blargg NES test ROMs (EMU198X_NES_SUITE)"]
    fn diagnostic_nes_suite() {
        // ⚠⚠ SKIP when the input is absent; never panic. This test lives in the
        // LIB target, which cargo runs before the integration targets, so a
        // panic here fast-fails the whole package: `cargo test
        // -p machine-nintendo-nes -- --ignored` reported "0 passed; 1 failed;
        // 19 filtered out" and ran no blargg ROM at all. The NES ignored suite
        // was therefore unrunnable on any machine without this variable set,
        // which is a plausible reason the core reached 3,300 lines of PPU with
        // no recorded accuracy measurement while other systems got campaigns.
        //
        // Every other ROM-dependent test here already does the right thing —
        // `blargg_root()` returns Option and the test becomes a no-op. Match it.
        let Ok(dir) = std::env::var("EMU198X_NES_SUITE") else {
            emu198x_test_skip::skip!("diagnostic_nes_suite: EMU198X_NES_SUITE not set");
        };
        // A set-but-wrong path is the same situation: report and skip rather
        // than take the package down with it.
        let Ok(entries) = std::fs::read_dir(&dir) else {
            emu198x_test_skip::skip!(
                "diagnostic_nes_suite: EMU198X_NES_SUITE is set but {dir} cannot be read"
            );
        };
        let mut roms: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "nes"))
            .collect();
        roms.sort();
        let frames = std::env::var("EMU198X_NES_FRAMES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(600);
        let (mut pass, mut total) = (0, 0);
        for rom in &roms {
            let name = rom
                .file_stem()
                .expect("ROM has a file stem")
                .to_string_lossy()
                .to_string();
            let code = run_blargg_nes(&std::fs::read(rom).expect("read NES ROM"), frames);
            total += 1;
            if code == Some(0) {
                pass += 1;
            }
            println!(
                "{name:<34} -> {code:?}  {}",
                if code == Some(0) { "PASS" } else { "fail" }
            );
        }
        println!(
            "\nNES suite {}: {pass}/{total}",
            dir.rsplit('/').nth(1).unwrap_or("")
        );
    }

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
    fn oamdma_write_arms_sprite_dma() {
        let mut nes = nop_nes();
        nes.cpu_write(0x4014, 0x02); // DMA from page $0200
        assert!(nes.sprite_dma_active, "OAM DMA armed");
        assert!(nes.dma_need_halt, "halt cycle pending");
        assert_eq!(nes.dma_page, 0x02);
    }

    #[test]
    fn oamdma_copies_256_bytes() {
        let mut nes = nop_nes();
        // Run past the reset bootstrap so the CPU is executing NOPs.
        for _ in 0..30 {
            nes.tick();
        }
        nes.cpu_write(0x4014, 0x02);
        // Tick until the OAM DMA completes (halt + 256 read/write pairs).
        let mut guard = 0;
        while nes.sprite_dma_active && guard < 4000 {
            nes.tick();
            guard += 1;
        }
        assert!(!nes.sprite_dma_active, "OAM DMA completes");
        assert_eq!(nes.sprite_dma_counter, 0x200, "256 bytes copied");
    }

    #[test]
    #[ignore = "diagnostic: prints OAM DMA length at each get/put alignment"]
    fn probe_oamdma_length_by_alignment() {
        for parity in 0..2u64 {
            let mut nes = nop_nes();
            for _ in 0..30 {
                nes.tick();
            }
            // Force the get/put phase the DMA will see, so both alignments
            // are measured from an otherwise identical machine state.
            // `tick` is one PPU dot; the parity counter only moves on the
            // one master tick in three that is a CPU cycle.
            while nes.cpu_cycle_count & 1 != parity {
                nes.tick();
            }
            nes.cpu_write(0x4014, 0x02);

            let mut start = None;
            let mut guard = 0;
            while nes.sprite_dma_active && guard < 8000 {
                nes.tick();
                if start.is_none() && nes.dma_halt_done {
                    start = Some(nes.cpu_cycle_count);
                }
                guard += 1;
            }
            let cycles = nes.cpu_cycle_count - start.expect("DMA halted");
            eprintln!("parity {parity}: OAM DMA took {} cycles", cycles + 1);
        }
    }

    /// Reproduce `sprdma_and_dmc_dma`'s experiment in-process: start an OAM
    /// transfer, raise a DMC sample DMA `t` CPU cycles later, and measure the
    /// combined stall. Mesen2 reports a table that alternates by one cycle
    /// with `t`'s parity; a flat column here localises the defect to the
    /// arbitration rather than to either DMA on its own.
    #[test]
    #[ignore = "diagnostic: prints combined OAM+DMC stall per DMC offset"]
    fn probe_combined_dma_by_dmc_offset() {
        for parity in 0..2u64 {
            let mut row = Vec::new();
            for t in 0..16u64 {
                let mut nes = nop_nes();
                for _ in 0..30 {
                    nes.tick();
                }
                while nes.cpu_cycle_count & 1 != parity {
                    nes.tick();
                }
                // Drive the DMC from its own timer rather than poking
                // `dma_pending` once. A bare OAM transfer is 513/514 cycles and
                // the ROM reports 525-528, so the DMC must steal repeatedly
                // across a single transfer — a single-shot request is the wrong
                // scenario, and the interaction between successive steals is
                // exactly what a single shot cannot show.
                nes.cpu_write(0x4010, 0x0F); // fastest rate: reload every 54 CPU cycles
                nes.cpu_write(0x4012, 0xC0); // sample at $C000
                nes.cpu_write(0x4013, 0xFF); // long enough to run throughout
                nes.cpu_write(0x4015, 0x10); // enable DMC

                // Slide the `$4014` write by `t` CPU cycles — the variable the
                // ROM's T+ column sweeps — against that free-running DMC.
                let base = nes.cpu_cycle_count;
                let mut written = false;
                let mut start = None;
                let mut guard = 0;
                while (nes.sprite_dma_active || !written) && guard < 8000 {
                    if !written && nes.cpu_cycle_count >= base + t {
                        nes.cpu_write(0x4014, 0x02);
                        written = true;
                    }
                    nes.tick();
                    if start.is_none() && nes.dma_halt_done {
                        start = Some(nes.cpu_cycle_count);
                    }
                    guard += 1;
                }
                row.push(nes.cpu_cycle_count - start.expect("DMA halted") + 1);
            }
            let cells: Vec<String> = row.iter().map(|c| c.to_string()).collect();
            eprintln!("parity {parity}: {}", cells.join(" "));
        }
    }

    #[test]
    #[ignore = "diagnostic: prints DMC DMA length at each get/put alignment"]
    fn probe_dmc_dma_length_by_alignment() {
        for parity in 0..2u64 {
            let mut nes = nop_nes();
            for _ in 0..30 {
                nes.tick();
            }
            while nes.cpu_cycle_count & 1 != parity {
                nes.tick();
            }
            nes.apu.dmc.current_address = 0xC000;
            nes.apu.dmc.bytes_remaining = 1;
            nes.apu.dmc.dma_pending = true;

            let mut start = None;
            let mut guard = 0;
            while (nes.apu.dmc.dma_pending || nes.dmc_dma_active) && guard < 200 {
                nes.tick();
                if start.is_none() && nes.dma_halt_done {
                    start = Some(nes.cpu_cycle_count);
                }
                guard += 1;
            }
            let cycles = nes.cpu_cycle_count - start.expect("DMA halted");
            eprintln!("parity {parity}: DMC DMA took {} cycles", cycles + 1);
        }
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
    fn dmc_dma_request_stalls_cpu_and_fetches_sample_byte() {
        let mut prg = vec![0xEA; 16384];
        prg[0] = 0xAB;
        prg[0x3FFC] = 0x00;
        prg[0x3FFD] = 0x80;
        let mapper = Box::new(Nrom::new(prg, Vec::new(), Mirroring::Horizontal));
        let mut nes = Nes::new(mapper);
        // Run past the reset bootstrap so the CPU is fetching opcodes.
        for _ in 0..30 {
            nes.tick();
        }
        nes.apu.dmc.current_address = 0xC000;
        nes.apu.dmc.bytes_remaining = 1;
        nes.apu.dmc.dma_pending = true;
        let cpu_cycles_before = nes.cpu.total_cycles;

        // DMC DMA now takes several CPU cycles (halt + dummy + possible
        // alignment + fetch); tick until it completes.
        let mut ticks = 0u32;
        while (nes.apu.dmc.dma_pending || nes.dmc_dma_active) && ticks < 60 {
            nes.tick();
            ticks += 1;
        }

        assert!(!nes.dmc_dma_active, "DMC DMA completes");
        assert!(!nes.apu.dmc.dma_pending);
        assert_eq!(
            nes.apu.dmc.current_address, 0xC001,
            "sample byte fetched and address advanced"
        );
        // The CPU was halted through the DMA, so it advanced by fewer
        // cycles than the CPU cycles that elapsed.
        let cpu_advanced = nes.cpu.total_cycles - cpu_cycles_before;
        assert!(
            cpu_advanced < u64::from(ticks),
            "CPU stalled during DMC DMA (advanced {cpu_advanced} of {ticks} ticks)"
        );
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
        // Tick past the first frame so the post-reset PPU write-lockout
        // (nesdev power-up state, #27) releases — a real game waits for two
        // VBLANKs before enabling NMI for exactly this reason. This also
        // carries the CPU past its 7-cycle reset bootstrap.
        for _ in 0..90_000 {
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
