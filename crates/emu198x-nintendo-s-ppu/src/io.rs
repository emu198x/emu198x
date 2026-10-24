//! The B-bus registers, `$2100–$213F`, and the VRAM, OAM and CGRAM ports.
//!
//! Reads of a write-only register return the open bus of the PPU that owns
//! it: PPU1 for `$2104–$212A`'s gaps and PPU2 for none, with the rest
//! returning the CPU's own open bus. Partially-driven reads keep the
//! undriven bits from the owning PPU's last read.

use crate::background::BgMode;
use crate::{SPpu, object};

impl SPpu {
    /// Latch the H/V counters into OPHCT/OPVCT.
    pub(crate) fn latch_counters(&mut self) {
        self.io.hcounter = self.counter.hdot() as u16;
        self.io.vcounter = self.counter.vcounter as u16;
        self.latch.counters = true;
    }

    fn active_display(&self) -> bool {
        !self.io.display_disable && self.counter.vcounter < self.state.vdisp
    }

    fn address_vram(&self) -> u16 {
        let a = self.io.vram_address;
        match self.io.vram_mapping {
            0 => a,
            1 => (a & 0xff00) | (a & 0x1f) << 3 | (a >> 5) & 7,
            2 => (a & 0xfe00) | (a & 0x3f) << 3 | (a >> 6) & 7,
            _ => (a & 0xfc00) | (a & 0x7f) << 3 | (a >> 7) & 7,
        }
    }

    fn read_vram(&self) -> u16 {
        if self.active_display() {
            return 0;
        }
        self.vram_word(u32::from(self.address_vram()))
    }

    fn write_vram(&mut self, high: bool, data: u8) {
        if self.active_display() {
            return;
        }
        let index = (u32::from(self.address_vram()) & self.vram_mask) as usize;
        let word = &mut self.vram[index];
        *word = if high {
            (*word & 0x00ff) | u16::from(data) << 8
        } else {
            (*word & 0xff00) | u16::from(data)
        };
    }

    /// During active display, OAM is addressed by the sprite the renderer
    /// is working on rather than the port.
    fn oam_redirect(&self, address: u16) -> u16 {
        if self.active_display() {
            if address & 0x200 == 0 {
                return (self.latch.oam_address << 2 | address & 1) & 0x3ff;
            }
            return 0x200 | (self.latch.oam_address >> 2) & 0x3ff;
        }
        address
    }

    fn read_oam(&self, address: u16) -> u8 {
        object::oam_read(&self.oam, self.oam_redirect(address) & 0x3ff)
    }

    fn write_oam(&mut self, address: u16, data: u8) {
        let address = self.oam_redirect(address) & 0x3ff;
        object::oam_write(&mut self.oam, address, data);
    }

    /// During the rendered part of a visible line, CGRAM is addressed by the
    /// palette entry the renderer last read.
    fn cgram_redirect(&self, address: u8) -> u8 {
        let h = self.counter.hcounter;
        let v = self.counter.vcounter;
        if !self.io.display_disable && v > 0 && v < self.state.vdisp && (88..1096).contains(&h) {
            self.latch.cgram_address
        } else {
            address
        }
    }

