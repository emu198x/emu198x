//! Resumable display-list byte consumption, clocked in native oscillator periods.
//!
//! Timing evidence: `reference/by-system/atari-7800/maria-dma-timing-evidence.md`
//! and the qualified traces in the docs repository's
//! `plans/2026-10-10-maria-fetch-traces/`. These stages do not schedule raster
//! startup, descriptor handoff or HALT/NMI yet.

use serde::{Deserialize, Serialize};

use super::{CTRL_CW, Maria};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub(super) enum Phase {
    #[default]
    Idle,
    HeaderLow,
    HeaderMode,
    HeaderHigh,
    HeaderWidth,
    HeaderPosition,
    Direct,
    CharacterMap,
    Indirect,
    IndirectSecond,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Fetch {
    pub phase: Phase,
    pub delay: u8,
    pub dl_addr: u16,
    pub gfx_addr: u16,
    pub char_addr: u16,
    pub hpos: u16,
    pub palette: u8,
    pub remaining: u8,
    pub offset: u8,
    pub long_header: bool,
    pub indirect: bool,
    pub end_header: bool,
    pub write_mode: bool,
    pub address: u16,
    pub data_in: u8,
    pub holey: bool,
}

impl Maria {
    pub(super) fn begin_fetch(&mut self) {
        self.fetch.dl_addr = self.zone_dl_addr;
        self.fetch.offset = self.zone_offset.wrapping_sub(self.zone_scanline) & 15;
        self.schedule_header(Phase::HeaderLow);
    }

    fn schedule_fetch(&mut self, phase: Phase, address: u16, delay: u8, graphics: bool) {
        self.fetch.phase = phase;
        self.fetch.address = address;
        self.fetch.delay = delay;
        // A hole suppresses the rest of the current character, including the
        // second indirect byte even if its address wraps out of the hole.
        self.fetch.holey = graphics && (self.fetch.holey || self.is_holey(address));
    }

    fn schedule_header(&mut self, phase: Phase) {
        let address = self.fetch.dl_addr;
        self.fetch.dl_addr = address.wrapping_add(1);
        self.schedule_fetch(phase, address, 4, false);
    }

    fn schedule_graphics(&mut self) {
        if self.fetch.indirect {
            self.schedule_fetch(Phase::CharacterMap, self.fetch.gfx_addr, 4, false);
        } else {
            let address = self
                .fetch
                .gfx_addr
                .wrapping_add(u16::from(self.fetch.offset) << 8);
            self.schedule_fetch(Phase::Direct, address, 6, true);
        }
    }

    fn finish_graphics_byte(&mut self) {
        self.fetch.remaining -= 1;
        self.fetch.gfx_addr = self.fetch.gfx_addr.wrapping_add(1);
        if self.fetch.remaining == 0 {
            self.schedule_header(Phase::HeaderLow);
        } else {
            self.schedule_graphics();
        }
    }

    pub(super) fn stop_fetch(&mut self) {
        self.fetch.phase = Phase::Idle;
        self.fetch.delay = 0;
    }

    /// Read strobe for the byte consumed by the next native tick. The current
    /// address/input remain in saved fields between this strobe and consumption.
    pub(super) fn fetch_read_address(&self) -> Option<u16> {
        (self.fetch.phase != Phase::Idle && self.fetch.delay == 1 && !self.fetch.holey)
            .then_some(self.fetch.address)
    }

    pub(super) fn tick_fetch(&mut self) {
        if self.fetch.phase == Phase::Idle {
            return;
        }
        self.fetch.delay -= 1;
        if self.fetch.delay != 0 {
            return;
        }
        let byte = if self.fetch.holey {
            0
        } else {
            self.fetch.data_in
        };
        self.dma_cycles += 1; // Compatibility helper's aggregate read count.
        match self.fetch.phase {
            Phase::Idle => {}
            Phase::HeaderLow => {
                self.fetch.gfx_addr = u16::from(byte);
                self.schedule_header(Phase::HeaderMode);
            }
            Phase::HeaderMode => {
                self.fetch.end_header = byte & 0x5f == 0;
                self.fetch.long_header = byte & 0x1f == 0;
                self.fetch.indirect = self.fetch.long_header && byte & 0x20 != 0;
                if self.fetch.long_header && !self.fetch.end_header {
                    self.fetch.write_mode = byte & 0x80 != 0;
                } else if !self.fetch.long_header {
                    self.fetch.palette = byte >> 5;
                    self.fetch.remaining = 32 - (byte & 31);
                }
                self.schedule_header(Phase::HeaderHigh);
            }
            Phase::HeaderHigh => {
                self.fetch.gfx_addr |= u16::from(byte) << 8;
                if self.fetch.end_header {
                    self.stop_fetch();
                } else if self.fetch.long_header {
                    self.schedule_header(Phase::HeaderWidth);
                } else {
                    self.schedule_header(Phase::HeaderPosition);
                }
            }
            Phase::HeaderWidth => {
                self.fetch.palette = byte >> 5;
                self.fetch.remaining = 32 - (byte & 31);
                self.schedule_header(Phase::HeaderPosition);
            }
            Phase::HeaderPosition => {
                self.fetch.hpos = u16::from(byte);
                self.schedule_graphics();
            }
            Phase::CharacterMap => {
                self.fetch.char_addr = (u16::from(self.chbase) << 8 | u16::from(byte))
                    .wrapping_add(u16::from(self.fetch.offset) << 8);
                self.schedule_fetch(Phase::Indirect, self.fetch.char_addr, 8, true);
            }
            Phase::Direct | Phase::Indirect | Phase::IndirectSecond => {
                if self.fetch.holey {
                    // The width counter ends this object after the current
                    // character's bus slots. No line-RAM write or HPOS advance
                    // occurs, even when Kangaroo mode would write zero pixels.
                    self.fetch.remaining = 1;
                } else {
                    let mut x = usize::from(self.fetch.hpos);
                    self.blit_byte(byte, &mut x, self.fetch.write_mode, self.fetch.palette);
                    self.fetch.hpos = x as u16;
                }
                if self.fetch.phase == Phase::Indirect && self.ctrl & CTRL_CW != 0 {
                    self.fetch.char_addr = self.fetch.char_addr.wrapping_add(1);
                    self.schedule_fetch(Phase::IndirectSecond, self.fetch.char_addr, 6, true);
                } else {
                    self.finish_graphics_byte();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MariaRegion;

    fn pending_pixels(chip: &Maria) -> Vec<u8> {
        (0..320)
            .map(|pixel| chip.cell_colour(chip.line_buffer[pixel / 2], pixel % 2 != 0))
            .collect()
    }

    fn fixture(mode: u8) -> (Maria, Vec<u8>) {
        let mut chip = Maria::new(MariaRegion::Ntsc);
        chip.zone_dl_addr = 0x1c00;
        chip.zone_offset = 1;
        chip.chbase = 0x80;
        chip.ctrl = if mode == 4 { CTRL_CW } else { 0 };
        chip.palettes[0] = [0x4e, 0x8e, 0xce];
        let mut memory = vec![0; 65536];
        memory[0x8000..0xa000].fill(0x55);
        if mode == 1 {
            memory[0x1c00..0x1c04].copy_from_slice(&[0, 0x1e, 0x90, 0]);
        } else {
            memory[0x1c00..0x1c05].copy_from_slice(&[
                0,
                if mode == 2 { 0x40 } else { 0x60 },
                0x90,
                0x1e,
                0,
            ]);
        }
        chip.begin_fetch();
        (chip, memory)
    }

    fn advance(chip: &mut Maria, memory: &[u8]) -> Option<u16> {
        let address = chip.fetch_read_address();
        if let Some(address) = address {
            chip.fetch.data_in = memory[usize::from(address)];
        }
        chip.tick_fetch();
        address
    }

    // Byte-latch timestamps relative to the first header byte in the qualified
    // FPGA trace, translated so our first scheduled read completes at tick 4.
    // Includes HPOS and the high-byte prefetch after the terminating marker.
    fn expected(mode: u8) -> Vec<(u16, u16)> {
        let mut reads = vec![(4, 0x1c00), (8, 0x1c01), (12, 0x1c02), (16, 0x1c03)];
        if mode != 1 {
            reads.push((20, 0x1c04));
        }
        let tail: &[(u16, u16)] = match mode {
            1 => &[
                (22, 0x9100),
                (28, 0x9101),
                (32, 0x1c04),
                (36, 0x1c05),
                (40, 0x1c06),
            ],
            2 => &[
                (26, 0x9100),
                (32, 0x9101),
                (36, 0x1c05),
                (40, 0x1c06),
                (44, 0x1c07),
            ],
            3 => &[
                (24, 0x9000),
                (32, 0x8155),
                (36, 0x9001),
                (44, 0x8155),
                (48, 0x1c05),
                (52, 0x1c06),
                (56, 0x1c07),
            ],
            4 => &[
                (24, 0x9000),
                (32, 0x8155),
                (38, 0x8156),
                (42, 0x9001),
                (50, 0x8155),
                (56, 0x8156),
                (60, 0x1c05),
                (64, 0x1c06),
                (68, 0x1c07),
            ],
            _ => panic!("unknown fixture mode"),
        };
        reads.extend_from_slice(tail);
        reads
    }

    #[test]
    fn byte_latches_match_all_four_reference_fetch_modes() {
        for mode in 1..=4 {
            let (mut chip, memory) = fixture(mode);
            let mut reads = Vec::new();
            for tick in 1..=100 {
                if let Some(address) = advance(&mut chip, &memory) {
                    reads.push((tick, address));
                }
            }
            assert_eq!(reads, expected(mode), "mode {mode}");
            assert_eq!(chip.fetch.phase, Phase::Idle);
            let pixels = if mode == 4 { 32 } else { 16 };
            assert!(
                pending_pixels(&chip)[..pixels]
                    .iter()
                    .all(|&pixel| pixel == 0x4e)
            );
            assert!(
                pending_pixels(&chip)[pixels..]
                    .iter()
                    .all(|&pixel| pixel == 0)
            );
        }
    }

    #[test]
    fn byte_latches_match_all_64_measured_reference_vectors() {
        let mut cases = std::collections::BTreeSet::new();
        for row in include_str!("../tests/data/fetch-traces.txt").lines() {
            if row.starts_with('#') || row.is_empty() {
                continue;
            }
            let fields: Vec<_> = row.split_whitespace().collect();
            assert_eq!(fields.len(), 4);
            let mode: u8 = fields[0].parse().expect("mode");
            let width: u8 = fields[1].parse().expect("width");
            let offset: u8 = fields[2].parse().expect("offset");
            assert!(cases.insert((mode, width, offset)), "duplicate vector");
            let expected: Vec<(u16, u16)> = fields[3]
                .split(',')
                .map(|event| {
                    let (tick, address) = event.split_once(':').expect("event");
                    (
                        tick.parse().expect("tick"),
                        u16::from_str_radix(address, 16).expect("address"),
                    )
                })
                .collect();
            let (mut chip, mut memory) = fixture(mode);
            memory[if mode == 1 { 0x1c01 } else { 0x1c03 }] = 32 - width;
            chip.zone_offset = offset;
            chip.begin_fetch();
            let mut reads = Vec::new();
            for tick in 1..=1024 {
                if let Some(address) = advance(&mut chip, &memory) {
                    reads.push((tick, address));
                }
            }
            assert_eq!(chip.fetch.phase, Phase::Idle, "{mode}/{width}/{offset}");
            assert_eq!(reads, expected, "{mode}/{width}/{offset}");
        }
        let inventory: std::collections::BTreeSet<_> = (1..=4)
            .flat_map(|mode| {
                [1, 2, 8, if mode == 1 { 31 } else { 32 }]
                    .into_iter()
                    .flat_map(move |width| [0, 1, 2, 15].map(|offset| (mode, width, offset)))
            })
            .collect();
        assert_eq!(cases, inventory);
    }

    #[test]
    fn memory_and_character_base_remain_live_between_fetches() {
        let (mut chip, mut memory) = fixture(3);
        memory[0xa155] = 0x55;
        let mut graphics = Vec::new();
        for tick in 1..=100 {
            if tick == 23 {
                chip.write(0x14, 0x90); // Reaches the first map-byte latch.
            }
            if tick == 25 {
                chip.write(0x14, 0xa0); // Too late for first; reaches second.
            }
            if tick == 31 {
                memory[0x9155] = 0xaa; // Reaches the already scheduled read.
            }
            let phase = chip.fetch.phase;
            if let Some(address) = advance(&mut chip, &memory)
                && phase == Phase::Indirect
            {
                graphics.push(address);
            }
        }
        assert_eq!(graphics, [0x9155, 0xa155]);
        assert_eq!(&pending_pixels(&chip)[..8], &[0x8e; 8]);
        assert_eq!(&pending_pixels(&chip)[8..16], &[0x4e; 8]);
    }

    #[test]
    fn short_headers_retain_the_extended_headers_write_mode() {
        let (mut chip, mut memory) = fixture(2);
        memory[0x1c01] = 0xc0;
        memory[0x1c03] = 0x1f;
        memory[0x1c05..0x1c09].copy_from_slice(&[0, 0x1f, 0x90, 8]);
        for _ in 0..100 {
            advance(&mut chip, &memory);
        }
        assert_eq!(chip.fetch.phase, Phase::Idle);
        assert!(chip.fetch.write_mode);
        chip.begin_fetch();
        assert!(
            chip.fetch.write_mode,
            "retained across display-list restart"
        );
    }

    #[test]
    fn direct_snapshot_resumes_every_fetch_phase_and_pending_input() {
        for mode in 1..=4 {
            let (mut chip, memory) = fixture(mode);
            let mut active_saves = 0;
            for elapsed in 1..=100 {
                // Save after memory has driven the input but before the latch.
                if let Some(address) = chip.fetch_read_address() {
                    chip.fetch.data_in = memory[usize::from(address)];
                }
                let saved = chip.save_state();
                let mut restored = Maria::new(MariaRegion::Ntsc);
                assert_eq!(
                    restored.load_state(&saved).expect("restore stage"),
                    saved.len()
                );
                if chip.fetch.phase != Phase::Idle {
                    active_saves += 1;
                }
                chip.tick_fetch();
                restored.tick_fetch();
                assert_eq!(restored.fetch, chip.fetch);
                assert_eq!(restored.line_buffer, chip.line_buffer);
                // Continue independently through the remaining reads, rather
                // than accepting byte-equal serialization as execution proof.
                let mut continuation = restored;
                let mut remaining_reads = Vec::new();
                for delta in 1..=100 {
                    if let Some(address) = advance(&mut continuation, &memory) {
                        remaining_reads.push((delta, address));
                    }
                }
                assert_eq!(continuation.fetch.phase, Phase::Idle);
                let expected_remaining: Vec<_> = expected(mode)
                    .into_iter()
                    .filter(|(tick, _)| *tick > elapsed)
                    .map(|(tick, address)| (tick - elapsed, address))
                    .collect();
                assert_eq!(
                    remaining_reads, expected_remaining,
                    "mode {mode}, save at {elapsed}"
                );
                let pixels = if mode == 4 { 32 } else { 16 };
                assert!(
                    pending_pixels(&continuation)[..pixels]
                        .iter()
                        .all(|&pixel| pixel == 0x4e)
                );
            }
            assert_eq!(active_saves, expected(mode).last().expect("reads").0);
        }
    }
}
