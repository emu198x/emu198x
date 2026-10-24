//! True hi-res boards: the Memotech HRG, the Quicksilva hi-res board and the
//! G007.
//!
//! The stock ZX81 can only show bitmaps by trickery -- pseudo hi-res pins the
//! row counter, WRX points `I` at RAM -- and [`crate::video`] models both. The
//! boards here are hardware. Each sits on the expansion edge, brings its own
//! memory and firmware, and takes over the pixels the ULA would have looked
//! up in the character set. The ZX81 still draws the picture by running
//! through a display file with the ULA forcing `NOP`s; what changes is where
//! each eight pixels come from.
//!
//! No source in the reference library describes these boards; the synthesis
//! `syntheses/sinclair-zx81/zx81-hi-res-techniques.md` only names them. The
//! decode below follows EightyOne (`Source/zx81/zx81.cpp`, consulted for facts
//! only), the one emulator that models all three. Where a detail is
//! EightyOne's own reading rather than a stated fact, it says so.
//!
//! # Memotech HRG
//!
//! - A 2 KB EPROM at `$2000-$27FF`, the board's graphics firmware.
//! - 1 KB of RAM that every write to `$0000-$03FF` lands in, and that
//!   replaces the ROM there for reads while `I` is odd.
//! - A mode latch loaded by an `IN` from any port whose low byte is `$5F`:
//!   the mode is the port's high byte, A8-A15. Zero is off; three inverts.
//! - With `I` odd and a mode set, each display fetch shows the display-file
//!   byte itself as eight pixels instead of naming a character. EightyOne
//!   leaves the stock path in charge until the first byte after the leading
//!   `NEWLINE`, and for the fetches made with `R` at `$80` or `$81`.
//!
//! # Quicksilva hi-res
//!
//! - 6 KB of RAM at `$A000-$B7FF`, 192 lines of 32 bytes: the bitmap.
//! - A 2 KB EPROM at `$2800-$2FFF`.
//! - Reading anywhere in `$2000-$3FFF` switches hi-res on; writing there
//!   switches it off.
//! - With it on and the video output not held at sync, every display fetch
//!   except a `NEWLINE` takes the next byte of the bitmap from an address
//!   counter. Any `OUT` puts the counter back at `$A000`, and it wraps
//!   there after `$B7FF`.
//!
//! # G007
//!
//! - 1 KB of RAM at `$2000-$23FF` and a 2 KB EPROM at `$2800-$2FFF`.
//! - `$0C00-$0CFF` always reads the EPROM's `$2C00-$2CFF`.
//! - While `I` is odd, `$0000-$00FF` and `$0200-$02FF` read the board's RAM
//!   at `$2000-$20FF` and `$2200-$22FF`.
//! - With `I` odd, every display fetch shows its byte as pixels -- a
//!   `NEWLINE` included, so no `HALT` ends the line. The board leaves that to
//!   the refresh-driven interrupt on A6, which `R` reaches on its own.
//!
//! # Firmware
//!
//! Each board's EPROM is a firmware image the user supplies, never one this
//! repository carries. Without one the EPROM reads `$FF`. The display path
//! does not need it: the boards' hi-res modes are reachable by a program that
//! sets them up itself, which is how the tests drive them.

use serde::{Deserialize, Serialize};

/// The size of each board's EPROM.
pub const BOARD_ROM_SIZE: usize = 0x800;

/// Which hi-res board is on the expansion edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HiResBoard {
    /// None: the stock machine, with pseudo hi-res and WRX still available.
    #[default]
    None,
    /// The Memotech HRG.
    Memotech,
    /// The Quicksilva hi-res graphics board.
    Quicksilva,
    /// The G007 hi-res graphics module.
    G007,
}

impl HiResBoard {
    /// Every choice, the stock machine first.
    pub const ALL: [Self; 4] = [Self::None, Self::Memotech, Self::Quicksilva, Self::G007];

    /// The id a command line or query names the board by.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Memotech => "memotech",
            Self::Quicksilva => "quicksilva",
            Self::G007 => "g007",
        }
    }

    /// Resolve an [`id`](Self::id).
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|board| board.id() == id)
    }

    /// The board's own RAM.
    const fn ram_size(self) -> usize {
        match self {
            Self::None => 0,
            Self::Memotech | Self::G007 => 0x400,
            Self::Quicksilva => 0x1800,
        }
    }
}