    /// Read a register. `address` is the low byte (`$00–$3F` for
    /// `$2100–$213F`); `cpu_mdr` is the CPU's open bus, returned by
    /// registers no PPU drives.
    pub fn read(&mut self, address: u8, cpu_mdr: u8) -> u8 {
        match address & 0x3f {
            0x04 | 0x05 | 0x06 | 0x08 | 0x09 | 0x0a | 0x14 | 0x15 | 0x16 | 0x18 | 0x19 | 0x1a
            | 0x24 | 0x25 | 0x26 | 0x28 | 0x29 | 0x2a => self.ppu1.mdr,
            0x34..=0x36 => {
                let product =
                    i32::from(self.io.m7a as i16) * i32::from((self.io.m7b >> 8) as u8 as i8);
                let byte = (product as u32 >> (8 * u32::from((address & 0x3f) - 0x34))) as u8;
                self.ppu1.mdr = byte;
                byte
            }
            0x37 => {
                if self.ext_latch {
                    self.latch_counters();
                }
                cpu_mdr
            }
            0x38 => {
                self.ppu1.mdr = self.read_oam(self.io.oam_address);
                self.io.oam_address = (self.io.oam_address + 1) & 0x3ff;
                self.obj_set_first_sprite();
                self.ppu1.mdr
            }
            0x39 => {
                self.ppu1.mdr = self.latch.vram as u8;
                if !self.io.vram_increment_mode {
                    self.latch.vram = self.read_vram();
                    self.io.vram_address = self
                        .io
                        .vram_address
                        .wrapping_add(self.io.vram_increment_size);
                }
                self.ppu1.mdr
            }
            0x3a => {
                self.ppu1.mdr = (self.latch.vram >> 8) as u8;
                if self.io.vram_increment_mode {
                    self.latch.vram = self.read_vram();
                    self.io.vram_address = self
                        .io
                        .vram_address
                        .wrapping_add(self.io.vram_increment_size);
                }
                self.ppu1.mdr
            }
            0x3b => {
                let index = usize::from(self.cgram_redirect(self.io.cgram_address));
                if self.io.cgram_address_latch {
                    let high = (self.cgram[index] >> 8) as u8 & 0x7f;
                    self.ppu2.mdr = (self.ppu2.mdr & 0x80) | high;
                    self.io.cgram_address = self.io.cgram_address.wrapping_add(1);
                } else {
                    self.ppu2.mdr = self.cgram[index] as u8;
                }
                self.io.cgram_address_latch = !self.io.cgram_address_latch;
                self.ppu2.mdr
            }
            0x3c => {
                if self.latch.hcounter {
                    self.ppu2.mdr = (self.ppu2.mdr & 0xfe) | (self.io.hcounter >> 8) as u8 & 1;
                } else {
                    self.ppu2.mdr = self.io.hcounter as u8;
                }
                self.latch.hcounter = !self.latch.hcounter;
                self.ppu2.mdr
            }
            0x3d => {
                if self.latch.vcounter {
                    self.ppu2.mdr = (self.ppu2.mdr & 0xfe) | (self.io.vcounter >> 8) as u8 & 1;
                } else {
                    self.ppu2.mdr = self.io.vcounter as u8;
                }
                self.latch.vcounter = !self.latch.vcounter;
                self.ppu2.mdr
            }
            0x3e => {
                let mut mdr = (self.ppu1.mdr & 0x10) | self.ppu1.version & 0x0f;
                mdr |= u8::from(self.obj.io.range_over) << 6;
                mdr |= u8::from(self.obj.io.time_over) << 7;
                self.ppu1.mdr = mdr;
                mdr
            }
            0x3f => {
                self.latch.hcounter = false;
                self.latch.vcounter = false;
                let mut mdr = (self.ppu2.mdr & 0x20) | self.ppu2.version & 0x0f;
                mdr |= u8::from(self.region == crate::Region::Pal) << 4;
                let latched = if self.ext_latch {
                    std::mem::take(&mut self.latch.counters)
                } else {
                    true
                };
                mdr |= u8::from(latched) << 6;
                mdr |= u8::from(self.counter.field) << 7;
                self.ppu2.mdr = mdr;
                mdr
            }
            _ => cpu_mdr,
        }
    }

