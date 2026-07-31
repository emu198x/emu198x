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
    //   bit 13  OUTLY  — port 1 X-pin output-enable
    //   bit 12  DATLY  — port 1 X-pin data / level
    //   bit 11  OUTRX  — port 0 Y-pin output-enable
    //   bit 10  DATRX  — port 0 Y-pin data / level
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
    /// Mouse buttons on POTGOR, per controller port, active-low (idle =
    /// high). The RIGHT button is the port's upper pot bit, the MIDDLE
    /// button the lower: port 0 → RIGHT = bit 10, MIDDLE = bit 8; port 1
    /// → RIGHT = bit 14, MIDDLE = bit 12. Verified against vAmiga
    /// (`Mouse::changePotgo`) and WinUAE (`inputdevice.cpp`); the right
    /// button is Intuition's menu button, so this mapping must be exact.
    pub const POTGOR_BTN_PORT0_MIDDLE: u16 = POTGO_DATLX; // bit 8
    pub const POTGOR_BTN_PORT0_RIGHT: u16 = POTGO_DATRX; // bit 10
    pub const POTGOR_BTN_PORT1_MIDDLE: u16 = POTGO_DATLY; // bit 12
    pub const POTGOR_BTN_PORT1_RIGHT: u16 = POTGO_DATRY; // bit 14
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
    /// Encoded-byte interval in FAST's normal 2 µs MFM bit-cell mode.
    ///
    /// Eight 2 µs cells are about 56 PAL colour clocks. FAST names the
    /// disk data clock, not a multiplier applied to an already-normal MFM
    /// stream.
    pub const DISK_BYTE_CCK_FAST: u8 = 56;
    /// Encoded-byte interval in the 4 µs GCR-compatible slow mode.
    pub const DISK_BYTE_CCK_SLOW: u8 = 112;
}

use bits::*;

// ─────────────────────────────────────────────────────────────────────
// Interrupt source enum + audio field enum (typed register API)
// ─────────────────────────────────────────────────────────────────────

/// Number of complete words in Paula's disk-DMA FIFO.
pub const DISK_DMA_FIFO_WORD_CAPACITY: usize = 3;

/// Direction of the words currently retained by Paula's disk-DMA FIFO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskDmaFifoDirection {
    /// Encoded words received from the drive and awaiting chip-RAM writes.
    Read,
    /// Words fetched from chip RAM and awaiting delivery to the drive.
    Write,
}

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
    /// Audio channel 0, routed right on OCS.
    Channel0,
    /// Audio channel 1, routed left on OCS.
    Channel1,
    /// Audio channel 2, routed left on OCS.
    Channel2,
    /// Audio channel 3, routed right on OCS.
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

