//! The unified per-CCK Amiga driver (#34).
//!
//! The OCS / ECS / AGA machine crates each carried a byte-identical
//! per-CCK `tick()` loop and `service_cpu_bus()` body — copy-pasted, so
//! every chipset fix (the blitter #31, the copper sync #33) had to land
//! three times. `AmigaDriver` hoists that shared body here as **provided
//! default methods** so it lives once.
//!
//! ## How the three variants plug in
//!
//! The driver reaches common chip state through accessor methods that
//! return the **base OCS chip types** (`Agnus`, `Denise`,
//! `Paula8364`, …). The ECS and AGA wrappers (`AgnusEcs`,
//! `AgnusAga`, the `Cpu68020`) `Deref` to those bases.
//!
//! Three kinds of method make that work:
//!
//! * **Accessors** (`agnus`, `agnus_mut`, …) — borrow one chip at a
//!   time. The default body uses them for the many sequential
//!   single-chip operations.
//! * **Targeted multi-borrow methods** (`copper_tick_cck`,
//!   `denise_tick`, …) — each encapsulates one place where the body
//!   needs *several* disjoint chip fields borrowed at once (e.g. Denise
//!   reads chip RAM while taking `&mut Agnus`). The split borrow happens
//!   inside the per-impl method, where the concrete struct's fields are
//!   visible; the shared body just calls it.
//! * **Variant helpers** (`advance_agnus_cck`,
//!   `dispatch_custom_write`, `dispatch_bus`, …) — behavior that can
//!   be overridden by a chip wrapper must resolve on the concrete
//!   variant before any base-type coercion. This includes programmable
//!   beam advancement, the custom-register write path, the A1200's
//!   extra Gayle arm, and each variant's own CPU `tick()`.
//!
//! `service_cpu_bus` and the local/motherboard response helpers are likewise
//! default methods; only `dispatch_bus` (the chip-select `or_else` chain)
//! stays per-variant. The CPU's shared bus-protocol fields (`state`,
//! `bus_status`, `ipl`) are read through `cpu_base` / `cpu_base_mut`; concrete
//! processor selection and exact clock conversion remain explicit driver
//! inputs.

use crate::board::{
    BusResponse, BusTransaction, CIA_E_CLOCK_DIVISOR, MotherboardBridgeAction, NTSC_SYSTEM_TICK_HZ,
    PAL_SYSTEM_TICK_HZ, SizedBusResponse, SizedBusTransaction, SynchronizedMotherboardBridge,
    TICKS_PER_CCK,
};
use crate::cia::Cia;
use crate::clock::{CpuClock, CpuDomainPhase};
use crate::copper::Copper;
use crate::memory::Memory;
use crate::rtc::Msm6242Rtc;
use commodore_agnus_ocs::{Agnus, AgnusRegion, BlitterCckOutcome, CckBusPlan, SlotOwner};
use commodore_paula_8364::{IntSource, Paula8364};
use motorola_68000::Cpu68000;
use motorola_68000::bus::{BusStatus, FunctionCode, interrupt_acknowledge_level};
use motorola_68000::cpu::State;
use peripheral_commodore_amiga_floppy::AmigaFloppyDrive;
use peripheral_commodore_amiga_keyboard::AmigaKeyboard;

/// One entry in the copper-MOVE debug log: `(cck, vpos, hpos, reg, val)`.
pub type CopperMoveLogEntry = (u64, u16, u16, u16, u16);

/// Maximum number of undrained CPU instruction boundaries retained by one
/// Amiga machine.
pub const AMIGA_CPU_BOUNDARY_QUEUE_CAPACITY: usize = 4096;

/// One instruction boundary crossed during a machine tick.
///
/// Faster processors can cross more than one boundary between two runtime
/// observations. Machines retain these records until the runtime drains them,
/// so tracing does not collapse a 14 MHz or 40 MHz processor to one entry per
/// 7 MHz Amiga system tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuBoundary {
    /// Amiga system tick on which the boundary was crossed.
    pub system_tick: u64,
    /// Address of the instruction that has just started.
    pub instr_start_pc: u32,
    /// Status register at the boundary.
    pub sr: u16,
    /// Instruction word observed at `instr_start_pc`.
    pub opcode: u16,
}

/// Side-effect-free view of one machine's board scheduler and CPU-domain
/// progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmigaSchedulerDiagnosticSnapshot {
    /// Completed Amiga system ticks.
    pub tick_count: u64,
    /// Completed colour clocks.
    pub cck_count: u64,
    /// Current half-CCK phase.
    pub cck_phase: u8,
    /// Current CIA E-clock divider phase.
    pub e_clock_phase: u64,
    /// Previously sampled vertical-blank request level.
    pub prev_vertb_level: bool,
    /// Previously sampled CIA-A interrupt level.
    pub prev_cia_a_irq: bool,
    /// Previously sampled CIA-B interrupt level.
    pub prev_cia_b_irq: bool,
    /// Previously sampled CIA-A serial-port direction.
    pub prev_cia_a_spmode: bool,
    /// Active processor clock numerator per Amiga system tick.
    pub cpu_clock_numerator: u64,
    /// Active processor clock denominator per Amiga system tick.
    pub cpu_clock_denominator: u64,
    /// Retained rational clock phase.
    pub cpu_clock_phase: u64,
    /// Maximum processor edges one Amiga system tick can emit.
    pub cpu_clock_maximum_edges_per_tick: u64,
    /// Whether no partially consumed system tick remains.
    pub cpu_domain_idle: bool,
    /// Processor edges still due in the current system tick.
    pub cpu_domain_edges_remaining: u64,
    /// Whether the current tick's motherboard admission slot remains.
    pub cpu_domain_motherboard_slot_pending: bool,
    /// Whether the clock and partially consumed CPU domain agree.
    pub cpu_domain_coherent: bool,
    /// Undrained instruction boundaries, oldest first.
    pub pending_cpu_boundaries: Vec<CpuBoundary>,
    /// Maximum number of undrained boundaries retained.
    pub pending_cpu_boundary_capacity: usize,
}

