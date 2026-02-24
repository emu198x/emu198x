//! The "Rock" - A Cycle-Strict Amiga Emulator.
//!
//! Foundation: Crystal-accuracy.
//! Bus Model: Reactive (Request/Acknowledge), not Predictive.
//! CPU Model: Ticks every 4 crystal cycles, polls bus until DTACK.

pub mod bus;
pub mod config;
pub mod memory;

use crate::memory::Memory;
use commodore_agnus_ocs::{Agnus, BlitterDmaOp, Copper};
use commodore_denise_ocs::DeniseOcs;
use commodore_paula_8364::Paula8364;
use drive_amiga_floppy::AmigaFloppyDrive;
use format_adf::Adf;
use mos_cia_8520::Cia8520;
use motorola_68000::cpu::Cpu68000;
use peripheral_amiga_keyboard::AmigaKeyboard;

// Re-export chip crates so tests and downstream users can access types.
pub use crate::config::{AmigaChipset, AmigaConfig, AmigaModel};
pub use commodore_agnus_ecs;
pub use commodore_agnus_ocs;
pub use commodore_denise_ecs;
pub use commodore_denise_ocs;
pub use commodore_paula_8364;
pub use drive_amiga_floppy;
pub use format_adf;
pub use mos_cia_8520;
use motorola_68000::bus::{BusStatus, FunctionCode, M68kBus};
pub use peripheral_amiga_keyboard;

/// Standard Amiga PAL Master Crystal Frequency (Hz)
pub const PAL_CRYSTAL_HZ: u64 = 28_375_160;
/// Standard Amiga NTSC Master Crystal Frequency (Hz)
pub const NTSC_CRYSTAL_HZ: u64 = 28_636_360;

/// Number of crystal ticks per Colour Clock (CCK)
pub const TICKS_PER_CCK: u64 = 8;
/// Number of crystal ticks per CPU Cycle
pub const TICKS_PER_CPU: u64 = 4;
/// Number of crystal ticks per CIA E-clock
pub const TICKS_PER_ECLOCK: u64 = 40;
/// Number of crystal ticks per PAL frame (A500/OCS timing).
pub const PAL_FRAME_TICKS: u64 = (commodore_agnus_ocs::PAL_CCKS_PER_LINE as u64)
    * (commodore_agnus_ocs::PAL_LINES_PER_FRAME as u64)
    * TICKS_PER_CCK;
/// Paula audio sample rate exposed to host runners.
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
const PAL_CCK_HZ: u64 = PAL_CRYSTAL_HZ / TICKS_PER_CCK;

/// Vertical start of visible display (PAL line $2C = 44).
const DISPLAY_VSTART: u16 = 0x2C;

#[derive(Debug, Clone)]
struct DiskDmaRuntime {
    data: Vec<u8>,
    byte_index: usize,
    words_remaining: u32,
    is_write: bool,
    wordsync_enabled: bool,
    wordsync_waiting: bool,
}

pub struct Amiga {
    pub master_clock: u64,
    pub chipset: AmigaChipset,
    pub cpu: Cpu68000,
    pub agnus: Agnus,
    pub memory: Memory,
    pub denise: DeniseOcs,
    pub copper: Copper,
    pub cia_a: Cia8520,
    pub cia_b: Cia8520,
    pub paula: Paula8364,
    pub floppy: AmigaFloppyDrive,
    pub keyboard: AmigaKeyboard,
    audio_sample_phase: u64,
    audio_buffer: Vec<f32>,
    disk_dma_runtime: Option<DiskDmaRuntime>,
    sprite_dma_phase: [u8; 8],
}

