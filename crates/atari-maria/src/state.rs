//! Versioned direct chip snapshots. Runtime snapshots also serialize the same
//! live fields through serde, inside the Atari 7800 versioned envelope.

use super::fetch::{Fetch, Phase};
use super::{ACTIVE_WIDTH, MAX_DMA_CYCLES_PER_LINE, Maria, MariaRegion};

const MAGIC: &[u8; 6] = b"MARIA\x01";

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(count)
            .ok_or("MARIA state size overflow")?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or("MARIA state truncated")?;
        self.position = end;
        Ok(bytes)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.bytes(1)?[0])
    }

    fn boolean(&mut self) -> Result<bool, String> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("Invalid MARIA boolean".into()),
        }
    }

    fn word(&mut self) -> Result<u16, String> {
        let bytes = self.bytes(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn phase(&mut self) -> Result<Phase, String> {
        match self.byte()? {
            0 => Ok(Phase::Idle),
            1 => Ok(Phase::HeaderLow),
            2 => Ok(Phase::HeaderMode),
            3 => Ok(Phase::HeaderHigh),
            4 => Ok(Phase::HeaderWidth),
            5 => Ok(Phase::HeaderPosition),
            6 => Ok(Phase::Direct),
            7 => Ok(Phase::CharacterMap),
            8 => Ok(Phase::Indirect),
            9 => Ok(Phase::IndirectSecond),
            _ => Err("Invalid MARIA fetch phase".into()),
        }
    }
}

impl Maria {
    /// Save registers, pending fetch/input state, line pixels and framebuffer.
    /// The direct format starts with `MARIA` and version 1; legacy unversioned
    /// register-only states cannot resume the pipeline and are rejected.
    #[must_use]
    pub fn save_state(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(80 + ACTIVE_WIDTH as usize + self.framebuffer.len() * 4);
        data.extend_from_slice(MAGIC);
        data.push(match self.region {
            MariaRegion::Ntsc => 0,
            MariaRegion::Pal => 1,
        });
        data.push(self.backgrnd);
        for palette in &self.palettes {
            data.extend_from_slice(palette);
        }
        data.extend_from_slice(&[
            self.ctrl,
            u8::from(self.wsync),
            self.dppl,
            self.dpph,
            self.chbase,
        ]);
        data.extend_from_slice(&self.scan_line.to_le_bytes());
        data.extend_from_slice(&[
            u8::from(self.vblank),
            u8::from(self.dli_pending),
            u8::from(self.frame_complete),
        ]);
        data.extend_from_slice(&self.dll_addr.to_le_bytes());
        data.extend_from_slice(&[self.zone_scanline, self.zone_height]);
        data.extend_from_slice(&self.zone_dl_addr.to_le_bytes());
        data.extend_from_slice(&[
            self.zone_offset,
            self.zone_holey,
            u8::from(self.zone_dli),
            u8::from(self.dll_active),
        ]);
        data.extend_from_slice(&self.dma_cycles.to_le_bytes());
        data.extend_from_slice(&[self.fetch.phase as u8, self.fetch.delay]);
        for value in [
            self.fetch.dl_addr,
            self.fetch.gfx_addr,
            self.fetch.char_addr,
            self.fetch.hpos,
        ] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        data.extend_from_slice(&[
            self.fetch.palette,
            self.fetch.remaining,
            self.fetch.offset,
            u8::from(self.fetch.long_header),
            u8::from(self.fetch.indirect),
            u8::from(self.fetch.end_header),
            u8::from(self.fetch.write_mode),
        ]);
        data.extend_from_slice(&self.fetch.address.to_le_bytes());
        data.extend_from_slice(&[self.fetch.data_in, u8::from(self.fetch.holey)]);
        data.extend_from_slice(&self.line_buffer);
        for pixel in &self.framebuffer {
            data.extend_from_slice(&pixel.to_le_bytes());
        }
        data
    }

    /// Restore a complete direct snapshot, returning its consumed byte count.
    ///
    /// # Errors
    /// Rejects old versions, truncation, a different region and invalid stages.
    /// Validation completes before any live state is replaced.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        let mut reader = Reader {
            bytes: data,
            position: 0,
        };
        if reader.bytes(MAGIC.len())? != MAGIC {
            return Err("Unsupported MARIA state version".into());
        }
        let region = match reader.byte()? {
            0 => MariaRegion::Ntsc,
            1 => MariaRegion::Pal,
            _ => return Err("Invalid MARIA region".into()),
        };
        if region != self.region {
            return Err("MARIA state region mismatch".into());
        }
        let mut restored = Self::new(region);
        restored.backgrnd = reader.byte()?;
        for palette in &mut restored.palettes {
            palette.copy_from_slice(reader.bytes(3)?);
        }
        restored.ctrl = reader.byte()?;
        restored.wsync = reader.boolean()?;
        restored.dppl = reader.byte()?;
        restored.dpph = reader.byte()?;
        restored.chbase = reader.byte()?;
        restored.scan_line = reader.word()?;
        restored.vblank = reader.boolean()?;
        restored.dli_pending = reader.boolean()?;
        restored.frame_complete = reader.boolean()?;
        restored.dll_addr = reader.word()?;
        restored.zone_scanline = reader.byte()?;
        restored.zone_height = reader.byte()?;
        restored.zone_dl_addr = reader.word()?;
        restored.zone_offset = reader.byte()?;
        restored.zone_holey = reader.byte()?;
        restored.zone_dli = reader.boolean()?;
        restored.dll_active = reader.boolean()?;
        restored.dma_cycles = reader.word()?;
        restored.fetch = Fetch {
            phase: reader.phase()?,
            delay: reader.byte()?,
            dl_addr: reader.word()?,
            gfx_addr: reader.word()?,
            char_addr: reader.word()?,
            hpos: reader.word()?,
            palette: reader.byte()?,
            remaining: reader.byte()?,
            offset: reader.byte()?,
            long_header: reader.boolean()?,
            indirect: reader.boolean()?,
            end_header: reader.boolean()?,
            write_mode: reader.boolean()?,
            address: reader.word()?,
            data_in: reader.byte()?,
            holey: reader.boolean()?,
        };
        let fetch = &restored.fetch;
        let needs_width =
            matches!(
                fetch.phase,
                Phase::HeaderPosition
                    | Phase::Direct
                    | Phase::CharacterMap
                    | Phase::Indirect
                    | Phase::IndirectSecond
            ) || (fetch.phase == Phase::HeaderHigh && !fetch.end_header && !fetch.long_header);
        if restored.scan_line >= region.lines_per_frame()
            || restored.zone_height > 16
            || restored.zone_scanline > 16
            || restored.zone_offset > 15
            || restored.zone_holey > 3
            || restored.dma_cycles > MAX_DMA_CYCLES_PER_LINE
            || fetch.delay > 8
            || (fetch.phase == Phase::Idle) != (fetch.delay == 0)
            || fetch.palette > 7
            || fetch.remaining > 32
            || fetch.offset > 15
            || fetch.hpos > 767
            || (needs_width && fetch.remaining == 0)
        {
            return Err("Invalid MARIA pipeline state".into());
        }
        restored
            .line_buffer
            .copy_from_slice(reader.bytes(ACTIVE_WIDTH as usize)?);
        for pixel in &mut restored.framebuffer {
            let bytes = reader.bytes(4)?;
            *pixel = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        *self = restored;
        Ok(reader.position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_pending_fetches_leave_the_chip_unchanged() {
        let mut chip = Maria::new(MariaRegion::Ntsc);
        chip.write(0, 0x4e);
        let before = chip.save_state();
        for phase in [
            Phase::HeaderPosition,
            Phase::Direct,
            Phase::CharacterMap,
            Phase::Indirect,
            Phase::IndirectSecond,
        ] {
            let mut invalid = Maria::new(MariaRegion::Ntsc);
            invalid.fetch.phase = phase;
            invalid.fetch.delay = 1;
            // A zero width would underflow on the next graphics latch.
            assert!(chip.load_state(&invalid.save_state()).is_err(), "{phase:?}");
            assert_eq!(chip.save_state(), before);
        }
        for (phase, delay) in [
            (Phase::Idle, 1),
            (Phase::HeaderLow, 0),
            (Phase::HeaderLow, 9),
        ] {
            let mut invalid = Maria::new(MariaRegion::Ntsc);
            invalid.fetch.phase = phase;
            invalid.fetch.delay = delay;
            assert!(chip.load_state(&invalid.save_state()).is_err());
            assert_eq!(chip.save_state(), before);
        }
    }

    #[test]
    fn legacy_truncated_and_wrong_region_states_leave_the_chip_unchanged() {
        let mut chip = Maria::new(MariaRegion::Pal);
        chip.write(0, 0x4e);
        let before = chip.save_state();
        let legacy = vec![0; 47];
        assert!(
            chip.load_state(&legacy)
                .expect_err("old layout")
                .contains("version")
        );
        let other_region = Maria::new(MariaRegion::Ntsc).save_state();
        assert!(
            chip.load_state(&other_region)
                .expect_err("region")
                .contains("region")
        );
        for length in (0..100).chain([before.len() / 2, before.len() - 4, before.len() - 1]) {
            assert!(
                chip.load_state(&before[..length]).is_err(),
                "prefix {length}"
            );
            assert_eq!(
                chip.save_state(),
                before,
                "prefix {length} mutated live state"
            );
        }
        assert_eq!(chip.save_state(), before);
    }
}