/// Side-effect-free view of the board-level encoded-track stream that feeds
/// Paula's disk controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmigaTrackStreamDiagnosticSnapshot {
    /// Whether an encoded-track cache is installed.
    pub cache_present: bool,
    /// Cylinder represented by the cache.
    pub cache_cylinder: Option<u32>,
    /// Head side represented by the cache.
    pub cache_head: Option<u32>,
    /// Encoded bytes held by the cache.
    pub cache_bytes: usize,
    /// Encoded words held by the cache.
    pub word_count: usize,
    /// Index of the next word delivered to Paula.
    pub word_cursor: usize,
    /// CCKs accumulated toward the next delivery.
    pub pacer_ccks: u16,
    /// Required CCK interval between delivered words.
    pub word_interval_ccks: u16,
}

/// Side-effect-free view of the board-level controller-port input state.
///
/// This records both the raw counter bytes presented through JOY0DAT /
/// JOY1DAT and the host-control latches that feed those counters. Paula-owned
/// proportional-input and auxiliary-button state remains in Paula's own
/// diagnostic snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmigaInputDiagnosticSnapshot {
    /// Port 0 horizontal counter byte.
    pub joy0_x: u8,
    /// Port 0 vertical counter byte.
    pub joy0_y: u8,
    /// Complete JOY0DAT readback.
    pub joy0dat: u16,
    /// Port 1 horizontal counter byte.
    pub joy1_x: u8,
    /// Port 1 vertical counter byte.
    pub joy1_y: u8,
    /// Complete JOY1DAT readback.
    pub joy1dat: u16,
    /// Whether port 0's active-low primary fire / left mouse button is held.
    pub port0_primary_button_pressed: bool,
    /// Whether port 1's active-low primary fire button is held.
    pub port1_primary_button_pressed: bool,
    /// Host-level port 1 joystick up latch.
    pub joystick1_up: bool,
    /// Host-level port 1 joystick down latch.
    pub joystick1_down: bool,
    /// Host-level port 1 joystick left latch.
    pub joystick1_left: bool,
    /// Host-level port 1 joystick right latch.
    pub joystick1_right: bool,
    /// Host-level port 1 primary fire latch.
    pub joystick1_fire: bool,
    /// Host-level port 1 second-button latch.
    pub joystick1_button2: bool,
    /// Host-level port 1 third-button latch.
    pub joystick1_button3: bool,
}

/// The shared per-CCK driver for every Amiga chipset variant.
///
/// Implemented by each machine struct **in its own crate** (so the
/// accessor / helper bodies reach private fields directly), then the
/// `tick` / `service_cpu_bus` defaults drive all three. See the module
/// docs for how the three method kinds compose.
pub trait AmigaDriver {
    // ---------- single-chip accessors (base OCS types via Deref) ----------

    fn agnus(&self) -> &Agnus;
    fn agnus_mut(&mut self) -> &mut Agnus;
    fn copper(&self) -> &Copper;
    fn copper_mut(&mut self) -> &mut Copper;
    fn paula(&self) -> &Paula8364;
    fn paula_mut(&mut self) -> &mut Paula8364;
    fn cia_a(&self) -> &Cia;
    fn cia_a_mut(&mut self) -> &mut Cia;
    fn cia_b(&self) -> &Cia;
    fn cia_b_mut(&mut self) -> &mut Cia;
    fn drive(&self) -> &AmigaFloppyDrive;
    fn drive_mut(&mut self) -> &mut AmigaFloppyDrive;
    fn keyboard_mut(&mut self) -> &mut AmigaKeyboard;
    fn memory(&self) -> &Memory;
    fn memory_mut(&mut self) -> &mut Memory;
    fn rtc_mut(&mut self) -> &mut Msm6242Rtc;
    /// CPU bus-protocol view — the Deref base shared by every 680x0.
    /// Used only for `state` / `bus_status` / `ipl`; the variant's own
    /// `tick()` runs through [`AmigaDriver::tick_cpu_with_ipl`].
    fn cpu_base(&self) -> &Cpu68000;
    fn cpu_base_mut(&mut self) -> &mut Cpu68000;
    /// Exact conversion from one Amiga system tick to active-CPU edges.
    fn cpu_clock_mut(&mut self) -> &mut CpuClock;
    /// Unconsumed processor edges when instruction stepping stops part-way
    /// through one Amiga system tick.
    fn cpu_domain_phase(&self) -> &CpuDomainPhase;
    fn cpu_domain_phase_mut(&mut self) -> &mut CpuDomainPhase;

