//! Commodore Amiga A500 (OCS) machine — master-clock-driven tick loop.
//!
//! Wires 68000 CPU, Agnus OCS, Denise OCS, Paula 8364, 2×CIA 8520, and Gary
//! together with the correct clock tree:
//!
//! | Clock           | Rate (PAL)       | Divider from master |
//! |-----------------|------------------|---------------------|
//! | Master (crystal) | 28.375 160 MHz  | 1                   |
//! | Colour clock     | 3.546 895 MHz   | 8                   |
//! | CPU (68000 φ)    | 7.093 790 MHz   | 4                   |
//! | E-clock          | 709.379 kHz     | 40                  |
//!
//! Ported from `~/Projects/Emu198x-archive/crates/machine-commodore-amiga/src/lib.rs`.

pub mod memory;

use crate::memory::Memory;
use commodore_agnus_ocs::{Agnus, BlitterDmaOp, Copper, SlotOwner};
use commodore_denise_ocs::DeniseOcs;
use commodore_gary::{ChipSelect, Gary};
use commodore_paula_8364::Paula8364;
use format_commodore_amiga_adf::Adf;
use mos_cia_8520::Cia8520;
use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::{Cpu68000, State};
use peripheral_commodore_amiga_floppy::AmigaFloppyDrive;
use peripheral_commodore_amiga_keyboard::AmigaKeyboard;
use std::collections::VecDeque;

// ─── Clock constants ──────────────────────────────────────────────────

/// CCKs per E-clock tick (CIAs). E-clock = 709.379 kHz = CCK / 5.
const ECLOCK_CCK_DIVISOR: u64 = 5;

/// Host audio sample rate.
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
/// CCK frequency for audio downsampling phase accumulator.
const PAL_CCK_HZ: u64 = 28_375_160 / 8;
/// MFM stream cadence: one word roughly every 112 CCKs at 300 RPM.
const DISK_STREAM_WORD_CCKS: u16 = 112;

/// Raster framebuffer dimensions (re-exported from Denise).
pub use commodore_denise_ocs::{PAL_RASTER_FB_HEIGHT, RASTER_FB_WIDTH};

// ─── Disk DMA runtime ─────────────────────────────────────────────────

struct DiskDmaRuntime {
    data: Vec<u8>,
    byte_index: usize,
    words_remaining: u32,
    is_write: bool,
    wordsync_enabled: bool,
    wordsync_waiting: bool,
}

struct DiskReadRuntime {
    data: Vec<u8>,
    byte_index: usize,
    word_cck_counter: u16,
    cylinder: u32,
    head: u32,
}

// ─── Machine ──────────────────────────────────────────────────────────

pub struct Amiga {
    pub cck_count: u64,
    pub cpu: Cpu68000,
    pub agnus: Agnus,
    pub copper: Copper,
    pub denise: DeniseOcs,
    pub paula: Paula8364,
    pub cia_a: Cia8520,
    pub cia_b: Cia8520,
    pub gary: Gary,
    pub memory: Memory,
    pub floppy: AmigaFloppyDrive,
    pub keyboard: AmigaKeyboard,

    // Input state
    pub mouse_x: u8,
    pub mouse_y: u8,
    pub joy1dat: u16,
    /// Button state (active-low): bit 0=LMB, 1=RMB, 2=MMB, 3=joy fire.
    pub input_buttons: u8,

    // Keyboard handshake edge detection
    cia_a_cra_sp_prev: bool,

    // Sprite DMA phase per sprite (0-4).
    sprite_dma_phase: [u8; 8],

    // Disk DMA
    disk_dma_runtime: Option<DiskDmaRuntime>,
    disk_read_runtime: Option<DiskReadRuntime>,

    // Register pipeline delays (Agnus→Denise 2 CCK)
    bplcon0_denise_pending: Option<(u16, u8)>,
    ddfstrt_pending: Option<(u16, u8)>,
    ddfstop_pending: Option<(u16, u8)>,
    color_pending: Vec<(usize, u16, u8)>,

    // Vertical bitplane DMA enable flip-flop
    bpl_dma_vactive_latch: bool,

    // Audio downsampling
    audio_sample_phase: u64,
    audio_buffer: Vec<f32>,
    audio_lpf_left: f32,
    audio_lpf_right: f32,
    audio_lpf_alpha: f32,

    // Serial port (minimal — TBE always set for Kickstart boot)
    serdatr: u16,
    serper: u16,
    serial_shift_countdown: u32,

    // Debug counters
    pub vertb_count: u64,
    pub reset_count: u64,
    pub debug_custom_write_log: VecDeque<String>,
    pub debug_cia_b_prb_log: VecDeque<String>,
    pub debug_cia_b_write_log: VecDeque<String>,
    pub debug_cia_a_read_log: VecDeque<String>,
    pub debug_cia_b_read_log: VecDeque<String>,
}

impl Amiga {
    /// Construct an A500 OCS PAL machine with the given Kickstart ROM.
    pub fn new(kickstart: Vec<u8>) -> Self {
        Self::new_with_slow_ram(kickstart, 0)
    }

    /// Construct with optional slow (ranger/trapdoor) RAM.
    pub fn new_with_slow_ram(kickstart: Vec<u8>, slow_ram_size: usize) -> Self {
        let chip_ram_size = 512 * 1024; // A500 = 512 KiB chip RAM
        let raster_fb_height = PAL_RASTER_FB_HEIGHT;

        let mut agnus = Agnus::new_with_region_lines(commodore_agnus_ocs::PAL_LINES_PER_FRAME);
        agnus.agnus_id = 0x10; // PAL OCS Fat Agnus 8371
        let denise = DeniseOcs::new_with_raster_height(raster_fb_height);
        let copper = Copper::new();
        let paula = Paula8364::default();
        // CIA-A PRA external inputs (active-low accent signals):
        //   Bit 7: /FIR1 = 1 (joystick fire not pressed)
        //   Bit 6: /FIR0 = 1 (joystick fire not pressed)
        //   Bit 5: /DSKRDY = 1 (drive not ready)
        //   Bit 4: /DSKTRACK0 = 0 (at track 0)
        //   Bit 3: /DSKPROT = 1 (not write protected)
        //   Bit 2: /DSKCHANGE = 0 (disk removed — no disk in drive)
        //   Bits 1,0: LED/OVL outputs, external pull-up = 1,1
        let mut cia_a = Cia8520::new("CIA-A");
        cia_a.external_a = 0xEB; // 0b_1110_1011
        let cia_b = Cia8520::new("CIA-B");
        let mut gary = Gary::new();
        gary.set_slow_ram_present(slow_ram_size > 0);
        let memory = Memory::new(chip_ram_size, kickstart, slow_ram_size);

        let mut cpu = Cpu68000::new();

        // Read initial SSP and PC from ROM (overlay maps ROM to $0).
        let ssp = (u32::from(memory.kickstart[0]) << 24)
            | (u32::from(memory.kickstart[1]) << 16)
            | (u32::from(memory.kickstart[2]) << 8)
            | u32::from(memory.kickstart[3]);
        let entry_pc = (u32::from(memory.kickstart[4]) << 24)
            | (u32::from(memory.kickstart[5]) << 16)
            | (u32::from(memory.kickstart[6]) << 8)
            | u32::from(memory.kickstart[7]);

        // Use reset_to — queues FetchIRC + PromoteIRC for proper
        // pipeline startup, matching the archive's init sequence.
        cpu.reset_to(ssp, entry_pc);

        // Audio low-pass filter: ~4.5 kHz cutoff at 48 kHz sample rate.
        let cutoff = 4500.0_f32;
        let omega = 2.0 * std::f32::consts::PI * cutoff / AUDIO_SAMPLE_RATE as f32;
        let lpf_alpha = omega / (1.0 + omega);

        Self {
            cck_count: 0,
            cpu,
            agnus,
            copper,
            denise,
            paula,
            cia_a,
            cia_b,
            gary,
            memory,
            floppy: AmigaFloppyDrive::new(),
            keyboard: AmigaKeyboard::new(),
            mouse_x: 0,
            mouse_y: 0,
            joy1dat: 0,
            input_buttons: 0x0F, // all buttons released (active-low)
            cia_a_cra_sp_prev: false,
            sprite_dma_phase: [0; 8],
            disk_dma_runtime: None,
            disk_read_runtime: None,
            bplcon0_denise_pending: None,
            ddfstrt_pending: None,
            ddfstop_pending: None,
            color_pending: Vec::new(),
            bpl_dma_vactive_latch: false,
            audio_sample_phase: 0,
            audio_buffer: Vec::with_capacity(2048),
            audio_lpf_left: 0.0,
            audio_lpf_right: 0.0,
            audio_lpf_alpha: lpf_alpha,
            serdatr: 0x3000, // TBE + TSRE set (transmit buffer empty)
            serper: 0,
            serial_shift_countdown: 0,
            vertb_count: 0,
            reset_count: 0,
            debug_custom_write_log: VecDeque::new(),
            debug_cia_b_prb_log: VecDeque::new(),
            debug_cia_b_write_log: VecDeque::new(),
            debug_cia_a_read_log: VecDeque::new(),
            debug_cia_b_read_log: VecDeque::new(),
        }
    }

    // ─── Single-bus-per-CCK tick ─────────────────────────────────────────
    //
    // One colour clock = one chip-bus transaction. Either DMA gets the
    // bus or the CPU does, never both. The CPU always gets 2 internal
    // clock ticks per CCK regardless, but DTACK is only asserted when
    // Agnus grants the CPU a bus slot.

    pub fn tick_cck(&mut self) {
        self.cck_count += 1;

        // 1. Advance beam FIRST (advance-then-act)
        self.agnus.tick_cck();
        let vpos = self.agnus.vpos;
        let hpos = self.agnus.hpos;

        // Begin-of-line housekeeping.
        if hpos == 0 {
            self.denise.begin_beam_line();
            self.update_bpl_dma_vactive_flipflop(vpos);
        }

        // VERTB interrupt + copper restart at frame start.
        if vpos == 0 && hpos == 0 {
            self.vertb_count += 1;
            self.paula.request_interrupt(5); // VERTB
            if self.agnus.dma_enabled(0x0080) {
                self.copper.restart_cop1();
            }
            // Sync interlace from Agnus to Denise.
            let interlace = (self.agnus.bplcon0 & 0x0004) != 0;
            self.denise.interlace_active = interlace;
            self.denise.lof = self.agnus.lof;
        }

        // CIA-B TOD input = HSYNC (once per line).
        if hpos == 0 {
            self.cia_b.tod_pulse();
        }
        // CIA-A TOD input = VSYNC (once per frame).
        if vpos == 0 && hpos == 0 {
            self.cia_a.tod_pulse();
        }

        // ── Drain pending register pipeline writes ────────────────
        self.drain_pipeline_writes();

        // ── Output pixels BEFORE DMA ──────────────────────────────
        // Shift registers hold data from the PREVIOUS fetch group.
        self.output_pixels(hpos, vpos);

        // ── DMA slots ─────────────────────────────────────────────
        let bus_plan = self.agnus.cck_bus_plan();
        let audio_dma_slot = bus_plan.audio_dma_service_channel;

        if bus_plan.disk_dma_slot_granted {
            self.service_disk_dma_slot();
        }
        if let Some(sprite) = bus_plan.sprite_dma_service_channel {
            self.service_sprite_dma_slot(sprite as usize);
        }

        let mut copper_used_chip_bus = false;
        let mut fetched_plane_0 = false;
        let mut bitplane_dma_fetch_plane = bus_plan.bitplane_dma_fetch_plane;
        if bitplane_dma_fetch_plane.is_some() && !self.bitplane_dma_vertical_active(vpos) {
            bitplane_dma_fetch_plane = None;
        }

        if let Some(plane) = bitplane_dma_fetch_plane {
            let idx = plane as usize;
            let addr = self.agnus.bpl_pt[idx];
            let hi = self.memory.read_chip_byte(addr);
            let lo = self.memory.read_chip_byte(addr | 1);
            let val = (u16::from(hi) << 8) | u16::from(lo);
            self.denise.load_bitplane(idx, val);
            self.agnus.bpl_pt[idx] = addr.wrapping_add(2);
            if plane == 0 {
                fetched_plane_0 = true;
                self.denise.queue_shift_load_from_bpl1dat();
            }
        } else if bus_plan.copper_dma_slot_granted {
            let memory = &self.memory;
            let res = self
                .copper
                .tick(vpos, hpos, self.agnus.blitter_busy, |addr| {
                    let hi = memory.read_chip_byte(addr);
                    let lo = memory.read_chip_byte(addr | 1);
                    (u16::from(hi) << 8) | u16::from(lo)
                });
            copper_used_chip_bus = res.is_some()
                || matches!(
                    self.copper.state,
                    commodore_agnus_ocs::CopperState::Fetch1
                        | commodore_agnus_ocs::CopperState::Fetch2
                );
            if let Some((reg, val)) = res {
                // COPCON protection: copper cannot write $000-$03E,
                // and $040-$07E only when CDANG is set.
                if reg >= 0x080 || (reg >= 0x040 && self.copper.danger) {
                    self.write_custom_reg(reg, val);
                }
            }
        }

        let audio_return_progress = bus_plan.paula_return_progress(copper_used_chip_bus);

        // ── Blitter progress ──────────────────────────────────────
        let blitter_progress = bus_plan.blitter_dma_progress_granted
            || (matches!(bus_plan.slot_owner, SlotOwner::Copper)
                && self.agnus.blitter_busy
                && self.agnus.dma_enabled(0x0040)
                && !copper_used_chip_bus);
        if let Some(blit_op) = self.agnus.tick_blitter_scheduler_op(blitter_progress) {
            let completed =
                execute_incremental_blitter_op(&mut self.agnus, &mut self.memory, blit_op);
            if completed {
                self.agnus.clear_blitter_scheduler();
                self.agnus.blitter_busy = false;
                self.paula.request_interrupt(6); // BLIT
            }
        }
        if self.agnus.blitter_exec_ready() && execute_blit(&mut self.agnus, &mut self.memory) {
            self.paula.request_interrupt(6); // BLIT
        }

        // ── Bitplane modulo after last fetch group ────────────────
        if fetched_plane_0 {
            let hires = (self.agnus.bplcon0 & 0x8000) != 0;
            let group_end_offset = if hires { 3 } else { 7 };
            let group_start = hpos.wrapping_sub(group_end_offset);
            let modulo_threshold = if hires {
                self.agnus.ddfstop.wrapping_add(4)
            } else {
                self.agnus.ddfstop
            };
            if group_start >= modulo_threshold {
                let num_bpl = self.agnus.num_bitplanes();
                for i in 0..num_bpl as usize {
                    let modulo = if i % 2 == 0 {
                        self.agnus.bpl1mod
                    } else {
                        self.agnus.bpl2mod
                    };
                    self.agnus.bpl_pt[i] = (self.agnus.bpl_pt[i] as i32 + modulo as i32) as u32;
                }
            }
        }

        // ── Paula audio DMA ───────────────────────────────────────
        self.paula.tick_audio_cck_with_bus(
            self.agnus.dmacon,
            audio_dma_slot,
            audio_return_progress,
            |addr| self.memory.read_chip_byte(addr),
        );
        self.tick_disk_read_stream();
        self.paula.tick_disk_cck();

        // ── Audio downsampling ────────────────────────────────────
        self.audio_sample_phase += u64::from(AUDIO_SAMPLE_RATE);
        while self.audio_sample_phase >= PAL_CCK_HZ {
            self.audio_sample_phase -= PAL_CCK_HZ;
            let (left, right) = self.paula.mix_audio_stereo();
            let a = self.audio_lpf_alpha;
            self.audio_lpf_left += a * (left - self.audio_lpf_left);
            self.audio_lpf_right += a * (right - self.audio_lpf_right);
            self.audio_buffer.push(self.audio_lpf_left);
            self.audio_buffer.push(self.audio_lpf_right);
        }

        // ── CPU: 2 clock ticks, bus only if CPU owns this CCK ─────
        let cpu_has_bus = bus_plan.cpu_chip_bus_granted;
        for _ in 0..2 {
            self.cpu.ipl = self.paula.compute_ipl();
            if cpu_has_bus {
                self.service_cpu_bus();
            } else if matches!(
                &self.cpu.state,
                State::BusCycle { .. } | State::TableWalk { .. }
            ) {
                self.cpu.bus_status = BusStatus::Wait;
            }
            self.cpu.tick();
        }

        // ── E-clock: every 5th CCK ───────────────────────────────
        if self.cck_count.is_multiple_of(ECLOCK_CCK_DIVISOR) {
            self.tick_eclock();
        }

        // ── Pending disk DMA start ────────────────────────────────
        if self.paula.disk_dma_pending {
            self.paula.disk_dma_pending = false;
            self.start_disk_dma_transfer();
        }

        // ── Serial port ───────────────────────────────────────────
        if self.serial_shift_countdown > 0 {
            self.serial_shift_countdown -= 1;
            if self.serial_shift_countdown == 0 {
                self.serdatr |= 0x3000; // TBE + TSRE
                self.paula.request_interrupt(0); // TBE
            }
        }
    }