/// The Quicksilva bitmap's first and one-past-last addresses.
const QS_BITMAP_START: u16 = 0xA000;
const QS_BITMAP_END: u16 = 0xB800;

/// The `NEWLINE` that ends a display-file line, and executes as `HALT`.
const NEWLINE: u8 = 0x76;

/// The fitted board and its state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct HiRes {
    board: HiResBoard,
    /// The EPROM, `$FF` until an image is loaded.
    rom: Vec<u8>,
    ram: Vec<u8>,
    /// Memotech: the mode the last `IN` from port `$5F` latched.
    memotech_mode: u8,
    /// Memotech: whether the display file's leading `NEWLINE` has passed
    /// since the NMI generator was last seen running.
    memotech_started: bool,
    /// Quicksilva: whether hi-res is switched on.
    qs_enabled: bool,
    /// Quicksilva: where the next bitmap byte comes from.
    qs_address: u16,
}

impl HiRes {
    pub(crate) fn new(board: HiResBoard) -> Self {
        Self {
            board,
            rom: if board == HiResBoard::None {
                Vec::new()
            } else {
                vec![0xFF; BOARD_ROM_SIZE]
            },
            ram: vec![0; board.ram_size()],
            memotech_mode: 0,
            memotech_started: false,
            qs_enabled: false,
            qs_address: QS_BITMAP_START,
        }
    }

    pub(crate) const fn board(&self) -> HiResBoard {
        self.board
    }

    /// Load the board's EPROM image.
    pub(crate) fn load_rom(&mut self, image: &[u8]) -> Result<(), String> {
        if self.board == HiResBoard::None {
            return Err("no hi-res board is fitted".to_owned());
        }
        if image.len() != BOARD_ROM_SIZE {
            return Err(format!(
                "a hi-res board EPROM is {BOARD_ROM_SIZE} bytes, got {}",
                image.len()
            ));
        }
        self.rom.copy_from_slice(image);
        Ok(())
    }

    fn rom_byte(&self, offset: u16) -> u8 {
        self.rom.get(usize::from(offset)).copied().unwrap_or(0xFF)
    }

    fn ram_byte(&self, offset: u16) -> u8 {
        self.ram.get(usize::from(offset)).copied().unwrap_or(0xFF)
    }

    /// The byte the board drives for a read of `addr` with `i` in the
    /// processor's `I` register, or `None` when the machine's own decode
    /// stands.
    pub(crate) fn read(&self, addr: u16, i: u8) -> Option<u8> {
        let i_odd = i & 1 != 0;
        match self.board {
            HiResBoard::None => None,
            HiResBoard::Memotech => match addr {
                0x0000..=0x03FF if i_odd => Some(self.ram_byte(addr)),
                0x2000..=0x27FF => Some(self.rom_byte(addr - 0x2000)),
                _ => None,
            },
            HiResBoard::Quicksilva => match addr {
                0x2800..=0x2FFF => Some(self.rom_byte(addr - 0x2800)),
                // The rest of the switching range: EightyOne returns its
                // empty memory there, which reads zero.
                0x2000..=0x3FFF => Some(0x00),
                QS_BITMAP_START..QS_BITMAP_END => Some(self.ram_byte(addr - QS_BITMAP_START)),
                _ => None,
            },
            HiResBoard::G007 => match addr {
                0x0000..=0x00FF | 0x0200..=0x02FF if i_odd => Some(self.ram_byte(addr)),
                0x0C00..=0x0CFF => Some(self.rom_byte(addr + 0x2000 - 0x2800)),
                0x2000..=0x23FF => Some(self.ram_byte(addr - 0x2000)),
                0x2800..=0x2FFF => Some(self.rom_byte(addr - 0x2800)),
                _ => None,
            },
        }
    }

    /// What a processor read does to the board beyond the byte it returns.
    pub(crate) fn note_read(&mut self, addr: u16) {
        if self.board == HiResBoard::Quicksilva && (0x2000..=0x3FFF).contains(&addr) {
            self.qs_enabled = true;
        }
    }

