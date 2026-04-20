//! Commodore 8364 Paula — interrupt controller, audio DMA + mixer, and
//! floppy-disk DMA / MFM front-end.
//!
//! Paula (one of the three custom chips on the Original Chipset Amiga)
//! owns three conceptually-distinct register groups that share a die:
//!
//!   1. **Interrupt controller** — INTENA/INTREQ (14 sources → 6 IPL).
//!   2. **Audio** — four identical DMA-driven channels with ADKCON
//!      modulation attach.
//!   3. **Disk** — DSKLEN + DSKSYNC + DSKDAT/DSKDATR + DSKBYTR, plus
//!      MFM byte-pacing (slow/fast via ADKCON) and an IPF variable-
//!      rate PLL.
//!
//! Paula does **not** own the disk-DMA pointer (DSKPT — that's Agnus),
//! nor serial/POTGO (historically in the Amiga machine crate; due to
//! be folded in alongside the floppy port — flagged in the Paula
//! Phase 1 gap-list doc).

use std::collections::VecDeque;

// ─────────────────────────────────────────────────────────────────────
// Public named-bit constants (HRM Appendix A)
// ─────────────────────────────────────────────────────────────────────

/// Named bit masks for Paula-owned registers. Matches HRM Appendix A
/// bit numbering so callers can read like the spec.
pub mod bits {
    // INTENA / INTREQ sources (each share layout).
    pub const INT_TBE:    u16 = 0x0001; // Serial transmit buffer empty
    pub const INT_DSKBLK: u16 = 0x0002; // Disk block finished
    pub const INT_SOFT:   u16 = 0x0004; // Software-requested
    pub const INT_PORTS:  u16 = 0x0008; // CIA-A /IRQ
    pub const INT_COPER:  u16 = 0x0010; // Copper
    pub const INT_VERTB:  u16 = 0x0020; // Vertical blank
    pub const INT_BLIT:   u16 = 0x0040; // Blitter finished
    pub const INT_AUD0:   u16 = 0x0080;
    pub const INT_AUD1:   u16 = 0x0100;
    pub const INT_AUD2:   u16 = 0x0200;
    pub const INT_AUD3:   u16 = 0x0400;
    pub const INT_RBF:    u16 = 0x0800; // Serial receive buffer full
    pub const INT_DSKSYN: u16 = 0x1000; // DSKDATR == DSKSYNC
    pub const INT_EXTER:  u16 = 0x2000; // CIA-B /IRQ
    pub const INT_INTEN:  u16 = 0x4000; // Master enable (bit 14)
    pub const INT_SETCLR: u16 = 0x8000; // Write flag: 1 = SET, 0 = CLEAR
    /// Mask covering every real source (bits 0..13). Bit 14 is the
    /// master-enable, not a pending source; bit 15 is the write flag.
    pub const INT_SOURCES: u16 = 0x3FFF;

    // DMACON bits Paula cares about.
    pub const DMA_AUD0:   u16 = 0x0001;
    pub const DMA_AUD1:   u16 = 0x0002;
    pub const DMA_AUD2:   u16 = 0x0004;
    pub const DMA_AUD3:   u16 = 0x0008;
    pub const DMA_DSK:    u16 = 0x0010;
    pub const DMA_MASTER: u16 = 0x0200;

    /// Per-channel audio DMA enable masks (indexed 0..=3).
    pub const DMA_AUD: [u16; 4] = [DMA_AUD0, DMA_AUD1, DMA_AUD2, DMA_AUD3];

    // DSKLEN bits.
    pub const DSKLEN_DMAEN:    u16 = 0x8000;
    pub const DSKLEN_WRITE:    u16 = 0x4000;

    // DSKBYTR read fields.
    pub const DSKBYTR_DSKBYT:    u16 = 0x8000;
    pub const DSKBYTR_DMAON:     u16 = 0x4000;
    pub const DSKBYTR_DISKWRITE: u16 = 0x2000;
    pub const DSKBYTR_WORDEQUAL: u16 = 0x1000;
    pub const DSKBYTR_DATA_MASK: u16 = 0x00FF;

    // ADKCON bits Paula uses.
    pub const ADKCON_PRECOMP1: u16 = 0x2000;
    pub const ADKCON_PRECOMP0: u16 = 0x1000;
    pub const ADKCON_MFMPREC:  u16 = 0x0800;
    pub const ADKCON_UARTBRK:  u16 = 0x0400;
    pub const ADKCON_WORDSYNC: u16 = 0x0200;
    /// Shared bit-8 meaning depending on context: MSBSYNC for serial,
    /// FAST-disk in the disk-byte timing path (HRM calls this FAST).
    pub const ADKCON_FAST:     u16 = 0x0100;
    /// Per-channel "channel N modulates channel N+1's period" enables.
    pub const ADKCON_USE_PER: [u16; 4] = [0x0010, 0x0020, 0x0040, 0x0080];
    /// Per-channel "channel N modulates channel N+1's volume" enables.
    pub const ADKCON_USE_VOL: [u16; 4] = [0x0001, 0x0002, 0x0004, 0x0008];