    // ─── CPU bus servicing ────────────────────────────────────────

    fn service_cpu_bus(&mut self) {
        // Extract bus cycle info to avoid borrowing self.cpu.state across mutable calls.
        let bus_info = match &self.cpu.state {
            State::BusCycle {
                addr,
                fc,
                is_read,
                is_word,
                data,
                cycle_count,
                ..
            } => Some((*addr, *fc, *is_read, *is_word, *data, *cycle_count)),
            State::TableWalk {
                walk_cycle_count,
                walk_addr,
                ..
            } => {
                if *walk_cycle_count >= 2 {
                    let val = self.memory.read_word(*walk_addr);
                    self.cpu.bus_status = BusStatus::Ready(val);
                } else {
                    self.cpu.bus_status = BusStatus::Wait;
                }
                return;
            }
            _ => {
                self.cpu.bus_status = BusStatus::Wait;
                return;
            }
        };

        let Some((addr, fc, is_read, is_word, data, cycle_count)) = bus_info else {
            return;
        };

        {
            // DTACK sampled at S4 = CPU clock 2 (tick index 2).
            if cycle_count < 2 {
                self.cpu.bus_status = BusStatus::Wait;
                return;
            }

            if fc == FunctionCode::InterruptAck {
                let level = self.paula.compute_ipl();
                self.cpu.bus_status = BusStatus::Ready(24 + u16::from(level));
                return;
            }

            let addr24 = addr & 0xFF_FFFF;

            match self.gary.decode(addr24) {
                ChipSelect::CiaA => {
                    let reg = ((addr24 >> 8) & 0x0F) as u8;
                    if is_read {
                        let val = if addr24 & 1 != 0 {
                            let cia_val = self.cia_a.read(reg);
                            if reg == 0x00 || reg == 0x0D {
                                self.debug_cia_a_read_log.push_back(format!(
                                    "reg=${reg:02X} fc={fc:?} is_word={is_word} val=${cia_val:02X}"
                                ));
                                if self.debug_cia_a_read_log.len() > 64 {
                                    self.debug_cia_a_read_log.pop_front();
                                }
                            }
                            u16::from(cia_val)
                        } else {
                            0xFF00
                        };
                        self.cpu.bus_status = BusStatus::Ready(val);
                    } else {
                        let should_write = (addr24 & 1 != 0) || is_word;
                        if should_write {
                            let val = data.unwrap_or(0) as u8;
                            self.cia_a.write(reg, val);
                            // PRA bit 0 = /OVL
                            if reg == 0 || reg == 2 {
                                let out = self.cia_a.port_a_output();
                                self.memory.overlay = out & 0x01 != 0;
                            }
                            // CRA bit 6 = SP direction (keyboard handshake)
                            if reg == 0x0E {
                                let sp_now = val & 0x40 != 0;
                                if self.cia_a_cra_sp_prev && !sp_now {
                                    self.keyboard.handshake();
                                }
                                self.cia_a_cra_sp_prev = sp_now;
                            }
                        }
                        self.cpu.bus_status = BusStatus::Ready(0);
                    }
                }

                ChipSelect::CiaB => {
                    let reg = ((addr24 >> 8) & 0x0F) as u8;
                    if is_read {
                        let val = if addr24 & 1 == 0 {
                            let cia_read = self.cia_b.read(reg);
                            if matches!(reg, 0x08..=0x0A | 0x0D | 0x0F) {
                                self.debug_cia_b_read_log.push_back(format!(
                                    "reg=${reg:02X} fc={fc:?} is_word={is_word} val=${cia_read:02X}"
                                ));
                                if self.debug_cia_b_read_log.len() > 64 {
                                    self.debug_cia_b_read_log.pop_front();
                                }
                            }
                            let cia_val = u16::from(cia_read);
                            // Word reads: CIA-B data on D8-D15 (high byte), D0-D7 float to 0xFF.
                            // Byte reads: return in low bits to match CPU ReadByte convention.
                            if is_word {
                                cia_val << 8 | 0x00FF
                            } else {
                                cia_val
                            }
                        } else {
                            0x00FF
                        };
                        self.cpu.bus_status = BusStatus::Ready(val);
                    } else {
                        let should_write = (addr24 & 1 == 0) || is_word;
                        if should_write {
                            let val = if is_word {
                                (data.unwrap_or(0) >> 8) as u8
                            } else {
                                data.unwrap_or(0) as u8
                            };
                            self.debug_cia_b_write_log.push_back(format!(
                                "reg=${reg:02X} fc={fc:?} is_word={is_word} val=${val:02X}"
                            ));
                            if self.debug_cia_b_write_log.len() > 64 {
                                self.debug_cia_b_write_log.pop_front();
                            }
                            self.cia_b.write(reg, val);
                            // Floppy control is on CIA-B PRB.
                            if reg == 0x01 {
                                let prb = self.cia_b.port_b_output();
                                let step = prb & 0x01 == 0;
                                // CIA-B PRB bit 1 is DIR: 0 = inward/towards higher tracks,
                                // 1 = outward/towards track 0.
                                let dir_inward = prb & 0x02 == 0;
                                let side_upper = prb & 0x04 == 0;
                                let sel = prb & 0x08 == 0;
                                let motor = prb & 0x80 == 0;
                                self.debug_cia_b_prb_log.push_back(format!(
                                    "val=${prb:02X} step={step} dir_inward={dir_inward} side_upper={side_upper} sel={sel} motor={motor}"
                                ));
                                if self.debug_cia_b_prb_log.len() > 32 {
                                    self.debug_cia_b_prb_log.pop_front();
                                }
                                self.floppy
                                    .update_control(step, dir_inward, side_upper, sel, motor);
                            }
                        }
                        self.cpu.bus_status = BusStatus::Ready(0);
                    }
                }

                ChipSelect::Custom => {
                    let offset = (addr24 & 0x1FE) as u16;
                    if is_read {
                        let word = self.read_custom_reg(offset);
                        let val = if is_word {
                            word
                        } else if addr24 & 1 == 0 {
                            u16::from((word >> 8) as u8)
                        } else {
                            u16::from(word as u8)
                        };
                        self.cpu.bus_status = BusStatus::Ready(val);
                    } else {
                        let val = if is_word {
                            data.unwrap_or(0)
                        } else {
                            let byte = data.unwrap_or(0) as u8;
                            let lane_word = if addr24 & 1 == 0 {
                                u16::from(byte) << 8
                            } else {
                                u16::from(byte)
                            };
                            // Set/clear registers (DMACON, INTENA, INTREQ,
                            // ADKCON) are zero-extended — the unused half
                            // is always 0.
                            if matches!(offset, 0x096 | 0x09A | 0x09C | 0x09E) {
                                lane_word
                            } else if let Some(current) = self.byte_merge_latch(offset) {
                                // Merge: preserve the OTHER byte from the
                                // current register value.
                                if addr24 & 1 == 0 {
                                    (current & 0x00FF) | lane_word
                                } else {
                                    (current & 0xFF00) | lane_word
                                }
                            } else {
                                lane_word
                            }
                        };
                        if matches!(offset, 0x09A | 0x09C) {
                            self.debug_custom_write_log.push_back(format!(
                                "addr=${addr24:06X} offset=${offset:03X} fc={fc:?} is_word={is_word} val=${val:04X}"
                            ));
                            if self.debug_custom_write_log.len() > 32 {
                                self.debug_custom_write_log.pop_front();
                            }
                        }
                        self.write_custom_reg(offset, val);
                        self.cpu.bus_status = BusStatus::Ready(0);
                    }
                }

                ChipSelect::ChipRam => {
                    // CPU only reaches here when Agnus granted a bus
                    // slot (cpu_has_bus check in tick_cck).
                    if is_read {
                        let val = if is_word {
                            self.memory.read_word(addr24)
                        } else {
                            u16::from(self.memory.read_byte(addr24))
                        };
                        self.cpu.bus_status = BusStatus::Ready(val);
                    } else {
                        let val = data.unwrap_or(0);
                        if is_word {
                            self.memory.write_word(addr24, val);
                        } else {
                            self.memory.write_byte(addr24, val as u8);
                        }
                        self.cpu.bus_status = BusStatus::Ready(0);
                    }
                }

                ChipSelect::SlowRam | ChipSelect::Rom | ChipSelect::Unmapped => {
                    if is_read {
                        let val = if is_word {
                            self.memory.read_word(addr24)
                        } else {
                            u16::from(self.memory.read_byte(addr24))
                        };
                        self.cpu.bus_status = BusStatus::Ready(val);
                    } else {
                        let val = data.unwrap_or(0);
                        if is_word {
                            self.memory.write_word(addr24, val);
                        } else {
                            self.memory.write_byte(addr24, val as u8);
                        }
                        self.cpu.bus_status = BusStatus::Ready(0);
                    }
                }

                // Autoconfig (Zorro expansion): no boards present.
                // Return $FFFF (bus floats high when nothing responds).
                ChipSelect::Autoconfig => {
                    self.cpu.bus_status = BusStatus::Ready(if is_read { 0xFFFF } else { 0 });
                }

                // Other peripheral spaces not wired yet.
                _ => {
                    self.cpu.bus_status = BusStatus::Ready(0);
                }
            }
        }

        // Handle RESET instruction output.
        if self.cpu.reset_out {
            self.cpu.reset_out = false;
            self.reset_count = self.reset_count.wrapping_add(1);
            self.cia_a.reset();
            self.cia_b.reset();
            self.memory.overlay = true;
            self.paula.reset();
            self.agnus.dmacon = 0;
        }
    }

    // ─── Byte-write merge latch ─────────────────────────────────────
    //
    // When the CPU does a byte write to a custom register, the other
    // half of the word must be preserved (read-back from the current
    // register value). Without this, a MOVE.B to BPLCON0 would clobber
    // the high byte containing the bitplane count.

    fn byte_merge_latch(&self, offset: u16) -> Option<u16> {
        match offset {
            0x040 => Some(self.agnus.bltcon0),
            0x042 => Some(self.agnus.bltcon1),
            0x08E => Some(self.agnus.diwstrt),
            0x090 => Some(self.agnus.diwstop),
            0x092 => Some(self.agnus.ddfstrt),
            0x094 => Some(self.agnus.ddfstop),
            0x098 => Some(self.denise.clxcon),
            0x100 => Some(self.agnus.bplcon0),
            0x102 => Some(self.denise.bplcon1),
            0x104 => Some(self.denise.bplcon2),
            0x108 => Some(self.agnus.bpl1mod as u16),
            0x10A => Some(self.agnus.bpl2mod as u16),
            0x180..=0x1BE => {
                let idx = ((offset - 0x180) / 2) as usize;
                Some(self.denise.palette[idx])
            }
            _ => None,
        }
    }

    // ─── Custom register reads ────────────────────────────────────

