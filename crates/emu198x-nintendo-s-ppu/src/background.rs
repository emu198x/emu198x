//! The four backgrounds: tile fetches, the per-dot shifter, offset-per-tile,
//! mosaic and Mode 7.
//!
//! Each background keeps a row of fetched tiles. The fetch slots fill tile
//! `n` while tile `n - 2` is being drawn; the shifter takes two bits a pixel
//! from each bitplane word. A tile's bitplanes are stored interleaved, pixel
//! 0 in bits 0–1, so one mask and shift yields a pixel's two bits from a
//! plane pair.

use serde::{Deserialize, Serialize};

use crate::SPpu;

/// A background's bit depth in the current mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum BgMode {
    #[default]
    Bpp2 = 0,
    Bpp4 = 1,
    Bpp8 = 2,
    Mode7 = 3,
    Inactive = 4,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct BgIo {
    pub screen_size: u8,
    pub screen_address: u16,
    pub tiledata_address: u16,
    pub tile_size: u8,
    pub mode: BgMode,
    pub priority: [u8; 2],
    pub above_enable: bool,
    pub below_enable: bool,
    pub hoffset: u16,
    pub voffset: u16,
}

/// A pixel a layer offers the DAC. Priority 0 is transparent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Pixel {
    pub priority: u8,
    pub palette: u8,
    pub palette_group: u8,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Output {
    pub above: Pixel,
    pub below: Pixel,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct BgMosaic {
    pub enable: bool,
    pub hcounter: u16,
    pub hoffset: u16,
    pub pixel: Pixel,
}

/// The offset-per-tile words BG3 fetches for BG1 and BG2.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Opt {
    pub hoffset: u16,
    pub voffset: u16,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Tile {
    pub address: u16,
    pub character: u16,
    pub palette: u8,
    pub palette_group: u8,
    pub priority: u8,
    pub hmirror: bool,
    pub vmirror: bool,
    pub data: [u16; 4],
}

/// Tiles a background holds: 33 a line, 66 in hi-res.
const TILES: usize = 66;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Background {
    pub io: BgIo,
    pub output: Output,
    pub mosaic: BgMosaic,
    pub opt: Opt,
    pub tiles: Vec<Tile>,
    pub rendering_index: u8,
    pub pixel_counter: u8,
}

impl Background {
    pub fn new() -> Self {
        Self {
            io: BgIo::default(),
            output: Output::default(),
            mosaic: BgMosaic::default(),
            opt: Opt::default(),
            tiles: vec![Tile::default(); TILES],
            rendering_index: 0,
            pixel_counter: 0,
        }
    }
}

/// Spread an 8-bit plane to the even bits of a word, as ares does with one
/// multiply.
fn spread(byte: u8, shift: u32, mask: u64) -> u16 {
    let x = u64::from(byte).wrapping_mul(0x0101_0101_0101_0101) & 0x8040_2010_0804_0201;
    (x.wrapping_mul(0x0102_0408_1020_4081) >> shift & mask) as u16
}

impl SPpu {
    pub(crate) fn hires(&self) -> bool {
        self.io.bg_mode == 5 || self.io.bg_mode == 6
    }

    pub(crate) fn mosaic_enabled(&self) -> bool {
        self.bg.iter().any(|bg| bg.mosaic.enable)
    }

    /// How far into the current mosaic block this line is.
    pub(crate) fn mosaic_voffset(&self) -> u32 {
        u32::from(self.mosaic.size).wrapping_sub(u32::from(self.mosaic.vcounter))
    }

    pub(crate) fn mosaic_scanline(&mut self) {
        if self.counter.vcounter == 1 {
            self.mosaic.vcounter = if self.mosaic_enabled() {
                self.mosaic.size + 1
            } else {
                0
            };
        }
        if self.mosaic.vcounter != 0 {
            self.mosaic.vcounter -= 1;
            if self.mosaic.vcounter == 0 {
                self.mosaic.vcounter = if self.mosaic_enabled() {
                    self.mosaic.size
                } else {
                    0
                };
            }
        }
    }

    pub(crate) fn bg_scanline(&mut self, id: usize) {
        let hires = u16::from(self.hires());
        let size = u16::from(self.mosaic.size);
        let bg = &mut self.bg[id];
        bg.mosaic.hcounter = size;
        bg.mosaic.hoffset = 0;
        bg.rendering_index = 0;
        bg.pixel_counter = (((bg.io.hoffset & 7) << hires) & 7) as u8;
        bg.opt = Opt::default();
    }

    /// H=56: drop the columns of the first tile that are scrolled off.
    pub(crate) fn bg_begin(&mut self, id: usize) {
        let bg = &mut self.bg[id];
        let shift = u32::from(bg.pixel_counter) << 1;
        for data in &mut bg.tiles[0].data {
            *data >>= shift;
        }
    }

    pub(crate) fn bg_fetch_name_table(&mut self, id: usize) {
        let v = self.counter.vcounter;
        if v == 0 {
            return;
        }
        let h = self.counter.hcounter;
        let hires = u32::from(self.hires());
        let first = ((h >> 5) << hires) as usize;
        let x = (h & !31) >> 2;
        let mut hpixel = x << hires;
        let mut vpixel = v;
        let io = self.bg[id].io;
        let mosaic = self.bg[id].mosaic.enable;
        let mut hscroll = u32::from(io.hoffset);
        let vscroll = u32::from(io.voffset);
        if hires == 1 {
            hscroll <<= 1;
            if self.io.interlace {
                vpixel = vpixel << 1 | u32::from(self.counter.field && !mosaic);
            }
        }
        if mosaic {
            let shift = u32::from(hires == 1 && self.io.interlace);
            vpixel = vpixel.wrapping_sub(self.mosaic_voffset() << shift);
        }

        for repeat in 0..=hires {
            let index = first + repeat as usize;
            if repeat == 1 {
                hpixel += 8;
            }
            let mut hoffset = hpixel.wrapping_add(hscroll);
            let mut voffset = vpixel.wrapping_add(vscroll);

            if matches!(self.io.bg_mode, 2 | 4 | 6) {
                let hlookup = u32::from(self.bg[2].opt.hoffset);
                let vlookup = u32::from(self.bg[2].opt.voffset);
                let valid = 13 + id as u32;
                if self.io.bg_mode == 4 {
                    if hlookup >> valid & 1 != 0 {
                        if hlookup & 0x8000 == 0 {
                            hoffset = hpixel + (hlookup & !7) + (hscroll & 7);
                        } else {
                            voffset = vpixel.wrapping_add(hlookup);
                        }
                    }
                } else {
                    if hlookup >> valid & 1 != 0 {
                        hoffset = hpixel + (hlookup & !7) + (hscroll & 7);
                    }
                    if vlookup >> valid & 1 != 0 {
                        voffset = vpixel.wrapping_add(vlookup);
                    }
                }
            }

            let tile_size = u32::from(io.tile_size);
            let width = 256 << hires;
            let hsize = width << tile_size << (io.screen_size & 1);
            let vsize = width << tile_size << (io.screen_size >> 1 & 1);
            hoffset &= hsize - 1;
            voffset &= vsize - 1;

            let vtiles = 3 + tile_size;
            let htiles = if hires == 0 { vtiles } else { 4 };
            let htile = hoffset >> htiles;
            let vtile = voffset >> vtiles;
            let address = screen_address(io, htile, vtile);
            let attributes = self.vram_word(u32::from(address));

            let mode = io.mode as u32;
            let character_mask = self.vram_mask >> (3 + mode);
            let character_index = u32::from(io.tiledata_address) >> (3 + mode);
            let palette_offset = if self.io.bg_mode == 0 {
                (id as u32) << 5
            } else {
                0
            };

            let tile = &mut self.bg[id].tiles[index % TILES];
            tile.character = attributes & 0x3ff;
            tile.palette_group = (attributes >> 10 & 7) as u8;
            tile.priority = io.priority[usize::from(attributes >> 13 & 1)];
            tile.hmirror = attributes & 0x4000 != 0;
            tile.vmirror = attributes & 0x8000 != 0;
            if htiles == 4 && (hoffset & 8 != 0) != tile.hmirror {
                tile.character = (tile.character + 1) & 0x3ff;
            }
            if vtiles == 4 && (voffset & 8 != 0) != tile.vmirror {
                tile.character = (tile.character + 16) & 0x3ff;
            }
            let origin = (u32::from(tile.character) + character_index) & character_mask & 0xffff;
            if tile.vmirror {
                voffset ^= 7;
            }
            tile.address = ((origin << (3 + mode)) + (voffset & 7)) as u16;
            let palette_size = 2u32 << mode;
            let group = u32::from(tile.palette_group)
                .checked_shl(palette_size)
                .unwrap_or(0);
            tile.palette = (palette_offset + group) as u8;
        }
    }

    /// BG3's offset-per-tile fetch: row `y` 0 for the horizontal word, 8
    /// for the vertical.
    pub(crate) fn bg_fetch_offset(&mut self, id: usize, y: u32) {
        let v = self.counter.vcounter;
        if v == 0 {
            return;
        }
        let h = self.counter.hcounter;
        let io = self.bg[id].io;
        let x = (h >> 5) << 3;
        let hoffset = x + (u32::from(io.hoffset) & !7);
        let voffset = y + u32::from(io.voffset);
        let vtiles = 3 + u32::from(io.tile_size);
        let htiles = if self.hires() { 3 } else { vtiles };
        let address = screen_address(io, hoffset >> htiles, voffset >> vtiles);
        let word = self.vram_word(u32::from(address));
        if y == 0 {
            self.bg[id].opt.hoffset = word;
        }
        if y == 8 {
            self.bg[id].opt.voffset = word;
        }
    }

    pub(crate) fn bg_fetch_character(&mut self, id: usize, index: u32, half: u32) {
        if self.counter.vcounter == 0 {
            return;
        }
        let hires = u32::from(self.hires());
        let slot = (((self.counter.hcounter >> 5) << hires) + half) as usize % TILES;
        let address = u32::from(self.bg[id].tiles[slot].address) + (index << 3);
        let mut data = self.vram_word(address);
        let tile = &mut self.bg[id].tiles[slot];
        if !tile.hmirror {
            data = u16::from((data as u8).reverse_bits())
                | u16::from(((data >> 8) as u8).reverse_bits()) << 8;
        }
        tile.data[index as usize] =
            spread(data as u8, 49, 0x5555) | spread((data >> 8) as u8, 48, 0xaaaa);
    }

    /// One pixel of background `id`: the sub-screen half (`below`), which
    /// clears the outputs and in hi-res draws the even pixel, or the
    /// main-screen half.
    pub(crate) fn bg_run(&mut self, id: usize, below: bool) {
        if self.counter.vcounter == 0 {
            return;
        }
        let hires = self.hires();
        if below {
            self.bg[id].output.above.priority = 0;
            self.bg[id].output.below.priority = 0;
            if !hires {
                return;
            }
        }
        if self.bg[id].io.mode == BgMode::Mode7 {
            self.bg_run_mode7(id);
            return;
        }
        let x = (self.counter.hcounter - 56) >> 2;
        let size = u16::from(self.mosaic.size);
        let bg = &mut self.bg[id];
        let mode = bg.io.mode;
        let tile = &mut bg.tiles[usize::from(bg.rendering_index) % TILES];
        let mut color = (tile.data[0] & 3) as u8;
        tile.data[0] >>= 2;
        if mode >= BgMode::Bpp4 {
            color |= ((tile.data[1] & 3) as u8) << 2;
        }
        tile.data[1] >>= 2;
        if mode >= BgMode::Bpp8 {
            color |= ((tile.data[2] & 3) as u8) << 4;
        }
        tile.data[2] >>= 2;
        if mode >= BgMode::Bpp8 {
            color |= ((tile.data[3] & 3) as u8) << 6;
        }
        tile.data[3] >>= 2;

        let mut pixel = Pixel {
            priority: tile.priority,
            palette: if color != 0 {
                tile.palette.wrapping_add(color)
            } else {
                0
            },
            palette_group: tile.palette_group,
        };
        bg.pixel_counter = (bg.pixel_counter + 1) & 7;
        if bg.pixel_counter == 0 {
            bg.rendering_index = (bg.rendering_index + 1) & 127;
        }

        let sampled = !hires || below;
        // The first pixel, and every mosaic-size'th after it, starts a new
        // mosaic block; the decrement happens only off the first pixel.
        if sampled
            && (x == 0 || {
                bg.mosaic.hcounter = bg.mosaic.hcounter.wrapping_sub(1);
                bg.mosaic.hcounter == 0
            })
        {
            bg.mosaic.hcounter = size;
            bg.mosaic.pixel = pixel;
        } else if bg.mosaic.enable {
            pixel = bg.mosaic.pixel;
        }
        if pixel.palette == 0 {
            return;
        }
        if (!hires || !below) && bg.io.above_enable {
            bg.output.above = pixel;
        }
        if (!hires || below) && bg.io.below_enable {
            bg.output.below = pixel;
        }
    }

    /// One Mode 7 pixel. BG2 (EXTBG) takes its priority from colour bit 7.
    fn bg_run_mode7(&mut self, id: usize) {
        let sign16 = |v: u16| i32::from(v as i16);
        let sign13 = |v: u16| (i32::from(v) << 19) >> 19;
        let clip = |n: i32| if n & 0x2000 != 0 { n | !1023 } else { n & 1023 };
        let a = sign16(self.io.m7a);
        let b = sign16(self.io.m7b);
        let c = sign16(self.io.m7c);
        let d = sign16(self.io.m7d);
        let hcenter = sign13(self.io.m7x);
        let vcenter = sign13(self.io.m7y);
        let hoffset = sign13(self.io.hoffset_mode7);
        let voffset = sign13(self.io.voffset_mode7);

        let mut x = u32::from(self.bg[id].mosaic.hoffset);
        let mut y = self.counter.vcounter;
        if self.bg[0].mosaic.enable {
            y = y.wrapping_sub(self.mosaic_voffset());
        }
        let size = u16::from(self.mosaic.size);
        let mosaic = &mut self.bg[id].mosaic;
        if !mosaic.enable {
            mosaic.hoffset = mosaic.hoffset.wrapping_add(1);
        } else {
            mosaic.hcounter = mosaic.hcounter.wrapping_sub(1);
            if mosaic.hcounter == 0 {
                mosaic.hcounter = size;
                mosaic.hoffset = mosaic.hoffset.wrapping_add(size);
            }
        }
        if self.io.hflip_mode7 {
            x = 255u32.wrapping_sub(x);
        }
        if self.io.vflip_mode7 {
            y = 255u32.wrapping_sub(y);
        }

        // ares mixes signed and unsigned here: the products with x and y
        // are unsigned, so the final shift is logical.
        let y = y as i32;
        let origin_x = (a.wrapping_mul(clip(hoffset - hcenter)) & !63)
            .wrapping_add(b.wrapping_mul(clip(voffset - vcenter)) & !63)
            .wrapping_add(b.wrapping_mul(y) & !63)
            .wrapping_add(hcenter << 8);
        let origin_y = (c.wrapping_mul(clip(hoffset - hcenter)) & !63)
            .wrapping_add(d.wrapping_mul(clip(voffset - vcenter)) & !63)
            .wrapping_add(d.wrapping_mul(y) & !63)
            .wrapping_add(vcenter << 8);
        let pixel_x = (origin_x as u32).wrapping_add((a as u32).wrapping_mul(x)) >> 8;
        let pixel_y = (origin_y as u32).wrapping_add((c as u32).wrapping_mul(x)) >> 8;
        let palette_address = (pixel_y & 7) << 3 | pixel_x & 7;
        let tile_address = (pixel_y >> 3 & 127) << 7 | pixel_x >> 3 & 127;
        let out_of_bounds = (pixel_x | pixel_y) & !1023 != 0;

        let tile = if self.io.repeat_mode7 == 3 && out_of_bounds {
            0
        } else {
            self.vram_word(tile_address) as u8
        };
        let mut palette = if self.io.repeat_mode7 == 2 && out_of_bounds {
            0
        } else {
            (self.vram_word(u32::from(tile) << 6 | palette_address) >> 8) as u8
        };
        let bg = &mut self.bg[id];
        let priority = if id == 0 {
            bg.io.priority[0]
        } else {
            let p = bg.io.priority[usize::from(palette >> 7)];
            palette &= 0x7f;
            p
        };
        if palette == 0 {
            return;
        }
        let pixel = Pixel {
            priority,
            palette,
            palette_group: 0,
        };
        if bg.io.above_enable {
            bg.output.above = pixel;
        }
        if bg.io.below_enable {
            bg.output.below = pixel;
        }
    }
}

/// The tilemap word address for tile (`htile`, `vtile`) of a background,
/// stepping into the next 32×32 screen when the size allows.
fn screen_address(io: BgIo, htile: u32, vtile: u32) -> u16 {
    let hscreen: u16 = if io.screen_size & 1 != 0 { 32 << 5 } else { 0 };
    let vscreen: u16 = if io.screen_size & 2 != 0 {
        32 << (5 + (io.screen_size & 1))
    } else {
        0
    };
    let mut offset = ((htile & 31) | (vtile & 31) << 5) as u16;
    if htile & 0x20 != 0 {
        offset = offset.wrapping_add(hscreen);
    }
    if vtile & 0x20 != 0 {
        offset = offset.wrapping_add(vscreen);
    }
    io.screen_address.wrapping_add(offset)
}
