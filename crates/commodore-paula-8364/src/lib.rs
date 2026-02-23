//! Commodore 8364 Paula — interrupt controller, audio DMA, and disk DMA.
//!
//! Paula manages the Amiga's interrupt priority system, mapping 14 interrupt
//! sources to 6 CPU interrupt levels. It also handles audio channel DMA and
//! floppy disk DMA (currently stubbed for boot-level emulation).

const AUDIO_DMA_MASTER: u16 = 0x0200;
const AUDIO_DMA_BITS: [u16; 4] = [0x0001, 0x0002, 0x0004, 0x0008];
const MIN_AUDIO_PERIOD_CCK: u16 = 124;

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
}

impl Default for AudioChannel {
    fn default() -> Self {
        Self {
            lc: 0,
            ptr: 0,
            len_words: 0,
            words_remaining: 0,
            per: 124,
            vol: 0,
            dat: 0,
            current_word: None,
            next_word: None,
            next_byte_is_hi: true,
            period_counter: 124,
            output_sample: 0,
            dma_active: false,
            dma_enabled_prev: false,
        }
    }
}

impl AudioChannel {
    fn effective_period(&self) -> u16 {
        self.per.max(MIN_AUDIO_PERIOD_CCK)
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
    }

    fn stop_dma(&mut self) {
        self.dma_active = false;
        self.current_word = None;
        self.next_word = None;
        self.next_byte_is_hi = true;
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
        // One-shot/non-DMA playback path: let CPU-written AUDxDAT feed the DAC.
        if !self.dma_active {
            self.current_word = Some(val);
            self.next_word = None;
            self.next_byte_is_hi = true;
            self.period_counter = self.effective_period();
        }
    }

    fn fetch_dma_word<F>(&mut self, mut read_chip_byte: F) -> bool
    where
        F: FnMut(u32) -> u8,
    {
        if !self.dma_active {
            return false;
        }

        // Keep one word actively playing and one word prefetched.
        if self.current_word.is_some() && self.next_word.is_some() {
            return false;
        }

        // End of block: raise audio IRQ and loop from LC while DMA remains enabled.
        let mut wrapped = false;
        if self.words_remaining == 0 {
            self.ptr = self.lc & 0x00FF_FFFE;
            self.words_remaining = self.programmed_length_words();
            if self.words_remaining == 0 {
                return false;
            }
            wrapped = true;
        }

        let hi = read_chip_byte(self.ptr);
        let lo = read_chip_byte(self.ptr | 1);
        self.ptr = self.ptr.wrapping_add(2);
        self.words_remaining = self.words_remaining.saturating_sub(1);

        let word = (u16::from(hi) << 8) | u16::from(lo);
        self.dat = word;

        if self.current_word.is_none() {
            self.current_word = Some(word);
            self.next_byte_is_hi = true;
        } else if self.next_word.is_none() {
            self.next_word = Some(word);
        }

        wrapped
    }

    fn tick_output(&mut self) {
        if self.period_counter == 0 {
            self.period_counter = self.effective_period();
        }

        self.period_counter = self.period_counter.saturating_sub(1);
        if self.period_counter != 0 {
            return;
        }
        self.period_counter = self.effective_period();

        if self.current_word.is_none()
            && let Some(next) = self.next_word.take()
        {
            self.current_word = Some(next);
            self.next_byte_is_hi = true;
        }

        let Some(word) = self.current_word else {
            return;
        };

        let sample_byte = if self.next_byte_is_hi {
            (word >> 8) as u8
        } else {
            word as u8
        };
        self.output_sample = sample_byte as i8;

        if self.next_byte_is_hi {
            self.next_byte_is_hi = false;
            return;
        }

        self.next_byte_is_hi = true;
        if let Some(next) = self.next_word.take() {
            self.current_word = Some(next);
        } else {
            self.current_word = None;
        }
    }