    // ---------- targeted multi-borrow operations ----------

    /// Run the copper for one CCK, feeding it chip RAM. Returns the
    /// `(reg, val)` of any MOVE it produced this cycle. Encapsulates the
    /// `&mut copper` + `&memory` split borrow. `copper_slot_granted` is
    /// Agnus's per-CCK grant (`current_slot` == Copper).
    fn copper_tick_cck(
        &mut self,
        vpos: u16,
        hpos: u16,
        copper_slot_granted: bool,
        blitter_busy: bool,
    ) -> Option<(u16, u16)>;

    /// Advance the blitter completion pipeline by one CCK.
    ///
    /// Startup, channel operations and final D consume
    /// `progress_granted`; the already-admitted internal result stage does
    /// not. Encapsulates the `&mut agnus` + `&mut memory` (`ChipRamBus`)
    /// split borrow.
    fn blitter_dma_step(&mut self, progress_granted: bool) -> BlitterCckOutcome;

    /// Step Paula's audio engine for one CCK, reading sample data from
    /// chip RAM. Encapsulates the `&mut paula` + `&memory` split borrow.
    fn audio_tick_cck(&mut self, dmacon: u16, slot: Option<u8>);

    /// Service one Agnus-granted disk-DMA cell.
    ///
    /// Read DMA dequeues one word from Paula's rotational FIFO into chip RAM.
    /// Write DMA moves one word from chip RAM into that FIFO when it has room.
    /// The independent track pacer fills or drains the FIFO; it never performs
    /// the memory transfer itself.
    fn service_disk_dma_slot(&mut self) {
        if self.paula().disk_write_dma_slot_requested() {
            let addr = self.agnus().dsk_pt & 0x001F_FFFE;
            let word = self.memory().read_word(addr);
            if self.paula_mut().accept_disk_write_dma_slot(word) {
                let next = self.agnus().dsk_pt.wrapping_add(2);
                self.agnus_mut().dsk_pt = next;
            }
            return;
        }

        if let Some(word) = self.paula_mut().service_disk_read_dma_slot() {
            let addr = self.agnus().dsk_pt & 0x001F_FFFE;
            self.memory_mut().write_word(addr, word);
            let next = self.agnus().dsk_pt.wrapping_add(2);
            self.agnus_mut().dsk_pt = next;
        }
    }

    /// Service one sprite-DMA slot for `channel`: fetch the control/data
    /// word from chip RAM at the sprite pointer and route it into Denise
    /// (SPRxPOS/CTL via the register dispatch, SPRxDATA/DATB into the
    /// serial shifter). Encapsulates the `&mut agnus` + `&memory` fetch
    /// split borrow *and* the `&mut denise` write, so the variant-typed
    /// `Denise<C>` never appears in the shared trait surface.
    fn service_sprite_dma(&mut self, channel: u8, second_word: bool);

    /// Tick Denise for this sub-CCK phase. Encapsulates the simultaneous
    /// `&mut denise`, `&mut agnus`, and `&memory` split borrow. The
    /// bitplane grant comes from the post-Copper concrete Agnus plan so
    /// Denise cannot silently recompute it through the OCS base view.
    fn denise_tick(&mut self, phase: u8, bitplane_dma_fetch_plane: Option<u8>);

    // ---------- per-tick scalar bookkeeping ----------

    fn cck_phase(&self) -> u8;
    fn set_cck_phase(&mut self, value: u8);
    fn prev_vertb_level(&self) -> bool;
    fn set_prev_vertb_level(&mut self, value: bool);
    fn prev_cia_a_spmode(&self) -> bool;
    fn set_prev_cia_a_spmode(&mut self, value: bool);
    fn prev_cia_a_irq(&self) -> bool;
    fn set_prev_cia_a_irq(&mut self, value: bool);
    fn prev_cia_b_irq(&self) -> bool;
    fn set_prev_cia_b_irq(&mut self, value: bool);
    fn e_clock_phase(&self) -> u64;
    fn set_e_clock_phase(&mut self, value: u64);
    fn track_pacer(&self) -> u16;
    fn set_track_pacer(&mut self, value: u16);
    fn tick_count(&self) -> u64;
    fn set_tick_count(&mut self, value: u64);
    fn push_copper_move_log(&mut self, entry: CopperMoveLogEntry);
    /// Retain the instruction boundary now visible on the active CPU.
    fn record_cpu_boundary(&mut self);

    // ---------- per-variant helpers ----------