/// Paula's per-channel audio DMA state machine (HRM ch. 5 / vAmiga
/// `StateMachine.cpp`). The HRM encodes states as 3-bit codes; vAmiga
/// uses five (`000/001/010/011/101`). We model the *startup* sequence
/// explicitly — `Idle`/`WaitWord1`/`WaitWord2` — and collapse the
/// steady-state output codes `010`/`011` into `Playing`, where the
/// existing period-counter + hi/lo byte stepping already reproduces
/// the `010↔011` loop. `next_byte_is_hi` distinguishes the two within
/// `Playing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum AudioState {
    /// `000` — DMA off. Idle, or CPU-driven AUDxDAT playback.
    #[default]
    Idle,
    /// `001` — DMA enabled; word 1 requested, awaiting its arrival.
    WaitWord1,
    /// `101` — word 1 arrived (a dummy fetch, discarded); word 2
    /// requested, awaiting it.
    WaitWord2,
    /// `010`/`011` — actively producing audio.
    Playing,
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
    state: AudioState,
    dma_active: bool,
    dma_enabled_prev: bool,
    dma_requests_pending: u8,
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
            state: AudioState::Idle,
            dma_active: false,
            dma_enabled_prev: false,
            dma_requests_pending: 0,
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

    /// `000 → 001`: DMA enabled while idle. Reload the length counter,
    /// point at the start of the sample, and request the first DMA word
    /// (`AUDxDR`). No interrupt fires here — the DMA-enable edge raises
    /// none. The first `AUDxIR` fires when word 1 arrives (`001 → 101`).
    /// The period counter is *not* loaded yet; that happens when the
    /// real sample word lands (`101 → 010`).
    fn start_dma(&mut self) {
        self.ptr = self.lc & 0x00FF_FFFE;
        self.words_remaining = self.programmed_length_words();
        self.current_word = None;
        self.next_word = None;
        self.next_byte_is_hi = true;
        self.dma_active = true;
        self.state = AudioState::WaitWord1;
        self.dma_requests_pending = 1;
    }

    fn stop_dma(&mut self) {
        self.dma_active = false;
        self.state = AudioState::Idle;
        self.current_word = None;
        self.next_word = None;
        self.next_byte_is_hi = true;
        self.dma_requests_pending = 0;
    }

    fn sync_dma_enable(&mut self, enabled: bool) {
        if enabled && !self.dma_enabled_prev {
            self.start_dma();
        } else if !enabled && self.dma_enabled_prev {
            self.stop_dma();
        }
        self.dma_enabled_prev = enabled;
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

    /// Service this channel's granted audio DMA slot. The word arrives
    /// the colour-clock Agnus grants the slot — that latency *is* the
    /// real bus latency, scheduled by the single-slot authority (#30) at
    /// hpos 0x0D/0F/11/13. There is no separate post-fetch countdown.
    ///
    /// The fetch advances the startup state machine and returns `true`
    /// when an audio interrupt should be raised this CCK.
    fn service_dma_slot<F>(&mut self, read_chip_byte: F) -> bool
    where
        F: FnMut(u32) -> u8,
    {
        if self.dma_requests_pending == 0 {
            return false;
        }
        let Some((word, wrapped)) = self.fetch_dma_word(read_chip_byte) else {
            return false;
        };
        self.dma_requests_pending = self.dma_requests_pending.saturating_sub(1);

        match self.state {
            AudioState::WaitWord1 => {
                // 001 → 101: word 1 is a dummy fetch. Discard the data,
                // reset the location pointer (AUDxDSR), request word 2,
                // and raise the startup interrupt (the CPU uses it to
                // swap double-buffer pointers).
                self.ptr = self.lc & 0x00FF_FFFE;
                self.words_remaining = self.programmed_length_words();
                self.state = AudioState::WaitWord2;
                self.dma_requests_pending = self.dma_requests_pending.saturating_add(1);
                true
            }
            AudioState::WaitWord2 => {
                // 101 → 010: the real first sample word. Load the period
                // counter and volume context, request the next word, and
                // output the high byte immediately (penhi). The period
                // counter then times the high → low byte step.
                self.period_counter = self.effective_period();
                self.current_word = Some(word);
                self.output_sample = (word >> 8) as u8 as i8;
                self.next_byte_is_hi = false;
                self.state = AudioState::Playing;
                self.dma_requests_pending = self.dma_requests_pending.saturating_add(1);
                false
            }
            AudioState::Playing => {
                // Steady state: top the 2-deep buffer up as it drains.
                // `wrapped` reports a length-counter wrap (loop point) —
                // that raises the per-buffer interrupt.
                self.push_dma_word(word);
                wrapped
            }
            AudioState::Idle => false,
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

/// Side-effect-free snapshot of Paula's interrupt and shared-control registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaulaInterruptDiagnosticSnapshot {
    /// Raw INTENA interrupt-enable register.
    pub intena: u16,
    /// Raw INTREQ interrupt-request register.
    pub intreq: u16,
    /// Raw ADKCON audio, disk, and UART control register.
    pub adkcon: u16,
    /// Unmasked interrupt sources that are currently pending.
    pub active_sources: u16,
    /// Interrupt priority level derived from the current INTENA and INTREQ state.
    pub ipl: u8,
}

/// Diagnostic name for one audio channel's internal DMA/playback state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaulaAudioDmaState {
    /// DMA is disabled, or CPU-written AUDxDAT playback is active.
    Idle,
    /// DMA was enabled and the channel is waiting for its dummy first word.
    WaitWord1,
    /// The dummy word arrived and the channel is waiting for its first sample.
    WaitWord2,
    /// The channel is producing bytes from its current and next words.
    Playing,
}

/// Side-effect-free snapshot of one Paula audio channel.
///
/// This exposes the complete implemented register, DMA, buffering, timing, and
/// playback state. Register values retain their raw programmed form while
/// derived fields such as [`Self::effective_period`] report the value used by
/// the playback engine.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PaulaAudioChannelDiagnosticSnapshot {
    /// Raw AUDxLC location register.
    pub location: u32,
    /// Current DMA fetch pointer.
    pub dma_pointer: u32,
    /// Raw AUDxLEN word count.
    pub length_words: u16,
    /// AUDxLEN interpreted by Paula, where zero represents 65,536 words.
    pub programmed_length_words: u32,
    /// Words remaining before the DMA buffer reloads.
    pub words_remaining: u32,
    /// Raw AUDxPER period register.
    pub period: u16,
    /// Period used by playback after applying Paula's minimum.
    pub effective_period: u16,
    /// Clamped AUDxVOL value.
    pub volume: u8,
    /// Stored AUDxDAT register/latch.
    pub data: u16,
    /// Word currently feeding the output byte latch.
    pub current_word: Option<u16>,
    /// Prefetched word waiting behind `current_word`.
    pub next_word: Option<u16>,
    /// Whether the next output transition selects the current word's high byte.
    pub next_byte_is_high: bool,
    /// CCKs remaining until the next output transition.
    pub period_counter: u16,
    /// Current signed eight-bit DAC sample latch.
    pub output_sample: i8,
    /// Current DMA/playback state-machine state.
    pub state: PaulaAudioDmaState,
    /// Whether the channel's DMA playback path is active.
    pub dma_active: bool,
    /// DMA-enable level retained from the previous audio tick.
    pub dma_enabled_previous: bool,
    /// Number of channel DMA requests waiting for an Agnus slot.
    pub dma_requests_pending: u8,
    /// Whether this channel modulates the next channel's period.
    pub period_modulation_enabled: bool,
    /// Whether this channel modulates the next channel's volume.
    pub volume_modulation_enabled: bool,
    /// Host-side mixer control applied to this channel.
    pub host_control: ChannelControl,
}