    /// A processor write. Returns whether the board took it; the machine's
    /// own decode drops every write in these ranges anyway.
    pub(crate) fn write(&mut self, addr: u16, value: u8) -> bool {
        let slot = match self.board {
            HiResBoard::None => return false,
            HiResBoard::Memotech => match addr {
                0x0000..=0x03FF => Some(addr),
                0x2000..=0x27FF => None,
                _ => return false,
            },
            HiResBoard::Quicksilva => match addr {
                0x2000..=0x3FFF => {
                    self.qs_enabled = false;
                    None
                }
                QS_BITMAP_START..QS_BITMAP_END => Some(addr - QS_BITMAP_START),
                _ => return false,
            },
            HiResBoard::G007 => match addr {
                0x2000..=0x23FF => Some(addr - 0x2000),
                0x2800..=0x2FFF => None,
                _ => return false,
            },
        };
        if let Some(byte) = slot.and_then(|offset| self.ram.get_mut(usize::from(offset))) {
            *byte = value;
        }
        true
    }

    /// An `IN`. The Memotech's mode latch answers port `$5F` with `$FF`.
    pub(crate) fn io_read(&mut self, port: u16) -> Option<u8> {
        if self.board == HiResBoard::Memotech && port & 0xFF == 0x5F {
            self.memotech_mode = (port >> 8) as u8;
            return Some(0xFF);
        }
        None
    }

    /// An `OUT`, which restarts the Quicksilva bitmap.
    pub(crate) fn io_write(&mut self) {
        self.qs_address = QS_BITMAP_START;
    }

    /// Every opcode fetch, display or not. The Memotech watches the NMI
    /// generator here to find the start of each display file.
    pub(crate) fn note_opcode_fetch(&mut self, nmi_enabled: bool) {
        if nmi_enabled {
            self.memotech_started = false;
        }
    }

    /// A display fetch -- an opcode fetch at or above `$8000` -- of `byte`.
    /// Returns the eight pixels the board puts on the shift register, or
    /// `None` to leave the fetch to the ULA.
    ///
    /// `r` is `R` as the fetch sees it, before this fetch's increment.
    /// EightyOne tests `R` after it, so the Memotech's `$80`/`$81` gate is
    /// applied to `R + 1` in the low seven bits.
    pub(crate) fn display_fetch(&mut self, byte: u8, i: u8, r: u8, vsync: bool) -> Option<u8> {
        let i_odd = i & 1 != 0;
        match self.board {
            HiResBoard::None => None,
            HiResBoard::Memotech if i_odd && self.memotech_mode != 0 => {
                if !self.memotech_started && byte != NEWLINE {
                    self.memotech_started = true;
                }
                let r_after = (r & 0x80) | (r.wrapping_add(1) & 0x7F);
                if !self.memotech_started || r_after == 0x80 || r_after == 0x81 {
                    return None;
                }
                Some(if self.memotech_mode == 3 { !byte } else { byte })
            }
            HiResBoard::Memotech => None,
            HiResBoard::Quicksilva if self.qs_enabled && !vsync && byte != NEWLINE => {
                let pixels = self.ram_byte(self.qs_address - QS_BITMAP_START);
                self.qs_address += 1;
                if self.qs_address == QS_BITMAP_END {
                    self.qs_address = QS_BITMAP_START;
                }
                Some(pixels)
            }
            HiResBoard::Quicksilva => None,
            HiResBoard::G007 if i_odd => Some(byte),
            HiResBoard::G007 => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for board in HiResBoard::ALL {
            assert_eq!(HiResBoard::from_id(board.id()), Some(board));
        }
        assert_eq!(HiResBoard::from_id("wrx"), None);
    }

    #[test]
    fn the_eprom_must_be_two_kilobytes() {
        let mut board = HiRes::new(HiResBoard::G007);
        assert!(board.load_rom(&[0; 100]).is_err());
        let mut image = vec![0; BOARD_ROM_SIZE];
        image[0x400] = 0xAB;
        board.load_rom(&image).expect("load");
        assert_eq!(board.read(0x2C00, 0), Some(0xAB));
        assert_eq!(board.read(0x0C00, 0), Some(0xAB), "$0C00 shows $2C00");
        assert!(HiRes::new(HiResBoard::None).load_rom(&image).is_err());
    }
}

/// The boards driven end to end: a synthetic ROM sets the board up, then
/// jumps into a display file and lets the processor run through it in FAST
/// mode, which is all the display mechanism needs to paint.
#[cfg(test)]
mod board_tests {
    use super::{BOARD_ROM_SIZE, HiResBoard};
    use crate::Zx81;