    /// Advance the concrete Agnus/Alice variant by one CCK. This must
    /// resolve before coercion to the OCS base so ECS programmable beam
    /// timing remains active.
    fn advance_agnus_cck(&mut self);
    /// Compute the concrete Agnus/Alice bus plan for the current CCK.
    /// ECS vertical display-window gating and future variant arbitration
    /// must resolve before coercion to the OCS base.
    fn agnus_bus_plan(&self) -> CckBusPlan;
    /// Route a custom-register write (`$DFFxxx`) to the owning chip. The
    /// ECS / AGA variants extend this with their extra registers, so it
    /// is per-variant.
    fn dispatch_custom_write(&mut self, offset: u16, val: u16);
    fn feed_next_write_word(&mut self);
    fn feed_next_mfm_word(&mut self);
    /// CCKs between rotational MFM words at the selected region and ADKCON
    /// rate.
    ///
    /// A 12,668-byte DD track at 300 RPM advances one byte about every 56 PAL
    /// CCKs, hence 112 CCKs per word. NTSC's faster colour clock makes that
    /// 113 CCKs. This is ADKCON.FAST's normal 2 µs MFM rate. Clearing FAST
    /// selects the 4 µs GCR-compatible clock and doubles the interval.
    fn disk_word_cck_interval(&self) -> u16 {
        const ADKCON_FAST: u16 = 0x0100;
        let mfm_fast = match self.agnus().region {
            AgnusRegion::Pal => 112,
            AgnusRegion::Ntsc => 113,
        };
        if self.paula().adkcon() & ADKCON_FAST != 0 {
            mfm_fast
        } else {
            mfm_fast * 2
        }
    }
    fn refresh_cia_a_external_inputs(&mut self);
    /// Tick the variant's *own* CPU after loading IPL — `Cpu68000` for
    /// OCS / ECS, `Cpu68020` for AGA. Kept per-variant so it is not
    /// routed through the Deref base (which would silently impose 68000
    /// timing on the 68020).
    fn tick_cpu_with_ipl(&mut self);
    /// The CPU bus-cycle chip-select chain. Per-variant: the A1200 adds a
    /// Gayle arm. Returns the response the addressed chip drove.
    fn dispatch_bus(&mut self, tx: &BusTransaction) -> BusResponse;
    /// Optional MC68020/MC68030 dynamic-sized responder.
    ///
    /// Returning `None` retains the legacy byte/word compatibility dispatch
    /// for this phase. Variants opt in only where both port width and lane
    /// behaviour are evidence-backed.
    fn dispatch_sized_bus(&mut self, tx: &SizedBusTransaction) -> Option<SizedBusResponse> {
        let _ = tx;
        None
    }
    /// Optional accelerator-local dynamic-sized responder.
    ///
    /// This runs before motherboard arbitration and before 32-bit CPU
    /// addresses are projected onto the Amiga's 24-bit bus.
    fn dispatch_local_sized_bus(&mut self, tx: &SizedBusTransaction) -> Option<SizedBusResponse> {
        let _ = tx;
        None
    }
    /// Optional accelerator-local compatibility responder.
    ///
    /// Instruction fetches currently use the shared word-shaped bus surface
    /// even on MC68020/MC68030 processors.
    fn dispatch_local_bus(&mut self, tx: &BusTransaction) -> Option<BusResponse> {
        let _ = tx;
        None
    }
    /// Optional timing state for non-local cycles crossing an asynchronous
    /// accelerator's synchronized motherboard bridge.
    fn motherboard_bridge_mut(&mut self) -> Option<&mut SynchronizedMotherboardBridge> {
        None
    }
    /// Apply the external-device reset asserted by the CPU's RESET
    /// instruction.
    ///
    /// The shared driver consumes the processor output after every active-CPU
    /// edge. Variants reset evidence-backed board and peripheral state here;
    /// processor and RAM state remain intact.
    fn reset_external_devices_from_cpu(&mut self) {}
    /// Project a processor address onto the motherboard bus.
    ///
    /// Stock MC68000 machines expose 24 address bits and therefore mask the
    /// shared core's `u32` address. Full-address accelerators override this to
    /// reject addresses that their bridge does not forward.
    fn motherboard_address(&self, address: u32) -> Option<u32> {
        Some(address & 0x00FF_FFFF)
    }

    // ---------- the shared body (provided) ----------

    /// One machine tick (master / 4 = half-CCK). The unified per-CCK
    /// loop: beam events → copper → blitter / audio / sprite DMA → disk
    /// → Denise pixel → CIA E-clock → CIA /IRQ inputs → CPU bus + tick.
    fn tick(&mut self) {
        let crossed_boundary = self.advance_cpu_domain(false);
        debug_assert!(!crossed_boundary);
    }

    /// Advance until one instruction boundary is crossed or the current
    /// system tick completes.
    ///
    /// Returning at a boundary may leave processor edges pending in the
    /// current system tick. A subsequent call to this method or [`Self::tick`]
    /// consumes those edges before advancing the chipset again.
    #[must_use]
    fn advance_to_cpu_boundary(&mut self) -> bool {
        self.advance_cpu_domain(true)
    }

