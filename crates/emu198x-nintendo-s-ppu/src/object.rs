//! Sprites: OAM, evaluation, the tile fetch and the per-pixel shifter.
//!
//! On each line, sprite evaluation checks one OAM entry every 8 clocks from
//! H=0 and notes up to 32 that cover the next line; a 33rd sets the range
//! flag. From H=1080 the fetch walks the noted sprites last first and reads
//! each 8-pixel tile column they put on screen, two VRAM words 4 clocks
//! apart, up to 34 tiles; a 35th sets the time flag. The tiles drawn on a
//! line were fetched at the end of the line before, so the renderer reads
//! the other half of a double buffer.
//!
//! The fetch is a resumable state machine rather than a loop, so a
//! force-blank toggled partway through it, or an OAM write landing between
//! two of its reads, takes effect at the clock it would on hardware.

use serde::{Deserialize, Serialize};

use crate::SPpu;
use crate::background::Pixel;

/// One OAM entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OamObject {
    pub x: u16,
    pub y: u8,
    pub character: u8,
    pub nameselect: bool,
    pub vflip: bool,
    pub hflip: bool,
    pub priority: u8,
    pub palette: u8,
    pub size: bool,
}

/// Read one byte of OAM: 0–511 the 128 four-byte entries, 512–543 the
/// table of X bit 8 and size bits (mirrored up to 1023).
pub(crate) fn oam_read(oam: &[OamObject], address: u16) -> u8 {
    if address & 0x200 == 0 {
        let o = &oam[usize::from(address >> 2) & 127];
        return match address & 3 {
            0 => o.x as u8,
            1 => o.y,
            2 => o.character,
            _ => {
                u8::from(o.nameselect)
                    | o.palette << 1
                    | o.priority << 4
                    | u8::from(o.hflip) << 6
                    | u8::from(o.vflip) << 7
            }
        };
    }
    let n = usize::from(address & 31) << 2;
    (0..4).fold(0, |data, i| {
        let o = &oam[n + i];
        data | (u8::from(o.x >> 8 != 0) | u8::from(o.size) << 1) << (2 * i)
    })
}