    fn read_custom_reg(&mut self, offset: u16) -> u16 {
        match offset {
            0x002 => {
                let busy = if self.agnus.blitter_busy { 0x4000 } else { 0 };
                self.agnus.dmacon | busy
            }
            0x004 => {
                // VPOSR: LOF (bit 15) + Agnus ID (bits 14-8) + V10-V8 (bits 2-0)
                let lof = if self.agnus.lof { 0x8000u16 } else { 0 };
                let id = (self.agnus.agnus_id & 0x7F) << 8;
                let v8 = (self.agnus.vpos >> 8) & 1;
                let v9 = (self.agnus.vpos >> 9) & 1;
                let v10 = (self.agnus.vpos >> 10) & 1;
                lof | id | (v10 << 2) | (v9 << 1) | v8
            }
            0x006 => {
                // VHPOSR: V7-V0 in high byte, H8-H0 in low byte
                ((self.agnus.vpos & 0xFF) << 8) | (self.agnus.hpos & 0xFF)
            }
            0x00A => {
                // JOY0DAT: mouse quadrature counters
                u16::from(self.mouse_y) << 8 | u16::from(self.mouse_x)
            }
            0x00C => self.joy1dat,
            0x00E => self.denise.read_clxdat(),
            0x010 => self.paula.adkcon,
            0x016 => {
                // POTGOR: active-low button state
                let rmb = (self.input_buttons >> 1) & 0x01;
                let mmb = (self.input_buttons >> 2) & 0x01;
                (0xFF00 & !(1u16 << 10) & !(1u16 << 8))
                    | (u16::from(rmb) << 10)
                    | (u16::from(mmb) << 8)
            }
            0x018 => self.serdatr,
            0x01A => self.paula.read_dskbytr(self.agnus.dmacon),
            0x01C => self.paula.intena,
            0x01E => self.paula.intreq,
            0x07C => 0xFFFF, // DENISEID (OCS = no ID)
            0x0A0..=0x0DA => self.paula.read_audio_register(offset).unwrap_or(0),
            // BEAMCON0 ($DFF1DC): ECS register, but Kickstart 1.3 reads it
            // for PAL/NTSC detection. Return $0020 (PAL) to match FS-UAE.
            0x1DC => {
                if self.agnus.agnus_id & 0x10 != 0 {
                    0x0020
                } else {
                    0x0000
                }
            }
            _ => 0,
        }
    }

    // ─── Custom register writes ───────────────────────────────────

    fn write_custom_reg(&mut self, offset: u16, val: u16) {
        self.queue_pipelined_write(offset, val);

        // Sprite pointer low-word write resets DMA fetch phase.
        if (0x120..=0x13E).contains(&offset) && (offset & 2) != 0 {
            let idx = ((offset - 0x120) / 4) as usize;
            if idx < 8 {
                self.sprite_dma_phase[idx] = 0;
            }
        }
        // Writing SPRxCTL disables sprite DMA until VSTART match.
        if (0x140..=0x17E).contains(&offset) {
            let sprite = ((offset - 0x140) / 8) as usize;
            let reg = ((offset - 0x140) % 8) / 2;
            if sprite < 8 && reg == 1 {
                self.sprite_dma_phase[sprite] = 4;
            }
        }
        // SERDAT ($030)
        if offset == 0x030 {
            self.serdatr &= !0x3000;
            let period = u32::from(self.serper & 0x7FFF) + 1;
            let bits = if self.serper & 0x8000 != 0 { 11 } else { 10 };
            self.serial_shift_countdown = period * bits;
        }
        if offset == 0x032 {
            self.serper = val;
        }
        // JOYTEST ($036)
        if offset == 0x036 {
            self.mouse_x = (val & 0xFF) as u8;
            self.mouse_y = (val >> 8) as u8;
            self.joy1dat = val;
        }
        write_custom_register(
            &mut self.agnus,
            &mut self.denise,
            &mut self.copper,
            &mut self.paula,
            offset,
            val,
        );
    }

    // ─── Pipeline drain ───────────────────────────────────────────

    fn queue_pipelined_write(&mut self, offset: u16, val: u16) {
        match offset {
            0x100 => self.bplcon0_denise_pending = Some((val, 2)),
            0x092 => self.ddfstrt_pending = Some((val, 2)),
            0x094 => self.ddfstop_pending = Some((val, 2)),
            0x180..=0x1BE => {
                let idx = ((offset - 0x180) / 2) as usize;
                self.color_pending.push((idx, val, 2));
            }
            _ => {}
        }
    }

    fn drain_pipeline_writes(&mut self) {
        if let Some((val, ref mut countdown)) = self.bplcon0_denise_pending {
            if *countdown <= 1 {
                self.denise.bplcon0 = val;
                self.bplcon0_denise_pending = None;
            } else {
                *countdown -= 1;
            }
        }
        if let Some((val, ref mut countdown)) = self.ddfstrt_pending {
            if *countdown <= 1 {
                self.agnus.ddfstrt = val;
                self.ddfstrt_pending = None;
            } else {
                *countdown -= 1;
            }
        }
        if let Some((val, ref mut countdown)) = self.ddfstop_pending {
            if *countdown <= 1 {
                self.agnus.ddfstop = val;
                self.ddfstop_pending = None;
            } else {
                *countdown -= 1;
            }
        }
        self.color_pending.retain_mut(|(idx, val, countdown)| {
            if *countdown <= 1 {
                self.denise.set_palette(*idx, *val);
                false
            } else {
                *countdown -= 1;
                true
            }
        });
    }

    // ─── Pixel output ─────────────────────────────────────────────

    fn output_pixels(&mut self, hpos: u16, vpos: u16) {
        let beam_x0 = u32::from(hpos) * 2;
        let beam_x1 = beam_x0 + 1;
        let beam_y = u32::from(vpos);

        let pixel0 = self
            .denise
            .output_pixel_with_beam(beam_x0, beam_y, beam_x0, beam_y);
        let pixel1 = self
            .denise
            .output_pixel_with_beam(beam_x1, beam_y, beam_x1, beam_y);

        // Write 8 sub-pixels per CCK (4 from pixel0, 4 from pixel1).
        for i in 0..4u8 {
            let ci = pixel0.quad_color_idx[i as usize];
            let rgb = self.denise.resolve_color_rgb12(ci);
            let argb = DeniseOcs::rgb12_to_argb32(rgb);
            self.denise.write_raster_pixel(hpos, vpos, i, argb);
        }
        for i in 0..4u8 {
            let ci = pixel1.quad_color_idx[i as usize];
            let rgb = self.denise.resolve_color_rgb12(ci);
            let argb = DeniseOcs::rgb12_to_argb32(rgb);
            self.denise.write_raster_pixel(hpos, vpos, 4 + i, argb);
        }
    }

    // ─── Sprite DMA ───────────────────────────────────────────────

    fn service_sprite_dma_slot(&mut self, sprite: usize) {
        if sprite >= 8 {
            return;
        }
        let vpos = self.agnus.vpos;
        let phase = self.sprite_dma_phase[sprite];

        match phase {
            0 => {
                // Fetch SPRxPOS
                let addr = self.agnus.spr_pt[sprite];
                let hi = self.memory.read_chip_byte(addr);
                let lo = self.memory.read_chip_byte(addr | 1);
                let val = (u16::from(hi) << 8) | u16::from(lo);
                self.denise.write_sprite_pos(sprite, val);
                self.agnus.spr_pt[sprite] = addr.wrapping_add(2);
                self.sprite_dma_phase[sprite] = 1;
            }
            1 => {
                // Fetch SPRxCTL
                let addr = self.agnus.spr_pt[sprite];
                let hi = self.memory.read_chip_byte(addr);
                let lo = self.memory.read_chip_byte(addr | 1);
                let val = (u16::from(hi) << 8) | u16::from(lo);
                self.denise.write_sprite_ctl(sprite, val);
                self.agnus.spr_pt[sprite] = addr.wrapping_add(2);
                // Derive VSTART/VSTOP from position/control words.
                let pos = self.denise.spr_pos[sprite];
                let ctl = self.denise.spr_ctl[sprite];
                let vstart = ((pos >> 8) & 0xFF) | ((ctl & 0x04) << 6);
                let vstop = ((ctl >> 8) & 0xFF) | ((ctl & 0x02) << 7);
                if Self::sprite_line_active(vpos, vstart, vstop) {
                    self.sprite_dma_phase[sprite] = 2;
                } else {
                    self.sprite_dma_phase[sprite] = 4; // wait for VSTART
                }
            }
            2 => {
                // Fetch SPRxDATA
                let addr = self.agnus.spr_pt[sprite];
                let hi = self.memory.read_chip_byte(addr);
                let lo = self.memory.read_chip_byte(addr | 1);
                let val = (u16::from(hi) << 8) | u16::from(lo);
                self.denise.write_sprite_data(sprite, val);
                self.agnus.spr_pt[sprite] = addr.wrapping_add(2);
                self.sprite_dma_phase[sprite] = 3;
            }
            3 => {
                // Fetch SPRxDATB
                let addr = self.agnus.spr_pt[sprite];
                let hi = self.memory.read_chip_byte(addr);
                let lo = self.memory.read_chip_byte(addr | 1);
                let val = (u16::from(hi) << 8) | u16::from(lo);
                self.denise.write_sprite_datb(sprite, val);
                self.agnus.spr_pt[sprite] = addr.wrapping_add(2);
                self.sprite_dma_phase[sprite] = 2; // next line
            }
            4 => {
                // Waiting for VSTART. Check on next sprite DMA slot.
                let pos = self.denise.spr_pos[sprite];
                let ctl = self.denise.spr_ctl[sprite];
                let vstart = ((pos >> 8) & 0xFF) | ((ctl & 0x04) << 6);
                if vpos == vstart {
                    self.sprite_dma_phase[sprite] = 0;
                }
            }
            _ => {}
        }
    }

    fn sprite_line_active(vpos: u16, vstart: u16, vstop: u16) -> bool {
        if vstart == vstop {
            return false;
        }
        if vstart < vstop {
            vpos >= vstart && vpos < vstop
        } else {
            vpos >= vstart || vpos < vstop
        }
    }

    // ─── Bitplane DMA vertical enable ─────────────────────────────

    fn update_bpl_dma_vactive_flipflop(&mut self, vpos: u16) {
        let diwstrt_v = (self.agnus.diwstrt >> 8) & 0xFF;
        let diwstop_v = (self.agnus.diwstop >> 8) & 0xFF;
        // Real hardware flip-flop: SET at VSTART, CLEAR at VSTOP.
        if vpos == diwstrt_v {
            self.bpl_dma_vactive_latch = true;
        }
        if vpos == diwstop_v | 0x100 {
            self.bpl_dma_vactive_latch = false;
        }
    }

    fn bitplane_dma_vertical_active(&self, _vpos: u16) -> bool {
        self.bpl_dma_vactive_latch
    }

    // ─── Disk DMA ─────────────────────────────────────────────────

    fn tick_disk_read_stream(&mut self) {
        if self.disk_dma_runtime.is_some() {
            return;
        }
        if !(self.floppy.selected() && self.floppy.read_data_available()) {
            self.disk_read_runtime = None;
            return;
        }

        let cylinder = self.floppy.cylinder();
        let head = self.floppy.head();
        let reload = self
            .disk_read_runtime
            .as_ref()
            .is_none_or(|runtime| runtime.cylinder != cylinder || runtime.head != head);
        if reload {
            let Some(data) = self.floppy.encode_mfm_track() else {
                self.disk_read_runtime = None;
                return;
            };
            self.disk_read_runtime = Some(DiskReadRuntime {
                data,
                byte_index: 0,
                word_cck_counter: 0,
                cylinder,
                head,
            });
        }

        let Some(runtime) = self.disk_read_runtime.as_mut() else {
            return;
        };
        if runtime.data.len() < 2 {
            return;
        }

        runtime.word_cck_counter = runtime.word_cck_counter.saturating_add(1);
        if runtime.word_cck_counter < DISK_STREAM_WORD_CCKS {
            return;
        }
        runtime.word_cck_counter -= DISK_STREAM_WORD_CCKS;

        let len = runtime.data.len();
        let hi = runtime.data[runtime.byte_index % len];
        let lo = runtime.data[(runtime.byte_index + 1) % len];
        runtime.byte_index = (runtime.byte_index + 2) % len;
        let word = (u16::from(hi) << 8) | u16::from(lo);

        let matched_sync = self.paula.note_disk_read_word(word);
        if matched_sync {
            self.paula.request_interrupt(12); // DSKSYN
        }
    }

    fn start_disk_dma_transfer(&mut self) {
        let word_count = (self.paula.dsklen & 0x3FFF) as u32;
        let is_write = self.paula.dsklen & 0x4000 != 0;

        if word_count == 0 {
            self.paula.request_interrupt(1); // DSKBLK
            self.disk_dma_runtime = None;
            return;
        }

        let data = self.floppy.encode_mfm_track().unwrap_or_default();
        let read_data_available = self.floppy.read_data_available();
        self.disk_dma_runtime = Some(DiskDmaRuntime {
            data,
            byte_index: 0,
            words_remaining: word_count,
            is_write,
            wordsync_enabled: !is_write && read_data_available && (self.paula.adkcon & 0x0400 != 0),
            wordsync_waiting: !is_write && read_data_available && (self.paula.adkcon & 0x0400 != 0),
        });
    }