    const INK: u32 = 0xFF00_0000;

    /// Two bytes no character set in the test ROM could produce: the ROM is
    /// zero above the program, so the stock path paints only paper.
    const PATTERN: [u8; 2] = [0x81, 0xA5];

    /// `DI`, `I` = `$1F` (odd, and still a ROM page, so not WRX), an `IN`
    /// from port `mode:$5F`, an access to `$2000`, a short delay to reach
    /// the visible lines, `R` = `$10`, then `JP $C000` into the display file.
    /// `access` is `$3A` (`LD A,(nn)`) or `$32` (`LD (nn),A`).
    fn rom_with(mode: u8, access: u8) -> Vec<u8> {
        let program = [
            0xF3, //             DI
            0x3E, 0x1F, //       LD A,$1F
            0xED, 0x47, //       LD I,A
            0x3E, mode, //       LD A,mode
            0xDB, 0x5F, //       IN A,($5F)
            access, 0x00, 0x20, // LD A,($2000) or LD ($2000),A
            0x01, 0xC8, 0x00, // LD BC,200
            0x0B, //             loop: DEC BC
            0x78, //             LD A,B
            0xB1, //             OR C
            0x20, 0xFB, //       JR NZ,loop
            0x3E, 0x10, //       LD A,$10
            0xED, 0x4F, //       LD R,A
            0xC3, 0x00, 0xC0, // JP $C000
        ];
        // The program runs from `$0400`: once `I` is odd the Memotech and the
        // G007 lay their RAM over parts of `$0000-$03FF`.
        let mut rom = vec![0; 0x2000];
        rom[..3].copy_from_slice(&[0xC3, 0x00, 0x04]); // JP $0400
        rom[0x400..0x400 + program.len()].copy_from_slice(&program);
        rom
    }

    fn rom(mode: u8) -> Vec<u8> {
        rom_with(mode, 0x3A)
    }

    /// A 16 KB machine whose display file at `$C000` (RAM at `$4000`)
    /// opens with `PATTERN` repeated.
    fn machine(board: HiResBoard, mode: u8) -> Zx81 {
        let mut sys = Zx81::new(rom(mode), 0x4000).expect("init");
        sys.set_hires_board(board);
        for offset in 0..64u16 {
            sys.poke(0x4000 + offset, PATTERN[usize::from(offset & 1)]);
        }
        sys
    }

    /// A 16 KB machine with a Quicksilva board whose bitmap is `PATTERN`
    /// repeated, running `rom`.
    fn quicksilva(rom: Vec<u8>) -> Zx81 {
        let mut sys = Zx81::new(rom, 0x4000).expect("init");
        sys.set_hires_board(HiResBoard::Quicksilva);
        for offset in 0..0x1800u16 {
            sys.poke(0xA000 + offset, PATTERN[usize::from(offset & 1)]);
        }
        sys
    }

    /// Whether some row shows `bytes` as contiguous pixels.
    fn shows(sys: &Zx81, bytes: &[u8]) -> bool {
        let want: Vec<bool> = bytes
            .iter()
            .flat_map(|byte| (0..8).map(move |bit| byte & (0x80 >> bit) != 0))
            .collect();
        let width = sys.framebuffer_width() as usize;
        sys.framebuffer().chunks(width).any(|row| {
            let ink: Vec<bool> = row.iter().map(|&pixel| pixel == INK).collect();
            ink.windows(want.len())
                .any(|window| window == want.as_slice())
        })
    }