    // Timing constants for Paula's audio + disk paths.
    /// HRM minimum playback period — below this the DMA slot cannot
    /// deliver in time. Writes below 124 are preserved for read-back.
    pub const AUDIO_MIN_PERIOD_CCK: u16 = 124;
    /// Colour-clocks from audio-DMA-slot service to word visible to
    /// the playback engine (HRM/WinUAE — modelled as a constant).
    pub const AUDIO_DMA_RETURN_LATENCY_CCK: u8 = 14;
    pub const DISK_BYTE_CCK_FAST: u8 = 14;
    pub const DISK_BYTE_CCK_SLOW: u8 = 28;
}

use bits::*;

// ─────────────────────────────────────────────────────────────────────
// Interrupt source enum + audio field enum (typed register API)
// ─────────────────────────────────────────────────────────────────────

/// An INTREQ source. Indexes the 14 interrupt-request bits by name so
/// callers don't use raw bit numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IntSource {
    Tbe    = 0,
    DskBlk = 1,
    Soft   = 2,
    Ports  = 3,
    Coper  = 4,
    Vertb  = 5,
    Blit   = 6,
    Aud0   = 7,
    Aud1   = 8,
    Aud2   = 9,
    Aud3   = 10,
    Rbf    = 11,
    DskSyn = 12,
    Exter  = 13,
}

impl IntSource {
    #[must_use]
    pub fn mask(self) -> u16 {
        1 << (self as u8)
    }
}

/// One of the six per-channel audio registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioField {
    LcHi = 0,
    LcLo = 1,
    Len  = 2,
    Per  = 3,
    Vol  = 4,
    Dat  = 5,
}

/// Address-decode helpers for code that speaks the Amiga custom-
/// register bus ($DFF0xx). The chip core itself does not know about
/// bus offsets — machine crates use this to map offsets to typed
/// register accesses.
pub mod decode {
    use super::AudioField;

    /// Decode a custom-register offset in `$DFF0A0..=$DFF0DA` into a
    /// (channel, field) pair, or `None` if the offset is outside the
    /// audio-register block.
    #[must_use]
    pub fn audio_register(offset: u16) -> Option<(u8, AudioField)> {
        if !(0x0A0..=0x0DA).contains(&offset) {
            return None;
        }
        let rel = offset - 0x0A0;
        let channel = (rel / 0x10) as u8;
        if channel >= 4 {
            return None;
        }
        let field = match (rel % 0x10) / 2 {
            0 => AudioField::LcHi,
            1 => AudioField::LcLo,
            2 => AudioField::Len,
            3 => AudioField::Per,
            4 => AudioField::Vol,
            5 => AudioField::Dat,
            _ => return None,
        };
        Some((channel, field))
    }
}

// ─────────────────────────────────────────────────────────────────────
// DAC lookup table
// ─────────────────────────────────────────────────────────────────────

/// DAC non-linearity lookup modelling the A500 resistor-ladder output.
/// Index 0 = $80 = -128, index 255 = $7F = +127 → normalised f32.
/// Polynomial approximation with a small cubic peak-compression term.
fn build_dac_table() -> [f32; 256] {
    let mut table = [0.0f32; 256];
    for i in 0..256u16 {
        let sample = i as u8 as i8;
        let x = f32::from(sample) / 128.0;
        let y = x - 0.02 * x * x * x;
        table[i as usize] = y;
    }
    table
}

static DAC_TABLE: std::sync::LazyLock<[f32; 256]> =
    std::sync::LazyLock::new(build_dac_table);

// ─────────────────────────────────────────────────────────────────────
// Audio channel
// ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioOutputEvent {
    HighByte(u16),
    LowByte(u16),
}

impl AudioOutputEvent {
    fn word(self) -> u16 {
        match self {
            Self::HighByte(w) | Self::LowByte(w) => w,
        }
    }

    fn is_word_complete(self) -> bool {
        matches!(self, Self::LowByte(_))
    }
}

#[derive(Debug, Clone, Copy)]
struct AudioChannel {
    lc: u32,
    ptr: u32,
    len_words: u16,
    words_remaining: u32,
    per: u16,
    vol: u8,
    dat: u16,
    current_word: Option<u16>,
    next_word: Option<u16>,
    next_byte_is_hi: bool,
    period_counter: u16,
    output_sample: i8,
    dma_active: bool,
    dma_enabled_prev: bool,
    dma_requests_pending: u8,
    dma_return_countdown: u8,
    dma_return_word: Option<u16>,
}