    /// Write a register. `address` is the low byte (`$00–$3F`).
    pub fn write(&mut self, address: u8, data: u8) {
        let bit = |n: u8| data >> n & 1 != 0;
        match address & 0x3f {
            0x00 => {
                if self.io.display_disable && self.counter.vcounter == self.state.vdisp {
                    self.obj_address_reset();
                }
                self.io.display_brightness = data & 15;
                self.io.display_disable = bit(7);
            }
            0x01 => {
                self.obj.io.tiledata_address = u16::from(data & 7) << 13;
                self.obj.io.nameselect = (data >> 3) & 3;
                self.obj.io.base_size = data >> 5;
            }
            0x02 => {
                self.io.oam_base_address =
                    (self.io.oam_base_address & 0x200) | u16::from(data) << 1;
                self.obj_address_reset();
            }
            0x03 => {
                self.io.oam_base_address =
                    (self.io.oam_base_address & 0x1ff) | u16::from(data & 1) << 9;
                self.io.oam_priority = bit(7);
                self.obj_address_reset();
            }
            0x04 => {
                let latch_bit = self.io.oam_address & 1 != 0;
                let address = self.io.oam_address;
                self.io.oam_address = (self.io.oam_address + 1) & 0x3ff;
                if !latch_bit {
                    self.latch.oam = data;
                }
                if address & 0x200 != 0 {
                    self.write_oam(address, data);
                } else if latch_bit {
                    let low = self.latch.oam;
                    self.write_oam(address & !1, low);
                    self.write_oam((address & !1) + 1, data);
                }
                self.obj_set_first_sprite();
            }
            0x05 => {
                self.io.bg_mode = data & 7;
                self.io.bg_priority = bit(3);
                for id in 0..4 {
                    self.bg[id].io.tile_size = data >> (4 + id) & 1;
                }
                self.update_video_mode();
            }
            0x06 => {
                let was = self.mosaic_enabled();
                for id in 0..4 {
                    self.bg[id].mosaic.enable = data >> id & 1 != 0;
                }
                self.mosaic.size = (data >> 4) + 1;
                if !was && self.mosaic_enabled() {
                    self.mosaic.vcounter = self.mosaic.size + 1;
                }
            }
            0x07..=0x0a => {
                let bg = &mut self.bg[usize::from(address & 0x3f) - 7];
                bg.io.screen_size = data & 3;
                bg.io.screen_address = u16::from(data >> 2) << 10;
            }
            0x0b => {
                self.bg[0].io.tiledata_address = u16::from(data & 15) << 12;
                self.bg[1].io.tiledata_address = u16::from(data >> 4) << 12;
            }
            0x0c => {
                self.bg[2].io.tiledata_address = u16::from(data & 15) << 12;
                self.bg[3].io.tiledata_address = u16::from(data >> 4) << 12;
            }
            0x0d..=0x14 => {
                let reg = (address & 0x3f) - 0x0d;
                let id = usize::from(reg / 2);
                if id == 0 {
                    let value = u16::from(data) << 8 | u16::from(self.latch.mode7);
                    if reg & 1 == 0 {
                        self.io.hoffset_mode7 = value;
                    } else {
                        self.io.voffset_mode7 = value;
                    }
                    self.latch.mode7 = data;
                }
                if reg & 1 == 0 {
                    self.bg[id].io.hoffset = u16::from(data) << 8
                        | u16::from(self.latch.bgofs_ppu1 & !7)
                        | u16::from(self.latch.bgofs_ppu2 & 7);
                    self.latch.bgofs_ppu1 = data;
                    self.latch.bgofs_ppu2 = data & 7;
                } else {
                    self.bg[id].io.voffset =
                        u16::from(data) << 8 | u16::from(self.latch.bgofs_ppu1);
                    self.latch.bgofs_ppu1 = data;
                }
            }
            0x15 => {
                self.io.vram_increment_size = [1, 32, 128, 128][usize::from(data & 3)];
                self.io.vram_mapping = (data >> 2) & 3;
                self.io.vram_increment_mode = bit(7);
            }
            0x16 => {
                self.io.vram_address = (self.io.vram_address & 0xff00) | u16::from(data);
                self.latch.vram = self.read_vram();
            }
            0x17 => {
                self.io.vram_address = (self.io.vram_address & 0x00ff) | u16::from(data) << 8;
                self.latch.vram = self.read_vram();
            }
            0x18 => {
                self.write_vram(false, data);
                if !self.io.vram_increment_mode {
                    self.io.vram_address = self
                        .io
                        .vram_address
                        .wrapping_add(self.io.vram_increment_size);
                }
            }
            0x19 => {
                self.write_vram(true, data);
                if self.io.vram_increment_mode {
                    self.io.vram_address = self
                        .io
                        .vram_address
                        .wrapping_add(self.io.vram_increment_size);
                }
            }
            0x1a => {
                self.io.hflip_mode7 = bit(0);
                self.io.vflip_mode7 = bit(1);
                self.io.repeat_mode7 = data >> 6;
            }
            0x1b..=0x20 => {
                let value = u16::from(data) << 8 | u16::from(self.latch.mode7);
                self.latch.mode7 = data;
                match address & 0x3f {
                    0x1b => self.io.m7a = value,
                    0x1c => self.io.m7b = value,
                    0x1d => self.io.m7c = value,
                    0x1e => self.io.m7d = value,
                    0x1f => self.io.m7x = value,
                    _ => self.io.m7y = value,
                }
            }
            0x21 => {
                self.io.cgram_address = data;
                self.io.cgram_address_latch = false;
            }
            0x22 => {
                if self.io.cgram_address_latch {
                    let index = usize::from(self.cgram_redirect(self.io.cgram_address));
                    self.cgram[index] = u16::from(data & 0x7f) << 8 | u16::from(self.latch.cgram);
                    self.io.cgram_address = self.io.cgram_address.wrapping_add(1);
                } else {
                    self.latch.cgram = data;
                }
                self.io.cgram_address_latch = !self.io.cgram_address_latch;
            }
            0x23 => {
                self.window.io.layer[0].set_select(data);
                self.window.io.layer[1].set_select(data >> 4);
            }
            0x24 => {
                self.window.io.layer[2].set_select(data);
                self.window.io.layer[3].set_select(data >> 4);
            }
            0x25 => {
                self.window.io.layer[4].set_select(data);
                self.window.io.col.set_select(data >> 4);
            }
            0x26 => self.window.io.one_left = data,
            0x27 => self.window.io.one_right = data,
            0x28 => self.window.io.two_left = data,
            0x29 => self.window.io.two_right = data,
            0x2a => {
                for layer in 0..4 {
                    self.window.io.layer[layer].mask = data >> (2 * layer) & 3;
                }
            }
            0x2b => {
                self.window.io.layer[4].mask = data & 3;
                self.window.io.col.mask = (data >> 2) & 3;
            }
            0x2c => {
                for id in 0..4 {
                    self.bg[id].io.above_enable = bit(id as u8);
                }
                self.obj.io.above_enable = bit(4);
            }
            0x2d => {
                for id in 0..4 {
                    self.bg[id].io.below_enable = bit(id as u8);
                }
                self.obj.io.below_enable = bit(4);
            }
            0x2e => {
                for layer in 0..5 {
                    self.window.io.layer[layer].above_enable = bit(layer as u8);
                }
            }
            0x2f => {
                for layer in 0..5 {
                    self.window.io.layer[layer].below_enable = bit(layer as u8);
                }
            }
            0x30 => {
                self.dac.io.direct_color = bit(0);
                self.dac.io.blend_mode = bit(1);
                self.window.io.col.below_mask = (data >> 4) & 3;
                self.window.io.col.above_mask = data >> 6;
            }
            0x31 => {
                for layer in 0..6 {
                    self.dac.io.color_enable[layer] = bit(layer as u8);
                }
                self.dac.io.color_halve = bit(6);
                self.dac.io.color_mode = bit(7);
            }
            0x32 => {
                if bit(5) {
                    self.dac.io.color_red = data & 31;
                }
                if bit(6) {
                    self.dac.io.color_green = data & 31;
                }
                if bit(7) {
                    self.dac.io.color_blue = data & 31;
                }
            }
            0x33 => {
                self.io.interlace = bit(0);
                self.obj.io.interlace = bit(1);
                self.io.overscan = bit(2);
                self.io.pseudo_hires = bit(3);
                self.io.extbg = bit(6);
                self.update_video_mode();
            }
            _ => {}
        }
    }