    #[test]
    fn a_stock_machine_paints_the_character_set_not_the_bytes() {
        let mut sys = machine(HiResBoard::None, 1);
        sys.run_frame();
        assert!(!shows(&sys, &PATTERN));
    }

    /// With a mode latched from port `$5F` and `I` odd, the display-file
    /// bytes are the pixels.
    #[test]
    fn the_memotech_shows_display_file_bytes_as_pixels() {
        let mut sys = machine(HiResBoard::Memotech, 1);
        sys.run_frame();
        assert!(shows(&sys, &PATTERN));
    }

    #[test]
    fn the_memotech_mode_three_inverts() {
        let mut sys = machine(HiResBoard::Memotech, 3);
        sys.run_frame();
        assert!(shows(&sys, &PATTERN.map(|byte| !byte)));
        assert!(!shows(&sys, &PATTERN));
    }

    #[test]
    fn the_memotech_is_off_until_a_mode_is_latched() {
        let mut sys = machine(HiResBoard::Memotech, 0);
        sys.run_frame();
        assert!(!shows(&sys, &PATTERN));
    }

    /// Writes to `$0000-$03FF` land in the board's RAM, which reads back
    /// only while `I` is odd; the EPROM sits at `$2000`.
    #[test]
    fn the_memotech_ram_overlays_the_rom_while_i_is_odd() {
        let mut sys = Zx81::new(rom(0), 0x400).expect("init");
        sys.set_hires_board(HiResBoard::Memotech);
        sys.poke(0x0100, 0x5A);
        assert_eq!(sys.peek(0x0100), 0x00, "I even: the ROM");
        sys.cpu_mut().regs.i = 0x1F;
        assert_eq!(sys.peek(0x0100), 0x5A, "I odd: the board's RAM");
        assert_eq!(sys.peek(0x2000), 0xFF, "an EPROM with no image");
        let mut image = vec![0; BOARD_ROM_SIZE];
        image[0] = 0xC9;
        sys.load_hires_rom(&image).expect("load");
        assert_eq!(sys.peek(0x2000), 0xC9);
    }

    /// The bitmap comes from the board's RAM through its own counter, not
    /// from the display file, once a read of `$2000` has switched it on.
    #[test]
    fn the_quicksilva_paints_its_bitmap() {
        let mut sys = quicksilva(rom(0));
        assert_eq!(sys.peek(0xA001), PATTERN[1], "the bitmap RAM reads back");
        sys.run_frame();
        assert!(shows(&sys, &PATTERN));
    }

    #[test]
    fn a_write_to_the_switching_range_leaves_the_quicksilva_off() {
        let mut sys = quicksilva(rom_with(0, 0x32));
        sys.run_frame();
        assert!(!shows(&sys, &PATTERN));
    }

    /// With `I` odd every display fetch is pixels, with no mode to set.
    #[test]
    fn the_g007_shows_display_file_bytes_while_i_is_odd() {
        let mut sys = machine(HiResBoard::G007, 0);
        sys.run_frame();
        assert!(shows(&sys, &PATTERN));
    }

    #[test]
    fn the_g007_maps_its_ram_and_eprom() {
        let mut sys = Zx81::new(rom(0), 0x400).expect("init");
        sys.set_hires_board(HiResBoard::G007);
        sys.poke(0x2000, 0x11);
        sys.poke(0x2200, 0x22);
        assert_eq!(sys.peek(0x2000), 0x11);
        assert_eq!(sys.peek(0x0000), 0xC3, "I even: the ROM");
        sys.cpu_mut().regs.i = 0x1F;
        assert_eq!(sys.peek(0x0000), 0x11, "I odd: $0000 shows $2000");
        assert_eq!(sys.peek(0x0200), 0x22, "and $0200 shows $2200");
        assert_eq!(sys.peek(0x0100), 0x00, "but $0100 stays ROM");
    }

    #[test]
    fn the_board_survives_a_snapshot() {
        let mut sys = machine(HiResBoard::G007, 0);
        sys.poke(0x2001, 0x77);
        let bytes = postcard::to_allocvec(&sys).expect("encode");
        let restored: Zx81 = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored.hires_board(), HiResBoard::G007);
        assert_eq!(restored.peek(0x2001), 0x77);
    }
}
