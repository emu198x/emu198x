//! Line RAM write/playback separation and native pixel timing.

use crate::{CTRL_KANGAROO, Maria, VISIBLE_TOP};

pub(super) const LINE_CELLS: usize = 160;

impl Maria {
    /// Sample one playback pixel on MARIA phase 0. At column 412 the old
    /// output buffer supplies the last active pixel before the newly built
    /// line takes its place. DMA writes on this edge enter the cleared buffer.
    pub(super) fn tick_video(&mut self, column: u16) {
        if !self.native_cycle.is_multiple_of(2) {
            return;
        }
        let y = (u32::from(self.scan_line)
            + u32::from(self.region.lines_per_frame())
            + self.region.border_top()
            - u32::from(VISIBLE_TOP))
            % u32::from(self.region.lines_per_frame());
        let left = self.region.border_left() as u16;
        if y < self.region.framebuffer_height() && (93 - left..413 + left).contains(&column) {
            let colour = if (93..413).contains(&column) {
                let pixel = usize::from(column - 93);
                self.cell_colour(self.playback_buffer[pixel / 2], pixel % 2 != 0)
            } else if self.ctrl & 8 != 0 {
                self.backgrnd
            } else {
                0
            };
            let x = u32::from(column + left - 93);
            let index = y * self.region.framebuffer_width() + x;
            self.framebuffer[index as usize] = self.colour_argb(colour);
        }
        if column == 412 {
            std::mem::swap(&mut self.line_buffer, &mut self.playback_buffer);
            self.line_buffer.fill(0);
        }
    }

    /// Store palette/colour selectors, not resolved colours. HPOS counts
    /// 160-wide cells in every mode and wraps as an eight-bit counter.
    /// Atari's software guide separates WM construction from RM playback.
    pub(super) fn blit_byte(
        &mut self,
        byte: u8,
        position: &mut usize,
        write_mode: bool,
        palette: u8,
    ) {
        let count = if write_mode { 2 } else { 4 };
        for cell in 0..count {
            let colour = (byte >> (6 - cell * 2)) & 3;
            let palette = if write_mode {
                (palette & 4) | ((byte >> (2 - cell * 2)) & 3)
            } else {
                palette
            };
            let x = (*position + cell) & 255;
            if x < LINE_CELLS && (colour != 0 || self.ctrl & CTRL_KANGAROO != 0) {
                self.line_buffer[x] = (palette << 2) | colour;
            }
        }
        *position = (*position + count) & 255;
    }