    /// Set each layer's bit depth and priorities for the current mode.
    pub(crate) fn update_video_mode(&mut self) {
        use BgMode::{Bpp2, Bpp4, Bpp8, Inactive, Mode7};
        self.state.vdisp = if self.io.overscan { 240 } else { 225 };
        let set = |ppu: &mut SPpu, modes: [BgMode; 4]| {
            for (bg, mode) in ppu.bg.iter_mut().zip(modes) {
                bg.io.mode = mode;
            }
        };
        let prio = |ppu: &mut SPpu, id: usize, values: &[u8]| {
            for (slot, value) in values.iter().enumerate() {
                ppu.bg[id].io.priority[slot] = *value;
            }
        };
        let obj = |ppu: &mut SPpu, values: [u8; 4]| ppu.obj.io.priority = values;
        match self.io.bg_mode {
            0 => {
                set(self, [Bpp2, Bpp2, Bpp2, Bpp2]);
                prio(self, 0, &[8, 11]);
                prio(self, 1, &[7, 10]);
                prio(self, 2, &[2, 5]);
                prio(self, 3, &[1, 4]);
                obj(self, [3, 6, 9, 12]);
            }
            1 => {
                set(self, [Bpp4, Bpp4, Bpp2, Inactive]);
                if self.io.bg_priority {
                    prio(self, 0, &[5, 8]);
                    prio(self, 1, &[4, 7]);
                    prio(self, 2, &[1, 10]);
                    obj(self, [2, 3, 6, 9]);
                } else {
                    prio(self, 0, &[6, 9]);
                    prio(self, 1, &[5, 8]);
                    prio(self, 2, &[1, 3]);
                    obj(self, [2, 4, 7, 10]);
                }
            }
            2..=5 => {
                let modes = match self.io.bg_mode {
                    2 => [Bpp4, Bpp4, Inactive, Inactive],
                    3 => [Bpp8, Bpp4, Inactive, Inactive],
                    4 => [Bpp8, Bpp2, Inactive, Inactive],
                    _ => [Bpp4, Bpp2, Inactive, Inactive],
                };
                set(self, modes);
                prio(self, 0, &[3, 7]);
                prio(self, 1, &[1, 5]);
                obj(self, [2, 4, 6, 8]);
            }
            6 => {
                set(self, [Bpp4, Inactive, Inactive, Inactive]);
                prio(self, 0, &[2, 5]);
                obj(self, [1, 3, 4, 6]);
            }
            _ => {
                if self.io.extbg {
                    set(self, [Mode7, Mode7, Inactive, Inactive]);
                    prio(self, 0, &[3]);
                    prio(self, 1, &[1, 5]);
                    obj(self, [2, 4, 6, 7]);
                } else {
                    set(self, [Mode7, Inactive, Inactive, Inactive]);
                    prio(self, 0, &[2]);
                    obj(self, [1, 3, 4, 5]);
                }
            }
        }
    }
}