    fn service_disk_dma_slot(&mut self) {
        let Some(runtime) = self.disk_dma_runtime.as_mut() else {
            return;
        };

        if runtime.words_remaining == 0 {
            self.disk_dma_runtime = None;
            return;
        }

        let mut completed = false;
        if !runtime.is_write {
            if !self.floppy.read_data_available() {
                return;
            }

            if runtime.data.len() < 2 {
                if let Some(data) = self.floppy.encode_mfm_track() {
                    runtime.data = data;
                    runtime.byte_index = 0;
                } else {
                    return;
                }
            }

            if runtime.data.len() >= 2 {
                let len = runtime.data.len();
                let hi = runtime.data[runtime.byte_index % len];
                let lo = runtime.data[(runtime.byte_index + 1) % len];
                runtime.byte_index = (runtime.byte_index + 2) % len;
                let word = (u16::from(hi) << 8) | u16::from(lo);

                let matched_sync = self.paula.note_disk_read_word(word);
                if matched_sync {
                    self.paula.request_interrupt(12); // DSKSYN
                }

                let suppress = if runtime.wordsync_enabled {
                    if runtime.wordsync_waiting {
                        if matched_sync {
                            runtime.wordsync_waiting = false;
                        }
                        true
                    } else {
                        matched_sync
                    }
                } else {
                    false
                };

                if !suppress {
                    let addr = self.agnus.dsk_pt;
                    self.memory.write_byte(addr, hi);
                    self.memory.write_byte(addr.wrapping_add(1), lo);
                    self.agnus.dsk_pt = addr.wrapping_add(2);
                    completed = true;
                }
            }
        } else {
            let addr = self.agnus.dsk_pt;
            let _hi = self.memory.read_chip_byte(addr);
            let _lo = self.memory.read_chip_byte(addr.wrapping_add(1));
            self.agnus.dsk_pt = addr.wrapping_add(2);
            completed = true;
        }

        if completed {
            runtime.words_remaining = runtime.words_remaining.saturating_sub(1);
            if runtime.words_remaining == 0 {
                self.disk_dma_runtime = None;
                self.paula.request_interrupt(1); // DSKBLK
            }
        }
    }

    // ─── E-clock ──────────────────────────────────────────────────

    fn tick_eclock(&mut self) {
        self.cia_a.tick();
        if self.cia_a.irq_active() {
            self.paula.request_interrupt(3); // PORTS (CIA-A)
        }
        self.cia_b.tick();
        if self.cia_b.irq_active() {
            self.paula.request_interrupt(13); // EXTER (CIA-B)
        }

        // Floppy motor spin-up and index pulse.
        if self.floppy.tick() {
            self.cia_b.flag_falling_edge();
            if self.cia_b.irq_active() {
                self.paula.request_interrupt(13); // EXTER (CIA-B)
            }
        }

        // Update CIA-A PRA with floppy status (active-low signals).
        let status = self.floppy.status();
        let mut ext_a = self.cia_a.external_a;
        // PA2: /DSKCHANGE
        if status.disk_change {
            ext_a &= !0x04;
        } else {
            ext_a |= 0x04;
        }
        // PA3: /DSKPROT
        if status.write_protect {
            ext_a &= !0x08;
        } else {
            ext_a |= 0x08;
        }
        // PA4: /DSKTRACK0
        if status.track0 {
            ext_a &= !0x10;
        } else {
            ext_a |= 0x10;
        }
        // PA5: /DSKRDY
        if status.ready {
            ext_a &= !0x20;
        } else {
            ext_a |= 0x20;
        }
        self.cia_a.external_a = ext_a;

        // Keyboard: tick and inject serial byte if ready.
        if let Some(byte) = self.keyboard.tick() {
            self.cia_a.receive_serial_byte(byte);
        }
    }

    // ─── Frame-level interface ────────────────────────────────────

    /// Advance by one PAL frame.
    pub fn run_frame(&mut self) {
        let ccks_per_frame = u64::from(self.agnus.lines_per_frame)
            * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);
        for _ in 0..ccks_per_frame {
            self.tick_cck();
        }
    }

    /// Drain interleaved stereo audio samples (f32, L,R,...).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.audio_buffer)
    }

    /// Access the raster framebuffer (ARGB32, superhires resolution).
    pub fn framebuffer(&self) -> &[u32] {
        &self.denise.framebuffer_raster
    }

    /// Framebuffer dimensions.
    pub fn framebuffer_size(&self) -> (u32, u32) {
        (self.denise.raster_fb_width, self.denise.raster_fb_height)
    }

    /// Insert an ADF disk image into the internal floppy drive (DF0:).
    pub fn insert_disk(&mut self, adf: Adf) {
        self.floppy.insert_disk(adf);
    }

    /// Eject the current DF0: disk.
    pub fn eject_disk(&mut self) {
        self.floppy.eject_disk();
    }

    /// Whether DF0: currently has a disk inserted.
    pub fn has_disk(&self) -> bool {
        self.floppy.has_disk()
    }

    /// Queue an Amiga keyboard event (raw Amiga keycode).
    pub fn key_event(&mut self, keycode: u8, pressed: bool) {
        self.keyboard.key_event(keycode, pressed);
    }
}

// ─── Shared custom register write dispatch ────────────────────────────

fn write_custom_register(
    agnus: &mut Agnus,
    denise: &mut DeniseOcs,
    copper: &mut Copper,
    paula: &mut Paula8364,
    offset: u16,
    val: u16,
) {
    match offset {
        // Blitter registers
        0x040 => agnus.bltcon0 = val,
        0x042 => agnus.bltcon1 = val,
        0x044 => agnus.blt_afwm = val,
        0x046 => agnus.blt_alwm = val,
        0x048 => agnus.blt_cpt = (agnus.blt_cpt & 0x0000_FFFF) | (u32::from(val) << 16),
        0x04A => agnus.blt_cpt = (agnus.blt_cpt & 0xFFFF_0000) | u32::from(val & 0xFFFE),
        0x04C => agnus.blt_bpt = (agnus.blt_bpt & 0x0000_FFFF) | (u32::from(val) << 16),
        0x04E => agnus.blt_bpt = (agnus.blt_bpt & 0xFFFF_0000) | u32::from(val & 0xFFFE),
        0x050 => agnus.blt_apt = (agnus.blt_apt & 0x0000_FFFF) | (u32::from(val) << 16),
        0x052 => agnus.blt_apt = (agnus.blt_apt & 0xFFFF_0000) | u32::from(val & 0xFFFE),
        0x054 => agnus.blt_dpt = (agnus.blt_dpt & 0x0000_FFFF) | (u32::from(val) << 16),
        0x056 => agnus.blt_dpt = (agnus.blt_dpt & 0xFFFF_0000) | u32::from(val & 0xFFFE),
        0x058 => {
            agnus.bltsize = val;
            agnus.start_blit();
        }
        0x060 => agnus.blt_cmod = val as i16,
        0x062 => agnus.blt_bmod = val as i16,
        0x064 => agnus.blt_amod = val as i16,
        0x066 => agnus.blt_dmod = val as i16,
        0x070 => agnus.blt_cdat = val,
        0x072 => agnus.blt_bdat = val,
        0x074 => agnus.blt_adat = val,

        // Copper
        0x080 => copper.cop1lc = (copper.cop1lc & 0x0000_FFFF) | (u32::from(val) << 16),
        0x082 => copper.cop1lc = (copper.cop1lc & 0xFFFF_0000) | u32::from(val & 0xFFFE),
        0x084 => copper.cop2lc = (copper.cop2lc & 0x0000_FFFF) | (u32::from(val) << 16),
        0x086 => copper.cop2lc = (copper.cop2lc & 0xFFFF_0000) | u32::from(val & 0xFFFE),
        0x088 => copper.restart_cop1(),
        0x08A => copper.restart_cop2(),

        // Display
        0x08E => agnus.diwstrt = val,
        0x090 => agnus.diwstop = val,
        // DDFSTRT/DDFSTOP pipelined — handled by queue_pipelined_write.
        0x092 | 0x094 => {}

        // DMA control
        0x096 => {
            if val & 0x8000 != 0 {
                agnus.dmacon |= val & 0x7FFF;
            } else {
                agnus.dmacon &= !(val & 0x7FFF);
            }
        }
        0x098 => denise.clxcon = val,

        // Interrupts
        0x09A => paula.write_intena(val),
        0x09C => paula.write_intreq(val),

        // Audio/disk control
        0x09E => paula.write_adkcon(val),

        // Disk
        0x020 => agnus.dsk_pt = (agnus.dsk_pt & 0x0000_FFFF) | (u32::from(val) << 16),
        0x022 => agnus.dsk_pt = (agnus.dsk_pt & 0xFFFF_0000) | u32::from(val & 0xFFFE),
        0x024 => paula.write_dsklen(val),
        0x026 => paula.write_dskdat(val),
        0x07E => paula.dsksync = val,

        // Serial — handled in write_custom_reg above.
        0x030 | 0x032 => {}

        // Copper danger
        0x02E => copper.danger = val & 0x02 != 0,

        // Bitplane control
        0x100 => agnus.bplcon0 = val,
        0x102 => denise.bplcon1 = val,
        0x104 => denise.bplcon2 = val,

        // Bitplane modulos
        0x108 => agnus.bpl1mod = val as i16,
        0x10A => agnus.bpl2mod = val as i16,

        // Bitplane pointers ($0E0-$0FE)
        0x0E0..=0x0FE => {
            let idx = ((offset - 0x0E0) / 4) as usize;
            if idx < 6 {
                if offset & 2 == 0 {
                    agnus.bpl_pt[idx] = (agnus.bpl_pt[idx] & 0x0000_FFFF) | (u32::from(val) << 16);
                } else {
                    agnus.bpl_pt[idx] = (agnus.bpl_pt[idx] & 0xFFFF_0000) | u32::from(val & 0xFFFE);
                }
            }
        }

        // Sprite pointers ($120-$13E)
        0x120..=0x13E => {
            let idx = ((offset - 0x120) / 4) as usize;
            if idx < 8 {
                agnus.write_sprite_pointer_reg(idx, (offset & 2) == 0, val);
            }
        }

        // Sprite data ($140-$17E)
        0x140..=0x17E => {
            let sprite = ((offset - 0x140) / 8) as usize;
            let reg = ((offset - 0x140) % 8) / 2;
            if sprite < 8 {
                match reg {
                    0 => denise.write_sprite_pos(sprite, val),
                    1 => denise.write_sprite_ctl(sprite, val),
                    2 => denise.write_sprite_data(sprite, val),
                    3 => denise.write_sprite_datb(sprite, val),
                    _ => {}
                }
            }
        }

        // Color palette — pipelined via queue_pipelined_write.
        0x180..=0x1BE => {}

        // Paula audio channels
        0x0A0..=0x0DA => {
            let _ = paula.write_audio_register(offset, val);
        }

        _ => {}
    }
}

// ─── Blitter helpers ──────────────────────────────────────────────────

fn execute_incremental_blitter_op(
    agnus: &mut Agnus,
    memory: &mut Memory,
    op: BlitterDmaOp,
) -> bool {
    let chip_len = memory.chip_ram.len();
    let chip_ram = std::cell::RefCell::new(memory.chip_ram.as_mut_slice());
    agnus.execute_incremental_blitter_op(
        op,
        |addr| {
            let a = (addr & 0x1F_FFFE) as usize;
            if a + 1 < chip_len {
                let chip_ram = chip_ram.borrow();
                (u16::from(chip_ram[a]) << 8) | u16::from(chip_ram[a + 1])
            } else {
                0
            }
        },
        |addr, val| {
            let a = (addr & 0x1F_FFFE) as usize;
            if a + 1 < chip_len {
                let mut chip_ram = chip_ram.borrow_mut();
                chip_ram[a] = (val >> 8) as u8;
                chip_ram[a + 1] = val as u8;
            }
        },
    )
}

