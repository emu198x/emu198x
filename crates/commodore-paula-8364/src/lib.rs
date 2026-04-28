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

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// ─────────────────────────────────────────────────────────────────────
// Public named-bit constants (HRM Appendix A)
// ─────────────────────────────────────────────────────────────────────

/// Named bit masks for Paula-owned registers. Matches HRM Appendix A
/// bit numbering so callers can read like the spec.
pub mod bits {
    // INTENA / INTREQ sources (each share layout).
    pub const INT_TBE: u16 = 0x0001; // Serial transmit buffer empty
    pub const INT_DSKBLK: u16 = 0x0002; // Disk block finished
    pub const INT_SOFT: u16 = 0x0004; // Software-requested
    pub const INT_PORTS: u16 = 0x0008; // CIA-A /IRQ
    pub const INT_COPER: u16 = 0x0010; // Copper
    pub const INT_VERTB: u16 = 0x0020; // Vertical blank
    pub const INT_BLIT: u16 = 0x0040; // Blitter finished
    pub const INT_AUD0: u16 = 0x0080;
    pub const INT_AUD1: u16 = 0x0100;
    pub const INT_AUD2: u16 = 0x0200;
    pub const INT_AUD3: u16 = 0x0400;
    pub const INT_RBF: u16 = 0x0800; // Serial receive buffer full
    pub const INT_DSKSYN: u16 = 0x1000; // DSKDATR == DSKSYNC
    pub const INT_EXTER: u16 = 0x2000; // CIA-B /IRQ
    pub const INT_INTEN: u16 = 0x4000; // Master enable (bit 14)
    pub const INT_SETCLR: u16 = 0x8000; // Write flag: 1 = SET, 0 = CLEAR
    /// Mask covering every real source (bits 0..13). Bit 14 is the
    /// master-enable, not a pending source; bit 15 is the write flag.
    pub const INT_SOURCES: u16 = 0x3FFF;

    // DMACON bits Paula cares about.
    pub const DMA_AUD0: u16 = 0x0001;
    pub const DMA_AUD1: u16 = 0x0002;
    pub const DMA_AUD2: u16 = 0x0004;
    pub const DMA_AUD3: u16 = 0x0008;
    pub const DMA_DSK: u16 = 0x0010;
    pub const DMA_MASTER: u16 = 0x0200;

    /// Per-channel audio DMA enable masks (indexed 0..=3).
    pub const DMA_AUD: [u16; 4] = [DMA_AUD0, DMA_AUD1, DMA_AUD2, DMA_AUD3];

    // DSKLEN bits.
    pub const DSKLEN_DMAEN: u16 = 0x8000;
    pub const DSKLEN_WRITE: u16 = 0x4000;

    // DSKBYTR read fields.
    pub const DSKBYTR_DSKBYT: u16 = 0x8000;
    pub const DSKBYTR_DMAON: u16 = 0x4000;
    pub const DSKBYTR_DISKWRITE: u16 = 0x2000;
    pub const DSKBYTR_WORDEQUAL: u16 = 0x1000;
    pub const DSKBYTR_DATA_MASK: u16 = 0x00FF;

    // SERDATR read fields (HRM §6 — Serial Port Hardware).
    pub const SERDATR_OVRUN: u16 = 0x8000; // receive overrun
    pub const SERDATR_RBF: u16 = 0x4000; // receive buffer full
    pub const SERDATR_TBE: u16 = 0x2000; // transmit buffer empty
    pub const SERDATR_TSRE: u16 = 0x1000; // transmit shift register empty
    pub const SERDATR_DATA_MASK: u16 = 0x00FF;

    // SERPER ($032 write) — 8-bit vs 9-bit selector + baud divisor.
    pub const SERPER_LONG: u16 = 0x8000; // 1 = 9 data bits, 0 = 8