/// Side-effect-free snapshot of Paula's four audio channels and host controls.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PaulaAudioDiagnosticSnapshot {
    /// Complete channel state in AUD0 through AUD3 order.
    pub channels: [PaulaAudioChannelDiagnosticSnapshot; 4],
    /// Global and per-channel host mixer controls.
    pub controls: AudioControls,
}

/// Side-effect-free snapshot of Paula's implemented UART state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaulaSerialDiagnosticSnapshot {
    /// Raw SERDAT transmit register.
    pub serdat: u16,
    /// Raw SERPER baud and word-length register.
    pub serper: u16,
    /// Composite SERDATR value without applying its read side effects.
    pub serdatr: u16,
    /// Raw receive-data latch.
    pub receive_data: u16,
    /// Whether the receive-data latch is full.
    pub receive_full: bool,
    /// Whether an unread receive byte was overwritten.
    pub receive_overrun: bool,
}

/// Side-effect-free snapshot of Paula's implemented pot-port state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaulaPotDiagnosticSnapshot {
    /// Latched POTGO value; the START strobe is not retained.
    pub potgo: u16,
    /// Raw externally supplied levels for the four pot pins.
    pub raw_pin_levels: u16,
    /// Composite POTGOR value derived from direction, output, and raw pin state.
    pub potgor: u16,
    /// Port 0 proportional-input counter.
    pub pot0dat: u16,
    /// Port 1 proportional-input counter.
    pub pot1dat: u16,
}

/// Side-effect-free summary of Paula's component-owned diagnostic logs.
///
/// Counts describe the entries currently retained by each log. INTENA and
/// INTREQ logs are bounded to their most recent 16 writes; disk-write logs
/// retain all entries until explicitly cleared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaulaLogDiagnosticSnapshot {
    /// Retained INTENA writes in oldest-to-newest order.
    pub intena_writes: Vec<u16>,
    /// Number of retained INTENA writes.
    pub intena_write_count: usize,
    /// Most recent INTENA write.
    pub last_intena_write: Option<u16>,
    /// Retained INTREQ writes in oldest-to-newest order.
    pub intreq_writes: Vec<u16>,
    /// Number of retained INTREQ writes.
    pub intreq_write_count: usize,
    /// Most recent INTREQ write.
    pub last_intreq_write: Option<u16>,
    /// Number of retained disk write-DMA words.
    pub disk_write_dma_count: usize,
    /// Most recent disk write-DMA word.
    pub last_disk_write_dma_word: Option<u16>,
    /// Number of retained disk PIO writes.
    pub disk_write_pio_count: usize,
    /// Most recent disk PIO write.
    pub last_disk_write_pio_word: Option<u16>,
}