    /// Shared implementation behind normal machine ticks and exact
    /// instruction stepping.
    fn advance_cpu_domain(&mut self, stop_at_cpu_boundary: bool) -> bool {
        if self.cpu_domain_phase().is_idle() {
            let phase = self.cck_phase();
            let mut bitplane_dma_fetch_plane = None;

            // ── CCK-granular events (phase 0 only) ───────────────────
            if phase == 0 {
                // Per-CCK bus-use observations remain valid across both
                // master/4 phases. Clear them only as a new CCK begins.
                self.agnus_mut().reset_sprite_bus_usage();
                self.agnus_mut().reset_blitter_cck_bus_state();

                // Advance the beam.
                self.advance_agnus_cck();

                // CIA-B TICK is wired to /HSYNC. Agnus exposes the current
                // fixed-sync, counter-visible approximation after the raw
                // sync edge and CIA input delay.
                if self.agnus().fixed_sync_cia_b_tod_event() {
                    self.cia_b_mut().tod_pulse();
                }

                // CIA-A TICK is wired to /VSYNC rather than VERTB. Until
                // the sync waveform and 8520 input delay are modelled
                // independently, Agnus exposes the fixed-sync,
                // counter-visible event.
                if self.agnus().fixed_sync_cia_a_tod_event() {
                    self.cia_a_mut().tod_pulse();
                }

                // Raster-line-zero VERTB event. The request is generated
                // once when the beam enters the vertical-blank interval;
                // the interval itself is not a level-sensitive interrupt
                // input.
                let vertb_level = self.agnus().vertb_level();
                let rising_edge = vertb_level && !self.prev_vertb_level();
                if rising_edge {
                    self.paula_mut().raise(IntSource::Vertb);
                }
                self.set_prev_vertb_level(vertb_level);

                // The automatic COP1LC strobe is a separate Agnus event,
                // even though fixed-sync Amiga hardware places it at the
                // same line-zero boundary as VERTB. Keeping the predicates
                // separate prevents later VSYNC/TOD timing work from
                // moving the Copper restart with the CIA event.
                if self.agnus().fixed_sync_copper_restart_event() {
                    self.copper_mut().jump1();
                }

                // Copper runs when DMACON.COPEN (bit 7) AND DMAEN (bit 9)
                // are both set. Agnus arbitrates the chip bus; pass the
                // current CCK's copper grant (`current_slot` == Copper,
                // the even free cells) so the copper only fetches on the
                // cells Agnus allocates to it (#30). Computed before the
                // copper's own MOVE dispatch so a same-CCK DMACON write
                // can't retroactively change this cell's grant.
                //
                // Reset the copper's per-CCK bus-usage flag every CCK
                // (whether or not the copper runs): the copper sets it only
                // when it actually fetches, and the CPU arbitration below
                // reads it so a parked/throttled copper yields its granted
                // cell to the CPU.
                self.copper_mut().bus_used_this_cck = false;
                let copper_slot_granted = self.agnus_bus_plan().copper_dma_slot_granted;
                if self.agnus().dmacon & 0x0280 == 0x0280 {
                    // Route copper MOVEs through the same custom-register
                    // dispatch the CPU uses. The copper can legitimately
                    // write any register (bitplane pointers, DMACON,
                    // INTENA, sprite pointers, DDF/DIW, modulos, etc.);
                    // routing only through Denise would silently drop
                    // the non-Denise ones.
                    let vpos = self.agnus().vpos;
                    let hpos = self.agnus().hpos;
                    // Copper WAIT/SKIP BFD=0 sees its own blitter-finished
                    // observation. It shares the A1000 startup exception with
                    // DMACONR, but retains busy one CCK longer after main finish
                    // while the final-D pipeline advances.
                    let blitter_busy = self.agnus().blitter_busy_copper();
                    if let Some((reg, val)) =
                        self.copper_tick_cck(vpos, hpos, copper_slot_granted, blitter_busy)
                    {
                        let cck = self.tick_count() / TICKS_PER_CCK;
                        self.push_copper_move_log((cck, vpos, hpos, reg, val));
                        self.dispatch_custom_write(reg, val);
                    }
                }

                // ── Paula audio engine — one step per CCK ────────────────
                // Audio DMA slot arbitration is Agnus's job now; we pull
                // the plan for this CCK and extract the audio grant. Paula
                // also needs the raw DMACON value for its master+channel
                // enable gates.
                let bus_plan = self.agnus_bus_plan();
                bitplane_dma_fetch_plane = bus_plan.bitplane_dma_fetch_plane;

                // ── Blitter DMA and completion pipeline ───────────────
                // The entry point runs every CCK. Startup/channel/final-D work
                // remains grant-gated; the internal final-result stage advances
                // after the last admitted main cycle without another bus grant.
                // Pre-AGA normal D blits emit INT_BLIT before final D, while
                // Alice delays that source to final D.
                let blitter_nasty_owned = bus_plan.blitter_chip_bus_granted;
                let blitter_outcome = self.blitter_dma_step(bus_plan.blitter_dma_progress_granted);
                self.agnus_mut()
                    .record_blitter_cck_bus_state(blitter_nasty_owned, blitter_outcome.bus_used);
                if blitter_outcome.interrupt {
                    self.paula_mut().raise(IntSource::Blit);
                }

                let slot = bus_plan.audio_dma_service_channel;
                let dmacon = self.agnus().dmacon;
                self.audio_tick_cck(dmacon, slot);

                // ── Sprite DMA — fetch the control/data words from chip RAM
                // at the sprite pointers and deliver them to Denise. Agnus
                // owns the per-sprite control/data state machine; the machine
                // reads chip RAM and routes the word to the matching SPRxPOS/
                // CTL/DATA/DATB register (the same path a CPU/copper write
                // takes). gap #162.
                if let Some(channel) = bus_plan.sprite_dma_service_channel {
                    // Sprites occupy odd cells 0x15..0x33, two per channel:
                    // word = ((hpos - 0x15) / 2) & 1 (#30). Word 0 is the
                    // control pair (SPRxPOS/CTL), word 1 the data pair.
                    let second_word = ((self.agnus().hpos.wrapping_sub(0x15)) / 2) & 1 == 1;
                    self.service_sprite_dma(channel, second_word);
                }

                // Disk memory traffic consumes only the fixed cells Agnus
                // granted in this already-sampled plan. Rotational stream
                // arrival remains independent below, through Paula's bounded
                // FIFO, so DSKBYTR and disk rotation continue even when DSKEN
                // is clear or the FIFO cannot use this cell.
                if bus_plan.disk_dma_slot_granted {
                    self.service_disk_dma_slot();
                }

                // ── Paula disk engine — DSKBYTR byte-latch + WORDEQUAL
                // delay. Ticked once per CCK. Paula owns the DMA arm
                // flip-flop, FIFO, word countdown, WORDSYNC gate and DSKBLK
                // interrupt; the machine layer supplies memory and media
                // movement at the two independently clocked boundaries.
                self.paula_mut().tick_disk_cck();

                // ── Rotational stream ↔ Paula FIFO ──────────────────
                // The encoded track moves independently of disk DMA cells.
                // In read mode, a paced word enters Paula's three-word FIFO.
                // In write mode, a paced word leaves it for the drive. Agnus
                // alone moves words between that FIFO and chip RAM above.
                let write_stream_active = self.paula().disk_write_stream_active();
                if write_stream_active || self.drive().read_data_available() {
                    if self.track_pacer() <= 1 {
                        if write_stream_active {
                            self.feed_next_write_word();
                        } else {
                            self.feed_next_mfm_word();
                        }
                        self.set_track_pacer(self.disk_word_cck_interval());
                    } else {
                        self.set_track_pacer(self.track_pacer() - 1);
                    }
                } else {
                    self.set_track_pacer(0);
                }
            }

            // ── Per-tick: Denise pixel + fetch/reload at phase 0 ────
            self.denise_tick(phase, bitplane_dma_fetch_plane);

            // ── CIA E-clock: every 10 master/4 ticks = master/40 ────
            self.set_e_clock_phase(self.e_clock_phase() + 1);
            if self.e_clock_phase() >= CIA_E_CLOCK_DIVISOR {
                self.set_e_clock_phase(0);
                self.cia_a_mut().phi2_pulse();
                self.cia_b_mut().phi2_pulse();

                // Floppy drive runs at E-clock rate (same rate as CIA
                // internal ticks). CIA-B PRB updates the control pins on
                // writes; the E-clock phase advances the mechanical drive
                // state and feeds status back onto CIA-A PRA.
                // CIA-B FLAG pin is wired to the floppy /INDEX pulse on the
                // Amiga; the drive emits one index pulse per revolution.
                if self.drive_mut().tick() {
                    self.cia_b_mut().flag_falling_edge();
                }
                self.refresh_cia_a_external_inputs();

                // Keyboard controller — detect CIA-A CRA bit 6 (SPMODE)
                // rising edge as the host handshake, then tick the state
                // machine and inject the next serial byte (if any).
                const CRA_SPMODE: u8 = 0x40;
                let spmode = self.cia_a().cra() & CRA_SPMODE != 0;
                if spmode && !self.prev_cia_a_spmode() {
                    self.keyboard_mut().handshake();
                }
                self.set_prev_cia_a_spmode(spmode);
                if let Some(byte) = self.keyboard_mut().tick() {
                    self.cia_a_mut().receive_serial_byte(byte);
                }
            }

            // ── Paula's level-sensitive CIA interrupt inputs ─────────
            // The CIA and expansion interrupt outputs share the active-
            // low INT2* and INT6* inputs. While either CIA holds its line
            // active, Paula must keep or make the corresponding request
            // visible. Clearing INTREQ alone therefore only has a bounded
            // effect; reading the CIA ICR releases the CIA contribution.
            // Exact Paula sampling phase is not yet modelled.
            let cia_a_irq = self.cia_a().irq_active();
            if cia_a_irq {
                self.paula_mut().raise(IntSource::Ports);
            }
            self.set_prev_cia_a_irq(cia_a_irq);

            let cia_b_irq = self.cia_b().irq_active();
            if cia_b_irq {
                self.paula_mut().raise(IntSource::Exter);
            }
            self.set_prev_cia_b_irq(cia_b_irq);

            // ── CPU clock domain ─────────────────────────────────────
            // The chipset remains on the master/4 system clock. Stock 68000
            // machines emit one edge here, an A1200 emits two, and an
            // asynchronous accelerator emits the exact rational number
            // retained by CpuClock.
            let cpu_edges = self.cpu_clock_mut().edges_for_tick();
            self.cpu_domain_phase_mut().begin_tick(cpu_edges);

            // A stopped or sub-system-clock processor can emit no edge in a
            // particular tick. There is no partial CPU phase to retain.
            if cpu_edges == 0 {
                self.finish_system_tick();
                return false;
            }
        }

        // Bus inputs are serviced before every CPU edge. The retained domain
        // phase supplies a motherboard admission slot only to the first edge,
        // including when execution resumes after an instruction-step stop.
        while let Some(motherboard_slot) = self.cpu_domain_phase_mut().take_edge() {
            let instruction_starts = self.cpu_base().instruction_starts;
            self.service_cpu_bus_with_motherboard_slot(motherboard_slot);
            self.tick_cpu_with_ipl();
            if self.cpu_base().reset_out {
                self.cpu_base_mut().reset_out = false;
                self.reset_external_devices_from_cpu();
            }
            if self.cpu_base().instruction_starts != instruction_starts {
                self.record_cpu_boundary();
                if stop_at_cpu_boundary {
                    if self.cpu_domain_phase().is_idle() {
                        self.finish_system_tick();
                    }
                    return true;
                }
            }
        }

        self.finish_system_tick();
        false
    }