    /// Interpret a saved cell using the current read mode and palette RAM.
    pub(super) fn cell_colour(&self, cell: u8, right: bool) -> u8 {
        let (palette, colour) = match self.ctrl & 3 {
            0 | 1 => (cell >> 2, cell & 3),
            mode => {
                let high = (cell >> if right { 0 } else { 1 }) & 1;
                if mode == 3 {
                    (cell >> 2, high << 1)
                } else {
                    let low = (cell >> if right { 2 } else { 3 }) & 1;
                    ((cell >> 2) & 4, (high << 1) | low)
                }
            }
        };
        if colour == 0 {
            self.backgrnd
        } else {
            self.palettes[usize::from(palette)][usize::from(colour - 1)]
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Maria, MariaRegion, VISIBLE_TOP};
    use std::collections::BTreeSet;

    fn raster_fixture([write_mode, read_mode, position, _, pal, _]: [u8; 6]) -> (Maria, Vec<u8>) {
        let mut chip = Maria::new(if pal == 0 {
            MariaRegion::Ntsc
        } else {
            MariaRegion::Pal
        });
        chip.clock.remaining = 2;
        chip.native_cycle = 224;
        chip.dma_address = 2;
        chip.ctrl = 0x48 | read_mode;
        chip.backgrnd = 2;
        chip.dpph = 0x18;
        for palette in 0..8 {
            for colour in 0..3 {
                chip.palettes[palette][colour] = 0x20 + (palette * 3 + colour + 1) as u8 * 6;
            }
        }
        let mut memory = vec![0; 65536];
        for zone in 0..256 {
            memory[0x1800 + zone * 3] = 1;
            memory[0x1801 + zone * 3] = 0x1c;
        }
        memory[0x1c00..0x1c05].copy_from_slice(&[
            0,
            0x40 | (write_mode << 7),
            0x90,
            0xbe,
            position,
        ]);
        memory[0x9000..0x9002].copy_from_slice(&[0x1b, 0x96]);
        memory[0x9100..0x9102].copy_from_slice(&[0xe4, 0x69]);
        (chip, memory)
    }

    #[test]
    fn native_line_transfer_matches_the_measured_edges() {
        let block = include_str!("../tests/data/raster-vectors.txt")
            .split("\nCASE ")
            .nth(1)
            .expect("first case");
        let transfers: BTreeSet<u32> = block
            .lines()
            .filter(|row| row.starts_with("TRANSFER "))
            .map(|row| {
                row.split_whitespace()
                    .nth(1)
                    .expect("tick")
                    .parse()
                    .expect("tick")
            })
            .collect();
        assert_eq!(transfers.len(), 6);
        let (mut chip, _) = raster_fixture([0, 0, 0, 0, 0, 0]);
        chip.ctrl = 0; // Isolate transfer from graphics construction.
        chip.line_buffer.fill(1);
        chip.playback_buffer.fill(2);
        let mut observed = 0;
        for tick in 257..32 + 908 * 22 {
            // Keep the two banks observably distinct even on empty lines.
            chip.line_buffer[0] = 1;
            chip.playback_buffer[0] = 2;
            chip.tick_dma();
            if (16..=21).contains(&chip.scan_line) {
                if transfers.contains(&tick) {
                    assert_eq!(chip.playback_buffer[0], 1, "missing transfer at {tick}");
                    assert_eq!(chip.line_buffer[0], 0, "uncleared input at {tick}");
                    observed += 1;
                } else {
                    assert_eq!(chip.playback_buffer[0], 2, "early/late transfer at {tick}");
                    assert_eq!(chip.line_buffer[0], 1);
                }
            }
        }
        assert_eq!(observed, 6);
    }

    #[test]
    fn native_playback_samples_live_registers_without_repainting_pixels() {
        let mut chip = fixture();
        chip.scan_line = 18;
        chip.native_cycle = 185; // Next phase 0 outputs active pixel zero.
        chip.playback_buffer[..2].fill(1);
        let width = chip.region.framebuffer_width() as usize;
        let start = 2 * width + chip.region.border_left() as usize;
        chip.tick_dma();
        chip.palettes[0][0] = 0xae;
        chip.tick_dma();
        chip.tick_dma();
        chip.ctrl = 3;
        chip.tick_dma();
        chip.tick_dma();
        chip.ctrl = 2;
        chip.tick_dma();
        chip.tick_dma();
        let expected = [0x4e, 0xae, 0x0e, 0x8e].map(|colour| chip.colour_argb(colour));
        assert_eq!(&chip.framebuffer[start..start + 4], &expected);
    }

    #[test]
    fn snapshots_resume_both_buffers_and_pending_output_pixels() {
        let mut samples = 0;
        for region in [MariaRegion::Ntsc, MariaRegion::Pal] {
            for column in [90, 92, 93, 94, 411, 412, 413, 414] {
                for phase in 0..2 {
                    let mut chip = Maria::new(region);
                    chip.scan_line = 18;
                    chip.native_cycle = column * 2 - phase;
                    chip.backgrnd = 0x0e;
                    chip.ctrl = 8;
                    chip.palettes[0] = [0x4e, 0x8e, 0xce];
                    chip.line_buffer.fill(1);
                    chip.playback_buffer.fill(2);
                    let saved = chip.save_state();
                    let mut restored = Maria::new(region);
                    assert_eq!(
                        restored.load_state(&saved).expect("pending playback"),
                        saved.len()
                    );
                    assert_eq!(restored.line_buffer, chip.line_buffer);
                    assert_eq!(restored.playback_buffer, chip.playback_buffer);
                    for tick in 0..32 {
                        // Later register changes must affect both resumed and
                        // uninterrupted output at the same native sample.
                        let control = 8 | ((tick / 5) & 3);
                        chip.write(0x1c, control);
                        restored.write(0x1c, control);
                        chip.write(1, if tick < 11 { 0x4e } else { 0xae });
                        restored.write(1, if tick < 11 { 0x4e } else { 0xae });
                        chip.tick_dma();
                        restored.tick_dma();
                        assert_eq!(restored.line_buffer, chip.line_buffer);
                        assert_eq!(restored.playback_buffer, chip.playback_buffer);
                        assert_eq!(
                            restored.framebuffer, chip.framebuffer,
                            "column {column}, phase {phase}, tick {tick}"
                        );
                        samples += 1;
                    }
                    assert_eq!(restored.save_state(), chip.save_state());
                }
            }
        }
        assert_eq!(samples, 1024);
    }

    #[test]
    fn native_pixels_match_all_160_full_clock_reference_windows() {
        let mut cases = BTreeSet::new();
        let mut checked = 0;
        let mut register_events = 0;
        for block in include_str!("../tests/data/raster-vectors.txt")
            .split("\nCASE ")
            .skip(1)
        {
            let (header, rows) = block.split_once('\n').expect("case header");
            let fields: Vec<_> = header.split_whitespace().collect();
            assert_eq!(fields.len(), 6);
            let case: [u8; 6] = std::array::from_fn(|i| fields[i].parse().expect("case field"));
            assert!(cases.insert(case));
            let expected: Vec<_> = rows
                .lines()
                .filter(|row| row.starts_with("ROW "))
                .enumerate()
                .map(|(index, row)| {
                    let fields: Vec<_> = row.split_whitespace().collect();
                    assert_eq!(fields.len(), 3);
                    assert_eq!(fields[1].parse::<usize>().expect("raster"), 16 + index);
                    hex_bytes(fields[2])
                })
                .collect();
            assert_eq!(expected.len(), 6);
            // These observations define input changes at the video stage.
            // CPU-bus-to-register propagation remains a separate timing gate.
            let registers: std::collections::BTreeMap<u32, (u8, u8)> = rows
                .lines()
                .filter(|row| row.starts_with("REGISTER "))
                .map(|row| {
                    let fields: Vec<_> = row.split_whitespace().collect();
                    assert_eq!(fields.len(), 4);
                    (
                        fields[1].parse().expect("register tick"),
                        (
                            u8::from_str_radix(fields[2], 16).expect("register address"),
                            u8::from_str_radix(fields[3], 16).expect("register value"),
                        ),
                    )
                })
                .collect();
            assert_eq!(registers.len(), usize::from(case[5]) * 3);
            let (mut chip, memory) = raster_fixture(case);
            let width = chip.region.framebuffer_width() as usize;
            assert!(expected.iter().all(|row| row.len() == width));
            let left = chip.region.border_left() as usize;
            let mut sampled_halt = false;
            let mut released = false;
            let mut address = 0x8000;
            for tick in 257..32 + 908 * 22 {
                let halt = chip.halt;
                let cpu_address = if case[5] != 0 && (16560..16576).contains(&tick) {
                    0x0037
                } else if case[5] != 0 && (16576..16592).contains(&tick) {
                    0x003c
                } else if case[5] != 0 && (16600..16616).contains(&tick) {
                    0x0020
                } else if case[3] != 0 && tick >= 1024 {
                    0x0280
                } else {
                    0x8000
                };
                if !released && !chip.dma_drive {
                    address = cpu_address;
                }
                chip.address_in = address;
                chip.dma_data_in = memory[usize::from(address)];
                let next_cycle = (chip.native_cycle + 1) % 908;
                let next_column = next_cycle.div_ceil(2) as usize % 454;
                let early_pixel = if next_cycle % 2 != 0
                    && (16..=21).contains(&chip.scan_line)
                    && (93 - left..413 + left).contains(&next_column)
                {
                    let y = chip.region.border_top() as usize + (chip.scan_line - 16) as usize;
                    let index = y * width + next_column + left - 93;
                    Some((index, chip.framebuffer[index]))
                } else {
                    None
                };
                chip.tick_dma();
                if let Some((index, before)) = early_pixel {
                    assert_eq!(
                        chip.framebuffer[index], before,
                        "early pixel: case {case:?}, tick {tick}"
                    );
                }
                released = sampled_halt;
                if chip.phi1 {
                    sampled_halt = halt;
                }
                assert!(!chip.dma_drive || released);
                if chip.dma_drive {
                    address = chip.dma_address;
                } else if !released {
                    address = cpu_address;
                }
                let column = chip.native_cycle.div_ceil(2) as usize % 454;
                if chip.native_cycle.is_multiple_of(2)
                    && (16..=21).contains(&chip.scan_line)
                    && (93 - left..413 + left).contains(&column)
                {
                    let x = column + left - 93;
                    let y = (chip.scan_line - 16) as usize;
                    let framebuffer_y = chip.region.border_top() as usize + y;
                    assert_eq!(
                        chip.framebuffer[framebuffer_y * width + x],
                        chip.colour_argb(expected[y][x]),
                        "case {case:?}, tick {tick}, raster {}, column {column}",
                        chip.scan_line
                    );
                    checked += 1;
                }
                if let Some(&(address, value)) = registers.get(&tick) {
                    chip.write(address & 31, value);
                    register_events += 1;
                }
            }
        }
        for write in 0..2 {
            for read in 0..4 {
                for position in [0, 1, 158, 255] {
                    for slow in 0..2 {
                        for pal in 0..2 {
                            assert!(cases.contains(&[write, read, position, slow, pal, 0]));
                            if position == 0 {
                                assert!(cases.contains(&[write, read, position, slow, pal, 1]));
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(cases.len(), 160);
        assert_eq!(checked, 356160);
        assert_eq!(register_events, 96);
    }

    fn reference_fixture() -> Maria {
        let mut chip = fixture();
        for palette in 0..8 {
            for colour in 0..3 {
                chip.palettes[palette][colour] = 0x41 + (palette * 3 + colour) as u8;
            }
        }
        chip
    }

    fn hex_bytes(value: &str) -> Vec<u8> {
        assert!(value.len().is_multiple_of(2));
        value
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).expect("hex ASCII"), 16)
                    .expect("hex byte")
            })
            .collect()
    }

    #[test]
    fn line_ram_matches_every_reference_byte_mode_palette_and_overlap() {
        let mut cases = BTreeSet::new();
        let mut boundaries = BTreeSet::new();
        let mut pixel_count = 0;
        for row in include_str!("../tests/data/line-ram-vectors.txt")
            .lines()
            .filter(|row| !row.starts_with('#'))
        {
            let fields: Vec<_> = row.split_whitespace().collect();
            if fields[0] == "VECTOR" {
                assert_eq!(fields.len(), 6);
                let case: [u8; 4] =
                    std::array::from_fn(|i| fields[i + 1].parse().expect("case field"));
                assert!(cases.insert(case), "duplicate case");
                let [write_mode, kangaroo, palette, read_mode] = case;
                let expected = hex_bytes(fields[5]);
                assert_eq!(expected.len(), 2048);
                let mut chip = reference_fixture();
                for byte in 0..=255_u8 {
                    chip.ctrl = 0;
                    chip.blit_byte(0xaa, &mut 0, false, 3);
                    chip.ctrl = (kangaroo << 2) | read_mode;
                    chip.blit_byte(byte, &mut 0, write_mode != 0, palette);
                    for pixel in 0..8 {
                        let actual = chip.cell_colour(chip.line_buffer[pixel / 2], pixel % 2 != 0);
                        assert_eq!(
                            actual,
                            expected[usize::from(byte) * 8 + pixel],
                            "case {case:?}, byte {byte:02x}, pixel {pixel}"
                        );
                        pixel_count += 1;
                    }
                }
            } else {
                assert_eq!(fields[0], "BOUND");
                assert_eq!(fields.len(), 5);
                let case: [u8; 3] =
                    std::array::from_fn(|i| fields[i + 1].parse().expect("case field"));
                assert!(boundaries.insert(case), "duplicate boundary");
                let [write_mode, read_mode, position_index] = case;
                let mut position = [1, 158, 159, 160, 254, 255][usize::from(position_index)];
                let expected = hex_bytes(fields[4]);
                assert_eq!(expected.len(), 320);
                let mut chip = reference_fixture();
                let mut background_position = 0;
                for _ in 0..40 {
                    chip.blit_byte(0xaa, &mut background_position, false, 3);
                }
                chip.ctrl = read_mode;
                for byte in [0xb6, 0x6d] {
                    chip.blit_byte(byte, &mut position, write_mode != 0, 5);
                }
                for (pixel, expected) in expected.into_iter().enumerate() {
                    assert_eq!(
                        chip.cell_colour(chip.line_buffer[pixel / 2], pixel % 2 != 0),
                        expected,
                        "boundary {case:?}, pixel {pixel}"
                    );
                    pixel_count += 1;
                }
            }
        }
        for write in 0..2 {
            for read in 0..4 {
                for kangaroo in 0..2 {
                    for palette in 0..8 {
                        assert!(cases.contains(&[write, kangaroo, palette, read]));
                    }
                }
                for position in 0..6 {
                    assert!(boundaries.contains(&[write, read, position]));
                }
            }
        }
        assert_eq!(cases.len(), 128);
        assert_eq!(boundaries.len(), 48);
        assert_eq!(pixel_count, 277504);
    }

    fn fixture() -> Maria {
        let mut chip = Maria::new(MariaRegion::Ntsc);
        chip.scan_line = VISIBLE_TOP;
        chip.backgrnd = 0x0e;
        chip.palettes[0] = [0x4e, 0x8e, 0xce];
        chip
    }

    fn first_pixels(chip: &mut Maria, count: usize) -> Vec<u32> {
        // Transfer the constructed bank at the real line-RAM swap edge,
        // then sample the following raster's active pixels through tick_dma.
        chip.scan_line = VISIBLE_TOP - 1;
        chip.native_cycle = 823;
        chip.tick_dma();
        for _ in 0..(908 - 824 + 186 + count as u16 * 2) {
            chip.tick_dma();
        }
        let left = chip.region.border_left() as usize;
        chip.framebuffer[left..left + count].to_vec()
    }

    #[test]
    fn palette_changes_remain_live_until_playback() {
        let mut chip = fixture();
        chip.blit_byte(0x55, &mut 0, false, 0);
        chip.palettes[0][0] = 0xae;
        let expected = chip.colour_argb(0xae);
        assert_eq!(first_pixels(&mut chip, 8), [expected; 8]);
    }

    #[test]
    fn read_mode_selects_320a_and_uses_colour_two() {
        let mut chip = fixture();
        chip.ctrl = 3;
        chip.blit_byte(0xa5, &mut 0, false, 0);
        let expected: Vec<_> = [0x8e, 0x0e, 0x8e, 0x0e, 0x0e, 0x8e, 0x0e, 0x8e]
            .map(|colour| chip.colour_argb(colour))
            .into();
        assert_eq!(first_pixels(&mut chip, 8), expected);
    }

    #[test]
    fn horizontal_positions_count_two_pixel_cells() {
        let mut chip = fixture();
        chip.line_buffer.fill(0);
        chip.blit_byte(0x55, &mut 1, false, 0);
        let expected: Vec<_> = [0x0e, 0x0e, 0x4e, 0x4e, 0x4e, 0x4e, 0x4e, 0x4e, 0x4e, 0x4e]
            .map(|colour| chip.colour_argb(colour))
            .into();
        assert_eq!(first_pixels(&mut chip, 10), expected);
    }

    #[test]
    fn extended_write_mode_stores_two_cells_per_byte() {
        let mut chip = fixture();
        chip.line_buffer.fill(0);
        chip.palettes[1] = [0xae, 0xbe, 0xde];
        let mut position = 0;
        // Two nonzero 160B colours: P1C1 and P0C2, then untouched background.
        chip.blit_byte(0x64, &mut position, true, 0);
        let expected: Vec<_> = [0xae, 0xae, 0x8e, 0x8e, 0x0e, 0x0e, 0x0e, 0x0e]
            .map(|colour| chip.colour_argb(colour))
            .into();
        assert_eq!(first_pixels(&mut chip, 8), expected);
        assert_eq!(position, 2);
    }
}