    // POTGO / POTGOR pin fields (HRM §6 Controller I/O).
    //
    //   bit 15  OUTRY  — port 1 Y-pin output-enable
    //   bit 14  DATRY  — port 1 Y-pin data (output) / level (input)
    //   bit 13  OUTLY  — port 1 X-pin output-enable  (often the "right mouse button" input)
    //   bit 12  DATLY  — port 1 X-pin data / level
    //   bit 11  OUTRX  — port 0 Y-pin output-enable
    //   bit 10  DATRX  — port 0 Y-pin data / level   (often the "middle mouse button" input)
    //   bit  9  OUTLX  — port 0 X-pin output-enable
    //   bit  8  DATLX  — port 0 X-pin data / level
    //   bit  0  START  — begin a new charge cycle (write side only)
    pub const POTGO_START: u16 = 0x0001;
    pub const POTGO_OUTRY: u16 = 0x8000;
    pub const POTGO_DATRY: u16 = 0x4000;
    pub const POTGO_OUTLY: u16 = 0x2000;
    pub const POTGO_DATLY: u16 = 0x1000;
    pub const POTGO_OUTRX: u16 = 0x0800;
    pub const POTGO_DATRX: u16 = 0x0400;
    pub const POTGO_OUTLX: u16 = 0x0200;
    pub const POTGO_DATLX: u16 = 0x0100;
    /// Convenient masks for the four pot pins (input readback in POTGOR
    /// uses the DAT bits to report the current pin level).
    pub const POTGOR_BTN_PORT0_MIDDLE: u16 = POTGO_DATRX;
    pub const POTGOR_BTN_PORT0_RIGHT: u16 = POTGO_DATLX;
    pub const POTGOR_BTN_PORT1_MIDDLE: u16 = POTGO_DATRY;
    pub const POTGOR_BTN_PORT1_RIGHT: u16 = POTGO_DATLY;
    /// DAT bit mask for all four pot pins in POTGOR.
    pub const POTGOR_DAT_ALL: u16 = POTGO_DATRY | POTGO_DATLY | POTGO_DATRX | POTGO_DATLX;

    // ADKCON bits Paula uses.
    //
    // Bit positions per HRM ch 5 (Disk Hardware) + amiga-custom-chips
    // reference: bits 14..8 are PRECOMP1, PRECOMP0, MFMPREC, UARTBRK,
    // WORDSYNC, MSBSYNC, FAST. Earlier versions of this file had every
    // upper constant shifted down by one bit (treating bit 8 as both
    // FAST and MSBSYNC), which silently broke WORDSYNC: KS 1.3 sets
    // bit 10 to enable disk-DMA sync gating, but the comparator
    // checked bit 9 (MSBSYNC) and never raised DSKSYN.
    pub const ADKCON_PRECOMP1: u16 = 0x4000;
    pub const ADKCON_PRECOMP0: u16 = 0x2000;
    pub const ADKCON_MFMPREC: u16 = 0x1000;
    pub const ADKCON_UARTBRK: u16 = 0x0800;
    pub const ADKCON_WORDSYNC: u16 = 0x0400;
    pub const ADKCON_MSBSYNC: u16 = 0x0200;
    pub const ADKCON_FAST: u16 = 0x0100;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum IntSource {
    Tbe = 0,
    DskBlk = 1,
    Soft = 2,
    Ports = 3,
    Coper = 4,
    Vertb = 5,
    Blit = 6,
    Aud0 = 7,
    Aud1 = 8,
    Aud2 = 9,
    Aud3 = 10,
    Rbf = 11,
    DskSyn = 12,
    Exter = 13,
}

impl IntSource {
    #[must_use]
    pub fn mask(self) -> u16 {
        1 << (self as u8)
    }
}

/// One of the six per-channel audio registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum AudioField {
    LcHi = 0,
    LcLo = 1,
    Len = 2,
    Per = 3,
    Vol = 4,
    Dat = 5,
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

/// Host-side Paula audio channel identifier.
///
/// These controls are deliberately outside the emulated register surface:
/// muting a channel here does not change AUDxVOL, AUDxDAT, DMA, IRQs, or
/// ADKCON modulation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PaulaChannel {
    /// Audio channel 0, routed left on OCS.
    Channel0,
    /// Audio channel 1, routed right on OCS.
    Channel1,
    /// Audio channel 2, routed right on OCS.
    Channel2,
    /// Audio channel 3, routed left on OCS.
    Channel3,
}