impl Amiga {
    pub fn new(kickstart: Vec<u8>) -> Self {
        Self::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ocs,
            kickstart,
        })
    }

    /// Construct a machine instance from a config object.
    ///
    /// Only A500/OCS is implemented today; later models will branch here.
    pub fn new_with_config(config: AmigaConfig) -> Self {
        let AmigaConfig {
            model,
            chipset,
            kickstart,
        } = config;
        match model {
            AmigaModel::A500 => {}
        }

        let agnus = match chipset {
            AmigaChipset::Ocs => {
                commodore_agnus_ecs::AgnusEcs::from_ocs(commodore_agnus_ocs::Agnus::new())
                    .into_inner()
            }
            AmigaChipset::Ecs => commodore_agnus_ecs::AgnusEcs::new().into_inner(),
        };
        let denise = match chipset {
            AmigaChipset::Ocs => {
                commodore_denise_ecs::DeniseEcs::from_ocs(commodore_denise_ocs::DeniseOcs::new())
                    .into_inner()
            }
            AmigaChipset::Ecs => commodore_denise_ecs::DeniseEcs::new().into_inner(),
        };

        let mut cpu = Cpu68000::new();
        let memory = Memory::new(512 * 1024, kickstart);

        // Initial reset vectors come from ROM (overlay is ON at power-on,
        // mapping Kickstart to $000000).
        let ssp = (u32::from(memory.kickstart[0]) << 24)
            | (u32::from(memory.kickstart[1]) << 16)
            | (u32::from(memory.kickstart[2]) << 8)
            | u32::from(memory.kickstart[3]);
        let pc = (u32::from(memory.kickstart[4]) << 24)
            | (u32::from(memory.kickstart[5]) << 16)
            | (u32::from(memory.kickstart[6]) << 8)
            | u32::from(memory.kickstart[7]);

        cpu.reset_to(ssp, pc);

        // CIA-A PRA external inputs (active-low accent signals):
        //   Bit 7: /FIR1 = 1 (joystick fire not pressed)
        //   Bit 6: /FIR0 = 1 (joystick fire not pressed)
        //   Bit 5: /DSKRDY = 1 (drive not ready)
        //   Bit 4: /DSKTRACK0 = 0 (at track 0)
        //   Bit 3: /DSKPROT = 1 (not write protected)
        //   Bit 2: /DSKCHANGE = 0 (disk removed — no disk in drive)
        //   Bits 1,0: LED/OVL outputs, external pull-up = 1,1
        let mut cia_a = Cia8520::new("A");
        cia_a.external_a = 0xEB; // 0b_1110_1011

        Self {
            master_clock: 0,
            chipset,
            cpu,
            agnus,
            memory,
            denise,
            copper: Copper::new(),
            cia_a,
            cia_b: Cia8520::new("B"),
            paula: Paula8364::new(),
            floppy: AmigaFloppyDrive::new(),
            keyboard: AmigaKeyboard::new(),
            audio_sample_phase: 0,
            audio_buffer: Vec::with_capacity((AUDIO_SAMPLE_RATE as usize / 50) * 4),
            disk_dma_runtime: None,
            sprite_dma_phase: [0; 8],
        }
    }

    pub fn tick(&mut self) {
        self.master_clock += 1;

        if self.master_clock % TICKS_PER_CCK == 0 {
            let vpos = self.agnus.vpos;
            let hpos = self.agnus.hpos;

            // VERTB fires at the start of vblank (beam at line 0, start of frame).
            // The check runs before tick_cck(), so vpos/hpos reflect the current
            // beam position. vpos=0, hpos=0 means the beam just wrapped from the
            // end of the previous frame.
            // CIA-B TOD input is HSYNC — pulse once per scanline.
            if hpos == 0 {
                self.cia_b.tod_pulse();
            }

            if vpos == 0 && hpos == 0 {
                // bit 5 = VERTB
                self.paula.request_interrupt(5);
                // Agnus restarts the copper from COP1LC at vertical blank,
                // but only when copper DMA is enabled (DMAEN + COPEN).
                if self.agnus.dma_enabled(0x0080) {
                    self.copper.restart_cop1();
                }
                // CIA-A TOD input is VSYNC — pulse once per frame.
                self.cia_a.tod_pulse();
            }

            // --- Output pixels BEFORE DMA ---
            // This creates the correct pipeline delay: shift registers hold
            // data from the PREVIOUS fetch group. New data loaded this CCK
            // won't appear until the next output.
            if let Some((fb_x, fb_y)) = self.beam_to_fb(vpos, hpos) {
                let beam_x = u32::from(hpos) * 2;
                let beam_y = u32::from(vpos);
                self.denise
                    .output_pixel_with_beam(fb_x, fb_y, beam_x, beam_y);
                self.denise
                    .output_pixel_with_beam(fb_x + 1, fb_y, beam_x + 1, beam_y);
            }

            // --- DMA slots ---
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
            if let Some(plane) = bus_plan.bitplane_dma_fetch_plane {
                let idx = plane as usize;
                let addr = self.agnus.bpl_pt[idx];
                let hi = self.memory.read_chip_byte(addr);
                let lo = self.memory.read_chip_byte(addr | 1);
                let val = (u16::from(hi) << 8) | u16::from(lo);
                self.denise.load_bitplane(idx, val);
                self.agnus.bpl_pt[idx] = addr.wrapping_add(2);
                if plane == 0 {
                    fetched_plane_0 = true;
                }
            } else if bus_plan.copper_dma_slot_granted {
                let copper_used_chip_bus_cell = std::cell::Cell::new(false);
                let res = {
                    let memory = &self.memory;
                    self.copper.tick(vpos, hpos, |addr| {
                        copper_used_chip_bus_cell.set(true);
                        let hi = memory.read_chip_byte(addr);
                        let lo = memory.read_chip_byte(addr | 1);
                        (u16::from(hi) << 8) | u16::from(lo)
                    })
                };
                copper_used_chip_bus = copper_used_chip_bus_cell.get();
                if let Some((reg, val)) = res {
                    // COPCON protection (HRM Ch.2): copper cannot write
                    // registers $000-$03E at all, and $040-$07E only
                    // when the CDANG (danger) bit is set in COPCON.
                    if reg >= 0x080 || (reg >= 0x040 && self.copper.danger) {
                        if reg == 0x09C && (val & 0x0010) != 0 {
                            self.paula.request_interrupt(4);
                        }
                        self.write_custom_reg(reg, val);
                    }
                }
            }
            let audio_return_progress_this_cck =
                bus_plan.paula_return_progress(copper_used_chip_bus);

            // BPL1DAT (plane 0) is always fetched last in each group.
            // Writing it triggers parallel load of all holding latches
            // into the shift registers.
            if fetched_plane_0 {
                self.denise.trigger_shift_load();

                // Apply bitplane modulo after the last fetch group of the line.
                // Plane 0 is at ddfseq position 7, so the group started at hpos-7.
                let group_start = hpos - 7;
                if group_start >= self.agnus.ddfstop {
                    let num_bpl = self.agnus.num_bitplanes();
                    for i in 0..num_bpl as usize {
                        let modulo = if i % 2 == 0 {
                            self.agnus.bpl1mod // Odd planes (BPL1/3/5)
                        } else {
                            self.agnus.bpl2mod // Even planes (BPL2/4/6)
                        };
                        self.agnus.bpl_pt[i] = (self.agnus.bpl_pt[i] as i32 + modulo as i32) as u32;
                    }
                }
            }

            self.paula.tick_audio_cck_with_bus(
                self.agnus.dmacon,
                audio_dma_slot,
                audio_return_progress_this_cck,
                |addr| self.memory.read_chip_byte(addr),
            );
            self.paula.tick_disk_cck();

            // Coarse blitter scheduler: preserve BUSY across CCKs so Agnus bus
            // arbitration (including nasty-mode CPU steals) affects machine
            // timing before the existing synchronous blit implementation runs.
            // Progress now advances only on explicit Agnus free-slot grants.
            if let Some(blit_op) = self
                .agnus
                .tick_blitter_scheduler_op(bus_plan.blitter_dma_progress_granted)
            {
                let incremental_completed =
                    execute_incremental_blitter_op(&mut self.agnus, &mut self.memory, blit_op);
                if incremental_completed {
                    self.agnus.clear_blitter_scheduler();
                    self.agnus.blitter_busy = false;
                    self.paula.request_interrupt(6);
                }
            }
            if self.agnus.blitter_exec_ready() {
                execute_blit(&mut self.agnus, &mut self.paula, &mut self.memory);
            }

            self.audio_sample_phase += u64::from(AUDIO_SAMPLE_RATE);
            while self.audio_sample_phase >= PAL_CCK_HZ {
                self.audio_sample_phase -= PAL_CCK_HZ;
                let (left, right) = self.paula.mix_audio_stereo();
                self.audio_buffer.push(left);
                self.audio_buffer.push(right);
            }

            self.agnus.tick_cck();

            // Check for pending disk DMA after CCK tick
            if self.paula.disk_dma_pending {
                self.paula.disk_dma_pending = false;
                self.start_disk_dma_transfer();
            }
        }

        if self.master_clock % TICKS_PER_CPU == 0 {
            let mut bus = AmigaBusWrapper {
                agnus: &mut self.agnus,
                memory: &mut self.memory,
                denise: &mut self.denise,
                copper: &mut self.copper,
                cia_a: &mut self.cia_a,
                cia_b: &mut self.cia_b,
                paula: &mut self.paula,
                floppy: &mut self.floppy,
                keyboard: &mut self.keyboard,
            };
            self.cpu.tick(&mut bus, self.master_clock);
        }

        if self.master_clock % TICKS_PER_ECLOCK == 0 {
            self.cia_a.tick();
            if self.cia_a.irq_active() {
                self.paula.request_interrupt(3);
            }
            self.cia_b.tick();
            if self.cia_b.irq_active() {
                self.paula.request_interrupt(13);
            }

            // Floppy drive motor spin-up timer
            self.floppy.tick();

            // Update CIA-A PRA with floppy status (active-low signals)
            let status = self.floppy.status();
            let mut ext_a = self.cia_a.external_a;
            // PA2: /DSKCHANGE — 0 when disk changed
            if status.disk_change {
                ext_a &= !0x04;
            } else {
                ext_a |= 0x04;
            }
            // PA3: /DSKPROT — 0 when write-protected
            if status.write_protect {
                ext_a &= !0x08;
            } else {
                ext_a |= 0x08;
            }
            // PA4: /DSKTRACK0 — 0 when at track 0
            if status.track0 {
                ext_a &= !0x10;
            } else {
                ext_a |= 0x10;
            }
            // PA5: /DSKRDY — 0 when motor at speed
            if status.ready {
                ext_a &= !0x20;
            } else {
                ext_a |= 0x20;
            }
            self.cia_a.external_a = ext_a;

            // Keyboard: tick and inject serial byte if ready
            if let Some(byte) = self.keyboard.tick() {
                self.cia_a.receive_serial_byte(byte);
            }
        }
    }

    pub fn write_custom_reg(&mut self, offset: u16, val: u16) {
        if (0x120..=0x13E).contains(&offset) && (offset & 2) != 0 {
            let idx = ((offset - 0x120) / 4) as usize;
            if idx < 8 {
                // Treat the low-word pointer write as the commit point for
                // restarting sprite DMA control-word fetch sequencing.
                self.sprite_dma_phase[idx] = 0;
            }
        }
        if (0x140..=0x17E).contains(&offset) {
            let sprite = ((offset - 0x140) / 8) as usize;
            let reg = ((offset - 0x140) % 8) / 2;
            if sprite < 8 && reg == 1 {
                // HRM: writing SPRxCTL disables the sprite DMA channel until the
                // vertical beam counter matches VSTART again.
                self.sprite_dma_phase[sprite] = 4;
            }
        }
        write_custom_register(
            &mut self.agnus,
            &mut self.denise,
            &mut self.copper,
            &mut self.paula,
            &mut self.memory,
            offset,
            val,
        );
    }

    /// Advance the machine by one PAL video frame (A500/OCS timing).
    pub fn run_frame(&mut self) {
        for _ in 0..PAL_FRAME_TICKS {
            self.tick();
        }
    }

    /// Borrow the current raw ARGB framebuffer (320x256).
    pub fn framebuffer(&self) -> &[u32] {
        &self.denise.framebuffer
    }

    /// Drain interleaved stereo audio samples (`f32`, `L,R,...`).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.audio_buffer)
    }

    /// Insert a disk image into the internal floppy drive (DF0:).
    pub fn insert_disk(&mut self, adf: Adf) {
        self.floppy.insert_disk(adf);
    }

    /// Eject the current DF0: disk, if any.
    pub fn eject_disk(&mut self) {
        self.floppy.eject_disk();
    }

    /// Return whether DF0: currently has a disk inserted.
    pub fn has_disk(&self) -> bool {
        self.floppy.has_disk()
    }

    /// Queue an Amiga keyboard event (raw Amiga keycode).
    pub fn key_event(&mut self, keycode: u8, pressed: bool) {
        self.keyboard.key_event(keycode, pressed);
    }

    /// Inject a keyboard handshake pulse from the host side.
    pub fn keyboard_handshake(&mut self) {
        self.keyboard.handshake();
    }

    /// Start a disk DMA transfer (DSKLEN double-write protocol).
    ///
    /// Data movement is performed incrementally on Agnus disk DMA slots.
    fn start_disk_dma_transfer(&mut self) {
        let word_count = (self.paula.dsklen & 0x3FFF) as u32;
        let is_write = self.paula.dsklen & 0x4000 != 0;

        if word_count == 0 {
            // Keep existing boot-level behavior: completion interrupt even with a
            // zero transfer count.
            self.paula.request_interrupt(1);
            self.disk_dma_runtime = None;
            return;
        }

        let data = if is_write {
            Vec::new()
        } else {
            self.floppy.encode_mfm_track().unwrap_or_default()
        };
        let wordsync_enabled = !is_write && (self.paula.adkcon & 0x0400 != 0);
        self.disk_dma_runtime = Some(DiskDmaRuntime {
            data,
            byte_index: 0,
            words_remaining: word_count,
            is_write,
            wordsync_enabled,
            wordsync_waiting: wordsync_enabled,
        });
    }

    /// Service one Agnus disk DMA slot.
    fn service_disk_dma_slot(&mut self) {
        if self.disk_dma_runtime.is_none() {
            // Simplified programmed-I/O disk write path: consume queued DSKDAT
            // words on disk slots when write mode is selected and no DMA
            // transfer is active.
            if (self.paula.dsklen & 0x4000) != 0
                && let Some(word) = self.paula.take_dskdat_queued_word()
            {
                self.floppy.note_write_mfm_word(word);
                self.paula.note_disk_write_pio_word(word);
            }
            return;
        }

        let Some(runtime) = self.disk_dma_runtime.as_mut() else {
            return;
        };

        if runtime.words_remaining == 0 {
            self.disk_dma_runtime = None;
            return;
        }

        let mut dma_word_completed = false;
        if !runtime.is_write {
            let mut stream_word: Option<(u8, u8, u16)> = None;
            if runtime.data.len() >= 2 {
                let len = runtime.data.len();
                let hi = runtime.data[runtime.byte_index % len];
                let lo = runtime.data[(runtime.byte_index + 1) % len];
                runtime.byte_index = (runtime.byte_index + 2) % len;
                let word = (u16::from(hi) << 8) | u16::from(lo);
                stream_word = Some((hi, lo, word));
            }

            if let Some((hi, lo, word)) = stream_word {
                // Simplified disk path: surface Paula disk read state from the
                // DMA stream even though the full serial disk decoder is not modeled.
                let matched_sync = self.paula.note_disk_read_word(word);
                if matched_sync {
                    self.paula.request_interrupt(12); // DSKSYN
                }

                let suppress_dma_write = if runtime.wordsync_enabled {
                    if runtime.wordsync_waiting {
                        if matched_sync {
                            // HRM: DMA starts with the following word after a DSKSYNC match.
                            runtime.wordsync_waiting = false;
                        }
                        true
                    } else {
                        // HRM: during read DMA, resync every time the match word is found.
                        matched_sync
                    }
                } else {
                    false
                };

                if !suppress_dma_write {
                    let mut addr = self.agnus.dsk_pt;
                    self.memory.write_byte(addr, hi);
                    addr = addr.wrapping_add(1);
                    self.memory.write_byte(addr, lo);
                    addr = addr.wrapping_add(1);
                    self.agnus.dsk_pt = addr;
                    dma_word_completed = true;
                }
            }
        } else {
            let mut addr = self.agnus.dsk_pt;
            let hi = self.memory.read_chip_byte(addr);
            addr = addr.wrapping_add(1);
            let lo = self.memory.read_chip_byte(addr);
            addr = addr.wrapping_add(1);
            let word = (u16::from(hi) << 8) | u16::from(lo);
            self.floppy.note_write_mfm_word(word);
            self.paula.note_disk_write_dma_word(word);
            self.agnus.dsk_pt = addr;
            dma_word_completed = true;
        }

        if dma_word_completed {
            runtime.words_remaining = runtime.words_remaining.saturating_sub(1);
            if runtime.words_remaining == 0 {
                self.disk_dma_runtime = None;
                // DSKBLK interrupt on transfer completion.
                self.paula.request_interrupt(1);
            }
        }
    }

    fn sprite_line_active(vpos: u16, vstart: u16, vstop: u16) -> bool {
        if vstart == vstop {
            return false;
        }
        if vstart < vstop {
            vpos >= vstart && vpos < vstop
        } else {
            // Wrapped sprite: active from VSTART..end_of_frame and 0..VSTOP-1.
            vpos >= vstart || vpos < vstop
        }
    }

    fn next_sprite_dma_vpos(vpos: u16) -> u16 {
        let next = vpos.wrapping_add(1);
        if next >= commodore_agnus_ocs::PAL_LINES_PER_FRAME {
            0
        } else {
            next
        }
    }

    /// Service one Agnus sprite DMA slot.
    ///
    /// Minimal OCS bring-up model: fetch one word from `SPRxPT` and advance a
    /// coarse sprite DMA phase machine:
    /// - first two slots after pointer reload fetch `SPRxPOS` / `SPRxCTL`
    /// - subsequent slots fetch `SPRxDATA` / `SPRxDATB` pairs
    ///
    /// This is still not a full hardware sprite DMA state machine, but it
    /// keeps Denise sprite registers coherent enough for basic rendering.
    fn service_sprite_dma_slot(&mut self, sprite: usize) {
        if sprite >= 8 {
            return;
        }
        let vpos = self.agnus.vpos;
        if self.sprite_dma_phase[sprite] == 4 {
            let pos = self.denise.spr_pos[sprite];
            let ctl = self.denise.spr_ctl[sprite];
            let vstart = (((ctl >> 2) & 0x0001) << 8) | ((pos >> 8) & 0x00FF);
            let vstop = (((ctl >> 1) & 0x0001) << 8) | ((ctl >> 8) & 0x00FF);
            // HRM: sprite DMA remains disabled until the beam equals VSTART.
            if vpos != vstart {
                return;
            }
            // If VSTOP==VSTART, no sprite lines are output; the next fetched
            // word pair becomes the next SPRxPOS/SPRxCTL instead of DATA/DATB.
            self.sprite_dma_phase[sprite] = if vstop == vstart { 0 } else { 2 };
        }

        let addr = self.agnus.spr_pt[sprite];
        let hi = self.memory.read_chip_byte(addr);
        let lo = self.memory.read_chip_byte(addr | 1);
        let word = (u16::from(hi) << 8) | u16::from(lo);
        match self.sprite_dma_phase[sprite] {
            0 => {
                self.denise.write_sprite_pos(sprite, word);
                self.sprite_dma_phase[sprite] = 1;
            }
            1 => {
                self.denise.write_sprite_ctl(sprite, word);
                let pos = self.denise.spr_pos[sprite];
                let ctl = self.denise.spr_ctl[sprite];
                let vstart = (((ctl >> 2) & 0x0001) << 8) | ((pos >> 8) & 0x00FF);
                let vstop = (((ctl >> 1) & 0x0001) << 8) | ((ctl >> 8) & 0x00FF);
                self.sprite_dma_phase[sprite] = if vstop != vstart && vpos == vstart {
                    2
                } else {
                    4
                };
            }
            2 => {
                self.denise.write_sprite_data(sprite, word);
                self.sprite_dma_phase[sprite] = 3;
            }
            _ => {
                self.denise.write_sprite_datb(sprite, word);
                let pos = self.denise.spr_pos[sprite];
                let ctl = self.denise.spr_ctl[sprite];
                let vstart = (((ctl >> 2) & 0x0001) << 8) | ((pos >> 8) & 0x00FF);
                let vstop = (((ctl >> 1) & 0x0001) << 8) | ((ctl >> 8) & 0x00FF);
                let next_vpos = Self::next_sprite_dma_vpos(vpos);
                self.sprite_dma_phase[sprite] =
                    if Self::sprite_line_active(next_vpos, vstart, vstop) {
                        2
                    } else {
                        0
                    };
            }
        }
        self.agnus.spr_pt[sprite] = addr.wrapping_add(2);
    }

    fn beam_to_fb(&self, vpos: u16, hpos_cck: u16) -> Option<(u32, u32)> {
        let fb_y = vpos.wrapping_sub(DISPLAY_VSTART);
        if fb_y >= commodore_denise_ocs::FB_HEIGHT as u16 {
            return None;
        }
        // First bitplane pixel appears 8 CCKs after DDFSTRT (one full
        // 8-CCK fetch group fills all plane latches and triggers load).
        let first_pixel_cck = self.agnus.ddfstrt.wrapping_add(8);
        let cck_offset = hpos_cck.wrapping_sub(first_pixel_cck);
        let fb_x = u32::from(cck_offset) * 2;
        if fb_x + 1 >= commodore_denise_ocs::FB_WIDTH {
            return None;
        }
        Some((fb_x, u32::from(fb_y)))
    }
}