    fn mix_sample(&self) -> f32 {
        let amplitude = f32::from(self.output_sample) / 128.0;
        let volume = f32::from(self.vol.min(64)) / 64.0;
        amplitude * volume
    }
}

pub struct Paula8364 {
    pub intena: u16,
    pub intreq: u16,
    pub adkcon: u16,
    pub dsklen: u16,
    pub dsklen_prev: u16,
    pub dsksync: u16,
    pub disk_dma_pending: bool,
    audio: [AudioChannel; 4],
}

impl Paula8364 {
    pub fn new() -> Self {
        Self {
            intena: 0,
            intreq: 0,
            adkcon: 0,
            dsklen: 0,
            dsklen_prev: 0,
            dsksync: 0,
            disk_dma_pending: false,
            audio: [AudioChannel::default(); 4],
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn write_intena(&mut self, val: u16) {
        if val & 0x8000 != 0 {
            self.intena |= val & 0x7FFF;
        } else {
            self.intena &= !(val & 0x7FFF);
        }
    }

    pub fn write_intreq(&mut self, val: u16) {
        if val & 0x8000 != 0 {
            self.intreq |= val & 0x7FFF;
        } else {
            self.intreq &= !(val & 0x7FFF);
        }
    }

    pub fn request_interrupt(&mut self, bit: u8) {
        self.intreq |= 1 << bit;
    }

    pub fn write_adkcon(&mut self, val: u16) {
        if val & 0x8000 != 0 {
            self.adkcon |= val & 0x7FFF;
        } else {
            self.adkcon &= !(val & 0x7FFF);
        }
    }

    /// Write one Paula audio register (AUDx*), returning true if handled.
    pub fn write_audio_register(&mut self, offset: u16, val: u16) -> bool {
        if !(0x0A0..=0x0DA).contains(&offset) {
            return false;
        }
        let rel = offset - 0x0A0;
        let channel = usize::from(rel / 0x10);
        if channel >= self.audio.len() {
            return false;
        }
        let reg = (rel % 0x10) / 2;
        let ch = &mut self.audio[channel];

        match reg {
            0 => {
                ch.lc = (ch.lc & 0x0000_FFFF) | (u32::from(val) << 16);
                true
            }
            1 => {
                ch.lc = (ch.lc & 0xFFFF_0000) | u32::from(val & 0xFFFE);
                true
            }
            2 => {
                ch.len_words = val;
                true
            }
            3 => {
                ch.per = val;
                if ch.period_counter == 0 {
                    ch.period_counter = ch.effective_period();
                }
                true
            }
            4 => {
                ch.vol = (val & 0x7F).min(64) as u8;
                true
            }
            5 => {
                ch.write_dat(val);
                true
            }
            _ => true,
        }
    }

    /// Read one Paula audio register (AUDx*), returning `None` if unsupported.
    pub fn read_audio_register(&self, offset: u16) -> Option<u16> {
        if !(0x0A0..=0x0DA).contains(&offset) {
            return None;
        }
        let rel = offset - 0x0A0;
        let channel = usize::from(rel / 0x10);
        if channel >= self.audio.len() {
            return None;
        }
        let reg = (rel % 0x10) / 2;
        let ch = &self.audio[channel];

        match reg {
            0 => Some((ch.lc >> 16) as u16),
            1 => Some((ch.lc & 0xFFFF) as u16),
            2 => Some(ch.len_words),
            3 => Some(ch.per),
            4 => Some(u16::from(ch.vol)),
            5 => Some(ch.dat),
            _ => Some(0),
        }
    }

    /// Tick Paula audio one color clock (CCK).
    ///
    /// `dmacon` is the current Agnus DMACON value. `audio_dma_slot` indicates
    /// whether this CCK is the dedicated DMA slot for a specific audio channel.
    pub fn tick_audio_cck<F>(
        &mut self,
        dmacon: u16,
        audio_dma_slot: Option<u8>,
        mut read_chip_byte: F,
    ) where
        F: FnMut(u32) -> u8,
    {
        let mut irq_mask = 0u16;
        for (index, channel) in self.audio.iter_mut().enumerate() {
            let dma_enabled =
                (dmacon & AUDIO_DMA_MASTER) != 0 && (dmacon & AUDIO_DMA_BITS[index]) != 0;
            if channel.sync_dma_enable(dma_enabled) {
                irq_mask |= 1 << (7 + index);
            }
        }

        if let Some(index_u8) = audio_dma_slot {
            let index = usize::from(index_u8);
            if index < self.audio.len() {
                let block_reloaded = self.audio[index].fetch_dma_word(&mut read_chip_byte);
                if block_reloaded {
                    irq_mask |= 1 << (7 + index);
                }
            }
        }

        for channel in &mut self.audio {
            channel.tick_output();
        }

        if irq_mask != 0 {
            self.intreq |= irq_mask;
        }
    }

    /// Mixed stereo output in the range `[-1.0, 1.0]`.
    pub fn mix_audio_stereo(&self) -> (f32, f32) {
        // OCS stereo routing: channels 0+3 left, 1+2 right.
        let left = (self.audio[0].mix_sample() + self.audio[3].mix_sample()) * 0.5;
        let right = (self.audio[1].mix_sample() + self.audio[2].mix_sample()) * 0.5;
        (left.clamp(-1.0, 1.0), right.clamp(-1.0, 1.0))
    }

    /// Double-write protocol: DMA starts only when DSKLEN is written
    /// twice in a row with bit 15 set. Sets `disk_dma_pending` for the
    /// machine crate to perform the actual transfer and fire DSKBLK.
    pub fn write_dsklen(&mut self, val: u16) {
        let prev = self.dsklen;
        self.dsklen = val;
        self.dsklen_prev = prev;

        // Detect double-write with DMA enable (bit 15 set on both writes).
        if val & 0x8000 != 0 && prev & 0x8000 != 0 {
            self.disk_dma_pending = true;
        }
    }

    pub fn compute_ipl(&self) -> u8 {
        // Master enable: bit 14
        if self.intena & 0x4000 == 0 {
            return 0;
        }

        let active = self.intena & self.intreq & 0x3FFF;
        if active == 0 {
            return 0;
        }

        // Amiga Hardware Reference Manual interrupt priority mapping:
        //   L6: bit 13 EXTER (CIA-B)
        //   L5: bit 12 DSKSYN, bit 11 RBF
        //   L4: bit 10 AUD3, bit 9 AUD2, bit 8 AUD1, bit 7 AUD0
        //   L3: bit 6 BLIT, bit 5 VERTB, bit 4 COPER
        //   L2: bit 3 PORTS (CIA-A)
        //   L1: bit 2 SOFT, bit 1 DSKBLK, bit 0 TBE
        if active & 0x2000 != 0 {
            return 6;
        } // EXTER
        if active & 0x1800 != 0 {
            return 5;
        } // DSKSYN, RBF
        if active & 0x0780 != 0 {
            return 4;
        } // AUD3-0
        if active & 0x0070 != 0 {
            return 3;
        } // BLIT, VERTB, COPER
        if active & 0x0008 != 0 {
            return 2;
        } // PORTS
        if active & 0x0007 != 0 {
            return 1;
        } // SOFT, DSKBLK, TBE

        0
    }
}

impl Default for Paula8364 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Paula8364;

    #[test]
    fn writes_and_reads_audio_registers() {
        let mut paula = Paula8364::new();
        assert!(paula.write_audio_register(0x0A0, 0x1234));
        assert!(paula.write_audio_register(0x0A2, 0x5678));
        assert!(paula.write_audio_register(0x0A4, 0x0020));
        assert!(paula.write_audio_register(0x0A6, 0x0100));
        assert!(paula.write_audio_register(0x0A8, 0x007F)); // clamps to 64
        assert!(paula.write_audio_register(0x0AA, 0xABCD));

        assert_eq!(paula.read_audio_register(0x0A0), Some(0x1234));
        assert_eq!(paula.read_audio_register(0x0A2), Some(0x5678 & 0xFFFE));
        assert_eq!(paula.read_audio_register(0x0A4), Some(0x0020));
        assert_eq!(paula.read_audio_register(0x0A6), Some(0x0100));
        assert_eq!(paula.read_audio_register(0x0A8), Some(64));
        assert_eq!(paula.read_audio_register(0x0AA), Some(0xABCD));
    }

    #[test]
    fn audio_dma_fetch_updates_left_mix() {
        let mut paula = Paula8364::new();
        let dmacon = 0x0200 | 0x0001; // DMAEN + AUD0EN

        assert!(paula.write_audio_register(0x0A0, 0x0000));
        assert!(paula.write_audio_register(0x0A2, 0x1000));
        assert!(paula.write_audio_register(0x0A4, 0x0001));
        assert!(paula.write_audio_register(0x0A6, 124));
        assert!(paula.write_audio_register(0x0A8, 64));

        let read = |addr: u32| -> u8 {
            match addr {
                0x0000_1000 => 0x7F,
                0x0000_1001 => 0x80,
                _ => 0,
            }
        };

        for _ in 0..124 {
            paula.tick_audio_cck(dmacon, Some(0), read);
        }
        let (left, right) = paula.mix_audio_stereo();

        assert!(left > 0.4, "left={left}");
        assert!(right.abs() < 0.01, "right={right}");
    }

    #[test]
    fn audio_period_write_is_clamped_for_playback_but_readback_preserves_value() {
        let mut paula = Paula8364::new();
        let dmacon = 0x0200 | 0x0001; // DMAEN + AUD0EN

        assert!(paula.write_audio_register(0x0A0, 0x0000));
        assert!(paula.write_audio_register(0x0A2, 0x1000));
        assert!(paula.write_audio_register(0x0A4, 0x0001));
        assert!(paula.write_audio_register(0x0A6, 1)); // below hardware minimum
        assert!(paula.write_audio_register(0x0A8, 64));
        assert_eq!(paula.read_audio_register(0x0A6), Some(1));

        let read = |addr: u32| -> u8 {
            match addr {
                0x0000_1000 => 0x7F,
                0x0000_1001 => 0x80,
                _ => 0,
            }
        };

        for _ in 0..123 {
            paula.tick_audio_cck(dmacon, Some(0), read);
        }
        let (left_before, _) = paula.mix_audio_stereo();
        assert!(left_before.abs() < 0.01, "left_before={left_before}");

        paula.tick_audio_cck(dmacon, Some(0), read);
        let (left_after, _) = paula.mix_audio_stereo();
        assert!(left_after > 0.4, "left_after={left_after}");
    }

    #[test]
    fn audio_dma_interrupt_occurs_on_block_start_and_reload() {
        let mut paula = Paula8364::new();
        let dmacon = 0x0200 | 0x0001; // DMAEN + AUD0EN

        assert!(paula.write_audio_register(0x0A0, 0x0000));
        assert!(paula.write_audio_register(0x0A2, 0x1000));
        assert!(paula.write_audio_register(0x0A4, 0x0001)); // one word block
        assert!(paula.write_audio_register(0x0A6, 124));

        let read = |_addr: u32| -> u8 { 0 };

        paula.tick_audio_cck(dmacon, Some(0), read);
        assert_ne!(paula.intreq & 0x0080, 0, "AUD0 IRQ should fire on DMA start");

        paula.intreq = 0;
        paula.tick_audio_cck(dmacon, Some(0), read);
        assert_ne!(
            paula.intreq & 0x0080,
            0,
            "AUD0 IRQ should fire when the one-word block reloads"
        );
    }
}
