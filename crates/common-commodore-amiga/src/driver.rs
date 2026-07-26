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
//! `service_cpu_bus` and `apply_bus_response` are likewise default
//! methods; only `dispatch_bus` (the chip-select `or_else` chain) stays
//! per-variant. The CPU's bus-protocol fields (`state`, `bus_status`,
//! `ipl`) are read through `cpu_base` / `cpu_base_mut` (the Deref base),
//! which is behaviour-preserving today; the future per-variant
//! bus-timing work (the composable-config `ActiveCpu` amendment) lands
//! at `dispatch_bus` / `tick_cpu_with_ipl`, not here.

use crate::board::{BusResponse, BusTransaction, CIA_E_CLOCK_DIVISOR, TICKS_PER_CCK};
use crate::cia::Cia;
use crate::copper::Copper;
use crate::memory::Memory;
use commodore_agnus_ocs::{Agnus, CckBusPlan, SlotOwner};
use commodore_paula_8364::{IntSource, Paula8364};
use motorola_68000::Cpu68000;
use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::State;
use peripheral_commodore_amiga_floppy::AmigaFloppyDrive;
use peripheral_commodore_amiga_keyboard::AmigaKeyboard;

/// One entry in the copper-MOVE debug log: `(cck, vpos, hpos, reg, val)`.
pub type CopperMoveLogEntry = (u64, u16, u16, u16, u16);

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
    /// CPU bus-protocol view — the Deref base shared by every 680x0.
    /// Used only for `state` / `bus_status` / `ipl`; the variant's own
    /// `tick()` runs through [`AmigaDriver::tick_cpu_with_ipl`].
    fn cpu_base(&self) -> &Cpu68000;
    fn cpu_base_mut(&mut self) -> &mut Cpu68000;

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

    /// Advance the blitter DMA by one granted CCK. Returns `true` if the
    /// blit drained this cycle (INT_BLIT should fire). Encapsulates the
    /// `&mut agnus` + `&mut memory` (`ChipRamBus`) split borrow.
    fn blitter_dma_step(&mut self) -> bool;

    /// Step Paula's audio engine for one CCK, reading sample data from
    /// chip RAM. Encapsulates the `&mut paula` + `&memory` split borrow.
    fn audio_tick_cck(&mut self, dmacon: u16, slot: Option<u8>);

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
    fn disk_word_cck_interval(&self) -> u16;
    fn refresh_cia_a_external_inputs(&mut self);
    /// Tick the variant's *own* CPU after loading IPL — `Cpu68000` for
    /// OCS / ECS, `Cpu68020` for AGA. Kept per-variant so it is not
    /// routed through the Deref base (which would silently impose 68000
    /// timing on the 68020).
    fn tick_cpu_with_ipl(&mut self);
    /// The CPU bus-cycle chip-select chain. Per-variant: the A1200 adds a
    /// Gayle arm. Returns the response the addressed chip drove.
    fn dispatch_bus(&mut self, tx: &BusTransaction) -> BusResponse;

    // ---------- the shared body (provided) ----------

    /// One machine tick (master / 4 = half-CCK). The unified per-CCK
    /// loop: beam events → copper → blitter / audio / sprite DMA → disk
    /// → Denise pixel → CIA E-clock → CIA /IRQ inputs → CPU bus + tick.
    fn tick(&mut self) {
        let phase = self.cck_phase();
        let mut bitplane_dma_fetch_plane = None;

        // ── CCK-granular events (phase 0 only) ───────────────────
        if phase == 0 {
            // Per-CCK bus-use observations remain valid across both
            // master/4 phases. Clear them only as a new CCK begins.
            self.agnus_mut().reset_sprite_bus_usage();

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
                // Copper WAIT/SKIP BFD=0 sees the externally visible
                // blitter-busy signal. A1000 Agnus delays that signal until
                // its first accepted/free progress CCK; internal arbitration
                // remains busy from BLTSIZE onward.
                let blitter_busy = self.agnus().blitter_busy_visible();
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

            // ── Blitter DMA — one startup outcome or channel op per
            // granted CCK (#31). A blit consumes real chip cycles and
            // contends for the bus rather than finishing instantly on the
            // BLTSIZE write. Later chips expose BBUSY immediately; A1000
            // does so after the first accepted startup CCK. INT_BLIT fires
            // on the CCK that drains the last operation.
            if bus_plan.blitter_dma_progress_granted && self.blitter_dma_step() {
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

            // ── Paula disk engine — DSKBYTR byte-pacing + WORDEQUAL
            // delay. Ticked once per CCK; no-op until a drive has
            // delivered a word via `tick_disk_dma_slot`. Paula owns
            // the DMA arm flip-flop, the word countdown, the WORDSYNC
            // gate, and the DSKBLK interrupt; the machine layer is
            // glue around those primitives.
            self.paula_mut().tick_disk_cck();

            // ── Floppy track-read path ──────────────────────────
            // With drive selected, motor spinning, disk present, and
            // Paula expecting data, feed MFM words word-by-word at
            // the disk byte rate.
            if self.paula().disk_dma_write_active() {
                // Disk WRITE DMA: pull words from chip RAM to the drive
                // at the disk byte rate (same pacer as the read path — a
                // transfer is either a read or a write, never both).
                if self.track_pacer() == 0 {
                    self.feed_next_write_word();
                    let interval = self.disk_word_cck_interval();
                    self.set_track_pacer(interval);
                } else {
                    self.set_track_pacer(self.track_pacer().saturating_sub(1));
                }
            } else if self.drive().read_data_available() {
                if self.track_pacer() == 0 {
                    self.feed_next_mfm_word();
                    let interval = self.disk_word_cck_interval();
                    self.set_track_pacer(interval);
                } else {
                    self.set_track_pacer(self.track_pacer().saturating_sub(1));
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

        // ── CPU: every master/4 tick = every CPU clock ──────────
        self.service_cpu_bus();
        self.tick_cpu_with_ipl();

        self.set_tick_count(self.tick_count() + 1);
        self.set_cck_phase(self.cck_phase() ^ 1);
    }

    /// Complete one CPU bus cycle: DTACK timing, chip-RAM arbitration,
    /// autovector, then the per-variant chip-select dispatch. Shared
    /// across variants — the only per-variant part is [`dispatch_bus`].
    ///
    /// [`dispatch_bus`]: AmigaDriver::dispatch_bus
    fn service_cpu_bus(&mut self) {
        // Snapshot the bus-cycle parameters out of the CPU state so we
        // can mutate self.memory and other chips without borrowing the
        // CPU mutably across helper boundaries.
        let bus_info = match &self.cpu_base().state {
            State::BusCycle {
                addr,
                fc,
                is_read,
                is_word,
                data,
                cycle_count,
                ..
            } => Some((*addr, *fc, *is_read, *is_word, *data, *cycle_count)),
            _ => None,
        };
        let Some((addr, fc, is_read, is_word, data, cycle_count)) = bus_info else {
            return;
        };

        // 68000 bus cycle is 4 CCKs (S0-S7). DTACK is sampled at S4 =
        // cycle 2. Complete the bus cycle on the first poll at or after
        // cycle 2 and hold the result steady.
        if cycle_count < 2 {
            self.cpu_base_mut().bus_status = BusStatus::Wait;
            return;
        }
        if matches!(
            self.cpu_base().bus_status,
            BusStatus::Ready(_) | BusStatus::Error
        ) {
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
        let addr24 = addr & 0xFF_FFFF;
        let is_chip_ram_access = addr24 < 0x20_0000 && (!is_read || !self.memory().overlay());
        let bus_plan = self.agnus_bus_plan();
        // A sprite control fetch can latch a new VSTOP and make a fresh
        // plan no longer show the request that consumed this CCK. Keep
        // actual sprite use authoritative until the next CCK rather than
        // retroactively granting the same bus cell to the CPU.
        let dma_holds_bus = self.agnus().sprite_bus_used_this_cck()
            || match bus_plan.slot_owner {
                SlotOwner::Cpu => !bus_plan.cpu_chip_bus_granted,
                SlotOwner::Copper => self.copper().bus_used_this_cck,
                _ => true,
            };
        if is_chip_ram_access && dma_holds_bus {
            self.cpu_base_mut().bus_status = BusStatus::Wait;
            return;
        }

        // Autovectored interrupts: the chipset drives /VPA during
        // InterruptAck and the CPU computes vector = 24 + IPL.
        if fc == FunctionCode::InterruptAck {
            let ipl = self.cpu_base().ipl & 0x07;
            self.cpu_base_mut().bus_status = BusStatus::Ready(24 + u16::from(ipl));
            return;
        }

        let tx = BusTransaction {
            addr: addr24,
            is_read,
            is_word,
            data: data.unwrap_or(0),
        };

        let response = self.dispatch_bus(&tx);
        self.apply_bus_response(&tx, response);
    }

    /// Apply one chip-select response to the CPU bus, applying the
    /// canonical lane-extraction rule once. `Byte` always lands in the
    /// low 8 bits; `Word` is byte-extracted by address parity for byte
    /// reads. Latches floating-bus state on every cycle.
    fn apply_bus_response(&mut self, tx: &BusTransaction, response: BusResponse) {
        if !tx.is_read {
            // Writes ack with Ready(0); the chip-arm handlers updated
            // chip state and the floating-bus latch already.
            self.memory_mut().set_last_bus_value(tx.data);
            self.cpu_base_mut().bus_status = BusStatus::Ready(0);
            return;
        }
        let val = match response {
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
        // Latch the chip's bus output for the next floating-bus read.
        match response {
            BusResponse::Word(w) => self.memory_mut().set_last_bus_value(w),
            BusResponse::Byte(b) => self.memory_mut().set_last_bus_value(u16::from(b)),
            BusResponse::WriteAck => {}
        }
        self.cpu_base_mut().bus_status = BusStatus::Ready(val);
    }
}
