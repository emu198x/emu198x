//! Discrete 320349 counters and CRTC boards. Evidence: primary PET board sheets
//! 320349-6/7/8, 8032029-3/10; VICE 3.10 pet.c and crtc.c.
//! The existing MC6845 counter stages cover the validated non-interlaced
//! editor rasters. Full MOS 6545 personality/interlace remains #166; this
//! board split does not qualify those unimplemented modes.
use emu198x_motorola_6845::Crtc6845;
use serde::{Deserialize, Serialize};

/// Physical configurations, independent of ROM contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Board {
    Pet2001n32k,
    Pet4032,
    Pet8032,
}
impl Board {
    pub const fn columns(self) -> u32 {
        if matches!(self, Self::Pet8032) {
            80
        } else {
            40
        }
    }
    pub const fn active_height(self) -> u32 {
        if matches!(self, Self::Pet8032) {
            250
        } else {
            200
        }
    }
    pub const fn video_mask(self) -> usize {
        if matches!(self, Self::Pet8032) {
            0x7ff
        } else {
            0x3ff
        }
    }
    pub const fn basic_start(self) -> u16 {
        if matches!(self, Self::Pet2001n32k) {
            0xc000
        } else {
            0xb000
        }
    }
}
#[derive(Serialize, Deserialize)]
pub(crate) enum Video {
    Discrete { x: u8, y: u16, video_on: bool },
    Crtc { chip: Crtc6845, y: u16 },
}
pub(crate) struct Beam {
    pub x: u8,
    pub y: u16,
    pub ma: u16,
    pub ra: u8,
    pub display: bool,
    pub reverse: bool,
    pub frame: bool,
}
impl Video {
    pub fn new(board: Board) -> Self {
        if board == Board::Pet2001n32k {
            return Self::Discrete {
                x: 0,
                y: 0,
                video_on: true,
            };
        }
        let mut chip = Crtc6845::new();
        // Convenient stable initial raster; real editors explicitly program
        // these values. VICE captures with 901498-01 and exact 901474-04.
        let regs = if board == Board::Pet8032 {
            [
                0x32, 0x28, 0x28, 8, 0x26, 2, 0x19, 0x20, 0, 9, 0, 0, 0x10, 0,
            ]
        } else {
            [
                0x31, 0x28, 0x29, 0x0f, 0x31, 0, 0x19, 0x25, 0, 7, 0, 0, 0x10, 0,
            ]
        };
        for (i, value) in regs.into_iter().enumerate() {
            chip.write_address(i as u8);
            chip.write_data(value);
        }
        Self::Crtc { chip, y: 0 }
    }
    pub fn tick(&mut self) -> Beam {
        match self {
            Self::Discrete { x, y, video_on } => {
                let mut beam = Beam {
                    x: *x,
                    y: *y,
                    ma: (*y / 8) * 40 + u16::from(*x),
                    ra: (*y % 8) as u8,
                    display: *x < 40 && *y < 200,
                    reverse: false,
                    frame: false,
                };
                // VIDEO ON changes after the final displayed character, not
                // at the frame boundary. VICE's discrete retrace path models
                // the same 200-line / 60-line interval.
                if *x == 39 {
                    if *y == 199 {
                        *video_on = false;
                    } else if *y == 259 {
                        *video_on = true;
                    }
                }
                *x += 1;
                if *x == 64 {
                    *x = 0;
                    *y += 1;
                    if *y == 260 {
                        *y = 0;
                        beam.frame = true;
                    }
                }
                beam
            }
            Self::Crtc { chip, y } => {
                let x = chip.horizontal_counter();
                let scan_y = *y;
                let ra = chip.raster_address();
                let frame = chip.tick();
                let beam = Beam {
                    x,
                    y: scan_y,
                    ma: chip.memory_address(),
                    ra,
                    display: chip.display_enable,
                    reverse: chip.memory_address() & 0x1000 == 0,
                    frame,
                };
                if frame {
                    *y = 0;
                } else if chip.horizontal_counter() == 0 {
                    *y = y.saturating_add(1);
                }
                beam
            }
        }
    }
    pub fn retrace_level(&self) -> bool {
        match self {
            Self::Discrete { video_on, .. } => *video_on,
            Self::Crtc { chip, .. } => !chip.vsync,
        }
    }
    pub fn read_status(&self) -> u8 {
        match self {
            Self::Discrete { .. } => 0xff,
            Self::Crtc { chip, .. } => {
                if chip.in_vertical_retrace() {
                    0x20
                } else {
                    0
                }
            }
        }
    }
    pub fn read_data(&self) -> u8 {
        match self {
            Self::Discrete { .. } => 0xff,
            Self::Crtc { chip, .. } => chip.read_data(),
        }
    }
    pub fn write_address(&mut self, value: u8) {
        if let Self::Crtc { chip, .. } = self {
            chip.write_address(value);
        }
    }
    pub fn write_data(&mut self, value: u8) {
        if let Self::Crtc { chip, .. } = self {
            chip.write_data(value);
        }
    }
    pub fn matches(&self, board: Board) -> bool {
        match self {
            Self::Discrete { x, y, .. } => board == Board::Pet2001n32k && *x < 64 && *y < 260,
            Self::Crtc { chip, y } => {
                board != Board::Pet2001n32k
                    && *y <= 4128
                    && chip.variant() == emu198x_motorola_6845::Crtc6845Variant::Mc6845
            }
        }
    }
    pub fn height(&self) -> u32 {
        match self {
            Self::Discrete { .. } => 200,
            Self::Crtc { chip, .. } => {
                u32::from(chip.regs()[6]) * (u32::from(chip.regs()[9] & 31) + 1)
            }
        }
    }
}