/// Synchronous area/line blit.
///
/// Ported from `~/Projects/Emu198x-archive/crates/machine-commodore-amiga/src/lib.rs`.
fn execute_blit(agnus: &mut Agnus, memory: &mut Memory) -> bool {
    let height = (agnus.bltsize >> 6) & 0x3FF;
    let width_words = agnus.bltsize & 0x3F;
    let height = if height == 0 { 1024 } else { height } as u32;
    let width_words = if width_words == 0 { 64 } else { width_words } as u32;

    // LINE mode
    if agnus.bltcon1 & 0x0001 != 0 {
        execute_blit_line(agnus, memory);
        return true;
    }

    let use_a = agnus.bltcon0 & 0x0800 != 0;
    let use_b = agnus.bltcon0 & 0x0400 != 0;
    let use_c = agnus.bltcon0 & 0x0200 != 0;
    let use_d = agnus.bltcon0 & 0x0100 != 0;
    let lf = agnus.bltcon0 as u8;
    let a_shift = (agnus.bltcon0 >> 12) & 0xF;
    let b_shift = (agnus.bltcon1 >> 12) & 0xF;
    let desc = agnus.bltcon1 & 0x0002 != 0;
    let fci = (agnus.bltcon1 & 0x0004) != 0;
    let ife = (agnus.bltcon1 & 0x0008) != 0;
    let efe = (agnus.bltcon1 & 0x0010) != 0;
    let fill_enabled = ife || efe;

    let mut apt = agnus.blt_apt;
    let mut bpt = agnus.blt_bpt;
    let mut cpt = agnus.blt_cpt;
    let mut dpt = agnus.blt_dpt;

    let read_word = |mem: &Memory, addr: u32| -> u16 {
        let hi = mem.read_chip_byte(addr);
        let lo = mem.read_chip_byte(addr | 1);
        (u16::from(hi) << 8) | u16::from(lo)
    };

    let ptr_step: i32 = if desc { -2 } else { 2 };
    let mut a_prev: u16 = 0;
    let mut b_prev: u16 = 0;

    for _row in 0..height {
        let mut fill_carry: u16 = if fci { 1 } else { 0 };

        for col in 0..width_words {
            let a_raw = if use_a {
                let w = read_word(&*memory, apt);
                apt = (apt as i32 + ptr_step) as u32;
                agnus.blt_adat = w;
                w
            } else {
                agnus.blt_adat
            };
            let b_raw = if use_b {
                let w = read_word(&*memory, bpt);
                bpt = (bpt as i32 + ptr_step) as u32;
                agnus.blt_bdat = w;
                w
            } else {
                agnus.blt_bdat
            };
            let c_val = if use_c {
                let w = read_word(&*memory, cpt);
                cpt = (cpt as i32 + ptr_step) as u32;
                agnus.blt_cdat = w;
                w
            } else {
                agnus.blt_cdat
            };

            let mut a_masked = a_raw;
            if col == 0 {
                a_masked &= agnus.blt_afwm;
            }
            if col == width_words - 1 {
                a_masked &= agnus.blt_alwm;
            }

            let a_combined = if desc {
                (u32::from(a_masked) << 16) | u32::from(a_prev)
            } else {
                (u32::from(a_prev) << 16) | u32::from(a_masked)
            };
            let a_shifted = if desc {
                (a_combined >> (16 - a_shift)) as u16
            } else {
                (a_combined >> a_shift) as u16
            };

            let b_combined = if desc {
                (u32::from(b_raw) << 16) | u32::from(b_prev)
            } else {
                (u32::from(b_prev) << 16) | u32::from(b_raw)
            };
            let b_shifted = if desc {
                (b_combined >> (16 - b_shift)) as u16
            } else {
                (b_combined >> b_shift) as u16
            };

            a_prev = a_masked;
            b_prev = b_raw;

            let mut result: u16 = 0;
            for bit in 0..16 {
                let a_bit = (a_shifted >> bit) & 1;
                let b_bit = (b_shifted >> bit) & 1;
                let c_bit = (c_val >> bit) & 1;
                let index = (a_bit << 2) | (b_bit << 1) | c_bit;
                if (lf >> index) & 1 != 0 {
                    result |= 1 << bit;
                }
            }

            if fill_enabled {
                let mut filled: u16 = 0;
                for bit in 0..16u16 {
                    let d_bit = (result >> bit) & 1;
                    fill_carry ^= d_bit;
                    let out = if efe { fill_carry ^ d_bit } else { fill_carry };
                    filled |= out << bit;
                }
                result = filled;
            }

            if use_d {
                memory.write_byte(dpt, (result >> 8) as u8);
                memory.write_byte(dpt | 1, result as u8);
                dpt = (dpt as i32 + ptr_step) as u32;
            }
        }

        let mod_dir: i32 = if desc { -1 } else { 1 };
        if use_a {
            apt = (apt as i32 + i32::from(agnus.blt_amod) * mod_dir) as u32;
        }
        if use_b {
            bpt = (bpt as i32 + i32::from(agnus.blt_bmod) * mod_dir) as u32;
        }
        if use_c {
            cpt = (cpt as i32 + i32::from(agnus.blt_cmod) * mod_dir) as u32;
        }
        if use_d {
            dpt = (dpt as i32 + i32::from(agnus.blt_dmod) * mod_dir) as u32;
        }
    }

    agnus.blt_apt = apt;
    agnus.blt_bpt = bpt;
    agnus.blt_cpt = cpt;
    agnus.blt_dpt = dpt;
    agnus.clear_blitter_scheduler();
    agnus.blitter_busy = false;
    true
}

/// Blitter LINE mode: Bresenham line drawing.
fn execute_blit_line(agnus: &mut Agnus, memory: &mut Memory) {
    let length = ((agnus.bltsize >> 6) & 0x3FF) as u32;
    let length = if length == 0 { 1024 } else { length };

    let ash = (agnus.bltcon0 >> 12) & 0xF;
    let lf = agnus.bltcon0 as u8;
    let use_b = agnus.bltcon0 & 0x0400 != 0;
    let sud = agnus.bltcon1 & 0x0010 != 0;
    let sul = agnus.bltcon1 & 0x0008 != 0;
    let aul = agnus.bltcon1 & 0x0004 != 0;
    let sing = agnus.bltcon1 & 0x0002 != 0;
    let oct_code = ((sud as u8) << 2) | ((sul as u8) << 1) | (aul as u8);
    let octant = match oct_code {
        0b000 => 6,
        0b001 => 1,
        0b010 => 5,
        0b011 => 2,
        0b100 => 7,
        0b101 => 4,
        0b110 => 0,
        0b111 => 3,
        _ => unreachable!(),
    };
    let (major_is_y, x_neg, y_neg) = match octant {
        0 => (false, false, false),
        1 => (true, false, false),
        2 => (true, true, false),
        3 => (false, true, false),
        4 => (false, true, true),
        5 => (true, true, true),
        6 => (true, false, true),
        7 => (false, false, true),
        _ => unreachable!(),
    };

    let mut error = agnus.blt_apt as i16;
    let error_add = agnus.blt_bmod;
    let error_sub = agnus.blt_amod;

    let mut cpt = agnus.blt_cpt;
    let mut dpt = agnus.blt_dpt;
    let mut pixel_bit = ash;
    let row_mod = agnus.blt_cmod;
    let mut texture = agnus.blt_bdat;

    let read_word = |mem: &Memory, addr: u32| -> u16 {
        let a = (addr & 0x1F_FFFE) as usize;
        if a + 1 < mem.chip_ram.len() {
            (u16::from(mem.chip_ram[a]) << 8) | u16::from(mem.chip_ram[a + 1])
        } else {
            0
        }
    };
    let write_word = |mem: &mut Memory, addr: u32, val: u16| {
        let a = (addr & 0x1F_FFFE) as usize;
        if a + 1 < mem.chip_ram.len() {
            mem.chip_ram[a] = (val >> 8) as u8;
            mem.chip_ram[a + 1] = val as u8;
        }
    };

    for _step in 0..length {
        let pixel_mask: u16 = 0x8000 >> pixel_bit;
        let a_val = pixel_mask;
        let b_val = if use_b {
            if texture & 0x8000 != 0 {
                0xFFFF
            } else {
                0x0000
            }
        } else {
            0xFFFF
        };

        let c_val = read_word(&*memory, cpt);
        agnus.blt_cdat = c_val;

        let mut result: u16 = 0;
        for bit in 0..16u16 {
            let a_bit = (a_val >> bit) & 1;
            let b_bit = (b_val >> bit) & 1;
            let c_bit = (c_val >> bit) & 1;
            let index = (a_bit << 2) | (b_bit << 1) | c_bit;
            if (lf >> index) & 1 != 0 {
                result |= 1 << bit;
            }
        }

        if sing {
            result = (result & pixel_mask) | (c_val & !pixel_mask);
        }

        write_word(memory, dpt, result);

        if use_b {
            texture = texture.rotate_left(1);
        }

        let step_x = |cpt: &mut u32, dpt: &mut u32, pixel_bit: &mut u16| {
            if x_neg {
                *pixel_bit = pixel_bit.wrapping_sub(1) & 0xF;
                if *pixel_bit == 15 {
                    *cpt = cpt.wrapping_sub(2);
                    *dpt = dpt.wrapping_sub(2);
                }
            } else {
                *pixel_bit = (*pixel_bit + 1) & 0xF;
                if *pixel_bit == 0 {
                    *cpt = cpt.wrapping_add(2);
                    *dpt = dpt.wrapping_add(2);
                }
            }
        };
        let step_y = |cpt: &mut u32, dpt: &mut u32| {
            if y_neg {
                *cpt = (*cpt as i32 + row_mod as i32) as u32;
                *dpt = (*dpt as i32 + row_mod as i32) as u32;
            } else {
                *cpt = (*cpt as i32 - row_mod as i32) as u32;
                *dpt = (*dpt as i32 - row_mod as i32) as u32;
            }
        };

        if error >= 0 {
            if major_is_y {
                step_y(&mut cpt, &mut dpt);
                step_x(&mut cpt, &mut dpt, &mut pixel_bit);
            } else {
                step_x(&mut cpt, &mut dpt, &mut pixel_bit);
                step_y(&mut cpt, &mut dpt);
            }
            error = error.wrapping_add(error_sub);
        } else {
            if major_is_y {
                step_y(&mut cpt, &mut dpt);
            } else {
                step_x(&mut cpt, &mut dpt, &mut pixel_bit);
            }
            error = error.wrapping_add(error_add);
        }
    }

    agnus.blt_apt = error as u16 as u32;
    agnus.blt_cpt = cpt;
    agnus.blt_dpt = dpt;
    agnus.blt_bdat = texture;
    agnus.clear_blitter_scheduler();
    agnus.blitter_busy = false;
}