/// Side-effect-free snapshot of Paula's implemented floppy-disk state.
///
/// The queued DSKDAT words are copied in consumer order so debuggers and
/// traces can inspect the queue without draining it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaulaDiskDiagnosticSnapshot {
    /// Raw DSKLEN register.
    pub dsklen: u16,
    /// Raw DSKSYNC register.
    pub dsksync: u16,
    /// Most recently received disk word.
    pub dskdatr: u16,
    /// Most recently written DSKDAT word.
    pub dskdat: u16,
    /// Byte currently presented in DSKBYTR.
    pub dskbytr_data: u8,
    /// Low byte waiting to advance into DSKBYTR.
    pub dskbytr_next_data: Option<u8>,
    /// CCK delay remaining before `dskbytr_next_data` advances.
    pub dskbytr_next_delay_cck: u8,
    /// Whether DSKBYTR.DSKBYT is latched.
    pub dskbytr_valid: bool,
    /// Whether DSKBYTR.WORDEQUAL is latched.
    pub dskbytr_wordequal: bool,
    /// CCK delay remaining before WORDEQUAL clears.
    pub dskbytr_wordequal_delay_cck: u8,
    /// DSKDAT writes waiting for the drive consumer, in dequeue order.
    pub dskdat_queue: Vec<u16>,
    /// Disk-DMA FIFO words in consumer order.
    pub disk_dma_fifo: Vec<u16>,
    /// Direction associated with the retained disk-DMA FIFO words.
    pub disk_dma_fifo_direction: Option<DiskDmaFifoDirection>,
    /// Number of complete words currently retained by the disk-DMA FIFO.
    pub disk_dma_fifo_count: usize,
    /// Whether the disk-DMA FIFO contains no complete words.
    pub disk_dma_fifo_empty: bool,
    /// Whether the disk-DMA FIFO contains all three complete words.
    pub disk_dma_fifo_full: bool,
    /// Whether the DSKLEN arming flip-flop has seen its first DMAEN write.
    pub dsklen_armed: bool,
    /// Whether a disk DMA transfer is pending.
    pub disk_dma_pending: bool,
    /// Number of words remaining in the captured transfer.
    pub disk_dma_words_remaining: u32,
    /// Captured DMA direction; `true` means chip RAM to disk.
    pub disk_dma_is_write: bool,
    /// Whether read DMA is waiting for its first WORDSYNC match.
    pub disk_dma_wordsync_waiting: bool,
    /// Whether a disk write-DMA transfer is currently active.
    pub disk_dma_write_active: bool,
    /// Whether write DMA is active or buffered words still await rotation.
    pub disk_write_stream_active: bool,
    /// Whether DSKLEN.DMAEN is set in the current register value.
    pub dsklen_dma_enabled: bool,
    /// Whether DSKLEN.WRITE is set in the current register value.
    pub dsklen_write_enabled: bool,
    /// Whether ADKCON.WORDSYNC is enabled.
    pub wordsync_enabled: bool,
    /// Whether ADKCON.FAST selects fast disk-byte pacing.
    pub fast_enabled: bool,
    /// Current configured delay between the two bytes of an MFM word.
    pub disk_byte_delay_cck: u8,
    /// Current disk PLL phase accumulator.
    pub disk_pll_phase: u16,
    /// Whether the disk PLL is using variable-rate input.
    pub disk_pll_variable_rate: bool,
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
    /// Three-word Paula disk FIFO, distinct from CPU-written DSKDAT.
    disk_dma_fifo: VecDeque<u16>,
    /// Direction of the transfer represented by `disk_dma_fifo`.
    disk_dma_fifo_direction: Option<DiskDmaFifoDirection>,
    disk_dma_pending: bool,
    /// DSKLEN arming flip-flop per HRM "turn DMAEN on twice" protocol.
    dsklen_armed: bool,
    /// Word countdown for the in-flight DMA transfer. Captured from
    /// `DSKLEN[13:0]` on the second arming write; decremented per
    /// successful chip-RAM write. Zero means "no transfer in flight".
    disk_dma_words_remaining: u32,
    /// `DSKLEN.WRITE` (bit 14) at arm time. CPU-driven write transfers
    /// (chip RAM → drive) suppress the read-side word delivery — the
    /// machine's MFM stream isn't part of a write transfer.
    disk_dma_is_write: bool,
    /// `true` until the first WORDSYNC match is observed. While true,
    /// incoming MFM words still update DSKBYTR/DSKDATR latches but
    /// are *not* written to chip RAM — software sees aligned data
    /// from the sync onward.
    disk_dma_wordsync_waiting: bool,

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
            disk_dma_fifo: VecDeque::with_capacity(DISK_DMA_FIFO_WORD_CAPACITY),
            disk_dma_fifo_direction: None,
            disk_dma_pending: false,
            dsklen_armed: false,
            disk_dma_words_remaining: 0,
            disk_dma_is_write: false,
            disk_dma_wordsync_waiting: false,
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

    /// Return a side-effect-free diagnostic snapshot of Paula's interrupt and
    /// shared-control registers, including the currently active sources and
    /// derived processor interrupt level.
    #[must_use]
    pub fn interrupt_diagnostic_snapshot(&self) -> PaulaInterruptDiagnosticSnapshot {
        PaulaInterruptDiagnosticSnapshot {
            intena: self.intena,
            intreq: self.intreq,
            adkcon: self.adkcon,
            active_sources: self.intena & self.intreq & INT_SOURCES,
            ipl: self.compute_ipl(),
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

    /// Return a side-effect-free diagnostic snapshot of every implemented
    /// audio register, DMA pipeline, playback latch, and host mixer control.
    #[must_use]
    pub fn audio_diagnostic_snapshot(&self) -> PaulaAudioDiagnosticSnapshot {
        let channels = std::array::from_fn(|index| {
            let channel = &self.audio[index];
            let state = match channel.state {
                AudioState::Idle => PaulaAudioDmaState::Idle,
                AudioState::WaitWord1 => PaulaAudioDmaState::WaitWord1,
                AudioState::WaitWord2 => PaulaAudioDmaState::WaitWord2,
                AudioState::Playing => PaulaAudioDmaState::Playing,
            };
            PaulaAudioChannelDiagnosticSnapshot {
                location: channel.lc,
                dma_pointer: channel.ptr,
                length_words: channel.len_words,
                programmed_length_words: channel.programmed_length_words(),
                words_remaining: channel.words_remaining,
                period: channel.per,
                effective_period: channel.effective_period(),
                volume: channel.vol,
                data: channel.dat,
                current_word: channel.current_word,
                next_word: channel.next_word,
                next_byte_is_high: channel.next_byte_is_hi,
                period_counter: channel.period_counter,
                output_sample: channel.output_sample,
                state,
                dma_active: channel.dma_active,
                dma_enabled_previous: channel.dma_enabled_prev,
                dma_requests_pending: channel.dma_requests_pending,
                period_modulation_enabled: self.adkcon & ADKCON_USE_PER[index] != 0,
                volume_modulation_enabled: self.adkcon & ADKCON_USE_VOL[index] != 0,
                host_control: self.audio_controls.channels[index],
            }
        });
        PaulaAudioDiagnosticSnapshot {
            channels,
            controls: self.audio_controls,
        }
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
    /// `ch`'s dedicated DMA slot — Paula services a pending fetch and
    /// the word arrives this CCK (the slot grant *is* the bus latency).
    pub fn tick_audio_cck<F>(
        &mut self,
        dmacon: u16,
        audio_dma_slot: Option<u8>,
        mut read_chip_byte: F,
    ) where
        F: FnMut(u32) -> u8,
    {
        let mut irq_mask: u16 = 0;
        for (index, channel) in self.audio.iter_mut().enumerate() {
            let dma_enabled = (dmacon & DMA_MASTER) != 0 && (dmacon & DMA_AUD[index]) != 0;
            channel.sync_dma_enable(dma_enabled);
        }

        if let Some(ch_u8) = audio_dma_slot
            && let Some(ch) = self.audio.get_mut(ch_u8 as usize)
            && ch.service_dma_slot(&mut read_chip_byte)
        {
            irq_mask |= INT_AUD0 << ch_u8;
        }

        let mut output_events = [None; 4];
        for (index, channel) in self.audio.iter_mut().enumerate() {
            // Skip the playback engine during the DMA startup waits
            // (001/101) — no output until the real sample word lands.
            // Non-DMA (CPU AUDxDAT) playback runs in `Idle` and is not
            // skipped.
            if channel.dma_active && channel.state != AudioState::Playing {
                continue;
            }
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

    /// Mixed stereo output in `[-1.0, 1.0]`. OCS routing: ch 1+2 → L,
    /// ch 0+3 → R. Modulator channels (ADKCON attach) are muted.
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
        let left = (s(1) + s(2)) * 0.5 * master_gain;
        let right = (s(0) + s(3)) * 0.5 * master_gain;
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
                // Second DMAEN write — arm completes. Capture the
                // transfer parameters so subsequent register thrash
                // mid-transfer doesn't retroactively change them.
                self.disk_dma_pending = true;
                self.dsklen_armed = false;
                let word_count = u32::from(val & 0x3FFF);
                let is_write = (val & DSKLEN_WRITE) != 0;
                let wordsync_enabled = !is_write && (self.adkcon & ADKCON_WORDSYNC) != 0;
                self.disk_dma_fifo.clear();
                self.disk_dma_fifo_direction = Some(if is_write {
                    DiskDmaFifoDirection::Write
                } else {
                    DiskDmaFifoDirection::Read
                });
                if word_count == 0 {
                    // Zero-length transfer fires DSKBLK at once per
                    // HRM ("a DSKLEN write with DMAEN set and length=0
                    // fires DSKBLK immediately").
                    self.complete_disk_dma();
                } else {
                    self.disk_dma_words_remaining = word_count;
                    self.disk_dma_is_write = is_write;
                    self.disk_dma_wordsync_waiting = wordsync_enabled;
                }
            } else {
                self.dsklen_armed = true;
            }
        } else {
            self.dsklen_armed = false;
        }
    }

    /// Atomically end the in-flight disk DMA transfer: clear pending
    /// + arming, drop transfer-state, and raise DSKBLK.
    pub fn complete_disk_dma(&mut self) {
        self.disk_dma_pending = false;
        self.dsklen_armed = false;
        self.disk_dma_words_remaining = 0;
        self.disk_dma_is_write = false;
        self.disk_dma_wordsync_waiting = false;
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

    /// Deliver one rotationally paced MFM word from the drive.
    ///
    /// DSKDATR, DSKBYTR, and the sync comparator advance regardless of
    /// whether DMA is armed. An active read transfer additionally queues
    /// the word in Paula's three-word FIFO. While WORDSYNC is waiting,
    /// unmatched words may accumulate but cannot be serviced; the first
    /// matching word clears that alignment data, opens the gate, and is
    /// itself discarded. A full FIFO retains its existing three words.
    pub fn receive_disk_read_word(&mut self, word: u16) {
        let matched_sync = self.note_disk_read_word(word);
        if !self.disk_dma_pending || self.disk_dma_is_write {
            return;
        }

        if self.disk_dma_wordsync_waiting && matched_sync {
            self.disk_dma_fifo.clear();
            self.disk_dma_fifo_direction = Some(DiskDmaFifoDirection::Read);
            self.disk_dma_wordsync_waiting = false;
            return;
        }

        if self.disk_dma_fifo_direction != Some(DiskDmaFifoDirection::Read) {
            self.disk_dma_fifo.clear();
            self.disk_dma_fifo_direction = Some(DiskDmaFifoDirection::Read);
        }
        if self.disk_dma_fifo.len() < DISK_DMA_FIFO_WORD_CAPACITY {
            self.disk_dma_fifo.push_back(word);
        }
    }

    /// Consume one Agnus-granted disk-DMA read slot.
    ///
    /// Returns the oldest queued drive word for the machine to write to
    /// chip RAM and advances the DSKLEN countdown only when a word was
    /// available. WORDSYNC-waiting, write, idle, and FIFO-empty states do
    /// not consume the grant.
    pub fn service_disk_read_dma_slot(&mut self) -> Option<u16> {
        if !self.disk_dma_pending
            || self.disk_dma_is_write
            || self.disk_dma_wordsync_waiting
            || self.disk_dma_fifo_direction != Some(DiskDmaFifoDirection::Read)
        {
            return None;
        }

        let word = self.disk_dma_fifo.pop_front()?;
        self.disk_dma_words_remaining = self.disk_dma_words_remaining.saturating_sub(1);
        if self.disk_dma_words_remaining == 0 {
            self.complete_disk_dma();
        }
        Some(word)
    }

    /// Whether an Agnus-granted disk slot can fetch another write word.
    #[must_use]
    pub fn disk_write_dma_slot_requested(&self) -> bool {
        self.disk_dma_pending
            && self.disk_dma_is_write
            && self.disk_dma_words_remaining > 0
            && self.disk_dma_fifo_direction == Some(DiskDmaFifoDirection::Write)
            && self.disk_dma_fifo.len() < DISK_DMA_FIFO_WORD_CAPACITY
    }

    /// Accept one chip-RAM word fetched during an Agnus-granted disk slot.
    ///
    /// The word enters Paula's FIFO and decrements the DSKLEN countdown only
    /// when the active write transfer requests a slot and the FIFO has room.
    /// The final accepted word raises DSKBLK but remains drainable by the
    /// rotational stream.
    pub fn accept_disk_write_dma_slot(&mut self, word: u16) -> bool {
        if !self.disk_write_dma_slot_requested() {
            return false;
        }

        self.disk_dma_fifo.push_back(word);
        self.disk_dma_words_remaining = self.disk_dma_words_remaining.saturating_sub(1);
        if self.disk_dma_words_remaining == 0 {
            self.complete_disk_dma();
        }
        true
    }

    /// Remove the oldest write-DMA word for rotational delivery to the drive.
    pub fn take_disk_write_stream_word(&mut self) -> Option<u16> {
        if self.disk_dma_fifo_direction != Some(DiskDmaFifoDirection::Write) {
            return None;
        }

        let word = self.disk_dma_fifo.pop_front()?;
        if self.disk_dma_fifo.is_empty() && !self.disk_dma_pending {
            self.disk_dma_fifo_direction = None;
        }
        Some(word)
    }

    /// Whether write DMA is still fetching or has FIFO words left to emit.
    #[must_use]
    pub fn disk_write_stream_active(&self) -> bool {
        self.disk_dma_fifo_direction == Some(DiskDmaFifoDirection::Write)
            && (self.disk_dma_pending || !self.disk_dma_fifo.is_empty())
    }

    /// Compatibility helper for callers that still combine drive arrival
    /// and memory service in one operation.
    ///
    /// New machine integration must call [`receive_disk_read_word`] at the
    /// rotational pace and [`service_disk_read_dma_slot`] only on an Agnus
    /// grant.
    pub fn tick_disk_dma_slot(&mut self, word: u16) -> Option<u16> {
        self.receive_disk_read_word(word);
        self.service_disk_read_dma_slot()
    }

    /// Compatibility helper for callers that still combine a write-DMA
    /// fetch with immediate drive delivery.
    ///
    /// New machine integration must call [`accept_disk_write_dma_slot`] on
    /// an Agnus grant and [`take_disk_write_stream_word`] at the rotational
    /// pace.
    pub fn tick_disk_write_dma_slot(&mut self, word: u16) -> Option<u16> {
        if !self.accept_disk_write_dma_slot(word) {
            return None;
        }
        self.take_disk_write_stream_word()
    }

    /// Whether a disk *write* DMA transfer is in flight, i.e. the
    /// machine should be pulling words from chip RAM at DSKPT and
    /// feeding them to the drive. Idle and read transfers return
    /// `false`. The write analogue of the read path's
    /// `drive.read_data_available()` gate.
    #[must_use]
    pub fn disk_dma_write_active(&self) -> bool {
        self.disk_dma_is_write && self.disk_dma_words_remaining > 0
    }

    /// Return a side-effect-free diagnostic snapshot of all implemented
    /// Paula disk-register, byte-latch, DMA, queue, and PLL state.
    #[must_use]
    pub fn disk_diagnostic_snapshot(&self) -> PaulaDiskDiagnosticSnapshot {
        PaulaDiskDiagnosticSnapshot {
            dsklen: self.dsklen,
            dsksync: self.dsksync,
            dskdatr: self.dskdatr,
            dskdat: self.dskdat,
            dskbytr_data: self.dskbytr_data,
            dskbytr_next_data: self.dskbytr_next_data,
            dskbytr_next_delay_cck: self.dskbytr_next_delay_cck,
            dskbytr_valid: self.dskbytr_valid,
            dskbytr_wordequal: self.dskbytr_wordequal,
            dskbytr_wordequal_delay_cck: self.dskbytr_wordequal_delay_cck,
            dskdat_queue: self.dskdat_queue.iter().copied().collect(),
            disk_dma_fifo: self.disk_dma_fifo.iter().copied().collect(),
            disk_dma_fifo_direction: self.disk_dma_fifo_direction,
            disk_dma_fifo_count: self.disk_dma_fifo.len(),
            disk_dma_fifo_empty: self.disk_dma_fifo.is_empty(),
            disk_dma_fifo_full: self.disk_dma_fifo.len() == DISK_DMA_FIFO_WORD_CAPACITY,
            dsklen_armed: self.dsklen_armed,
            disk_dma_pending: self.disk_dma_pending,
            disk_dma_words_remaining: self.disk_dma_words_remaining,
            disk_dma_is_write: self.disk_dma_is_write,
            disk_dma_wordsync_waiting: self.disk_dma_wordsync_waiting,
            disk_dma_write_active: self.disk_dma_write_active(),
            disk_write_stream_active: self.disk_write_stream_active(),
            dsklen_dma_enabled: self.dsklen & DSKLEN_DMAEN != 0,
            dsklen_write_enabled: self.dsklen & DSKLEN_WRITE != 0,
            wordsync_enabled: self.adkcon & ADKCON_WORDSYNC != 0,
            fast_enabled: self.adkcon & ADKCON_FAST != 0,
            disk_byte_delay_cck: self.disk_byte_cck_delay(),
            disk_pll_phase: self.disk_pll_phase,
            disk_pll_variable_rate: self.disk_pll_variable_rate,
        }
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

    /// Return a side-effect-free diagnostic snapshot of the UART registers and
    /// receive latches. Unlike [`Self::read_serdatr`], this does not clear RBF
    /// or the overrun latch.
    #[must_use]
    pub fn serial_diagnostic_snapshot(&self) -> PaulaSerialDiagnosticSnapshot {
        PaulaSerialDiagnosticSnapshot {
            serdat: self.serdat,
            serper: self.serper,
            serdatr: self.peek_serdatr(),
            receive_data: self.serial_rx_byte,
            receive_full: self.serial_rx_full,
            receive_overrun: self.serial_rx_overrun,
        }
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
                // Output mode: the pin is driven by POTGO, but the mouse
                // right/middle button is a switch to ground that pulls the
                // line low whenever it is held — it wins over the driver.
                // The OS exploits exactly this to read the digital buttons:
                // it drives the pins high (OUTxx=1, DATxx=1) and then reads
                // POTGOR to see the button pull a line low. Wired-AND the
                // driven value with the injected external level so a held
                // button still reads low. Without this AND, Intuition never
                // sees the right (menu) button while the OS has the pins in
                // output mode. (#63)
                self.potgo & dat_mask & self.pot_pin_levels
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

    /// Return a side-effect-free diagnostic snapshot of the pot-port output
    /// latch, raw external pin levels, composite POTGOR value, and counters.
    #[must_use]
    pub fn pot_diagnostic_snapshot(&self) -> PaulaPotDiagnosticSnapshot {
        PaulaPotDiagnosticSnapshot {
            potgo: self.potgo,
            raw_pin_levels: self.pot_pin_levels,
            potgor: self.peek_potgor(),
            pot0dat: self.pot0dat,
            pot1dat: self.pot1dat,
        }
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

    /// Return a side-effect-free summary of Paula's component-owned
    /// diagnostic logs. The bounded interrupt logs are copied in retained
    /// order; disk-write logs expose only their count and most recent value.
    #[must_use]
    pub fn log_diagnostic_snapshot(&self) -> PaulaLogDiagnosticSnapshot {
        PaulaLogDiagnosticSnapshot {
            intena_writes: self.intena_write_log.iter().copied().collect(),
            intena_write_count: self.intena_write_log.len(),
            last_intena_write: self.intena_write_log.back().copied(),
            intreq_writes: self.intreq_write_log.iter().copied().collect(),
            intreq_write_count: self.intreq_write_log.len(),
            last_intreq_write: self.intreq_write_log.back().copied(),
            disk_write_dma_count: self.disk_write_dma_log.len(),
            last_disk_write_dma_word: self.disk_write_dma_log.last().copied(),
            disk_write_pio_count: self.disk_write_pio_log.len(),
            last_disk_write_pio_word: self.disk_write_pio_log.last().copied(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Internal unit tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_write_dma_consumes_words_then_raises_dskblk() {
        let mut p = Paula8364::new();
        // Arm a 2-word WRITE transfer: DSKLEN double-write with the
        // WRITE bit + DMAEN + length 2.
        let dsklen = bits::DSKLEN_DMAEN | bits::DSKLEN_WRITE | 2;
        p.write_dsklen(dsklen);
        p.write_dsklen(dsklen);
        assert!(
            p.disk_dma_write_active(),
            "write transfer should be in flight"
        );

        // The write slot returns each word for the machine to feed the
        // drive, and counts down. The read slot must NOT drive a write.
        assert_eq!(
            p.tick_disk_dma_slot(0xFFFF),
            None,
            "read slot ignores a write transfer"
        );
        assert_eq!(p.tick_disk_write_dma_slot(0x1234), Some(0x1234));
        assert!(p.disk_dma_write_active(), "still one word to go");
        assert_eq!(
            p.intreq() & bits::INT_DSKBLK,
            0,
            "DSKBLK not raised mid-transfer"
        );

        // Final word completes the transfer and raises DSKBLK.
        assert_eq!(p.tick_disk_write_dma_slot(0x5678), Some(0x5678));
        assert!(!p.disk_dma_write_active(), "transfer drained");
        assert_eq!(
            p.intreq() & bits::INT_DSKBLK,
            bits::INT_DSKBLK,
            "DSKBLK raised on completion"
        );

        // Drained: further calls yield nothing.
        assert_eq!(p.tick_disk_write_dma_slot(0x9ABC), None);
    }

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

    /// Regression for #63: the OS reads the digital mouse buttons by
    /// driving the pot pins high in output mode (POTGO `OUTxx=1, DATxx=1`)
    /// and watching POTGOR for a button pulling a line low. A held
    /// right/middle button is a switch to ground that wins over the
    /// driver, so it must read low even in output mode. Before the fix,
    /// output mode reported the driven (high) value and Intuition never
    /// saw the right (menu) button.
    #[test]
    fn held_mouse_button_pulls_potgor_low_even_in_output_mode() {
        let mut p = Paula8364::new();

        // OS drives port-0 pins high to scan the buttons — real Kickstart
        // 2.04 writes 0x0F00 (OUTLX|DATLX|OUTRX|DATRX).
        p.write_potgo(POTGO_OUTRX | POTGO_DATRX | POTGO_OUTLX | POTGO_DATLX);
        assert_ne!(
            p.peek_potgor() & POTGO_DATRX,
            0,
            "released right button idles high"
        );

        // Held → the switch to ground wins over the output driver.
        p.set_pot_pin_level(POTGOR_BTN_PORT0_RIGHT, false);
        assert_eq!(
            p.peek_potgor() & POTGO_DATRX,
            0,
            "held right button must read low in output mode"
        );

        // Release returns the line high.
        p.set_pot_pin_level(POTGOR_BTN_PORT0_RIGHT, true);
        assert_ne!(p.peek_potgor() & POTGO_DATRX, 0, "release returns high");

        // Input mode (OUTRX=0) was already correct — held still reads low.
        p.write_potgo(0);
        p.set_pot_pin_level(POTGOR_BTN_PORT0_RIGHT, false);
        assert_eq!(
            p.peek_potgor() & POTGO_DATRX,
            0,
            "held right button reads low in input mode"
        );
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
        assert_eq!(left, 0.0);
        assert!(right > 0.4);

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