/// Write one byte of OAM.
pub(crate) fn oam_write(oam: &mut [OamObject], address: u16, data: u8) {
    if address & 0x200 == 0 {
        let o = &mut oam[usize::from(address >> 2) & 127];
        match address & 3 {
            0 => o.x = (o.x & 0x100) | u16::from(data),
            1 => o.y = data,
            2 => o.character = data,
            _ => {
                o.nameselect = data & 1 != 0;
                o.palette = (data >> 1) & 7;
                o.priority = (data >> 4) & 3;
                o.hflip = data & 0x40 != 0;
                o.vflip = data & 0x80 != 0;
            }
        }
        return;
    }
    let n = usize::from(address & 31) << 2;
    for i in 0..4 {
        let bits = data >> (2 * i);
        let o = &mut oam[n + i];
        o.x = (o.x & 0xff) | u16::from(bits & 1) << 8;
        o.size = bits & 2 != 0;
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ObjIo {
    pub above_enable: bool,
    pub below_enable: bool,
    pub interlace: bool,
    pub tiledata_address: u16,
    pub nameselect: u8,
    pub base_size: u8,
    pub first_sprite: u8,
    pub priority: [u8; 4],
    pub range_over: bool,
    pub time_over: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Item {
    pub valid: bool,
    pub index: u8,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ObjTile {
    pub valid: bool,
    pub x: u16,
    pub priority: u8,
    pub palette: u8,
    pub hflip: bool,
    pub data: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Object {
    pub io: ObjIo,
    pub latch_first_sprite: u8,
    pub x: u32,
    pub y: u32,
    pub item_count: u32,
    pub tile_count: u32,
    /// Which half of the double buffer this line fills.
    pub active: usize,
    pub items: [Vec<Item>; 2],
    pub tiles: [Vec<ObjTile>; 2],
    pub above: Pixel,
    pub below: Pixel,
}

impl Object {
    pub fn new() -> Self {
        Self {
            io: ObjIo::default(),
            latch_first_sprite: 0,
            x: 0,
            y: 0,
            item_count: 0,
            tile_count: 0,
            active: 0,
            items: [vec![Item::default(); 32], vec![Item::default(); 32]],
            tiles: [vec![ObjTile::default(); 34], vec![ObjTile::default(); 34]],
            above: Pixel::default(),
            below: Pixel::default(),
        }
    }
}

/// Where the sprite tile fetch has got to on this line.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct FetchState {
    pub active: bool,
    /// Clocks until the next piece of work.
    pub wait: u32,
    /// The item being walked, counting down from 31.
    pub item: i32,
    pub in_sprite: bool,
    pub tile_width: u32,
    pub x: u32,
    pub y: u32,
    pub chrx: u32,
    pub chry: u32,
    pub tiledata_address: u32,
    pub tx: u32,
    pub priority: u8,
    pub palette: u8,
    pub hflip: bool,
    /// The second word of a tile is still to read.
    pub high_pending: bool,
    pub slot: usize,
    pub address: u32,
}

impl SPpu {
    fn sprite_width(&self, o: &OamObject) -> u32 {
        const SMALL: [u32; 8] = [8, 8, 8, 16, 16, 32, 16, 16];
        const LARGE: [u32; 8] = [16, 32, 64, 32, 64, 64, 32, 32];
        let base = usize::from(self.obj.io.base_size);
        if o.size { LARGE[base] } else { SMALL[base] }
    }

    fn sprite_height(&self, o: &OamObject) -> u32 {
        const SMALL: [u32; 8] = [8, 8, 8, 16, 16, 32, 32, 32];
        const LARGE: [u32; 8] = [16, 32, 64, 32, 64, 64, 64, 32];
        let base = usize::from(self.obj.io.base_size);
        if o.size {
            LARGE[base]
        } else if self.obj.io.interlace && base >= 6 {
            // Hardware quirk: the tall small sizes halve in interlace.
            16
        } else {
            SMALL[base]
        }
    }

    pub(crate) fn obj_address_reset(&mut self) {
        self.io.oam_address = self.io.oam_base_address;
        self.obj_set_first_sprite();
    }

    pub(crate) fn obj_set_first_sprite(&mut self) {
        self.obj.io.first_sprite = if self.io.oam_priority {
            (self.io.oam_address >> 2) as u8 & 127
        } else {
            0
        };
    }

    pub(crate) fn obj_scanline(&mut self) {
        self.obj.latch_first_sprite = self.obj.io.first_sprite;
        self.obj.x = 0;
        self.obj.y = self.counter.vcounter;
        self.obj.item_count = 0;
        self.obj.tile_count = 0;
        self.obj.active ^= 1;
        let active = self.obj.active;
        for item in &mut self.obj.items[active] {
            item.valid = false;
        }
        for tile in &mut self.obj.tiles[active] {
            tile.valid = false;
        }
        if self.obj.y == self.state.vdisp && !self.io.display_disable {
            self.obj_address_reset();
        }
    }

    fn on_scanline(&self, o: &OamObject) -> bool {
        let width = self.sprite_width(o);
        if o.x > 256 && u32::from(o.x) + width - 1 < 512 {
            return false;
        }
        let height = self.sprite_height(o) >> u32::from(self.obj.io.interlace);
        let mut index = i64::from(self.obj.y % 256) - i64::from(o.y);
        if index < 0 {
            index += 256;
        }
        index < i64::from(height)
    }

    pub(crate) fn obj_evaluate(&mut self, index: u8) {
        if self.io.display_disable || self.obj.item_count > 32 {
            return;
        }
        let sprite = self.obj.latch_first_sprite.wrapping_add(index) & 127;
        let o = self.oam[usize::from(sprite)];
        if !self.on_scanline(&o) {
            return;
        }
        self.latch.oam_address = u16::from(sprite);
        self.obj.item_count += 1;
        if self.obj.item_count <= 32 {
            let active = self.obj.active;
            self.obj.items[active][self.obj.item_count as usize - 1] = Item {
                valid: true,
                index: sprite,
            };
        }
    }

    pub(crate) fn obj_run(&mut self) {
        self.obj.above.priority = 0;
        self.obj.below.priority = 0;
        let x = self.obj.x;
        self.obj.x += 1;
        let shown = self.obj.active ^ 1;
        for n in 0..34 {
            let tile = self.obj.tiles[shown][n];
            if !tile.valid {
                break;
            }
            let tile_x = (i32::from(tile.x) << 23) >> 23;
            let px = x as i32 - tile_x;
            if px & !7 != 0 {
                continue;
            }
            let shift = if tile.hflip { px } else { 7 - px } as u32;
            let color = (tile.data >> shift & 1)
                + (tile.data >> (shift + 7) & 2)
                + (tile.data >> (shift + 14) & 4)
                + (tile.data >> (shift + 21) & 8);
            if color != 0 {
                let pixel = Pixel {
                    priority: self.obj.io.priority[usize::from(tile.priority)],
                    palette: tile.palette.wrapping_add(color as u8),
                    palette_group: 0,
                };
                if self.obj.io.above_enable {
                    self.obj.above = pixel;
                }
                if self.obj.io.below_enable {
                    self.obj.below = pixel;
                }
            }
        }
    }

    pub(crate) fn obj_fetch_begin(&mut self) {
        self.fetch = FetchState {
            active: true,
            item: 31,
            ..FetchState::default()
        };
    }

    /// End a fetch the line has run out of time for.
    ///
    /// A fetch fits in the line unless force blank is toggled partway
    /// through it, because each sprite skipped while blanked costs 8
    /// clocks. That cost is ares's model; on hardware the fetch occupies
    /// fixed slots and cannot reach the next line. ares runs the fetch on
    /// regardless, which skips the next line entirely. This model stops it
    /// at the line's end, setting the overflow flags as a finished fetch
    /// would: the one place it knowingly parts from ares.
    pub(crate) fn obj_fetch_abandon(&mut self) {
        if self.fetch.active {
            self.obj.io.range_over |= self.obj.item_count > 32;
            self.obj.io.time_over |= self.obj.tile_count > 34;
            self.fetch.active = false;
            self.fetch.wait = 0;
        }
    }

    /// One two-clock step of the tile fetch.
    pub(crate) fn obj_fetch_step(&mut self) {
        if self.fetch.active && self.fetch.wait == 0 {
            self.obj_fetch_resume();
        }
        if self.fetch.wait > 0 {
            self.fetch.wait -= 2;
        }
    }

    /// Run the fetch until it next waits on the clock, or finishes.
    fn obj_fetch_resume(&mut self) {
        let active = self.obj.active;
        loop {
            if self.fetch.high_pending {
                if !self.io.display_disable {
                    let word = self.vram_word(self.fetch.address + 8);
                    let tile = &mut self.obj.tiles[active][self.fetch.slot];
                    tile.data = (tile.data & 0x0000_ffff) | u32::from(word) << 16;
                }
                self.fetch.high_pending = false;
                self.fetch.tx += 1;
                self.fetch.wait = 4;
                return;
            }
            if self.fetch.in_sprite {
                let f = self.fetch;
                let mut tx = f.tx;
                let mut started = false;
                while tx < f.tile_width {
                    let sx = (f.x + (tx << 3)) & 511;
                    if f.x != 256 && sx >= 256 && sx + 7 < 512 {
                        tx += 1;
                        continue;
                    }
                    let count = self.obj.tile_count;
                    self.obj.tile_count += 1;
                    if count >= 34 {
                        break;
                    }
                    let slot = self.obj.tile_count as usize - 1;
                    let mx = if f.hflip { f.tile_width - 1 - tx } else { tx };
                    let pos = f.tiledata_address + ((f.chry + ((f.chrx + mx) & 15)) << 4);
                    let address = (pos & 0xfff0) + (f.y & 7);
                    let low = (!self.io.display_disable).then(|| self.vram_word(address));
                    let tile = &mut self.obj.tiles[active][slot];
                    tile.valid = true;
                    tile.x = sx as u16;
                    tile.priority = f.priority;
                    tile.palette = f.palette;
                    tile.hflip = f.hflip;
                    if let Some(word) = low {
                        tile.data = (tile.data & 0xffff_0000) | u32::from(word);
                    }
                    self.fetch.tx = tx;
                    self.fetch.slot = slot;
                    self.fetch.address = address;
                    self.fetch.high_pending = true;
                    self.fetch.wait = 4;
                    started = true;
                    break;
                }
                if started {
                    return;
                }
                self.fetch.in_sprite = false;
                self.fetch.item -= 1;
                continue;
            }
            if self.fetch.item < 0 {
                self.obj.io.range_over |= self.obj.item_count > 32;
                self.obj.io.time_over |= self.obj.tile_count > 34;
                self.fetch.active = false;
                return;
            }
            let item = self.obj.items[active][self.fetch.item as usize];
            if !item.valid {
                self.fetch.item -= 1;
                continue;
            }
            if self.io.display_disable || self.counter.vcounter >= self.state.vdisp.wrapping_sub(1)
            {
                self.fetch.item -= 1;
                self.fetch.wait = 8;
                return;
            }
            self.latch.oam_address = u16::from(item.index);
            let o = self.oam[usize::from(item.index)];
            let width = self.sprite_width(&o);
            let height = self.sprite_height(&o);
            let mut y = self.obj.y.wrapping_sub(u32::from(o.y)) & 255;
            if self.obj.io.interlace {
                y <<= 1;
            }
            if o.vflip {
                y = if width == height {
                    height.wrapping_sub(1).wrapping_sub(y)
                } else if y < width {
                    width.wrapping_sub(1).wrapping_sub(y)
                } else {
                    (width + (width - 1)).wrapping_sub(y.wrapping_sub(width))
                };
            }
            if self.obj.io.interlace {
                let field = u32::from(self.counter.field);
                y = if o.vflip {
                    y.wrapping_sub(field)
                } else {
                    y.wrapping_add(field)
                };
            }
            let mut tiledata_address = u32::from(self.obj.io.tiledata_address);
            if o.nameselect {
                tiledata_address += (1 + u32::from(self.obj.io.nameselect)) << 12;
            }
            let y = y & 255;
            self.fetch.in_sprite = true;
            self.fetch.tile_width = width >> 3;
            self.fetch.x = u32::from(o.x) & 511;
            self.fetch.y = y;
            self.fetch.chrx = u32::from(o.character) & 15;
            self.fetch.chry = (((u32::from(o.character) >> 4) + (y >> 3)) & 15) << 4;
            self.fetch.tiledata_address = tiledata_address & 0xffff;
            self.fetch.tx = 0;
            self.fetch.priority = o.priority;
            self.fetch.palette = 128 + (o.palette << 4);
            self.fetch.hflip = o.hflip;
        }
    }
}