impl PaulaChannel {
    const fn index(self) -> usize {
        match self {
            Self::Channel0 => 0,
            Self::Channel1 => 1,
            Self::Channel2 => 2,
            Self::Channel3 => 3,
        }
    }

    /// Human-readable channel label for frontend status messages.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Channel0 => "channel 0",
            Self::Channel1 => "channel 1",
            Self::Channel2 => "channel 2",
            Self::Channel3 => "channel 3",
        }
    }
}

/// Per-channel host mixer control.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ChannelControl {
    enabled: bool,
    gain: f32,
}

impl Default for ChannelControl {
    fn default() -> Self {
        Self {
            enabled: true,
            gain: 1.0,
        }
    }
}

impl ChannelControl {
    /// Whether this channel contributes to host audio output.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Linear channel gain after sanitisation, clamped to 0.0..=1.0.
    #[must_use]
    pub const fn gain(self) -> f32 {
        self.gain
    }

    fn apply(self, sample: f32) -> f32 {
        if self.enabled {
            sample * sanitize_gain(self.gain)
        } else {
            0.0
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn set_gain(&mut self, gain: f32) {
        self.gain = sanitize_gain(gain);
    }
}

/// Host-side audio controls for Paula output.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AudioControls {
    master_gain: f32,
    channels: [ChannelControl; 4],
}

impl Default for AudioControls {
    fn default() -> Self {
        Self {
            master_gain: 1.0,
            channels: [ChannelControl::default(); 4],
        }
    }
}

impl AudioControls {
    /// Master gain applied to Paula's host output.
    #[must_use]
    pub const fn master_gain(self) -> f32 {
        self.master_gain
    }

    /// Set master gain. Non-finite values become 0.0; finite values clamp to
    /// 0.0..=1.0.
    pub fn set_master_gain(&mut self, gain: f32) {
        self.master_gain = sanitize_gain(gain);
    }

    /// Return control state for one Paula channel.
    #[must_use]
    pub const fn channel(self, channel: PaulaChannel) -> ChannelControl {
        self.channels[channel.index()]
    }

    /// Enable or disable one Paula channel in the host mixer.
    pub fn set_channel_enabled(&mut self, channel: PaulaChannel, enabled: bool) {
        self.channels[channel.index()].set_enabled(enabled);
    }

    /// Set one Paula channel gain. Non-finite values become 0.0; finite values
    /// clamp to 0.0..=1.0.
    pub fn set_channel_gain(&mut self, channel: PaulaChannel, gain: f32) {
        self.channels[channel.index()].set_gain(gain);
    }