// ─── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(
        clippy::manual_range_contains,
        clippy::needless_range_loop,
        clippy::unwrap_used
    )]

    use super::*;
    use std::path::Path;

    fn dummy_kickstart() -> Vec<u8> {
        // 256 KiB Kickstart ROM. Write initial SSP at $0, PC at $4.
        let mut ks = vec![0u8; 256 * 1024];
        // SSP = $080000 (top of 512K chip RAM)
        ks[0] = 0x00;
        ks[1] = 0x08;
        ks[2] = 0x00;
        ks[3] = 0x00;
        // PC = $F80008 (first instruction in ROM after vectors)
        ks[4] = 0x00;
        ks[5] = 0xF8;
        ks[6] = 0x00;
        ks[7] = 0x08;
        // Write BRA.S * ($60FE) at the entry point ($F80008 → offset $8).
        ks[8] = 0x60;
        ks[9] = 0xFE;
        ks
    }

    fn make_bootable_adf() -> Adf {
        let mut data = vec![0u8; format_commodore_amiga_adf::ADF_SIZE_DD];

        // DOS\0 header
        data[0] = b'D';
        data[1] = b'O';
        data[2] = b'S';
        data[3] = 0;

        // Standard root block pointer.
        let root_block: u32 = 880;
        data[8] = (root_block >> 24) as u8;
        data[9] = (root_block >> 16) as u8;
        data[10] = (root_block >> 8) as u8;
        data[11] = root_block as u8;

        // Boot code:
        //   MOVE.L #$DEADBEEF, ($7FC00).L
        //   MOVEQ  #0, D0
        //   RTS
        let code: &[u8] = &[
            0x23, 0xFC, // MOVE.L #imm, (xxx).L
            0xDE, 0xAD, 0xBE, 0xEF, //   #$DEADBEEF
            0x00, 0x07, 0xFC, 0x00, //   $0007FC00
            0x70, 0x00, // MOVEQ #0, D0
            0x4E, 0x75, // RTS
        ];
        data[12..12 + code.len()].copy_from_slice(code);

        // Bootblock checksum: total sum with carry must equal $FFFF_FFFF.
        let mut sum: u32 = 0;
        for i in 0..256 {
            let offset = i * 4;
            let long = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            let (next, carry) = sum.overflowing_add(long);
            sum = next;
            if carry {
                sum = sum.wrapping_add(1);
            }
        }
        data[4..8].copy_from_slice(&(!sum).to_be_bytes());

        Adf::from_bytes(data).expect("synthetic boot ADF should be valid")
    }

    #[test]
    fn amiga_new_defaults() {
        let amiga = Amiga::new(dummy_kickstart());
        assert_eq!(amiga.memory.chip_ram.len(), 512 * 1024);
        assert!(amiga.memory.overlay);
        assert_eq!(amiga.cpu.regs.sr & 0x2000, 0x2000, "supervisor mode");
    }

    #[test]
    fn overlay_maps_vectors_from_rom() {
        let amiga = Amiga::new(dummy_kickstart());
        // With overlay, address $0 should read from Kickstart.
        assert_eq!(amiga.memory.read_byte(0), 0x00);
        assert_eq!(amiga.memory.read_byte(1), 0x08);
    }

    #[test]
    fn vertb_fires_after_one_frame() {
        let mut amiga = Amiga::new(dummy_kickstart());
        assert_eq!(amiga.vertb_count, 0);
        amiga.run_frame();
        // Should have fired exactly once (at vpos=0, hpos=0).
        assert!(amiga.vertb_count >= 1);
    }

    #[test]
    fn cpu_ticks_every_cck() {
        let mut amiga = Amiga::new(dummy_kickstart());
        // After 1 CCK, CPU gets 2 clock ticks.
        amiga.tick_cck();
        assert!(amiga.cck_count == 1);
    }

    #[test]
    fn cia_ticks_on_eclock() {
        let mut amiga = Amiga::new(dummy_kickstart());
        // CIA-A timer A: set a short period and start it.
        amiga.cia_a.write(4, 10); // TA lo
        amiga.cia_a.write(5, 0); // TA hi
        amiga.cia_a.write(0x0E, 0x01); // CRA: start timer
        let initial = amiga.cia_a.read(4);
        // Tick 5 CCKs = 1 E-clock.
        for _ in 0..5 {
            amiga.tick_cck();
        }
        let after = amiga.cia_a.read(4);
        assert_ne!(initial, after, "CIA timer should have counted down");
    }

    #[test]
    fn custom_reg_write_dmacon() {
        let mut amiga = Amiga::new(dummy_kickstart());
        assert_eq!(amiga.agnus.dmacon, 0);
        amiga.write_custom_reg(0x096, 0x8200); // SET + DMAEN
        assert_eq!(amiga.agnus.dmacon & 0x0200, 0x0200);
    }

    #[test]
    fn custom_reg_write_applies_pipelined_palette_and_bplcon0() {
        let mut amiga = Amiga::new(dummy_kickstart());

        amiga.write_custom_reg(0x100, 0x2302);
        amiga.write_custom_reg(0x180, 0x000F);
        amiga.write_custom_reg(0x182, 0x0FFF);

        assert_eq!(amiga.agnus.bplcon0, 0x2302);
        assert_eq!(amiga.denise.bplcon0, 0);
        assert_eq!(amiga.denise.palette[0], 0);
        assert_eq!(amiga.denise.palette[1], 0);

        amiga.tick_cck();
        assert_eq!(amiga.denise.bplcon0, 0);
        assert_eq!(amiga.denise.palette[0], 0);
        assert_eq!(amiga.denise.palette[1], 0);

        amiga.tick_cck();
        assert_eq!(amiga.denise.bplcon0, 0x2302);
        assert_eq!(amiga.denise.palette[0], 0x000F);
        assert_eq!(amiga.denise.palette[1], 0x0FFF);
    }

    #[test]
    fn custom_reg_read_vhposr() {
        let mut amiga = Amiga::new(dummy_kickstart());
        let vhposr = amiga.read_custom_reg(0x006);
        // At startup, vpos=0, hpos=0.
        assert_eq!(vhposr, 0x0000);
    }

    #[test]
    fn pre_dma_selected_spun_up_disk_surfaces_dskbytr_bytes() {
        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.insert_disk(make_bootable_adf());
        amiga.floppy.acknowledge_disk_change();
        amiga.floppy
            .update_control(false, false, false, true, true);

        for _ in 0..350_000 {
            amiga.floppy.tick();
        }
        assert!(amiga.floppy.motor_spinning());

        let track = amiga
            .floppy
            .encode_mfm_track()
            .expect("selected disk should expose MFM track data");
        let expected_hi = u16::from(track[0]);
        let expected_lo = u16::from(track[1]);

        for _ in 0..u32::from(DISK_STREAM_WORD_CCKS) {
            amiga.tick_cck();
        }

        let first = amiga.read_custom_reg(0x01A);
        assert_ne!(first & 0x8000, 0, "first pre-DMA disk byte should be visible");
        assert_eq!(first & 0x00FF, expected_hi, "expected high byte first");

        let immediate_second = amiga.read_custom_reg(0x01A);
        assert_eq!(
            immediate_second & 0x8000,
            0,
            "second byte should not be visible immediately"
        );

        let mut delayed_second = None;
        for _ in 0..64 {
            amiga.tick_cck();
            let value = amiga.read_custom_reg(0x01A);
            if value & 0x8000 != 0 {
                delayed_second = Some(value);
                break;
            }
        }

        let second = delayed_second.expect("expected delayed low byte to arrive");
        assert_eq!(second & 0x00FF, expected_lo, "expected low byte second");
    }

    #[test]
    #[ignore]
    fn synthetic_bootable_adf_executes_bootblock() {
        let rom_path = Path::new("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom");
        if !rom_path.exists() {
            eprintln!("Skipping: missing Kickstart ROM at {}", rom_path.display());
            return;
        }

        let mut amiga =
            Amiga::new(std::fs::read(rom_path).expect("Kickstart 1.3 ROM should read"));
        amiga.insert_disk(make_bootable_adf());
        amiga.floppy.acknowledge_disk_change();

        let marker_addr = 0x0007_FC00u32;
        let expected = 0xDEAD_BEEFu32;
        let mut executed = false;

        // Roughly 10-12 seconds of PAL time, matching the older ADF boot diag.
        for _ in 0..600 {
            amiga.run_frame();
            let observed = (u32::from(amiga.memory.read_chip_byte(marker_addr)) << 24)
                | (u32::from(amiga.memory.read_chip_byte(marker_addr + 1)) << 16)
                | (u32::from(amiga.memory.read_chip_byte(marker_addr + 2)) << 8)
                | u32::from(amiga.memory.read_chip_byte(marker_addr + 3));
            if observed == expected {
                executed = true;
                break;
            }
        }

        assert!(
            executed,
            "synthetic bootblock never executed; PC=${:08X} cyl={} ready={} motor_on={} motor_spinning={} DSKLEN=${:04X} DSKPT=${:08X} DSKSYNC=${:04X} DSKDATR=${:04X}",
            amiga.cpu.instr_start_pc,
            amiga.floppy.cylinder(),
            amiga.floppy.status().ready,
            amiga.floppy.motor_on(),
            amiga.floppy.motor_spinning(),
            amiga.paula.dsklen,
            amiga.agnus.dsk_pt,
            amiga.paula.dsksync,
            amiga.paula.dskdatr,
        );
    }

    #[test]
    fn intreq_routes_to_ipl() {
        let mut amiga = Amiga::new(dummy_kickstart());
        // Enable VERTB interrupt.
        amiga.paula.write_intena(0xC020); // SET + INTEN + VERTB
        amiga.paula.request_interrupt(5); // VERTB
        let ipl = amiga.paula.compute_ipl();
        assert_eq!(ipl, 3, "VERTB should be IPL 3");
    }

    #[test]
    fn audio_buffer_fills_after_frame() {
        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.run_frame();
        let audio = amiga.take_audio_buffer();
        // At 48 kHz, ~20ms PAL frame ≈ 960 stereo samples = 1920 f32s.
        assert!(audio.len() > 1000, "audio buffer should have samples");
    }

    #[test]
    fn framebuffer_has_expected_size() {
        let amiga = Amiga::new(dummy_kickstart());
        let (w, h) = amiga.framebuffer_size();
        assert_eq!(w, RASTER_FB_WIDTH);
        assert_eq!(h, PAL_RASTER_FB_HEIGHT);
        assert_eq!(
            amiga.framebuffer().len() as u32,
            w * h,
            "framebuffer length should be width * height"
        );
    }

    /// Instruction-level CPU trace against real Kickstart 1.3.
    /// Dumps PC, opcode, disassembly, and key registers at each
    /// instruction boundary. Output goes to /tmp/amiga_trace.txt.
    #[test]
    #[ignore]
    fn trace_kickstart_boot() {
        use std::io::Write;

        let rom_path = "/Users/stevehill/Projects/Emu198x-archive/roms/kick13.rom";
        let Ok(kickstart) = std::fs::read(rom_path) else {
            eprintln!("Skipping: cannot read {rom_path}");
            return;
        };

        let mut amiga = Amiga::new(kickstart);
        let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
            * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

        let mut out =
            std::io::BufWriter::new(std::fs::File::create("/tmp/amiga_trace.txt").unwrap());

        let mut instr_count = 0u64;
        let mut logged_count = 0u64;
        let max_logged = 10_000u64;
        let mut prev_instr_pc = amiga.cpu.instr_start_pc;
        // Track how many times we've logged each PC — skip after 3
        let mut pc_log_count = std::collections::HashMap::<u32, u32>::new();

        'outer: for _frame in 0..200u32 {
            for _ in 0..ccks_per_frame {
                amiga.tick_cck();

                let cur_pc = amiga.cpu.instr_start_pc;
                if cur_pc != prev_instr_pc {
                    prev_instr_pc = cur_pc;
                    instr_count += 1;

                    let entry = pc_log_count.entry(cur_pc).or_insert(0);
                    *entry += 1;
                    if *entry > 3 {
                        continue;
                    }

                    let read_byte = |addr: u32| -> u8 { amiga.memory.read_byte(addr) };
                    let (dis, _len) = motorola_68000::disasm::disassemble(cur_pc, read_byte);

                    let r = &amiga.cpu.regs;
                    let a7 = if r.sr & 0x2000 != 0 { r.ssp } else { r.usp };
                    writeln!(
                        out,
                        "{instr_count:6} PC={cur_pc:06X} SR={:04X} \
                         D0={:08X} D1={:08X} D2={:08X} D3={:08X} \
                         A0={:08X} A6={:08X} A7={a7:08X}  {dis}",
                        r.sr, r.d[0], r.d[1], r.d[2], r.d[3], r.a[0], r.a[6],
                    )
                    .unwrap();

                    logged_count += 1;
                    if logged_count >= max_logged {
                        break 'outer;
                    }
                }
            }
        }

        out.flush().unwrap();
        eprintln!("Wrote {instr_count} instructions to /tmp/amiga_trace.txt");
    }

    /// COP2 chase: trace the moment a stray COPJMP2 sends the Copper
    /// into garbage memory, by watching graphics.library state.
    ///
    /// Watches:
    ///   - ExecBase (at $00000004) → AddLibrary JMP at ExecBase-$18C.
    ///   - AddLibrary entry with A1=lib_base; locks GfxBase when
    ///     the added library's ln_Name is "graphics.library".
    ///   - graphics.library LVOs (MrgCop -$D2, MakeVPort -$D8,
    ///     LoadView -$DE, InitView -$168) — logs each call with A0/A1.
    ///   - gb_ActiView (+0x22), gb_copinit (+0x26), gb_LOFlist (+0x32),
    ///     gb_SHFlist (+0x36). The list pointers are *direct* copper
    ///     instruction stream pointers (not cprlist struct pointers);
    ///     the graphics VBlank handler at $FC6D6C reads gb_LOFlist and
    ///     writes it straight to COP2LC.
    ///   - COP1LC / COP2LC via amiga.copper.cop{1,2}lc.
    ///   - DMACON.COPEN (bit 7) transitions, with current list state.
    ///   - Copper PC transitions that become equal to cop1lc or cop2lc
    ///     (= effect of COPJMP1/2 strobes). On each COP2LC strobe,
    ///     dumps 32 words at the target and classifies against the
    ///     gb_LOFlist/SHFlist/copinit pointers.
    ///   - Dumps GfxBase[0..0x60] once, right after graphics.library
    ///     is locked, to verify struct offsets and see raw field layout.
    ///
    /// Output: /tmp/cop2_chase.txt.
    #[test]
    #[ignore]
    fn trace_cop2_chase() {
        use std::io::Write;

        let rom_path = "/Users/stevehill/Projects/Emu198x-archive/roms/kick13.rom";
        let Ok(kickstart) = std::fs::read(rom_path) else {
            eprintln!("Skipping: cannot read {rom_path}");
            return;
        };

        let mut amiga = Amiga::new(kickstart);
        let ccks_per_line = u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);
        let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame) * ccks_per_line;
        let max_ticks = 200 * ccks_per_frame;

        let mut out =
            std::io::BufWriter::new(std::fs::File::create("/tmp/cop2_chase.txt").unwrap());

        // Composed reads (memory exposes read_byte/read_word only).
        let read_w = |mem: &memory::Memory, a: u32| mem.read_word(a);
        let read_l = |mem: &memory::Memory, a: u32| {
            (u32::from(mem.read_word(a)) << 16) | u32::from(mem.read_word(a.wrapping_add(2)))
        };
        let read_cstr = |mem: &memory::Memory, mut a: u32, max: usize| -> String {
            let mut s = String::new();
            for _ in 0..max {
                let b = mem.read_byte(a);
                if b == 0 {
                    break;
                }
                if b.is_ascii_graphic() || b == b' ' {
                    s.push(b as char);
                } else {
                    s.push('?');
                }
                a = a.wrapping_add(1);
            }
            s
        };

        let mut exec_base: u32 = 0;
        let mut add_library_entry: u32 = 0;
        let mut gfx_base: u32 = 0;
        let mut lvo_mrgcop: u32 = 0;
        let mut lvo_makevport: u32 = 0;
        let mut lvo_loadview: u32 = 0;
        let mut lvo_initview: u32 = 0;

        let mut prev_cop1lc: u32 = 0;
        let mut prev_cop2lc: u32 = 0;
        let mut prev_actiview: u32 = 0;
        let mut prev_copinit: u32 = 0;
        let mut prev_loflist: u32 = 0;
        let mut prev_shflist: u32 = 0;
        let mut prev_copen: bool = false;
        let mut prev_copper_pc: u32 = amiga.copper.pc;
        let mut prev_instr_pc: u32 = amiga.cpu.instr_start_pc;

        let mut copjmp2_events: u32 = 0;
        let max_copjmp2_events: u32 = 25;

        let dump_words = |out: &mut std::io::BufWriter<std::fs::File>,
                          mem: &memory::Memory,
                          base: u32,
                          n: u32| {
            for i in 0..n {
                if i % 8 == 0 {
                    write!(out, "\n       ${:06X}:", base.wrapping_add(i * 2)).unwrap();
                }
                write!(out, " {:04X}", mem.read_word(base.wrapping_add(i * 2))).unwrap();
            }
            writeln!(out).unwrap();
        };

        // Word-level watcher for chip RAM $002360-$003500 (2248 words,
        // 4496 bytes) — covers both the copper-list region AND the
        // addresses where the Copper is reading MOVE-COP1LC patterns
        // from ($002CE6, $002F96, $003282).
        // Activated once gb_LOFlist is first observed to be set.
        const WATCH_BASE: u32 = 0x002360;
        const WATCH_WORDS: usize = 2248;
        let mut watch_snapshot: [u16; WATCH_WORDS] = [0; WATCH_WORDS];
        let mut watch_active: bool = false;
        let mut watch_deadline_tick: u64 = 0;
        let mut watch_change_count: u32 = 0;
        let max_watch_changes: u32 = 3000;

        for tick in 0..max_ticks {
            // Capture PC BEFORE ticking, so if a COP2LC write happens during
            // this cck the logged PC is the instruction that caused it —
            // not whatever the CPU has advanced to by the time we poll.
            let pc_before_tick = amiga.cpu.instr_start_pc;
            amiga.tick_cck();

            let frame = (tick / ccks_per_frame) as u32;
            let line = ((tick % ccks_per_frame) / ccks_per_line) as u32;
            let hpos = (tick % ccks_per_line) as u32;
            let stamp = format!("[F{frame:03} L{line:03} H{hpos:03}]");

            // 1. ExecBase discovery
            if exec_base == 0 {
                let candidate = read_l(&amiga.memory, 0x4);
                if candidate != 0
                    && candidate >= 0x400
                    && candidate < 0x10_0000
                    && (candidate & 1) == 0
                {
                    exec_base = candidate;
                    writeln!(out, "{stamp} ExecBase = ${exec_base:06X}").unwrap();
                    let jmp_at = exec_base.wrapping_sub(0x18C);
                    if read_w(&amiga.memory, jmp_at) == 0x4EF9 {
                        add_library_entry = read_l(&amiga.memory, jmp_at.wrapping_add(2));
                        writeln!(out, "    AddLibrary entry @ ${add_library_entry:06X}").unwrap();
                    } else {
                        writeln!(
                            out,
                            "    (AddLibrary JMP not yet installed at ${jmp_at:06X})"
                        )
                        .unwrap();
                    }
                }
            }

            // Refresh AddLibrary entry if ExecBase known but JMP wasn't installed yet.
            if exec_base != 0 && add_library_entry == 0 {
                let jmp_at = exec_base.wrapping_sub(0x18C);
                if read_w(&amiga.memory, jmp_at) == 0x4EF9 {
                    add_library_entry = read_l(&amiga.memory, jmp_at.wrapping_add(2));
                    writeln!(
                        out,
                        "{stamp} AddLibrary entry resolved @ ${add_library_entry:06X}"
                    )
                    .unwrap();
                }
            }

            // Instruction-boundary logic
            let cur_pc = amiga.cpu.instr_start_pc;
            if cur_pc != prev_instr_pc {
                prev_instr_pc = cur_pc;

                // 2. AddLibrary invocation
                if add_library_entry != 0 && cur_pc == add_library_entry {
                    let lib_base = amiga.cpu.regs.a[1];
                    let name_ptr = read_l(&amiga.memory, lib_base.wrapping_add(0x0A));
                    let name = if name_ptr != 0 && name_ptr < 0x100_0000 {
                        read_cstr(&amiga.memory, name_ptr, 32)
                    } else {
                        String::from("<invalid>")
                    };
                    writeln!(
                        out,
                        "{stamp} AddLibrary: lib=${lib_base:06X} name=\"{name}\""
                    )
                    .unwrap();
                    if name == "graphics.library" && gfx_base == 0 {
                        gfx_base = lib_base;
                        writeln!(out, "    >>> GfxBase locked = ${gfx_base:06X}").unwrap();
                        for (fname, lvo, slot) in [
                            ("MrgCop", 0xD2u32, &mut lvo_mrgcop),
                            ("MakeVPort", 0xD8, &mut lvo_makevport),
                            ("LoadView", 0xDE, &mut lvo_loadview),
                            ("InitView", 0x168, &mut lvo_initview),
                        ] {
                            let jmp_at = gfx_base.wrapping_sub(lvo);
                            let op = read_w(&amiga.memory, jmp_at);
                            if op == 0x4EF9 {
                                *slot = read_l(&amiga.memory, jmp_at.wrapping_add(2));
                                writeln!(out, "    {fname} (-${lvo:03X}) entry @ ${:06X}", *slot)
                                    .unwrap();
                            } else {
                                writeln!(out, "    {fname} (-${lvo:03X}) JMP not 0x4EF9 at ${jmp_at:06X} (got ${op:04X})").unwrap();
                            }
                        }
                        // One-shot dump of GfxBase[0..0x60] to verify offsets.
                        writeln!(out, "    GfxBase dump (first $60 bytes):").unwrap();
                        dump_words(&mut out, &amiga.memory, gfx_base, 0x30);
                        // And dump the expected named fields explicitly:
                        let actiview = read_l(&amiga.memory, gfx_base.wrapping_add(0x22));
                        let copinit = read_l(&amiga.memory, gfx_base.wrapping_add(0x26));
                        let cia = read_l(&amiga.memory, gfx_base.wrapping_add(0x2A));
                        let blitter = read_l(&amiga.memory, gfx_base.wrapping_add(0x2E));
                        let loflist = read_l(&amiga.memory, gfx_base.wrapping_add(0x32));
                        let shflist = read_l(&amiga.memory, gfx_base.wrapping_add(0x36));
                        writeln!(out, "    gb_ActiView (+0x22) = ${actiview:08X}").unwrap();
                        writeln!(out, "    gb_copinit  (+0x26) = ${copinit:08X}").unwrap();
                        writeln!(out, "    gb_cia      (+0x2A) = ${cia:08X}").unwrap();
                        writeln!(out, "    gb_blitter  (+0x2E) = ${blitter:08X}").unwrap();
                        writeln!(out, "    gb_LOFlist  (+0x32) = ${loflist:08X}").unwrap();
                        writeln!(out, "    gb_SHFlist  (+0x36) = ${shflist:08X}").unwrap();
                    }
                }

                // 3. Graphics LVO entry hits
                for (fname, target) in [
                    ("MrgCop", lvo_mrgcop),
                    ("MakeVPort", lvo_makevport),
                    ("LoadView", lvo_loadview),
                    ("InitView", lvo_initview),
                ] {
                    if target != 0 && cur_pc == target {
                        let r = &amiga.cpu.regs;
                        let a7 = if r.sr & 0x2000 != 0 { r.ssp } else { r.usp };
                        let ret_addr = read_l(&amiga.memory, a7);
                        writeln!(
                            out,
                            "{stamp} gfx.{fname}: A0=${:06X} A1=${:06X} ret=${ret_addr:06X}",
                            r.a[0] & 0xFF_FFFF,
                            r.a[1] & 0xFF_FFFF,
                        )
                        .unwrap();
                    }
                }
            }

            // 4a. COP1LC / COP2LC changes
            let cop1lc = amiga.copper.cop1lc;
            let cop2lc = amiga.copper.cop2lc;
            if cop1lc != prev_cop1lc {
                let r = &amiga.cpu.regs;
                let in_isr = r.sr & 0x2000 != 0;
                writeln!(out,
                    "{stamp} COP1LC: ${prev_cop1lc:06X} -> ${cop1lc:06X}  (write-PC=${pc_before_tick:06X} {})",
                    if in_isr { "SV" } else { "USR" },
                ).unwrap();
                prev_cop1lc = cop1lc;
            }
            if cop2lc != prev_cop2lc {
                let r = &amiga.cpu.regs;
                let in_isr = r.sr & 0x2000 != 0;
                writeln!(out,
                    "{stamp} COP2LC: ${prev_cop2lc:06X} -> ${cop2lc:06X}  (write-PC=${pc_before_tick:06X} {}, A0=${:06X} A1=${:06X} D0=${:08X})",
                    if in_isr { "SV" } else { "USR" },
                    r.a[0] & 0xFF_FFFF,
                    r.a[1] & 0xFF_FFFF,
                    r.d[0],
                ).unwrap();
                prev_cop2lc = cop2lc;
            }

            // 4b. GfxBase direct copper-list pointers. These are RAW copper
            // instruction stream pointers (see graphics VBlank handler at
            // $FC6D6C: MOVE.L $32(A1),D0 / MOVE.L D0,$84(A0) — writes
            // gb_LOFlist directly into COP2LC, no cprlist dereference).
            if gfx_base != 0 {
                let actiview = read_l(&amiga.memory, gfx_base.wrapping_add(0x22));
                let copinit = read_l(&amiga.memory, gfx_base.wrapping_add(0x26));
                let loflist = read_l(&amiga.memory, gfx_base.wrapping_add(0x32));
                let shflist = read_l(&amiga.memory, gfx_base.wrapping_add(0x36));
                if actiview != prev_actiview {
                    writeln!(
                        out,
                        "{stamp} gb_ActiView: ${prev_actiview:08X} -> ${actiview:08X}"
                    )
                    .unwrap();
                    prev_actiview = actiview;
                }
                if copinit != prev_copinit {
                    writeln!(
                        out,
                        "{stamp} gb_copinit:  ${prev_copinit:08X} -> ${copinit:08X}"
                    )
                    .unwrap();
                    if copinit != 0 && copinit < 0x20_0000 {
                        writeln!(out, "    copinit contents (46 words, 92 bytes):").unwrap();
                        dump_words(&mut out, &amiga.memory, copinit, 46);
                    }
                    // Activate the chip-RAM watcher as early as possible —
                    // once gb_copinit is first set, before either copper list
                    // has been populated. This captures writes to BOTH
                    // the COP1 list (at gb_copinit = $002368) and the COP2
                    // list (at gb_LOFlist, which comes later).
                    if !watch_active && prev_copinit == 0 && copinit != 0 {
                        for i in 0..WATCH_WORDS {
                            watch_snapshot[i] = amiga
                                .memory
                                .read_word(WATCH_BASE.wrapping_add((i as u32) * 2));
                        }
                        watch_active = true;
                        watch_deadline_tick = tick + 30 * ccks_per_frame;
                        writeln!(out,
                            "    chip-RAM watcher ACTIVE (on gb_copinit set) on ${WATCH_BASE:06X}..${:06X} (until tick {watch_deadline_tick})",
                            WATCH_BASE + (WATCH_WORDS as u32) * 2,
                        ).unwrap();
                    }
                    prev_copinit = copinit;
                }
                if loflist != prev_loflist {
                    writeln!(
                        out,
                        "{stamp} gb_LOFlist:  ${prev_loflist:08X} -> ${loflist:08X}"
                    )
                    .unwrap();
                    if loflist != 0 && loflist < 0x20_0000 {
                        writeln!(out, "    loflist first 16 words:").unwrap();
                        dump_words(&mut out, &amiga.memory, loflist, 16);
                    }
                    // Activate the chip-RAM watcher on the first gb_LOFlist
                    // transition from zero; snapshot the current state so we
                    // log only subsequent changes.
                    if !watch_active && prev_loflist == 0 && loflist != 0 {
                        for i in 0..WATCH_WORDS {
                            watch_snapshot[i] = amiga
                                .memory
                                .read_word(WATCH_BASE.wrapping_add((i as u32) * 2));
                        }
                        watch_active = true;
                        // Watch for ~10 frames worth of ccks, enough to
                        // capture late writes (end markers, late init).
                        watch_deadline_tick = tick + 10 * ccks_per_frame;
                        writeln!(out,
                            "    chip-RAM watcher ACTIVE on ${WATCH_BASE:06X}..${:06X} (until tick {watch_deadline_tick})",
                            WATCH_BASE + (WATCH_WORDS as u32) * 2,
                        ).unwrap();
                    }
                    prev_loflist = loflist;
                }
                if shflist != prev_shflist {
                    writeln!(
                        out,
                        "{stamp} gb_SHFlist:  ${prev_shflist:08X} -> ${shflist:08X}"
                    )
                    .unwrap();
                    if shflist != 0 && shflist < 0x20_0000 {
                        writeln!(out, "    shflist first 16 words:").unwrap();
                        dump_words(&mut out, &amiga.memory, shflist, 16);
                    }
                    prev_shflist = shflist;
                }
            }

            // 4b2. Chip-RAM word watcher (only while active, capped).
            if watch_active && watch_change_count < max_watch_changes && tick <= watch_deadline_tick
            {
                for i in 0..WATCH_WORDS {
                    let addr = WATCH_BASE.wrapping_add((i as u32) * 2);
                    let now = amiga.memory.read_word(addr);
                    if now != watch_snapshot[i] {
                        let r = &amiga.cpu.regs;
                        let a7 = if r.sr & 0x2000 != 0 { r.ssp } else { r.usp };
                        writeln!(out,
                            "{stamp} WATCH ${addr:06X}: {:04X} -> {:04X}  (PC=${cur_pc:06X} A7=${a7:08X} D0=${:08X} D1=${:08X} A0=${:08X} A1=${:08X})",
                            watch_snapshot[i], now,
                            r.d[0], r.d[1],
                            r.a[0] & 0xFF_FFFF,
                            r.a[1] & 0xFF_FFFF,
                        ).unwrap();
                        watch_snapshot[i] = now;
                        watch_change_count += 1;
                        if watch_change_count >= max_watch_changes {
                            writeln!(
                                out,
                                "    (watch change cap {max_watch_changes} reached — deactivating)"
                            )
                            .unwrap();
                            watch_active = false;
                            break;
                        }
                    }
                }
                if tick > watch_deadline_tick {
                    writeln!(out, "{stamp} WATCH deadline reached — deactivating").unwrap();
                    watch_active = false;
                }
            }

            // 4c. DMACON:COPEN (bit 7 = $0080) transitions.
            let dmacon = amiga.agnus.dmacon;
            let copen_now = dmacon & 0x0080 != 0;
            if copen_now != prev_copen {
                let co = if gfx_base != 0 {
                    read_l(&amiga.memory, gfx_base.wrapping_add(0x26))
                } else {
                    0
                };
                writeln!(out,
                    "{stamp} DMACON.COPEN: {} -> {}  (DMACON=${dmacon:04X}, COP1LC=${:06X}, COP2LC=${:06X}, gb_copinit=${co:08X}, CPU PC=${cur_pc:06X})",
                    if prev_copen { "ON" } else { "OFF" },
                    if copen_now { "ON" } else { "OFF" },
                    amiga.copper.cop1lc,
                    amiga.copper.cop2lc,
                ).unwrap();
                prev_copen = copen_now;
            }

            // 5. Copper jump detection (COPJMP effects on copper.pc)
            let copper_pc = amiga.copper.pc;
            if copper_pc != prev_copper_pc {
                let sequential = copper_pc == prev_copper_pc.wrapping_add(2)
                    || copper_pc == prev_copper_pc.wrapping_add(4);
                if !sequential {
                    let to_cop1 = copper_pc == cop1lc;
                    let to_cop2 = copper_pc == cop2lc;
                    if to_cop2 && copjmp2_events < max_copjmp2_events {
                        copjmp2_events += 1;
                        writeln!(out,
                            "{stamp} *** COPJMP2 effect: copper PC ${prev_copper_pc:06X} -> ${copper_pc:06X} (=COP2LC) ***"
                        ).unwrap();
                        write!(out, "    Words at target (64 words, 128 bytes):").unwrap();
                        let mut found_end = false;
                        for i in 0..64u32 {
                            let w = read_w(&amiga.memory, copper_pc.wrapping_add(i * 2));
                            if i % 8 == 0 {
                                write!(out, "\n       ${:06X}:", copper_pc.wrapping_add(i * 2))
                                    .unwrap();
                            }
                            write!(out, " {w:04X}").unwrap();
                            // Look for a WAIT($FFFF,$FFFE) end marker.
                            if i > 0 && (i & 1) == 1 {
                                let prev =
                                    read_w(&amiga.memory, copper_pc.wrapping_add((i - 1) * 2));
                                if prev == 0xFFFF && w == 0xFFFE {
                                    found_end = true;
                                }
                            }
                        }
                        writeln!(out).unwrap();
                        writeln!(
                            out,
                            "    end-marker WAIT($FFFF,$FFFE) in first 64 words: {}",
                            if found_end { "FOUND" } else { "NOT FOUND" },
                        )
                        .unwrap();
                        if gfx_base != 0 {
                            let lof = read_l(&amiga.memory, gfx_base.wrapping_add(0x32));
                            let shf = read_l(&amiga.memory, gfx_base.wrapping_add(0x36));
                            let ci = read_l(&amiga.memory, gfx_base.wrapping_add(0x26));
                            let classification = if copper_pc == lof && lof != 0 {
                                "matches gb_LOFlist (normal)"
                            } else if copper_pc == shf && shf != 0 {
                                "matches gb_SHFlist (interlace short frame)"
                            } else if copper_pc == ci && ci != 0 {
                                "matches gb_copinit (boot power-on list)"
                            } else {
                                "!!! MATCHES NONE of gb_LOFlist/SHFlist/copinit !!!"
                            };
                            writeln!(out, "    classification: {classification}").unwrap();
                            writeln!(out,
                                "    gb_LOFlist=${lof:08X}  gb_SHFlist=${shf:08X}  gb_copinit=${ci:08X}"
                            ).unwrap();
                        } else {
                            writeln!(out, "    classification: GfxBase not yet known").unwrap();
                        }
                    } else if to_cop1 {
                        // Don't spam COP1 jumps — just record first few as sanity.
                        if copjmp2_events == 0 && tick < ccks_per_frame * 50 {
                            writeln!(out, "{stamp} copper -> COP1LC (${copper_pc:06X})").unwrap();
                        }
                    }
                }
                prev_copper_pc = copper_pc;
            }

            if copjmp2_events >= max_copjmp2_events {
                writeln!(
                    out,
                    "{stamp} reached max COPJMP2 events ({max_copjmp2_events}), stopping"
                )
                .unwrap();
                break;
            }
        }

        out.flush().unwrap();
        eprintln!("trace_cop2_chase: wrote /tmp/cop2_chase.txt ({copjmp2_events} COPJMP2 events)");
    }

    #[test]
    #[ignore]
    fn copinit_alloc_size() {
        let rom_path = "/Users/stevehill/Projects/Emu198x-archive/roms/kick13.rom";
        let Ok(kickstart) = std::fs::read(rom_path) else {
            eprintln!("Skipping: cannot read {rom_path}");
            return;
        };
        let mut amiga = Amiga::new(kickstart);
        let ccks_per_line = u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);
        let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame) * ccks_per_line;
        let max_ticks = 100 * ccks_per_frame;

        let read_l = |mem: &memory::Memory, a: u32| {
            (u32::from(mem.read_word(a)) << 16) | u32::from(mem.read_word(a + 2))
        };

        let mut alloc_mem_entry: u32 = 0;
        let mut prev_pc: u32 = 0;
        let mut found_exec = false;

        for tick in 0..max_ticks {
            amiga.tick_cck();

            let frame = tick / ccks_per_frame;

            // Keep refreshing AllocMem entry once ExecBase appears
            if !found_exec || alloc_mem_entry == 0 {
                let eb = read_l(&amiga.memory, 4);
                if eb >= 0x400 && eb < 0x10_0000 && (eb & 1) == 0 {
                    let jmp_at = eb.wrapping_sub(0xC6);
                    if amiga.memory.read_word(jmp_at) == 0x4EF9 {
                        let entry = read_l(&amiga.memory, jmp_at + 2);
                        if entry != alloc_mem_entry {
                            alloc_mem_entry = entry;
                            if !found_exec {
                                eprintln!("ExecBase=${eb:06X} AllocMem entry=${entry:06X}");
                                found_exec = true;
                            }
                        }
                    }
                }
            }

            let cur_pc = amiga.cpu.instr_start_pc;
            if cur_pc == prev_pc {
                continue;
            }
            prev_pc = cur_pc;

            if alloc_mem_entry != 0 && cur_pc == alloc_mem_entry && frame >= 80 {
                let size = amiga.cpu.regs.d[0];
                let attrs = amiga.cpu.regs.d[1];
                let sp = amiga.cpu.regs.ssp;
                let ret = read_l(&amiga.memory, sp);
                eprintln!(
                    "[F{frame:03}] AllocMem size={size} (${size:X}) attrs=${attrs:08X} ret=${ret:06X}"
                );
            }
        }
    }

    /// Diagnostic: boot Kickstart and dump display + chip state.
    #[test]
    #[ignore]
    fn diag_display_state() {
        let rom_path = "/Users/stevehill/Projects/Emu198x-archive/roms/kick13.rom";
        let Ok(kickstart) = std::fs::read(rom_path) else {
            eprintln!("Skipping: cannot read {rom_path}");
            return;
        };

        let mut amiga = Amiga::new(kickstart);

        for frame in 0..200u32 {
            amiga.run_frame();

            if frame == 50 || frame == 99 || frame == 199 {
                eprintln!("=== Frame {frame} ===");
                eprintln!(
                    "CPU: PC=${:06X} SR=${:04X}",
                    amiga.cpu.regs.pc, amiga.cpu.regs.sr
                );
                eprintln!(
                    "DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X}",
                    amiga.agnus.dmacon, amiga.paula.intena, amiga.paula.intreq
                );
                eprintln!(
                    "COP1LC=${:06X} COP2LC=${:06X} CopperPC=${:06X} state={:?}",
                    amiga.copper.cop1lc, amiga.copper.cop2lc, amiga.copper.pc, amiga.copper.state
                );
                eprintln!(
                    "BPLCON0=${:04X} (Denise=${:04X}) BPLCON1=${:04X} BPLCON2=${:04X}",
                    amiga.agnus.bplcon0,
                    amiga.denise.bplcon0,
                    amiga.denise.bplcon1,
                    amiga.denise.bplcon2
                );
                eprintln!(
                    "DIWSTRT=${:04X} DIWSTOP=${:04X} DDFSTRT=${:04X} DDFSTOP=${:04X}",
                    amiga.agnus.diwstrt,
                    amiga.agnus.diwstop,
                    amiga.agnus.ddfstrt,
                    amiga.agnus.ddfstop
                );
                eprintln!(
                    "BPL1MOD={:04X} BPL2MOD={:04X}",
                    amiga.agnus.bpl1mod as u16, amiga.agnus.bpl2mod as u16
                );
                for i in 0..6 {
                    eprintln!("  BPL{}PT=${:06X}", i + 1, amiga.agnus.bpl_pt[i]);
                }
                eprintln!(
                    "COLOR00=${:04X} COLOR01=${:04X} COLOR02=${:04X} COLOR03=${:04X}",
                    amiga.denise.palette[0],
                    amiga.denise.palette[1],
                    amiga.denise.palette[2],
                    amiga.denise.palette[3]
                );
                // Dump first 32 words at chip RAM $0
                eprint!("ChipRAM[0..64]:");
                for i in 0..32u32 {
                    let addr = i * 2;
                    let w = amiga.memory.read_word(addr);
                    if i % 8 == 0 {
                        eprint!("\n  ${addr:04X}:");
                    }
                    eprint!(" {w:04X}");
                }
                eprintln!();
                // Count non-zero pixels in framebuffer
                let fb = amiga.framebuffer();
                let nonzero = fb.iter().filter(|&&p| p != 0 && p != 0xFF000000).count();
                let total = fb.len();
                eprintln!("Framebuffer: {nonzero}/{total} non-black pixels");
                // Sample a few pixels from the middle
                let (w, h) = amiga.framebuffer_size();
                let mid_y = h / 2;
                let mid_x = w / 2;
                for dy in [0i32, -20, 20] {
                    let y = (mid_y as i32 + dy) as u32;
                    let idx = (y * w + mid_x) as usize;
                    if idx < fb.len() {
                        eprintln!("  pixel({mid_x},{y}) = ${:08X}", fb[idx]);
                    }
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn trace_waitio_replymsg_path() {
        use std::io::Write;

        fn read_u32(mem: &memory::Memory, addr: u32) -> u32 {
            (u32::from(mem.read_word(addr)) << 16) | u32::from(mem.read_word(addr.wrapping_add(2)))
        }

        let rom_path = "/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom";
        let Ok(kickstart) = std::fs::read(rom_path) else {
            eprintln!("Skipping: cannot read {rom_path}");
            return;
        };

        let ccks_per_frame = u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE)
            * u64::from(commodore_agnus_ocs::PAL_LINES_PER_FRAME);
        let max_ticks = 1_700 * ccks_per_frame;

        let discover_waitio = |kickstart: &[u8]| -> Option<(u32, u32)> {
            let mut amiga = Amiga::new_with_slow_ram(kickstart.to_vec(), 512 * 1024);
            for _ in 0..max_ticks {
                amiga.tick_cck();
            }
            if amiga.cpu.instr_start_pc != 0xFC0734 {
                return None;
            }
            let ioreq = amiga.cpu.regs.a[1];
            let reply_port = read_u32(&amiga.memory, ioreq.wrapping_add(0x0E));
            Some((ioreq, reply_port))
        };

        let Some((ioreq, reply_port)) = discover_waitio(&kickstart) else {
            panic!("did not end in WaitIO at $FC0734");
        };

        let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);
        let mut out = std::io::BufWriter::new(
            std::fs::File::create("/tmp/amiga_waitio_replymsg_trace.txt").unwrap(),
        );

        writeln!(
            out,
            "watching ioreq=${ioreq:08X} reply_port=${reply_port:08X}"
        )
        .unwrap();

        let mut prev_instr_pc = u32::MAX;
        let mut prev_ln_type = 0xFF;
        let mut prev_reply_ptr = u32::MAX;
        let mut prev_succ = u32::MAX;
        let mut prev_pred = u32::MAX;
        let mut prev_head = u32::MAX;
        let mut prev_tail = u32::MAX;
        let mut prev_tailpred = u32::MAX;

        for tick in 0..max_ticks {
            amiga.tick_cck();

            let pc = amiga.cpu.instr_start_pc;
            let ln_type = amiga.memory.read_byte(ioreq.wrapping_add(0x08));
            let reply_ptr = read_u32(&amiga.memory, ioreq.wrapping_add(0x0E));
            let succ = read_u32(&amiga.memory, ioreq);
            let pred = read_u32(&amiga.memory, ioreq.wrapping_add(0x04));
            let head = read_u32(&amiga.memory, reply_port.wrapping_add(20));
            let tail = read_u32(&amiga.memory, reply_port.wrapping_add(24));
            let tailpred = read_u32(&amiga.memory, reply_port.wrapping_add(28));

            if ln_type != prev_ln_type
                || reply_ptr != prev_reply_ptr
                || succ != prev_succ
                || pred != prev_pred
                || head != prev_head
                || tail != prev_tail
                || tailpred != prev_tailpred
            {
                let (dis, _) =
                    motorola_68000::disasm::disassemble(pc, |addr| amiga.memory.read_byte(addr));
                let sp = amiga.cpu.regs.active_sp();
                let ret = read_u32(&amiga.memory, sp);
                writeln!(
                    out,
                    "[tick {tick:>9}] state pc=${pc:06X} ret=${ret:08X} \
                     A0=${:08X} A1=${:08X} D0=${:08X} D1=${:08X}  {}",
                    amiga.cpu.regs.a[0],
                    amiga.cpu.regs.a[1],
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.d[1],
                    dis,
                )
                .unwrap();
                writeln!(
                    out,
                    "             type={prev_ln_type:02X}->{ln_type:02X} \
                     reply=${prev_reply_ptr:08X}->{reply_ptr:08X} \
                     succ=${prev_succ:08X}->{succ:08X} pred=${prev_pred:08X}->{pred:08X}"
                )
                .unwrap();
                writeln!(
                    out,
                    "             head=${prev_head:08X}->{head:08X} \
                     tail=${prev_tail:08X}->{tail:08X} \
                     tailpred=${prev_tailpred:08X}->{tailpred:08X}"
                )
                .unwrap();

                prev_ln_type = ln_type;
                prev_reply_ptr = reply_ptr;
                prev_succ = succ;
                prev_pred = pred;
                prev_head = head;
                prev_tail = tail;
                prev_tailpred = tailpred;
            }

            if pc != prev_instr_pc
                && ((0xFC1B70..=0xFC1C30).contains(&pc) || pc == 0xFE9E30 || pc == 0xFC0734)
            {
                let (dis, _) =
                    motorola_68000::disasm::disassemble(pc, |addr| amiga.memory.read_byte(addr));
                let sp = amiga.cpu.regs.active_sp();
                let ret = read_u32(&amiga.memory, sp);
                writeln!(
                    out,
                    "[tick {tick:>9}] exec  pc=${pc:06X} ret=${ret:08X} \
                     A0=${:08X} A1=${:08X} D0=${:08X} D1=${:08X} \
                     type={ln_type:02X} head=${head:08X} tailpred=${tailpred:08X}  {}",
                    amiga.cpu.regs.a[0],
                    amiga.cpu.regs.a[1],
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.d[1],
                    dis,
                )
                .unwrap();
                prev_instr_pc = pc;
            }
        }

        out.flush().unwrap();
        eprintln!(
            "trace_waitio_replymsg_path: wrote /tmp/amiga_waitio_replymsg_trace.txt for ioreq=${ioreq:08X} reply_port=${reply_port:08X}"
        );
    }

    #[test]
    #[ignore]
    fn disasm_trackdisk_hotspots() {
        let rom_path = "/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom";
        let Ok(kickstart) = std::fs::read(rom_path) else {
            eprintln!("Skipping: cannot read {rom_path}");
            return;
        };

        let amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);
        let read_byte = |addr: u32| -> u8 { amiga.memory.read_byte(addr) };

        for &base in &[
            0xFC0734u32,
            0xFC0788,
            0xFE9A90,
            0xFE9AA8,
            0xFE9720u32,
            0xFE97BE,
            0xFE9AAC,
            0xFE9C4E,
            0xFE9E30,
            0xFE9E80,
        ] {
            eprintln!("== ${base:06X} ==");
            let mut pc = base;
            for _ in 0..16 {
                let (dis, len) = motorola_68000::disasm::disassemble(pc, read_byte);
                eprintln!("${pc:06X}: {dis}");
                pc = pc.wrapping_add(len as u32);
            }
        }
    }
}