pub struct AmigaBusWrapper<'a> {
    pub agnus: &'a mut Agnus,
    pub memory: &'a mut Memory,
    pub denise: &'a mut DeniseOcs,
    pub copper: &'a mut Copper,
    pub cia_a: &'a mut Cia8520,
    pub cia_b: &'a mut Cia8520,
    pub paula: &'a mut Paula8364,
    pub floppy: &'a mut AmigaFloppyDrive,
    pub keyboard: &'a mut AmigaKeyboard,
}

impl<'a> M68kBus for AmigaBusWrapper<'a> {
    fn poll_ipl(&mut self) -> u8 {
        self.paula.compute_ipl()
    }
    fn poll_interrupt_ack(&mut self, level: u8) -> BusStatus {
        BusStatus::Ready(24 + level as u16)
    }
    fn reset(&mut self) {
        // RESET instruction asserts the hardware reset line for 124 CPU cycles.
        // This resets all peripherals to their power-on state.
        self.cia_a.reset();
        self.cia_b.reset();
        // After CIA-A reset, DDR-A = 0 (all inputs). On the A500, the /OVL
        // pin has a pull-up resistor, so with CIA-A not driving it, overlay
        // defaults to ON — ROM mapped at $0.
        self.memory.overlay = true;
        // Reset custom chip state
        self.paula.reset();
        self.agnus.dmacon = 0;
    }