    fn sanitized(mut self) -> Self {
        self.master_gain = sanitize_gain(self.master_gain);
        for channel in &mut self.channels {
            channel.set_gain(channel.gain);
        }
        self
    }
}

fn sanitize_gain(gain: f32) -> f32 {
    if gain.is_finite() {
        gain.clamp(0.0, 1.0)
    } else {
        0.0
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

static DAC_TABLE: std::sync::LazyLock<[f32; 256]> = std::sync::LazyLock::new(build_dac_table);

// ─────────────────────────────────────────────────────────────────────
// Audio channel
// ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
        if self.len_words == 0 {
            65_536
        } else {
            u32::from(self.len_words)
        }
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

        let byte = if self.next_byte_is_hi {
            (word >> 8) as u8
        } else {
            word as u8
        };
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioChannelSnapshot {
    pub period: u16,
    pub volume: u8,
    pub sample: i8,
}

// ─────────────────────────────────────────────────────────────────────
// Paula8364 — main type
// ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize)]
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
    audio_controls: AudioControls,

    // ── Serial (UART) ─────────────────────────────────────────────
    // Per HRM §6. Paula owns a byte-level UART with programmable baud
    // and 8- or 9-bit data. For now we model the register surface and
    // the two IRQs (TBE transmit-buffer-empty, RBF receive-buffer-full)
    // without per-bit timing: writes to SERDAT raise INT_TBE on the
    // next IPL sample, and `receive_serial_byte` queues one byte for
    // the CPU to read via SERDATR.
    serdat: u16,
    serper: u16,
    /// Latest received byte (nine-bit in LONG mode — we only model the
    /// low 8 bits; bit 8 is stored but not exercised).
    serial_rx_byte: u16,
    serial_rx_full: bool,
    serial_rx_overrun: bool,

    // ── POTGO / POTxDAT ────────────────────────────────────────────
    /// Last POTGO write. Mostly stored for readback; the start bit is
    /// a strobe and does not latch into the register.
    potgo: u16,
    /// Live level on the four pot pins (port 0 X/Y and port 1 X/Y).
    /// Defaults to all-high; future peripheral code updates via
    /// `set_pot_pin_level`.
    pot_pin_levels: u16,
    /// Per-port proportional-input counter. In real hardware these
    /// rise from 0 to the time taken for the pot RC network to charge;
    /// software reads them after starting a charge cycle (via
    /// `POTGO.START`) to decide the paddle position.
    pot0dat: u16,
    pot1dat: u16,

    // Diagnostic logs — not part of the chip's behavioural contract.
    intena_write_log: VecDeque<u16>,
    intreq_write_log: VecDeque<u16>,
    disk_write_dma_log: Vec<u16>,
    disk_write_pio_log: Vec<u16>,
}

impl Default for Paula8364 {
    fn default() -> Self {
        Self::new()
    }
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
            audio_controls: AudioControls::default(),
            serdat: 0,
            serper: 0,
            serial_rx_byte: 0,
            serial_rx_full: false,
            serial_rx_overrun: false,
            potgo: 0,
            pot_pin_levels: POTGOR_DAT_ALL,
            pot0dat: 0,
            pot1dat: 0,
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

    #[must_use]
    pub fn intena(&self) -> u16 {
        self.intena
    }
    #[must_use]
    pub fn intreq(&self) -> u16 {
        self.intreq
    }
    #[must_use]
    pub fn adkcon(&self) -> u16 {
        self.adkcon
    }

    /// Compute the 68000 IPL this chipset is requesting. Bit 14
    /// (INTEN) gates everything; per-level priority per HRM Table 3-3.
    #[must_use]
    pub fn compute_ipl(&self) -> u8 {
        if self.intena & INT_INTEN == 0 {
            return 0;
        }
        let active = self.intena & self.intreq & INT_SOURCES;
        if active & INT_EXTER != 0 {
            6
        } else if active & (INT_RBF | INT_DSKSYN) != 0 {
            5
        } else if active & (INT_AUD0 | INT_AUD1 | INT_AUD2 | INT_AUD3) != 0 {
            4
        } else if active & (INT_COPER | INT_VERTB | INT_BLIT) != 0 {
            3
        } else if active & INT_PORTS != 0 {
            2
        } else if active & (INT_TBE | INT_DSKBLK | INT_SOFT) != 0 {
            1
        } else {
            0
        }
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
        let Some(channel) = self.audio.get_mut(ch as usize) else {
            return;
        };
        match field {
            AudioField::LcHi => channel.lc = (channel.lc & 0x0000_FFFF) | (u32::from(val) << 16),
            AudioField::LcLo => channel.lc = (channel.lc & 0xFFFF_0000) | u32::from(val & 0xFFFE),
            AudioField::Len => channel.len_words = val,
            AudioField::Per => channel.write_period(val),
            AudioField::Vol => channel.write_volume(val),
            AudioField::Dat => channel.write_dat(val),
        }
    }

    #[must_use]
    pub fn read_audio(&self, ch: u8, field: AudioField) -> u16 {
        let Some(channel) = self.audio.get(ch as usize) else {
            return 0;
        };
        match field {
            AudioField::LcHi => (channel.lc >> 16) as u16,
            AudioField::LcLo => (channel.lc & 0xFFFF) as u16,
            AudioField::Len => channel.len_words,
            AudioField::Per => channel.per,
            AudioField::Vol => u16::from(channel.vol),
            AudioField::Dat => channel.dat,
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

    /// Current host-side audio controls.
    #[must_use]
    pub const fn audio_controls(&self) -> AudioControls {
        self.audio_controls
    }

    /// Replace all host-side audio controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.audio_controls = controls.sanitized();
    }

    /// Enable or disable one Paula channel in the host mixer.
    pub fn set_audio_channel_enabled(&mut self, channel: PaulaChannel, enabled: bool) {
        self.audio_controls.set_channel_enabled(channel, enabled);
    }

    /// Set one Paula channel's host mixer gain.
    pub fn set_audio_channel_gain(&mut self, channel: PaulaChannel, gain: f32) {
        self.audio_controls.set_channel_gain(channel, gain);
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
            let dma_enabled = (dmacon & DMA_MASTER) != 0 && (dmacon & DMA_AUD[index]) != 0;
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
            if self.audio_channel_is_modulator(i) {
                0.0
            } else {
                self.audio_controls.channels[i].apply(self.audio[i].mix_sample())
            }
        };
        let master_gain = self.audio_controls.master_gain();
        let left = (s(0) + s(3)) * 0.5 * master_gain;
        let right = (s(1) + s(2)) * 0.5 * master_gain;
        (left.clamp(-1.0, 1.0), right.clamp(-1.0, 1.0))
    }

    fn audio_channel_is_modulator(&self, index: usize) -> bool {
        (self.adkcon & ADKCON_USE_VOL[index] != 0) || (self.adkcon & ADKCON_USE_PER[index] != 0)
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
    pub fn dskdat_queue_len(&self) -> usize {
        self.dskdat_queue.len()
    }

    pub fn set_dsksync(&mut self, val: u16) {
        self.dsksync = val;
    }

    #[must_use]
    pub fn dsksync(&self) -> u16 {
        self.dsksync
    }
    #[must_use]
    pub fn dsklen(&self) -> u16 {
        self.dsklen
    }
    #[must_use]
    pub fn dskdat(&self) -> u16 {
        self.dskdat
    }
    #[must_use]
    pub fn dskdatr(&self) -> u16 {
        self.dskdatr
    }
    #[must_use]
    pub fn disk_dma_pending(&self) -> bool {
        self.disk_dma_pending
    }

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
        self.dskbytr_wordequal_delay_cck = if wordequal {
            self.disk_byte_cck_delay()
        } else {
            0
        };
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
        let dmaon = (self.dsklen & DSKLEN_DMAEN != 0)
            && (dmacon & (DMA_MASTER | DMA_DSK)) == (DMA_MASTER | DMA_DSK);
        let diskwrite = self.dsklen & DSKLEN_WRITE != 0;
        let mut value = u16::from(self.dskbytr_data);
        if self.dskbytr_valid {
            value |= DSKBYTR_DSKBYT;
        }
        if dmaon {
            value |= DSKBYTR_DMAON;
        }
        if diskwrite {
            value |= DSKBYTR_DISKWRITE;
        }
        if self.dskbytr_wordequal {
            value |= DSKBYTR_WORDEQUAL;
        }
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
        if self.adkcon & ADKCON_FAST != 0 {
            DISK_BYTE_CCK_FAST
        } else {
            DISK_BYTE_CCK_SLOW
        }
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

    pub fn disk_pll_reset(&mut self) {
        self.disk_pll_phase = 0;
    }

    pub fn set_disk_pll_variable_rate(&mut self, enabled: bool) {
        self.disk_pll_variable_rate = enabled;
    }

    #[must_use]
    pub fn disk_pll_variable_rate(&self) -> bool {
        self.disk_pll_variable_rate
    }

    // ─── Serial UART ──────────────────────────────────────────────────

    /// CPU write to SERDAT (\$030). Starts the transmitter; we model
    /// completion as instantaneous at the chip level — INT_TBE is
    /// raised immediately so any driver using "write byte, wait for
    /// TBE IRQ" progresses. SERDATR.TBE + TSRE stay set throughout.
    pub fn write_serdat(&mut self, val: u16) {
        self.serdat = val;
        self.raise(IntSource::Tbe);
    }

    /// CPU write to SERPER (\$032). Baud-rate divisor + LONG flag.
    pub fn write_serper(&mut self, val: u16) {
        self.serper = val;
    }

    #[must_use]
    pub fn serdat(&self) -> u16 {
        self.serdat
    }
    #[must_use]
    pub fn serper(&self) -> u16 {
        self.serper
    }

    /// Read SERDATR (\$018) with its HRM side effects: RBF + OVRUN
    /// clear on a successful read. Returns the composite status word
    /// with data bits in the low byte.
    pub fn read_serdatr(&mut self) -> u16 {
        let value = self.peek_serdatr();
        self.serial_rx_full = false;
        self.serial_rx_overrun = false;
        value
    }

    /// Side-effect-free SERDATR view.
    #[must_use]
    pub fn peek_serdatr(&self) -> u16 {
        let mut v = self.serial_rx_byte & SERDATR_DATA_MASK;
        if self.serial_rx_overrun {
            v |= SERDATR_OVRUN;
        }
        if self.serial_rx_full {
            v |= SERDATR_RBF;
        }
        // Transmitter is modelled as always idle after a write.
        v | SERDATR_TBE | SERDATR_TSRE
    }

    /// Inject an incoming serial byte — the hook a future serial
    /// peripheral (modem, MIDI, null-modem pair, etc.) calls on each
    /// received byte. Raises INT_RBF and sets OVRUN if RBF was still
    /// pending from a previous unread byte.
    pub fn receive_serial(&mut self, byte: u8) {
        if self.serial_rx_full {
            self.serial_rx_overrun = true;
        }
        self.serial_rx_byte = u16::from(byte);
        self.serial_rx_full = true;
        self.raise(IntSource::Rbf);
    }

    // ─── POTGO / POTxDAT ──────────────────────────────────────────────

    /// CPU write to POTGO (\$034). The START bit is a strobe; we zero
    /// the pot counters as a real charge cycle would.
    pub fn write_potgo(&mut self, val: u16) {
        self.potgo = val & !POTGO_START;
        if val & POTGO_START != 0 {
            self.pot0dat = 0;
            self.pot1dat = 0;
        }
    }

    /// Side-effect-free POTGOR (\$016) read — returns the OUT bits
    /// as last written plus the live DAT pin levels (button state or
    /// driven-output value, depending on direction).
    #[must_use]
    pub fn peek_potgor(&self) -> u16 {
        let out_bits = self.potgo & (POTGO_OUTRY | POTGO_OUTLY | POTGO_OUTRX | POTGO_OUTLX);
        // For each DAT bit: if the pin is configured as output, report
        // the latched POTGO data; if input, report the live pin level.
        let mut dat = 0u16;
        for (out_mask, dat_mask) in [
            (POTGO_OUTRY, POTGO_DATRY),
            (POTGO_OUTLY, POTGO_DATLY),
            (POTGO_OUTRX, POTGO_DATRX),
            (POTGO_OUTLX, POTGO_DATLX),
        ] {
            let pin = if self.potgo & out_mask != 0 {
                self.potgo & dat_mask
            } else {
                self.pot_pin_levels & dat_mask
            };
            dat |= pin;
        }
        out_bits | dat
    }

    #[must_use]
    pub fn potgo(&self) -> u16 {
        self.potgo
    }
    #[must_use]
    pub fn pot0dat(&self) -> u16 {
        self.pot0dat
    }
    #[must_use]
    pub fn pot1dat(&self) -> u16 {
        self.pot1dat
    }

    /// Inject the live level of one of the four pot pins. `mask` must
    /// be one of `POTGOR_BTN_*` / `POTGO_DAT*` constants. `high = true`
    /// is the idle/released state; `false` is pulled low (button press
    /// or driven 0).
    pub fn set_pot_pin_level(&mut self, mask: u16, high: bool) {
        if high {
            self.pot_pin_levels |= mask;
        } else {
            self.pot_pin_levels &= !mask;
        }
    }

    /// Inject pot-counter values (for tests and future paddle / light
    /// pen peripherals). 10-bit saturating per HRM.
    pub fn set_pot_data(&mut self, port: u8, value: u16) {
        match port {
            0 => self.pot0dat = value & 0x03FF,
            1 => self.pot1dat = value & 0x03FF,
            _ => {}
        }
    }

    // ─── Diagnostic logs (not behavioural) ────────────────────────────

    /// Append an observed disk-write DMA word to the diagnostic log.
    /// Floppy peripheral calls this as it consumes DSKDAT via DMA.
    pub fn note_disk_write_dma_word(&mut self, word: u16) {
        self.disk_write_dma_log.push(word);
    }

    /// Append an observed disk-write PIO word.
    pub fn note_disk_write_pio_word(&mut self, word: u16) {
        self.disk_write_pio_log.push(word);
    }

    #[must_use]
    pub fn debug_disk_write_dma_log(&self) -> &[u16] {
        &self.disk_write_dma_log
    }

    #[must_use]
    pub fn debug_disk_write_pio_log(&self) -> &[u16] {
        &self.disk_write_pio_log
    }

    pub fn clear_debug_disk_write_dma_log(&mut self) {
        self.disk_write_dma_log.clear();
    }
    pub fn clear_debug_disk_write_pio_log(&mut self) {
        self.disk_write_pio_log.clear();
    }

    /// Most-recent INTENA writes (up to 16 deep, oldest first).
    #[must_use]
    pub fn debug_intena_writes(&self) -> &VecDeque<u16> {
        &self.intena_write_log
    }

    /// Most-recent INTREQ writes (up to 16 deep, oldest first).
    #[must_use]
    pub fn debug_intreq_writes(&self) -> &VecDeque<u16> {
        &self.intreq_write_log
    }
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

    #[test]
    fn host_audio_controls_do_not_change_audio_registers() {
        let mut p = Paula8364::new();
        p.write_audio(0, AudioField::Vol, 64);
        p.write_audio(0, AudioField::Dat, 0x7F00);

        p.set_audio_channel_enabled(PaulaChannel::Channel0, false);

        assert!(!p.audio_controls().channel(PaulaChannel::Channel0).enabled());
        assert_eq!(p.read_audio(0, AudioField::Vol), 64);
        assert_eq!(p.read_audio(0, AudioField::Dat), 0x7F00);
    }

    #[test]
    fn host_audio_controls_mute_channel_output_only() {
        let mut p = Paula8364::new();
        p.audio[0].output_sample = 127;
        p.audio[0].vol = 64;

        let (left, right) = p.mix_audio_stereo();
        assert!(left > 0.4);
        assert_eq!(right, 0.0);

        p.set_audio_channel_enabled(PaulaChannel::Channel0, false);
        let (muted_left, muted_right) = p.mix_audio_stereo();
        assert_eq!(muted_left, 0.0);
        assert_eq!(muted_right, 0.0);
        assert_eq!(p.audio[0].output_sample, 127);
        assert_eq!(p.audio[0].vol, 64);
    }

    #[test]
    fn host_audio_controls_clamp_gain() {
        let mut controls = AudioControls::default();
        controls.set_master_gain(2.0);
        controls.set_channel_gain(PaulaChannel::Channel2, f32::NAN);
        controls.set_channel_gain(PaulaChannel::Channel3, -1.0);

        assert_eq!(controls.master_gain(), 1.0);
        assert_eq!(controls.channel(PaulaChannel::Channel2).gain(), 0.0);
        assert_eq!(controls.channel(PaulaChannel::Channel3).gain(), 0.0);
    }
}