impl Default for AudioChannel {
    fn default() -> Self {
        Self {
            lc: 0,
            ptr: 0,
            len_words: 0,
            words_remaining: 0,
            per: AUDIO_MIN_PERIOD_CCK,
            vol: 0,
            dat: 0,
            current_word: None,
            next_word: None,
            next_byte_is_hi: true,
            period_counter: AUDIO_MIN_PERIOD_CCK,
            output_sample: 0,
            dma_active: false,
            dma_enabled_prev: false,
            dma_requests_pending: 0,
            dma_return_countdown: 0,
            dma_return_word: None,
        }
    }
}

impl AudioChannel {
    fn effective_period(&self) -> u16 {
        self.per.max(AUDIO_MIN_PERIOD_CCK)
    }

    fn programmed_length_words(&self) -> u32 {
        if self.len_words == 0 { 65_536 } else { u32::from(self.len_words) }
    }

    fn start_dma(&mut self) {
        self.ptr = self.lc & 0x00FF_FFFE;
        self.words_remaining = self.programmed_length_words();
        self.current_word = None;
        self.next_word = None;
        self.next_byte_is_hi = true;
        self.period_counter = self.effective_period();
        self.dma_active = true;
        // Seed two requests so current+next fill quickly while still
        // routing the actual fetches through audio DMA slots.
        self.dma_requests_pending = 2;
        self.dma_return_countdown = 0;
        self.dma_return_word = None;
    }

    fn stop_dma(&mut self) {
        self.dma_active = false;
        self.current_word = None;
        self.next_word = None;
        self.next_byte_is_hi = true;
        self.dma_requests_pending = 0;
        self.dma_return_countdown = 0;
        self.dma_return_word = None;
    }

    fn sync_dma_enable(&mut self, enabled: bool) -> bool {
        let mut block_started = false;
        if enabled && !self.dma_enabled_prev {
            self.start_dma();
            block_started = true;
        } else if !enabled && self.dma_enabled_prev {
            self.stop_dma();
        }
        self.dma_enabled_prev = enabled;
        block_started
    }

    fn write_dat(&mut self, val: u16) {
        self.dat = val;
        // Non-DMA playback: CPU-written AUDxDAT feeds the DAC directly.
        if !self.dma_active {
            self.current_word = Some(val);
            self.next_word = None;
            self.next_byte_is_hi = true;
            self.period_counter = self.effective_period();
        }
    }

    fn write_period(&mut self, val: u16) {
        self.per = val;
        if self.period_counter == 0 {
            self.period_counter = self.effective_period();
        }
    }

    fn write_volume(&mut self, val: u16) {
        self.vol = (val & 0x7F).min(64) as u8;
    }

    fn push_dma_word(&mut self, word: u16) {
        self.dat = word;
        if self.current_word.is_none() {
            self.current_word = Some(word);
            self.next_byte_is_hi = true;
        } else if self.next_word.is_none() {
            self.next_word = Some(word);
        }
    }

    fn fetch_dma_word<F>(&mut self, mut read_chip_byte: F) -> Option<(u16, bool)>
    where
        F: FnMut(u32) -> u8,
    {
        if !self.dma_active {
            return None;
        }
        if self.current_word.is_some() && self.next_word.is_some() {
            return None;
        }

        let mut wrapped = false;
        if self.words_remaining == 0 {
            self.ptr = self.lc & 0x00FF_FFFE;
            self.words_remaining = self.programmed_length_words();
            if self.words_remaining == 0 {
                return None;
            }
            wrapped = true;
        }

        let hi = read_chip_byte(self.ptr);
        let lo = read_chip_byte(self.ptr | 1);
        self.ptr = self.ptr.wrapping_add(2);
        self.words_remaining = self.words_remaining.saturating_sub(1);
        Some(((u16::from(hi) << 8) | u16::from(lo), wrapped))
    }

    fn queue_dma_request(&mut self) {
        if self.dma_active {
            self.dma_requests_pending = self.dma_requests_pending.saturating_add(1);
        }
    }

    fn service_dma_slot<F>(&mut self, read_chip_byte: F) -> Option<bool>
    where
        F: FnMut(u32) -> u8,
    {
        if self.dma_requests_pending == 0 {
            return None;
        }
        if self.dma_return_word.is_some() {
            return None;
        }
        let (word, wrapped) = self.fetch_dma_word(read_chip_byte)?;
        self.dma_requests_pending = self.dma_requests_pending.saturating_sub(1);
        self.dma_return_word = Some(word);
        self.dma_return_countdown = AUDIO_DMA_RETURN_LATENCY_CCK;
        Some(wrapped)
    }