    fn poll_cycle(
        &mut self,
        addr: u32,
        fc: FunctionCode,
        is_read: bool,
        is_word: bool,
        data: Option<u16>,
    ) -> BusStatus {
        // Amiga uses autovectors for all hardware interrupts.
        // The CPU issues an IACK bus cycle with FC=InterruptAck. On real
        // hardware the address bus carries the level in A1-A3, but the CPU
        // core uses a fixed address ($FFFFFF). Compute the pending level
        // from Paula's IPL state instead.
        if fc == FunctionCode::InterruptAck {
            let level = self.paula.compute_ipl() as u16;
            return BusStatus::Ready(24 + level);
        }

        let addr = addr & 0xFFFFFF;

        // CIA-A ($BFE001, odd bytes, accent on D0-D7)
        // CIA-A is accent wired to the low byte of the data bus.
        // It responds to odd byte accesses AND word accesses (both
        // assert /LDS). A word write to an even CIA-A address still
        // delivers data to CIA-A via D0-D7.
        if (addr & 0xFFF000) == 0xBFE000 {
            let reg = ((addr >> 8) & 0x0F) as u8;
            if is_read {
                if addr & 1 != 0 {
                    return BusStatus::Ready(u16::from(self.cia_a.read(reg)));
                }
                return BusStatus::Ready(0xFF00);
            } else {
                // CIA-A receives data from D0-D7 (low byte) on:
                //   - Byte writes to odd addresses (data in low byte)
                //   - Word writes to any address (both UDS/LDS active)
                let should_write = (addr & 1 != 0) || is_word;
                if should_write {
                    let val = data.unwrap_or(0) as u8; // low byte = D0-D7
                    self.cia_a.write(reg, val);
                    if reg == 0 {
                        let out = self.cia_a.port_a_output();
                        self.memory.overlay = out & 0x01 != 0;
                    }
                    // CRA write with bit 6 set = SP output mode = keyboard handshake
                    if reg == 0x0E && val & 0x40 != 0 {
                        self.keyboard.handshake();
                    }
                }
                return BusStatus::Ready(0);
            }
        }

        // CIA-B ($BFD000, even bytes, accent on D8-D15)
        // CIA-B is wired to the high byte of the data bus (D8-D15).
        // On real hardware, byte writes to even addresses put the byte on D8-D15.
        // However, our CPU always places byte data in the low byte of the data word
        // (bits 0-7), regardless of address alignment. For word writes, D8-D15
        // contains the high byte as expected.
        if (addr & 0xFFF000) == 0xBFD000 {
            let reg = ((addr >> 8) & 0x0F) as u8;
            if is_read {
                if addr & 1 == 0 {
                    return BusStatus::Ready(u16::from(self.cia_b.read(reg)) << 8 | 0x00FF);
                }
                return BusStatus::Ready(0x00FF);
            } else {
                let should_write = (addr & 1 == 0) || is_word;
                if should_write {
                    let val = if is_word {
                        (data.unwrap_or(0) >> 8) as u8 // word: CIA-B data from D8-D15
                    } else {
                        data.unwrap_or(0) as u8 // byte: CPU puts value in low byte
                    };
                    self.cia_b.write(reg, val);
                    // PRB write: update floppy drive control signals
                    if reg == 0x01 {
                        let prb = self.cia_b.port_b_output();
                        // Active-low signals: asserted when bit is 0
                        let step = prb & 0x01 == 0; // PB0: /DSKSTEP
                        let dir_inward = prb & 0x02 == 0; // PB1: /DSKDIREC (0=inward)
                        let side_upper = prb & 0x04 == 0; // PB2: /DSKSIDE (0=upper/head 1)
                        let sel = prb & 0x08 == 0; // PB3: /DSKSEL0
                        let motor = prb & 0x80 == 0; // PB7: /DSKMOTOR
                        self.floppy
                            .update_control(step, dir_inward, side_upper, sel, motor);
                    }
                }
                return BusStatus::Ready(0);
            }
        }

        // Custom Registers ($DFF000)
        if (addr & 0xFFF000) == 0xDFF000 {
            let offset = (addr & 0x1FE) as u16;
            if !is_read {
                let val = data.unwrap_or(0);
                write_custom_register(
                    self.agnus,
                    self.denise,
                    self.copper,
                    self.paula,
                    self.memory,
                    offset,
                    val,
                );
            } else {
                // Custom register read: get the 16-bit value, then for
                // byte reads extract the correct byte. On the 68000 bus,
                // even-address bytes come from D8-D15 (high byte) and
                // odd-address bytes from D0-D7 (low byte). The CPU's
                // ReadByte stores the value as-is, so we must place the
                // relevant byte in the position the CPU expects (low byte).
                let word = match offset {
                    // DMACONR: DMA control (active bits) + blitter busy/zero
                    0x002 => {
                        let busy = if self.agnus.blitter_busy { 0x4000 } else { 0 };
                        self.agnus.dmacon | busy
                    }
                    0x004 => (self.agnus.vpos >> 8) & 1,
                    0x006 => ((self.agnus.vpos & 0xFF) << 8) | (self.agnus.hpos & 0xFF),
                    0x008 => self.paula.dskdatr,
                    0x00A | 0x00C => 0,
                    0x00E => self.denise.read_clxdat(),
                    0x010 => self.paula.adkcon,
                    0x016 => 0xFF00,
                    0x018 => 0x39FF,
                    0x01A => self.paula.read_dskbytr(self.agnus.dmacon),
                    0x01C => self.paula.intena,
                    0x01E => self.paula.intreq,
                    0x0A0..=0x0DA => self.paula.read_audio_register(offset).unwrap_or(0),
                    0x07C => 0xFFFF,
                    _ => 0,
                };
                // For byte reads, extract the correct byte from the word.
                // 68000 bus: even addr → high byte (D8-D15), odd → low byte (D0-D7).
                // The CPU's ReadByte stores the value as u16 and uses the low byte,
                // so we place the relevant byte in bits 7-0.
                if !is_word {
                    let byte = if addr & 1 == 0 {
                        (word >> 8) as u8
                    } else {
                        word as u8
                    };
                    return BusStatus::Ready(u16::from(byte));
                }
                return BusStatus::Ready(word);
            }
            return BusStatus::Ready(0);
        }

        if addr < 0x200000 {
            let bus_plan = self.agnus.cck_bus_plan();
            if bus_plan.cpu_chip_bus_granted {
                if is_read {
                    let val = if is_word {
                        let hi = self.memory.read_byte(addr);
                        let lo = self.memory.read_byte(addr | 1);
                        (u16::from(hi) << 8) | u16::from(lo)
                    } else {
                        u16::from(self.memory.read_byte(addr))
                    };
                    BusStatus::Ready(val)
                } else {
                    let val = data.unwrap_or(0);
                    if is_word {
                        self.memory.write_byte(addr, (val >> 8) as u8);
                        self.memory.write_byte(addr | 1, val as u8);
                    } else {
                        self.memory.write_byte(addr, val as u8);
                    }
                    BusStatus::Ready(0)
                }
            } else {
                BusStatus::Wait
            }
        } else {
            if is_read {
                let val = if is_word {
                    let hi = self.memory.read_byte(addr);
                    let lo = self.memory.read_byte(addr | 1);
                    (u16::from(hi) << 8) | u16::from(lo)
                } else {
                    u16::from(self.memory.read_byte(addr))
                };
                BusStatus::Ready(val)
            } else {
                BusStatus::Ready(0)
            }
        }
    }
}