    /// Complete the chipset bookkeeping for a system tick whose CPU edges
    /// have all been consumed.
    fn finish_system_tick(&mut self) {
        debug_assert!(self.cpu_domain_phase().is_idle());
        let ticks_per_second = match self.agnus().region {
            AgnusRegion::Pal => PAL_SYSTEM_TICK_HZ,
            AgnusRegion::Ntsc => NTSC_SYSTEM_TICK_HZ,
        };
        self.rtc_mut().advance_system_ticks(1, ticks_per_second);
        self.set_tick_count(self.tick_count() + 1);
        self.set_cck_phase(self.cck_phase() ^ 1);
    }

    /// Complete one CPU bus cycle: DTACK timing, chip-RAM arbitration,
    /// autovector, then the per-variant chip-select dispatch. Shared
    /// across variants — the only per-variant part is [`dispatch_bus`].
    ///
    /// [`dispatch_bus`]: AmigaDriver::dispatch_bus
    fn service_cpu_bus(&mut self) {
        self.service_cpu_bus_with_motherboard_slot(true);
    }

    /// Service one active-CPU edge.
    ///
    /// `motherboard_slot` is true once per Amiga system tick. Stock machines
    /// ignore it; asynchronous accelerator bridges use it to admit at most one
    /// non-local completion on the slower motherboard clock.
    fn service_cpu_bus_with_motherboard_slot(&mut self, motherboard_slot: bool) {
        // Snapshot the bus-cycle parameters out of the CPU state so we
        // can mutate self.memory and other chips without borrowing the
        // CPU mutably across helper boundaries.
        let bus_info = {
            let cpu = self.cpu_base();
            match &cpu.state {
                State::BusCycle {
                    addr,
                    fc,
                    is_read,
                    is_word,
                    data,
                    cycle_count,
                    ..
                } => Some((
                    *addr,
                    *fc,
                    *is_read,
                    *is_word,
                    *data,
                    *cycle_count,
                    cpu.active_bus_transfer
                        .map(|_| (cpu.bus_transfer_size, cpu.bus_data_out)),
                )),
                _ => None,
            }
        };
        let Some((addr, fc, is_read, is_word, data, cycle_count, sized_phase)) = bus_info else {
            return;
        };

        // The shared compatibility cycle reaches its response point at
        // cycle 2. Complete on the first eligible poll at or after that
        // point and hold the result steady.
        if cycle_count < 2 {
            self.cpu_base_mut().bus_status = BusStatus::Wait;
            return;
        }
        if matches!(
            self.cpu_base().bus_status,
            BusStatus::Ready(_) | BusStatus::ReadySized { .. } | BusStatus::Error
        ) {
            return;
        }

        // Accelerator-local responders see the full processor address and
        // complete before motherboard synchronization or chip-bus
        // arbitration. CPU-space cycles such as interrupt acknowledge never
        // reach ordinary memory decoders. MC68020/MC68030 data operands
        // retain their SIZ value; instruction fetches use the compatibility
        // hook below.
        if fc != FunctionCode::InterruptAck {
            if let Some((remaining, sized_data)) = sized_phase {
                let tx = SizedBusTransaction {
                    addr,
                    is_read,
                    remaining,
                    data: sized_data,
                };
                if let Some(response) = self.dispatch_local_sized_bus(&tx) {
                    self.cpu_base_mut().bus_status = BusStatus::ReadySized {
                        data: response.data,
                        port: response.port,
                    };
                    return;
                }
            }

            let local_tx = BusTransaction {
                addr,
                is_read,
                is_word,
                data: data.unwrap_or(0),
            };
            if let Some(response) = self.dispatch_local_bus(&local_tx) {
                self.apply_local_bus_response(&local_tx, response);
                return;
            }
        }

        if let Some(bridge) = self.motherboard_bridge_mut() {
            match bridge.poll(motherboard_slot) {
                MotherboardBridgeAction::Wait => {
                    self.cpu_base_mut().bus_status = BusStatus::Wait;
                    return;
                }
                MotherboardBridgeAction::Complete(status) => {
                    self.cpu_base_mut().bus_status = status;
                    return;
                }
                MotherboardBridgeAction::Access => {}
            }
        }

        // Autovectored interrupts: the current shared compatibility path
        // collapses the board's VPA/AVEC response and the CPU's internal
        // vector generation into Ready(24 + level). Ready does not describe
        // literal DTACK and bus data for this cycle. The MC68000-shaped
        // address carries the accepted level on A3-A1; live IPL may have
        // changed by the time this bus cycle is serviced.
        if fc == FunctionCode::InterruptAck {
            let acknowledged_level = interrupt_acknowledge_level(addr);
            assert_ne!(
                acknowledged_level, 0,
                "interrupt acknowledge cannot encode request level 0"
            );
            self.complete_motherboard_response(BusStatus::Ready(
                24 + u16::from(acknowledged_level),
            ));
            return;
        }

        // Chip-RAM bus arbitration: Agnus owns the chip-RAM bus during
        // DMA slots; CPU chip-RAM accesses stall to the next cell the
        // concrete chipset plan leaves to the CPU. The plan's explicit
        // CPU grant includes blitter-nasty ownership, which raw
        // `slot_owner == Cpu` does not. The copper is special: Agnus
        // grants it every even free cell, but a parked (WAIT) or
        // throttled copper does not actually drive the bus, so those
        // cells fall through to the CPU (matching real Agnus /
        // vAmiga's busOwner). We therefore stall on a copper-granted
        // cell only when the copper truly fetched it this CCK
        // (`bus_used_this_cck`).
        // Reads with OVL on land in ROM (not contended); writes always
        // hit chip RAM (OVL only gates reads); non-chip-RAM accesses
        // (CIA / custom / slow / ROM / unmapped) bypass arbitration.
        let Some(addr24) = self.motherboard_address(addr) else {
            self.complete_motherboard_response(BusStatus::Error);
            return;
        };
        let is_chip_ram_access = addr24 < 0x20_0000 && (!is_read || !self.memory().overlay());
        let bus_plan = self.agnus_bus_plan();
        // A sprite control fetch can latch a new VSTOP and make a fresh
        // plan no longer show the request that consumed this CCK. Keep
        // actual sprite use authoritative until the next CCK rather than
        // retroactively granting the same bus cell to the CPU.
        let blitter_holds_bus = if self.agnus().blitter_cck_bus_state_recorded() {
            self.agnus().blitter_bus_used_this_cck() || self.agnus().blitter_nasty_owned_this_cck()
        } else {
            // Test/backdoor and other direct service callers may not have
            // run the phase-0 DMA body. Preserve the general live-plan
            // invariant in that case.
            matches!(bus_plan.slot_owner, SlotOwner::Cpu) && !bus_plan.cpu_chip_bus_granted
        };
        let dma_holds_bus = self.agnus().sprite_bus_used_this_cck()
            || blitter_holds_bus
            || match bus_plan.slot_owner {
                // Blitter ownership for the current CPU/free cell is
                // represented by the two pre-service/actual-use latches
                // above. Re-reading the live blitter request here could see
                // the next line operation and retroactively consume a
                // bus-free ONEDOT would-be-write cell.
                SlotOwner::Cpu => false,
                SlotOwner::Copper => self.copper().bus_used_this_cck,
                _ => true,
            };
        if is_chip_ram_access && dma_holds_bus {
            self.cpu_base_mut().bus_status = BusStatus::Wait;
            return;
        }

        if let Some((remaining, sized_data)) = sized_phase {
            let tx = SizedBusTransaction {
                addr: addr24,
                is_read,
                remaining,
                data: sized_data,
            };
            if let Some(response) = self.dispatch_sized_bus(&tx) {
                self.complete_motherboard_response(BusStatus::ReadySized {
                    data: response.data,
                    port: response.port,
                });
                return;
            }
        }

        let tx = BusTransaction {
            addr: addr24,
            is_read,
            is_word,
            data: data.unwrap_or(0),
        };

        let response = self.dispatch_bus(&tx);
        let status = self.motherboard_bus_response_status(&tx, response);
        self.complete_motherboard_response(status);
    }