    fn tick_dma_return(&mut self, return_progress_this_cck: bool) {
        if self.dma_return_word.is_none() {
            return;
        }
        if return_progress_this_cck && self.dma_return_countdown > 0 {
            self.dma_return_countdown -= 1;
        }
        if self.dma_return_countdown == 0
            && let Some(word) = self.dma_return_word.take()
        {
            self.push_dma_word(word);
        }
    }

    fn tick_output(&mut self, consume_word_each_transition: bool) -> Option<AudioOutputEvent> {
        if self.period_counter == 0 {
            self.period_counter = self.effective_period();
        }
        self.period_counter = self.period_counter.saturating_sub(1);
        if self.period_counter != 0 {
            return None;
        }
        self.period_counter = self.effective_period();

        if self.current_word.is_none()
            && let Some(next) = self.next_word.take()
        {
            self.current_word = Some(next);
            self.next_byte_is_hi = true;
        }
        let word = self.current_word?;

        let byte = if self.next_byte_is_hi { (word >> 8) as u8 } else { word as u8 };
        self.output_sample = byte as i8;

        if self.next_byte_is_hi {
            self.next_byte_is_hi = false;
            if consume_word_each_transition && let Some(next) = self.next_word.take() {
                self.current_word = Some(next);
            }
            return Some(AudioOutputEvent::HighByte(word));
        }

        self.next_byte_is_hi = true;
        if let Some(next) = self.next_word.take() {
            self.current_word = Some(next);
        } else if !consume_word_each_transition {
            self.current_word = None;
        }
        Some(AudioOutputEvent::LowByte(word))
    }

    fn mix_sample(&self) -> f32 {
        let idx = (self.output_sample as u8) as usize;
        let amplitude = DAC_TABLE[idx];
        let volume = f32::from(self.vol.min(64)) / 64.0;
        amplitude * volume
    }
}

/// Snapshot view of one audio channel's live output state. Returned
/// from [`Paula8364::audio_state`] for debuggers/level meters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioChannelSnapshot {
    pub period: u16,
    pub volume: u8,
    pub sample: i8,
}

// ─────────────────────────────────────────────────────────────────────
// Paula8364 — main type
// ─────────────────────────────────────────────────────────────────────

pub struct Paula8364 {
    intena: u16,
    intreq: u16,
    adkcon: u16,

    dsklen: u16,
    dsksync: u16,
    dskdatr: u16,
    dskdat: u16,

    dskbytr_data: u8,
    dskbytr_next_data: Option<u8>,
    dskbytr_next_delay_cck: u8,
    dskbytr_valid: bool,
    dskbytr_wordequal: bool,
    dskbytr_wordequal_delay_cck: u8,

    dskdat_queue: VecDeque<u16>,
    disk_dma_pending: bool,
    /// DSKLEN arming flip-flop per HRM "turn DMAEN on twice" protocol.
    dsklen_armed: bool,

    /// IPF variable-rate PLL phase accumulator (16 = word ready).
    disk_pll_phase: u16,
    disk_pll_variable_rate: bool,

    audio: [AudioChannel; 4],

    // Diagnostic logs — not part of the chip's behavioural contract.
    intena_write_log: VecDeque<u16>,
    intreq_write_log: VecDeque<u16>,
    disk_write_dma_log: Vec<u16>,
    disk_write_pio_log: Vec<u16>,
}

impl Default for Paula8364 {
    fn default() -> Self { Self::new() }
}