/// Shared custom register write dispatch used by both CPU and copper paths.
fn write_custom_register(
    agnus: &mut Agnus,
    denise: &mut DeniseOcs,
    copper: &mut Copper,
    paula: &mut Paula8364,
    _memory: &mut Memory,
    offset: u16,
    val: u16,
) {
    match offset {
        // Blitter registers
        0x040 => agnus.bltcon0 = val,
        0x042 => agnus.bltcon1 = val,
        0x044 => agnus.blt_afwm = val,
        0x046 => agnus.blt_alwm = val,
        0x048 => agnus.blt_cpt = (agnus.blt_cpt & 0x0000FFFF) | (u32::from(val) << 16),
        0x04A => agnus.blt_cpt = (agnus.blt_cpt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x04C => agnus.blt_bpt = (agnus.blt_bpt & 0x0000FFFF) | (u32::from(val) << 16),
        0x04E => agnus.blt_bpt = (agnus.blt_bpt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x050 => agnus.blt_apt = (agnus.blt_apt & 0x0000FFFF) | (u32::from(val) << 16),
        0x052 => agnus.blt_apt = (agnus.blt_apt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x054 => agnus.blt_dpt = (agnus.blt_dpt & 0x0000FFFF) | (u32::from(val) << 16),
        0x056 => agnus.blt_dpt = (agnus.blt_dpt & 0xFFFF0000) | u32::from(val & 0xFFFE),
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
        0x080 => copper.cop1lc = (copper.cop1lc & 0x0000FFFF) | (u32::from(val) << 16),
        0x082 => copper.cop1lc = (copper.cop1lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x084 => copper.cop2lc = (copper.cop2lc & 0x0000FFFF) | (u32::from(val) << 16),
        0x086 => copper.cop2lc = (copper.cop2lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x088 => copper.restart_cop1(),
        0x08A => copper.restart_cop2(),

        // Display
        0x08E => agnus.diwstrt = val,
        0x090 => agnus.diwstop = val,
        0x092 => agnus.ddfstrt = val,
        0x094 => agnus.ddfstop = val,

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
        0x020 => agnus.dsk_pt = (agnus.dsk_pt & 0x0000FFFF) | (u32::from(val) << 16),
        0x022 => agnus.dsk_pt = (agnus.dsk_pt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x024 => paula.write_dsklen(val),
        0x026 => paula.write_dskdat(val),
        0x07E => paula.dsksync = val,

        // Serial (discard)
        0x030 | 0x032 => {}

        // Copper danger
        0x02E => copper.danger = val & 0x02 != 0,

        // Bitplane control
        0x100 => {
            agnus.bplcon0 = val;
            denise.bplcon0 = val;
        }
        0x102 => denise.bplcon1 = val,
        0x104 => denise.bplcon2 = val,

        // Bitplane modulos
        0x108 => agnus.bpl1mod = val as i16,
        0x10A => agnus.bpl2mod = val as i16,

        // Bitplane pointers ($0E0-$0EE)
        0x0E0..=0x0EE => {
            let idx = ((offset - 0x0E0) / 4) as usize;
            if offset & 2 == 0 {
                agnus.bpl_pt[idx] = (agnus.bpl_pt[idx] & 0x0000FFFF) | (u32::from(val) << 16);
            } else {
                agnus.bpl_pt[idx] = (agnus.bpl_pt[idx] & 0xFFFF0000) | u32::from(val & 0xFFFE);
            }
        }

        // Sprite pointers ($120-$13E)
        0x120..=0x13E => {
            let idx = ((offset - 0x120) / 4) as usize;
            if idx < 8 {
                agnus.write_sprite_pointer_reg(idx, (offset & 2) == 0, val);
            }
        }

        // Sprite data ($140-$17E): 8 sprites x 4 regs (POS, CTL, DATA, DATB)
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

        // Color palette ($180-$1BE)
        0x180..=0x1BE => {
            let idx = ((offset - 0x180) / 2) as usize;
            denise.set_palette(idx, val);
        }

        // Paula audio channels (AUD0-AUD3)
        0x0A0..=0x0DA => {
            let _ = paula.write_audio_register(offset, val);
        }

        _ => {}
    }
}

/// Execute one queued blitter DMA timing op against any incremental blitter
/// runtime currently active in Agnus (today: line mode only).
fn execute_incremental_blitter_op(
    agnus: &mut Agnus,
    memory: &mut Memory,
    op: BlitterDmaOp,
) -> bool {
    let chip_len = memory.chip_ram.len();
    let chip_ptr = memory.chip_ram.as_mut_ptr();
    agnus.execute_incremental_blitter_op(
        op,
        |addr| {
            let a = addr & 0x1FFFFE;
            if (a as usize + 1) < chip_len {
                // SAFETY: `chip_ptr` points to `memory.chip_ram` for this
                // function call, and bounds are checked before access.
                unsafe {
                    (u16::from(*chip_ptr.add(a as usize)) << 8)
                        | u16::from(*chip_ptr.add(a as usize + 1))
                }
            } else {
                0
            }
        },
        |addr, val| {
            let a = addr & 0x1FFFFE;
            if (a as usize + 1) < chip_len {
                // SAFETY: `chip_ptr` points to `memory.chip_ram` for this
                // function call, and bounds are checked before access.
                unsafe {
                    *chip_ptr.add(a as usize) = (val >> 8) as u8;
                    *chip_ptr.add(a as usize + 1) = val as u8;
                }
            }
        },
    )
}

/// Execute a blitter operation synchronously when the coarse scheduler matures.
///
/// On real hardware the blitter runs in DMA slots over many CCKs. We still run
/// the whole operation instantly here, but only after a coarse per-CCK delay so
/// `BLTBUSY` and nasty-mode arbitration persist across CCKs.
fn execute_blit(agnus: &mut Agnus, paula: &mut Paula8364, memory: &mut Memory) {
    let height = (agnus.bltsize >> 6) & 0x3FF;
    let width_words = agnus.bltsize & 0x3F;
    let height = if height == 0 { 1024 } else { height } as u32;
    let width_words = if width_words == 0 { 64 } else { width_words } as u32;

    // LINE mode (BLTCON1 bit 0): Bresenham line drawing.
    // Uses a completely different algorithm from area mode.
    if agnus.bltcon1 & 0x0001 != 0 {
        execute_blit_line(agnus, paula, memory);
        return;
    }

    let use_a = agnus.bltcon0 & 0x0800 != 0;
    let use_b = agnus.bltcon0 & 0x0400 != 0;
    let use_c = agnus.bltcon0 & 0x0200 != 0;
    let use_d = agnus.bltcon0 & 0x0100 != 0;
    let lf = agnus.bltcon0 as u8; // minterm function (low 8 bits)
    let a_shift = (agnus.bltcon0 >> 12) & 0xF;
    let b_shift = (agnus.bltcon1 >> 12) & 0xF;
    let desc = agnus.bltcon1 & 0x0002 != 0;
    let fci = (agnus.bltcon1 & 0x0004) != 0; // Fill Carry Input
    let ife = (agnus.bltcon1 & 0x0008) != 0; // Inclusive Fill Enable
    let efe = (agnus.bltcon1 & 0x0010) != 0; // Exclusive Fill Enable
    let fill_enabled = ife || efe;

    let mut apt = agnus.blt_apt;
    let mut bpt = agnus.blt_bpt;
    let mut cpt = agnus.blt_cpt;
    let mut dpt = agnus.blt_dpt;

    let read_word = |mem: &Memory, addr: u32| -> u16 {
        let hi = mem.read_chip_byte(addr);
        let lo = mem.read_chip_byte(addr.wrapping_add(1));
        (u16::from(hi) << 8) | u16::from(lo)
    };

    let write_word = |mem: &mut Memory, addr: u32, val: u16| {
        mem.write_byte(addr, (val >> 8) as u8);
        mem.write_byte(addr.wrapping_add(1), val as u8);
    };

    let ptr_step: i32 = if desc { -2 } else { 2 };

    // The barrel shifter carries bits across rows — a_prev/b_prev are only
    // zeroed once before the entire blit, NOT per-row (HRM p. 179-180).
    let mut a_prev: u16 = 0;
    let mut b_prev: u16 = 0;

    for _row in 0..height {
        let mut fill_carry: u16 = if fci { 1 } else { 0 };

        for col in 0..width_words {
            // Read source channels.
            // DMA reads update the holding registers (BLTADAT/BLTBDAT/BLTCDAT)
            // so subsequent blits with the channel disabled see the last DMA value.
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

            // Apply first/last word masks to A channel
            let mut a_masked = a_raw;
            if col == 0 {
                a_masked &= agnus.blt_afwm;
            }
            if col == width_words - 1 {
                a_masked &= agnus.blt_alwm;
            }

            // Barrel shift A: combine with previous word.
            // In DESC mode the shift direction reverses (left instead of
            // right), so the combined word order must be swapped.
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

            // Barrel shift B
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

            // Compute minterm for each bit
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

            // Area fill: process bits right-to-left (bit 0 to bit 15),
            // toggling fill state at each '1' bit in the result.
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

            // Write D channel
            if use_d {
                write_word(memory, dpt, result);
                dpt = (dpt as i32 + ptr_step) as u32;
            }
        }

        // Apply modulos at end of each row.
        // HRM p. 182/199: In descending mode the blitter subtracts modulos.
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

    // Update pointer registers
    agnus.blt_apt = apt;
    agnus.blt_bpt = bpt;
    agnus.blt_cpt = cpt;
    agnus.blt_dpt = dpt;

    agnus.clear_blitter_scheduler();
    agnus.blitter_busy = false;
    paula.request_interrupt(6); // bit 6 = BLIT
}

/// Blitter LINE mode: Bresenham line drawing.
///
/// In line mode the blitter draws one pixel per "row" of BLTSIZE, stepping
/// through a Bresenham decision variable stored in BLTAPT.
///
/// Register usage in line mode:
///   BLTCON0 bits 15-12: Starting pixel position within word (ASH)
///   BLTCON0 bits 11-8:  Channel enables (must have A,C,D; B optional for texture)
///   BLTCON0 bits 7-0:   Minterm (usually $CA for normal, $0A for XOR)
///   BLTCON1 bits 15-12: Not used (BSH ignored in line mode)
///   BLTCON1 bit 4:      SUD (sign of dY step: 0=down, 1=up)
///   BLTCON1 bit 3:      SUL (sign of dX step: 0=right, 1=left)
///   BLTCON1 bit 2:      AUL (which axis is major: 0=X, 1=Y)
///   BLTCON1 bit 1:      SING (single-bit mode — only set pixel, don't clear)
///   BLTCON1 bit 0:      LINE (must be 1)
///   BLTAPT:  Initial Bresenham error accumulator (2*dy - dx, sign-extended)
///   BLTBDAT: Line texture pattern (usually $FFFF for solid)
///   BLTCPT/BLTDPT: Destination address (same value, points to first pixel's word)
///   BLTAMOD: 4*(dy - dx) — Bresenham decrement (when error >= 0)
///   BLTBMOD: 4*dy        — Bresenham increment (when error < 0)
///   BLTCMOD/BLTDMOD: Destination row modulo (bytes per row of the bitmap)
///   BLTAFWM: $8000 (not really used — the single-pixel mask comes from ASH)
///   BLTSIZE: height field = line length in pixels, width field = 2 (always)
fn execute_blit_line(agnus: &mut Agnus, paula: &mut Paula8364, memory: &mut Memory) {
    let length = ((agnus.bltsize >> 6) & 0x3FF) as u32;
    let length = if length == 0 { 1024 } else { length };

    let ash = (agnus.bltcon0 >> 12) & 0xF;
    let lf = agnus.bltcon0 as u8;
    let use_b = agnus.bltcon0 & 0x0400 != 0;
    // Octant control bits (HRM Appendix A):
    // SUD/SUL/AUL are not simple X/Y sign + major-axis flags. They form a
    // hardware-specific octant code and must be decoded via the HRM table.
    let sud = agnus.bltcon1 & 0x0010 != 0;
    let sul = agnus.bltcon1 & 0x0008 != 0;
    let aul = agnus.bltcon1 & 0x0004 != 0;
    let sing = agnus.bltcon1 & 0x0002 != 0;
    let oct_code = ((sud as u8) << 2) | ((sul as u8) << 1) | (aul as u8);
    // Code (SUD:SUL:AUL) -> octant index, per HRM table.
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
        0 => (false, false, false), // +X, +Y, X-major
        1 => (true, false, false),  // +X, +Y, Y-major
        2 => (true, true, false),   // -X, +Y, Y-major
        3 => (false, true, false),  // -X, +Y, X-major
        4 => (false, true, true),   // -X, -Y, X-major
        5 => (true, true, true),    // -X, -Y, Y-major
        6 => (true, false, true),   // +X, -Y, Y-major
        7 => (false, false, true),  // +X, -Y, X-major
        _ => unreachable!(),
    };

    let mut error = agnus.blt_apt as i16;
    let error_add = agnus.blt_bmod; // 4*dy (added when error < 0)
    let error_sub = agnus.blt_amod; // 4*(dy-dx) (added when error >= 0, typically negative)

    let mut cpt = agnus.blt_cpt;
    let mut dpt = agnus.blt_dpt;
    let mut pixel_bit = ash; // Current pixel position within word (0-15)

    let row_mod = agnus.blt_cmod as i16; // Destination row stride in bytes

    // Texture pattern from channel B (rotated each step)
    let mut texture = agnus.blt_bdat;
    let texture_enabled = use_b;

    fn read_word(mem: &Memory, addr: u32) -> u16 {
        let a = addr & 0x1FFFFE;
        if (a as usize + 1) < mem.chip_ram.len() {
            (u16::from(mem.chip_ram[a as usize]) << 8) | u16::from(mem.chip_ram[a as usize + 1])
        } else {
            0
        }
    }
    fn write_word(mem: &mut Memory, addr: u32, val: u16) {
        let a = addr & 0x1FFFFE;
        if (a as usize + 1) < mem.chip_ram.len() {
            mem.chip_ram[a as usize] = (val >> 8) as u8;
            mem.chip_ram[a as usize + 1] = val as u8;
        }
    }

    for _step in 0..length {
        // Build the single-pixel mask from the current bit position
        let pixel_mask: u16 = 0x8000 >> pixel_bit;

        // Channel A = the single pixel mask
        let a_val = pixel_mask;

        // Channel B = texture bit (MSB of rotating texture register)
        let b_val = if texture_enabled {
            if texture & 0x8000 != 0 {
                0xFFFF
            } else {
                0x0000
            }
        } else {
            0xFFFF
        };

        // Channel C = destination read-back (DMA updates the holding register)
        let c_val = read_word(&*memory, cpt);
        agnus.blt_cdat = c_val;

        // Compute minterm per bit
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

        // In SING mode, only modify the single pixel — keep other bits from C
        if sing {
            result = (result & pixel_mask) | (c_val & !pixel_mask);
        }

        // Write result to destination
        write_word(memory, dpt, result);

        // Rotate texture pattern (shift left by 1, wrap MSB to LSB)
        if texture_enabled {
            texture = texture.rotate_left(1);
        }

        // Bresenham step: decide whether to step on major axis only, or both axes.
        // Address updates are decoded from the HRM octant table above.
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
            // In Amiga blitter line mode, BLTCPT/BLTDPT use a bottom-up raster
            // address convention (HRM/SPG examples compute the start row as
            // (rows - y - 1)). Therefore screen Y+ ("down") moves to a LOWER
            // memory address, and screen Y- ("up") moves to a HIGHER address.
            if y_neg {
                *cpt = (*cpt as i32 + row_mod as i32) as u32;
                *dpt = (*dpt as i32 + row_mod as i32) as u32;
            } else {
                *cpt = (*cpt as i32 - row_mod as i32) as u32;
                *dpt = (*dpt as i32 - row_mod as i32) as u32;
            }
        };

        if error >= 0 {
            // Step on BOTH axes (diagonal move)
            if major_is_y {
                step_y(&mut cpt, &mut dpt);
                step_x(&mut cpt, &mut dpt, &mut pixel_bit);
            } else {
                step_x(&mut cpt, &mut dpt, &mut pixel_bit);
                step_y(&mut cpt, &mut dpt);
            }
            error = error.wrapping_add(error_sub as i16);
        } else {
            // Step on major axis ONLY
            if major_is_y {
                step_y(&mut cpt, &mut dpt);
            } else {
                step_x(&mut cpt, &mut dpt, &mut pixel_bit);
            }
            error = error.wrapping_add(error_add as i16);
        }
    }

    // Update registers
    agnus.blt_apt = error as u16 as u32;
    agnus.blt_cpt = cpt;
    agnus.blt_dpt = dpt;
    agnus.blt_bdat = texture;

    agnus.clear_blitter_scheduler();
    agnus.blitter_busy = false;
    paula.request_interrupt(6);
}

#[cfg(test)]
mod tests {
    use super::{Amiga, AmigaChipset, AmigaConfig, AmigaModel};

    fn dummy_kickstart() -> Vec<u8> {
        // Minimal reset vectors (SSP=0, PC=0) are enough for constructor tests.
        vec![0; 8]
    }

    #[test]
    fn amiga_new_defaults_to_ocs_chipset() {
        let amiga = Amiga::new(dummy_kickstart());
        assert_eq!(amiga.chipset, AmigaChipset::Ocs);
    }

    #[test]
    fn amiga_config_accepts_ecs_chipset_selection() {
        let amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });
        assert_eq!(amiga.chipset, AmigaChipset::Ecs);
    }
}