    /// Apply one accelerator-local compatibility response without changing
    /// motherboard floating-bus residue.
    fn apply_local_bus_response(&mut self, tx: &BusTransaction, response: BusResponse) {
        let status = Self::bus_response_status(tx, response);
        self.cpu_base_mut().bus_status = status;
    }

    /// Convert one motherboard chip-select response into CPU input and update
    /// the motherboard's floating-bus residue.
    fn motherboard_bus_response_status(
        &mut self,
        tx: &BusTransaction,
        response: BusResponse,
    ) -> BusStatus {
        if !tx.is_read {
            self.memory_mut().set_last_bus_value(tx.data);
        } else {
            match response {
                BusResponse::Word(word) => self.memory_mut().set_last_bus_value(word),
                BusResponse::Byte(byte) => self.memory_mut().set_last_bus_value(u16::from(byte)),
                BusResponse::WriteAck => {}
            }
        }
        Self::bus_response_status(tx, response)
    }

    /// Apply the canonical compatibility byte-lane rule exactly once.
    fn bus_response_status(tx: &BusTransaction, response: BusResponse) -> BusStatus {
        if !tx.is_read {
            return BusStatus::Ready(0);
        }
        let value = match response {
            BusResponse::Byte(b) => u16::from(b),
            BusResponse::Word(w) => {
                if tx.is_word {
                    w
                } else if tx.addr & 1 == 0 {
                    (w >> 8) & 0xFF
                } else {
                    w & 0xFF
                }
            }
            BusResponse::WriteAck => 0, // unreachable on reads
        };
        BusStatus::Ready(value)
    }

    /// Deliver a motherboard response directly on stock machines or retain
    /// it in an accelerator bridge until the return synchronization slot.
    fn complete_motherboard_response(&mut self, status: BusStatus) {
        if let Some(bridge) = self.motherboard_bridge_mut() {
            bridge.latch_response(status);
            self.cpu_base_mut().bus_status = BusStatus::Wait;
        } else {
            self.cpu_base_mut().bus_status = status;
        }
    }
}