impl Paula8364 {
    #[must_use]
    pub fn new() -> Self {
        Self {
            intena: 0,
            intreq: 0,
            adkcon: 0,
            dsklen: 0,
            dsksync: 0,
            dskdatr: 0,
            dskdat: 0,
            dskbytr_data: 0,
            dskbytr_next_data: None,
            dskbytr_next_delay_cck: 0,
            dskbytr_valid: false,
            dskbytr_wordequal: false,
            dskbytr_wordequal_delay_cck: 0,
            dskdat_queue: VecDeque::new(),
            disk_dma_pending: false,
            dsklen_armed: false,
            disk_pll_phase: 0,
            disk_pll_variable_rate: false,
            audio: [AudioChannel::default(); 4],
            intena_write_log: VecDeque::new(),
            intreq_write_log: VecDeque::new(),
            disk_write_dma_log: Vec::new(),
            disk_write_pio_log: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    // ─── Interrupt controller ─────────────────────────────────────────

    /// CPU write to INTENA ($DFF09A). Bit 15 = SET, bit 15 clear = CLEAR.
    pub fn write_intena(&mut self, val: u16) {
        Self::push_log(&mut self.intena_write_log, val);
        Self::apply_set_clear(&mut self.intena, val);
    }

    /// CPU write to INTREQ ($DFF09C). Same SET/CLEAR semantics.
    pub fn write_intreq(&mut self, val: u16) {
        Self::push_log(&mut self.intreq_write_log, val);
        Self::apply_set_clear(&mut self.intreq, val);
    }

    /// CPU write to ADKCON ($DFF09E). Same SET/CLEAR semantics.
    pub fn write_adkcon(&mut self, val: u16) {
        Self::apply_set_clear(&mut self.adkcon, val);
    }

    /// Raise one or more INTREQ sources by bit-mask.
    pub fn raise_intreq(&mut self, mask: u16) {
        self.intreq |= mask & INT_SOURCES;
    }

    /// Raise one INTREQ source by name.
    pub fn raise(&mut self, source: IntSource) {
        self.raise_intreq(source.mask());
    }

    #[must_use] pub fn intena(&self) -> u16 { self.intena }
    #[must_use] pub fn intreq(&self) -> u16 { self.intreq }
    #[must_use] pub fn adkcon(&self) -> u16 { self.adkcon }

    /// Compute the 68000 IPL this chipset is requesting. Bit 14
    /// (INTEN) gates everything; per-level priority per HRM Table 3-3.
    #[must_use]
    pub fn compute_ipl(&self) -> u8 {
        if self.intena & INT_INTEN == 0 {
            return 0;
        }
        let active = self.intena & self.intreq & INT_SOURCES;
        if active & INT_EXTER != 0 { 6 }
        else if active & (INT_RBF | INT_DSKSYN) != 0 { 5 }
        else if active & (INT_AUD0 | INT_AUD1 | INT_AUD2 | INT_AUD3) != 0 { 4 }
        else if active & (INT_COPER | INT_VERTB | INT_BLIT) != 0 { 3 }
        else if active & INT_PORTS != 0 { 2 }
        else if active & (INT_TBE | INT_DSKBLK | INT_SOFT) != 0 { 1 }
        else { 0 }
    }

    fn apply_set_clear(reg: &mut u16, val: u16) {
        if val & INT_SETCLR != 0 {
            *reg |= val & INT_SOURCES | (val & INT_INTEN);
        } else {
            *reg &= !(val & (INT_SOURCES | INT_INTEN));
        }
    }

    fn push_log(log: &mut VecDeque<u16>, val: u16) {
        log.push_back(val);
        if log.len() > 16 {
            log.pop_front();
        }
    }

    // ─── Audio register access (typed) ────────────────────────────────

    pub fn write_audio(&mut self, ch: u8, field: AudioField, val: u16) {
        let Some(channel) = self.audio.get_mut(ch as usize) else { return };
        match field {
            AudioField::LcHi => channel.lc = (channel.lc & 0x0000_FFFF) | (u32::from(val) << 16),
            AudioField::LcLo => channel.lc = (channel.lc & 0xFFFF_0000) | u32::from(val & 0xFFFE),
            AudioField::Len  => channel.len_words = val,
            AudioField::Per  => channel.write_period(val),
            AudioField::Vol  => channel.write_volume(val),
            AudioField::Dat  => channel.write_dat(val),
        }
    }

    #[must_use]
    pub fn read_audio(&self, ch: u8, field: AudioField) -> u16 {
        let Some(channel) = self.audio.get(ch as usize) else { return 0 };
        match field {
            AudioField::LcHi => (channel.lc >> 16) as u16,
            AudioField::LcLo => (channel.lc & 0xFFFF) as u16,
            AudioField::Len  => channel.len_words,
            AudioField::Per  => channel.per,
            AudioField::Vol  => u16::from(channel.vol),
            AudioField::Dat  => channel.dat,
        }
    }

    /// Per-channel live-output snapshot for debuggers / level meters.
    #[must_use]
    pub fn audio_state(&self, ch: u8) -> Option<AudioChannelSnapshot> {
        self.audio.get(ch as usize).map(|c| AudioChannelSnapshot {
            period: c.per,
            volume: c.vol,
            sample: c.output_sample,
        })
    }

    // ─── Audio tick ───────────────────────────────────────────────────

    /// Advance Paula's audio path one colour clock (CCK).
    ///
    /// `dmacon` is the current Agnus DMACON value; Paula reads
    /// DMAEN (bit 9) and AUDx (bits 0-3) to gate DMA.
    /// `audio_dma_slot`, if `Some(ch)`, indicates this CCK is channel
    /// `ch`'s dedicated DMA slot — Paula may service a fetch request.
    /// `return_progress_this_cck` lets the machine stall DMA-return
    /// pacing during bus contention (pass `true` under normal bus
    /// conditions).
    pub fn tick_audio_cck<F>(
        &mut self,
        dmacon: u16,
        audio_dma_slot: Option<u8>,
        return_progress_this_cck: bool,
        mut read_chip_byte: F,
    ) where
        F: FnMut(u32) -> u8,
    {
        let mut irq_mask: u16 = 0;
        for (index, channel) in self.audio.iter_mut().enumerate() {
            let dma_enabled =
                (dmacon & DMA_MASTER) != 0 && (dmacon & DMA_AUD[index]) != 0;
            if channel.sync_dma_enable(dma_enabled) {
                irq_mask |= INT_AUD0 << index;
            }
            channel.tick_dma_return(return_progress_this_cck);
        }

        if let Some(ch_u8) = audio_dma_slot
            && let Some(ch) = self.audio.get_mut(ch_u8 as usize)
            && ch.service_dma_slot(&mut read_chip_byte) == Some(true)
        {
            irq_mask |= INT_AUD0 << ch_u8;
        }

        let mut output_events = [None; 4];
        for (index, channel) in self.audio.iter_mut().enumerate() {
            let combined_attach = (self.adkcon & ADKCON_USE_PER[index]) != 0
                && (self.adkcon & ADKCON_USE_VOL[index]) != 0;
            let event = channel.tick_output(combined_attach);
            if event.is_some_and(AudioOutputEvent::is_word_complete) && !channel.dma_active {
                irq_mask |= INT_AUD0 << index;
            }
            output_events[index] = event;
        }

        for (index, event) in output_events.into_iter().enumerate() {
            if let Some(ev) = event {
                if self.audio_dma_request_on_event(index, ev) {
                    self.audio[index].queue_dma_request();
                }
                self.apply_audio_modulation_event(index, ev);
            }
        }

        self.intreq |= irq_mask;
    }

    /// Mixed stereo output in `[-1.0, 1.0]`. OCS routing: ch 0+3 → L,
    /// ch 1+2 → R. Modulator channels (ADKCON attach) are muted.
    #[must_use]
    pub fn mix_audio_stereo(&self) -> (f32, f32) {
        let s = |i: usize| -> f32 {
            if self.audio_channel_is_modulator(i) { 0.0 } else { self.audio[i].mix_sample() }
        };
        let left = (s(0) + s(3)) * 0.5;
        let right = (s(1) + s(2)) * 0.5;
        (left.clamp(-1.0, 1.0), right.clamp(-1.0, 1.0))
    }

    fn audio_channel_is_modulator(&self, index: usize) -> bool {
        (self.adkcon & ADKCON_USE_VOL[index] != 0)
            || (self.adkcon & ADKCON_USE_PER[index] != 0)
    }

    fn audio_dma_request_on_event(&self, index: usize, event: AudioOutputEvent) -> bool {
        let use_vol = (self.adkcon & ADKCON_USE_VOL[index]) != 0;
        let use_per = (self.adkcon & ADKCON_USE_PER[index]) != 0;
        match (use_per, use_vol, event) {
            (false, false, AudioOutputEvent::LowByte(_)) => true,
            (false, false, AudioOutputEvent::HighByte(_)) => false,
            (false, true, AudioOutputEvent::LowByte(_)) => true,
            (false, true, AudioOutputEvent::HighByte(_)) => false,
            (true, false, AudioOutputEvent::HighByte(_)) => true,
            (true, false, AudioOutputEvent::LowByte(_)) => false,
            (true, true, _) => true,
        }
    }

    fn apply_audio_modulation_event(&mut self, source: usize, event: AudioOutputEvent) {
        let use_vol = (self.adkcon & ADKCON_USE_VOL[source]) != 0;
        let use_per = (self.adkcon & ADKCON_USE_PER[source]) != 0;
        if !use_vol && !use_per {
            return;
        }
        let should_apply = match (use_per, use_vol, event) {
            (true, false, AudioOutputEvent::HighByte(_)) => true,
            (true, false, AudioOutputEvent::LowByte(_)) => false,
            (false, true, AudioOutputEvent::HighByte(_)) => false,
            (false, true, AudioOutputEvent::LowByte(_)) => true,
            (true, true, _) => true,
            (false, false, _) => false,
        };
        if !should_apply || source + 1 >= self.audio.len() {
            return;
        }
        let word = event.word();
        let target = &mut self.audio[source + 1];
        match (use_per, use_vol, event) {
            (true, true, AudioOutputEvent::HighByte(_)) => target.write_period(word),
            (true, true, AudioOutputEvent::LowByte(_)) => target.write_volume(word),
            (true, false, _) => target.write_period(word),
            (false, true, _) => target.write_volume(word),
            (false, false, _) => {}
        }
    }

    // ─── Disk registers + DMA arming ──────────────────────────────────

    pub fn write_dsklen(&mut self, val: u16) {
        self.dsklen = val;
        if val & DSKLEN_DMAEN != 0 {
            if self.dsklen_armed {
                self.disk_dma_pending = true;
                self.dsklen_armed = false;
            } else {
                self.dsklen_armed = true;
            }
        } else {
            self.dsklen_armed = false;
        }
    }

    /// Called by the machine when a disk DMA transfer completes.
    /// Atomically clears the pending flag, disarms, and raises DSKBLK.
    pub fn complete_disk_dma(&mut self) {
        self.disk_dma_pending = false;
        self.dsklen_armed = false;
        self.raise(IntSource::DskBlk);
    }

    pub fn write_dskdat(&mut self, val: u16) {
        self.dskdat = val;
        self.dskdat_queue.push_back(val);
    }

    /// Drive consumer API: pull the next queued DSKDAT write.
    pub fn take_dskdat_queued_word(&mut self) -> Option<u16> {
        self.dskdat_queue.pop_front()
    }

    #[must_use]
    pub fn dskdat_queue_len(&self) -> usize { self.dskdat_queue.len() }

    pub fn set_dsksync(&mut self, val: u16) { self.dsksync = val; }

    #[must_use] pub fn dsksync(&self) -> u16 { self.dsksync }
    #[must_use] pub fn dsklen(&self) -> u16 { self.dsklen }
    #[must_use] pub fn dskdat(&self) -> u16 { self.dskdat }
    #[must_use] pub fn dskdatr(&self) -> u16 { self.dskdatr }
    #[must_use] pub fn disk_dma_pending(&self) -> bool { self.disk_dma_pending }

    /// Called by the drive when a fresh MFM word has arrived. Returns
    /// `true` iff it matches DSKSYNC.
    ///
    /// Sync-match gating follows HRM: if `ADKCON.WORDSYNC` is set and
    /// the word matches DSKSYNC, Paula raises `INT_DSKSYN` directly —
    /// the sync comparator is in Paula, not in the drive peripheral.
    /// `DSKBYTR.WORDEQUAL` latches on the comparison result itself,
    /// independently of WORDSYNC (HRM: "the comparator is always
    /// running; WORDSYNC controls only the interrupt and the DMA
    /// word-boundary gate").
    pub fn note_disk_read_word(&mut self, word: u16) -> bool {
        self.dskdatr = word;
        self.dskbytr_data = (word >> 8) as u8;
        self.dskbytr_next_data = Some(word as u8);
        self.dskbytr_next_delay_cck = self.disk_byte_cck_delay();
        self.dskbytr_valid = true;
        let wordequal = word == self.dsksync;
        self.dskbytr_wordequal = wordequal;
        self.dskbytr_wordequal_delay_cck = if wordequal { self.disk_byte_cck_delay() } else { 0 };
        if wordequal && self.adkcon & ADKCON_WORDSYNC != 0 {
            self.raise(IntSource::DskSyn);
        }
        wordequal
    }

    /// Read DSKBYTR with its documented side effect: DSKBYT clears.
    pub fn read_dskbytr(&mut self, dmacon: u16) -> u16 {
        let value = self.peek_dskbytr(dmacon);
        self.dskbytr_valid = false;
        value
    }

    /// Side-effect-free DSKBYTR view for debuggers/tracing.
    #[must_use]
    pub fn peek_dskbytr(&self, dmacon: u16) -> u16 {
        let dmaon = (self.dsklen & DSKLEN_DMAEN != 0) && (dmacon & (DMA_MASTER | DMA_DSK)) == (DMA_MASTER | DMA_DSK);
        let diskwrite = self.dsklen & DSKLEN_WRITE != 0;
        let mut value = u16::from(self.dskbytr_data);
        if self.dskbytr_valid { value |= DSKBYTR_DSKBYT; }
        if dmaon { value |= DSKBYTR_DMAON; }
        if diskwrite { value |= DSKBYTR_DISKWRITE; }
        if self.dskbytr_wordequal { value |= DSKBYTR_WORDEQUAL; }
        value
    }

    pub fn tick_disk_cck(&mut self) {
        if self.dskbytr_wordequal && self.dskbytr_wordequal_delay_cck != 0 {
            self.dskbytr_wordequal_delay_cck -= 1;
            if self.dskbytr_wordequal_delay_cck == 0 {
                self.dskbytr_wordequal = false;
            }
        }

        if self.dskbytr_next_data.is_some() && self.dskbytr_next_delay_cck != 0 {
            self.dskbytr_next_delay_cck -= 1;
        }

        if self.dskbytr_next_data.is_some()
            && self.dskbytr_next_delay_cck == 0
            && let Some(next) = self.dskbytr_next_data.take()
        {
            // Simplified overrun model — HRM mentions an overrun bit
            // we do not implement (trackdisk.device does not rely on
            // it). Unread earlier byte is replaced.
            self.dskbytr_data = next;
            self.dskbytr_valid = true;
        }
    }

    fn disk_byte_cck_delay(&self) -> u8 {
        if self.adkcon & ADKCON_FAST != 0 { DISK_BYTE_CCK_FAST } else { DISK_BYTE_CCK_SLOW }
    }

    // ─── Disk PLL (IPF variable-rate) ─────────────────────────────────

    /// Accumulate one bit-cell timing into the disk PLL phase. Returns
    /// `true` when 16 bits have accumulated — one MFM word is ready.
    pub fn disk_pll_accumulate(&mut self, bit_cells: u16) -> bool {
        self.disk_pll_phase += bit_cells;
        if self.disk_pll_phase >= 16 {
            self.disk_pll_phase -= 16;
            true
        } else {
            false
        }
    }

    pub fn disk_pll_reset(&mut self) { self.disk_pll_phase = 0; }

    pub fn set_disk_pll_variable_rate(&mut self, enabled: bool) {
        self.disk_pll_variable_rate = enabled;
    }

    #[must_use]
    pub fn disk_pll_variable_rate(&self) -> bool { self.disk_pll_variable_rate }

    // ─── Diagnostic logs (not behavioural) ────────────────────────────

    /// Append an observed disk-write DMA word to the diagnostic log.
    /// Floppy peripheral calls this as it consumes DSKDAT via DMA.
    pub fn note_disk_write_dma_word(&mut self, word: u16) { self.disk_write_dma_log.push(word); }

    /// Append an observed disk-write PIO word.
    pub fn note_disk_write_pio_word(&mut self, word: u16) { self.disk_write_pio_log.push(word); }

    #[must_use]
    pub fn debug_disk_write_dma_log(&self) -> &[u16] { &self.disk_write_dma_log }

    #[must_use]
    pub fn debug_disk_write_pio_log(&self) -> &[u16] { &self.disk_write_pio_log }

    pub fn clear_debug_disk_write_dma_log(&mut self) { self.disk_write_dma_log.clear(); }
    pub fn clear_debug_disk_write_pio_log(&mut self) { self.disk_write_pio_log.clear(); }

    /// Most-recent INTENA writes (up to 16 deep, oldest first).
    #[must_use]
    pub fn debug_intena_writes(&self) -> &VecDeque<u16> { &self.intena_write_log }

    /// Most-recent INTREQ writes (up to 16 deep, oldest first).
    #[must_use]
    pub fn debug_intreq_writes(&self) -> &VecDeque<u16> { &self.intreq_write_log }
}

// ─────────────────────────────────────────────────────────────────────
// Internal unit tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_registers_round_trip_per_channel_via_typed_api() {
        let mut p = Paula8364::new();
        p.write_audio(0, AudioField::LcHi, 0x0012);
        p.write_audio(0, AudioField::LcLo, 0x3456);
        p.write_audio(0, AudioField::Len, 0x0008);
        p.write_audio(0, AudioField::Per, 500);
        p.write_audio(0, AudioField::Vol, 32);
        assert_eq!(p.read_audio(0, AudioField::LcHi), 0x0012);
        assert_eq!(p.read_audio(0, AudioField::LcLo), 0x3456);
        assert_eq!(p.read_audio(0, AudioField::Len), 0x0008);
        assert_eq!(p.read_audio(0, AudioField::Per), 500);
        assert_eq!(p.read_audio(0, AudioField::Vol), 32);
    }

    #[test]
    fn decode_audio_register_maps_every_channel_field() {
        use decode::audio_register;
        assert_eq!(audio_register(0x0A0), Some((0, AudioField::LcHi)));
        assert_eq!(audio_register(0x0A2), Some((0, AudioField::LcLo)));
        assert_eq!(audio_register(0x0A4), Some((0, AudioField::Len)));
        assert_eq!(audio_register(0x0A6), Some((0, AudioField::Per)));
        assert_eq!(audio_register(0x0A8), Some((0, AudioField::Vol)));
        assert_eq!(audio_register(0x0AA), Some((0, AudioField::Dat)));
        assert_eq!(audio_register(0x0B0), Some((1, AudioField::LcHi)));
        assert_eq!(audio_register(0x0DA), Some((3, AudioField::Dat)));
        assert_eq!(audio_register(0x09E), None);
        // Odd bytes inside the audio block map to the same field as the
        // preceding even byte — real Amiga custom-register bus ignores A0.
        assert_eq!(audio_register(0x0A1), Some((0, AudioField::LcHi)));
    }

    #[test]
    fn intsource_mask_matches_named_bits() {
        assert_eq!(IntSource::Vertb.mask(), INT_VERTB);
        assert_eq!(IntSource::Aud3.mask(), INT_AUD3);
        assert_eq!(IntSource::Exter.mask(), INT_EXTER);
    }

    #[test]
    fn raise_by_source_sets_exactly_one_intreq_bit() {
        let mut p = Paula8364::new();
        p.raise(IntSource::Coper);
        assert_eq!(p.intreq(), INT_COPER);
        p.raise(IntSource::Vertb);
        assert_eq!(p.intreq(), INT_COPER | INT_VERTB);
    }
}
