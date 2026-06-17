//! Atari 2600 cartridge handling.
//!
//! Supports 2KB and 4KB (no banking) ROMs, plus F8 (8KB / 2 banks),
//! F6 (16KB / 4 banks), F4 (32KB / 8 banks) bank-switching via
//! hotspot detection. Reads or writes to specific addresses in the
//! `$1000-$1FFF` range trigger bank switches.
//!
//! Adapted from `Emu198x-Oldest/crates/machine-atari-2600/src/cartridge.rs`
//! (2026-06-01).

/// Cartridge banking scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankingScheme {
    /// 2KB or 4KB, no banking.
    None,
    /// F8: 8KB, 2 banks. Hotspots `$1FF8`/`$1FF9`.
    F8,
    /// F6: 16KB, 4 banks. Hotspots `$1FF6-$1FF9`.
    F6,
    /// F4: 32KB, 8 banks. Hotspots `$1FF4-$1FFB`.
    F4,
    /// Parker Brothers E0: 8KB as eight 1KB banks. The 4KB window is four 1KB
    /// slices — slices 0/1/2 are independently switchable (hotspots `$1FE0-$1FE7`,
    /// `$1FE8-$1FEF`, `$1FF0-$1FF7`), slice 3 is fixed to bank 7.
    E0,
    /// CBS RAM+ (FA): 12KB as three 4KB banks (hotspots `$1FF8`/`$1FF9`/`$1FFA`)
    /// plus 256 bytes of on-cart RAM — write port `$1000-$10FF`, read port
    /// `$1100-$11FF`.
    Fa,
    /// EF: 64KB as sixteen 4KB banks, selected by hotspots `$1FE0-$1FEF`.
    /// Pure address-decode (no RAM); the EFSC variant adds Superchip RAM.
    Ef,
    /// UA Limited: 8KB, two 4KB banks. Unusually, the bank-select hotspots sit
    /// *outside* the cart window — accessing `$0220` selects bank 0 and `$0240`
    /// bank 1 (low TIA-mirror addresses the cart snoops off the bus). The
    /// swapped-hotspot Digivision variant isn't modelled yet.
    Ua,
}

pub struct Cartridge {
    rom: Vec<u8>,
    scheme: BankingScheme,
    bank: usize,
    bank_size: usize,
    /// E0 only: the bank mapped into each of the three switchable 1KB slices.
    /// Slice 3 is always bank 7, so it isn't tracked here.
    e0_segments: [usize; 3],
    /// On-cart RAM (FA / CBS RAM+: 256 bytes). Empty for schemes without RAM.
    ram: Vec<u8>,
}

/// E0 slice size: 1 KB.
const E0_SLICE: usize = 1024;

impl Cartridge {
    /// Parse a ROM and detect the banking scheme from its size.
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        let (scheme, bank_size) = match data.len() {
            0..=2048 => (BankingScheme::None, data.len()),
            2049..=4096 => (BankingScheme::None, data.len()),
            // 8 KB is ambiguous: plain F8 or Parker Brothers E0. Distinguish by
            // scanning for an E0 hotspot-access signature (Stella's heuristic),
            // since both are the same length (#412).
            8192 if is_probably_e0(data) => (BankingScheme::E0, 4096),
            // UA also shares the 8 KB size; detect it by its hotspot-access
            // signature, ahead of the plain-F8 fallback.
            8192 if is_probably_ua(data) => (BankingScheme::Ua, 4096),
            8192 => (BankingScheme::F8, 4096),
            // 12 KB is unique to CBS RAM+ (FA) — three 4 KB banks + 256 B RAM.
            12288 => (BankingScheme::Fa, 4096),
            16384 => (BankingScheme::F6, 4096),
            32768 => (BankingScheme::F4, 4096),
            // 64 KB is EF (sixteen 4 KB banks). EFSC shares the size but adds
            // Superchip RAM — deferred to the Superchip-overlay work.
            65536 => (BankingScheme::Ef, 4096),
            other => return Err(format!("Unsupported ROM size: {other} bytes")),
        };
        let num_banks = data.len().checked_div(bank_size).unwrap_or(1);
        // Power-on bank, per Stella's per-scheme `getStartBank`. Most multi-bank
        // schemes (F8/F6/F4/FA) boot from the last bank, but EF explicitly
        // resets to bank 1 — its reset vector isn't replicated across all 16
        // banks, so the last-bank default would misboot a real EF cart.
        let bank = match scheme {
            BankingScheme::Ef => 1,
            BankingScheme::Ua => 0,
            _ => num_banks.saturating_sub(1),
        };
        let ram = if scheme == BankingScheme::Fa {
            vec![0u8; 256]
        } else {
            Vec::new()
        };
        Ok(Self {
            rom: data.to_vec(),
            scheme,
            bank,
            bank_size,
            // E0 power-on slice mapping (Stella's default: 4/5/6); the cart's
            // own startup code reprograms the slices before drawing.
            e0_segments: [4, 5, 6],
            ram,
        })
    }

    /// Read a byte from the cart at `$1000-$1FFF` (also fires hotspot
    /// detection for bank switching).
    pub fn read(&mut self, addr: u16) -> u8 {
        self.check_hotspot(addr);
        self.byte_at(addr)
    }

    /// The cart byte mapped at `addr`, with no bank-switch side effect.
    fn byte_at(&self, addr: u16) -> u8 {
        let offset = (addr & 0x0FFF) as usize;
        if self.scheme == BankingScheme::E0 {
            // Four 1KB slices: 0/1/2 follow their segment banks, slice 3 ($1C00-
            // $1FFF) is fixed to bank 7.
            let (seg_bank, slice_off) = match offset {
                0x000..=0x3FF => (self.e0_segments[0], offset),
                0x400..=0x7FF => (self.e0_segments[1], offset - 0x400),
                0x800..=0xBFF => (self.e0_segments[2], offset - 0x800),
                _ => (7, offset - 0xC00),
            };
            return self
                .rom
                .get(seg_bank * E0_SLICE + slice_off)
                .copied()
                .unwrap_or(0);
        }
        if self.scheme == BankingScheme::Fa {
            // The RAM read port ($1100-$11FF) overlays the bank window; the
            // write port ($1000-$10FF) reads back ROM (undefined on hardware).
            if (0x100..0x200).contains(&offset) {
                return self.ram.get(offset - 0x100).copied().unwrap_or(0);
            }
            return self
                .rom
                .get(self.bank * self.bank_size + offset)
                .copied()
                .unwrap_or(0);
        }
        if self.bank_size <= 2048 {
            self.rom[offset % self.rom.len()]
        } else {
            let idx = self.bank * self.bank_size + offset;
            self.rom.get(idx).copied().unwrap_or(0)
        }
    }

    /// Write to cart space — fires hotspot detection and, on FA, stores to the
    /// on-cart RAM through its write port (`$1000-$10FF`).
    pub fn write(&mut self, addr: u16, value: u8) {
        self.check_hotspot(addr);
        if self.scheme == BankingScheme::Fa {
            let offset = (addr & 0x0FFF) as usize;
            if offset < 0x100
                && let Some(cell) = self.ram.get_mut(offset)
            {
                *cell = value;
            }
        }
    }

    /// Snoop any bus access for schemes whose bank-select hotspots fall
    /// *outside* the `$1000-$1FFF` cart window. UA watches low TIA-mirror
    /// addresses (incomplete address decoding lets the cart see them), so the
    /// machine forwards every access here. Window-hotspot schemes ignore it,
    /// and their own switching stays in [`Self::read`]/[`Self::write`].
    pub fn snoop(&mut self, addr: u16) {
        if self.scheme == BankingScheme::Ua {
            // `$0220` → bank 0, `$0240` → bank 1. The mask folds the address
            // mirrors the real titles use (e.g. `$02C0`) onto these two cases.
            match addr & 0x1260 {
                0x0220 => self.bank = 0,
                0x0240 => self.bank = 1,
                _ => {}
            }
        }
    }

    /// Current bank.
    #[must_use]
    pub fn bank(&self) -> usize {
        self.bank
    }

    /// Banking scheme.
    #[must_use]
    pub fn scheme(&self) -> BankingScheme {
        self.scheme
    }

    fn check_hotspot(&mut self, addr: u16) {
        match self.scheme {
            BankingScheme::None => {}
            BankingScheme::F8 => match addr {
                0x1FF8 => self.bank = 0,
                0x1FF9 => self.bank = 1,
                _ => {}
            },
            BankingScheme::F6 => match addr {
                0x1FF6 => self.bank = 0,
                0x1FF7 => self.bank = 1,
                0x1FF8 => self.bank = 2,
                0x1FF9 => self.bank = 3,
                _ => {}
            },
            BankingScheme::F4 => match addr {
                0x1FF4 => self.bank = 0,
                0x1FF5 => self.bank = 1,
                0x1FF6 => self.bank = 2,
                0x1FF7 => self.bank = 3,
                0x1FF8 => self.bank = 4,
                0x1FF9 => self.bank = 5,
                0x1FFA => self.bank = 6,
                0x1FFB => self.bank = 7,
                _ => {}
            },
            // E0: each switchable slice picks one of the eight 1KB banks from
            // the low 3 bits of the hotspot address.
            BankingScheme::E0 => match addr {
                0x1FE0..=0x1FE7 => self.e0_segments[0] = usize::from(addr & 0x07),
                0x1FE8..=0x1FEF => self.e0_segments[1] = usize::from(addr & 0x07),
                0x1FF0..=0x1FF7 => self.e0_segments[2] = usize::from(addr & 0x07),
                _ => {}
            },
            BankingScheme::Fa => match addr {
                0x1FF8 => self.bank = 0,
                0x1FF9 => self.bank = 1,
                0x1FFA => self.bank = 2,
                _ => {}
            },
            // EF: sixteen banks across the $1FE0-$1FEF hotspot window.
            BankingScheme::Ef => {
                if (0x1FE0..=0x1FEF).contains(&addr) {
                    self.bank = usize::from(addr - 0x1FE0);
                }
            }
            // UA switches on out-of-window addresses, handled in `snoop`.
            BankingScheme::Ua => {}
        }
    }
}

impl Cartridge {
    /// Read ROM at the current bank/slice mapping with no bank-switch side
    /// effect (the debugger's view; `read` checks hotspots and may switch).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.byte_at(addr)
    }
}

/// Whether an 8 KB image is a Parker Brothers E0 cart rather than plain F8.
///
/// Both are 8 KB, so size alone can't tell them apart. E0 carts switch banks by
/// accessing `$1FE0-$1FF9` with absolute addressing; scan for the known
/// instruction signatures (ported from Stella's `isProbablyE0`, attributed to
/// MESS) that catch the real E0 titles without false-positiving on F8.
fn is_probably_e0(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 3]; 8] = [
        [0x8D, 0xE0, 0x1F], // STA $1FE0
        [0x8D, 0xE0, 0x5F], // STA $5FE0
        [0x8D, 0xE9, 0xFF], // STA $FFE9
        [0x0C, 0xE0, 0x1F], // NOP $1FE0
        [0xAD, 0xE0, 0x1F], // LDA $1FE0
        [0xAD, 0xE9, 0xFF], // LDA $FFE9
        [0xAD, 0xED, 0xFF], // LDA $FFED
        [0xAD, 0xF3, 0xBF], // LDA $BFF3
    ];
    SIGNATURES
        .iter()
        .any(|sig| rom.windows(sig.len()).any(|w| w == sig))
}

/// How many times `sig` occurs in `rom`.
fn count_bytes(rom: &[u8], sig: &[u8]) -> usize {
    rom.windows(sig.len()).filter(|w| *w == sig).count()
}

/// Whether an 8 KB image is a UA Limited cart. Like E0, it shares 8 KB with
/// plain F8, so detection scans for the instruction signatures that access the
/// `$0220`/`$0240` (and mirror) bankswitch hotspots — ported from Stella's
/// `isProbablyUA`.
fn is_probably_ua(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 3]; 7] = [
        [0x8D, 0x40, 0x02], // STA $240 (Funky Fish, Pleiades)
        [0xAD, 0x40, 0x02], // LDA $240
        [0xBD, 0x1F, 0x02], // LDA $21F,X (Gingerbread Man)
        [0x2C, 0xC0, 0x02], // BIT $2C0 (Time Pilot)
        [0x8D, 0xC0, 0x02], // STA $2C0 (Fathom, Vanguard)
        [0xAD, 0xC0, 0x02], // LDA $2C0 (Mickey)
        [0x2C, 0xB0, 0x0F], // BIT $FB0 (Digivision Beamrider)
    ];
    SIGNATURES.iter().any(|sig| count_bytes(rom, sig) >= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an 8 KB image whose eight 1 KB banks are filled with the bank
    /// index, optionally carrying an E0 signature so detection fires.
    fn banked_8k(with_e0_sig: bool) -> Vec<u8> {
        let mut rom = vec![0u8; 8192];
        for bank in 0..8 {
            rom[bank * 1024..(bank + 1) * 1024].fill(bank as u8);
        }
        if with_e0_sig {
            // STA $FFE9 somewhere in the image (one of Stella's E0 signatures).
            rom[0x10..0x13].copy_from_slice(&[0x8D, 0xE9, 0xFF]);
        }
        rom
    }

    #[test]
    fn detect_e0_vs_f8_for_8k() {
        // Plain 8K with no E0 signature → F8.
        assert_eq!(
            Cartridge::from_rom(&banked_8k(false)).expect("8K").scheme(),
            BankingScheme::F8
        );
        // 8K carrying an E0 access signature → E0.
        assert_eq!(
            Cartridge::from_rom(&banked_8k(true)).expect("8K").scheme(),
            BankingScheme::E0
        );
    }

    #[test]
    fn e0_slices_switch_independently_and_slice3_is_fixed() {
        let mut cart = Cartridge::from_rom(&banked_8k(true)).expect("E0");

        // Slice 3 ($1C00-$1FFF) is always bank 7.
        assert_eq!(cart.read(0x1C00), 7);

        // Each switchable slice selects its bank via its hotspot group.
        cart.read(0x1FE3); // slice 0 → bank 3
        cart.read(0x1FEA); // slice 1 → bank 2 ($1FEA & 7)
        cart.read(0x1FF5); // slice 2 → bank 5
        assert_eq!(cart.read(0x1000), 3, "slice 0 → bank 3");
        assert_eq!(cart.read(0x1400), 2, "slice 1 → bank 2");
        assert_eq!(cart.read(0x1800), 5, "slice 2 → bank 5");
        assert_eq!(cart.read(0x1C00), 7, "slice 3 stays bank 7");

        // Re-pointing one slice doesn't disturb the others.
        cart.read(0x1FE0); // slice 0 → bank 0
        assert_eq!(cart.read(0x1000), 0, "slice 0 → bank 0");
        assert_eq!(cart.read(0x1400), 2, "slice 1 unchanged");
    }

    /// Build a 12 KB CBS RAM+ image whose three 4 KB banks are each filled
    /// with the bank index.
    fn banked_12k() -> Vec<u8> {
        let mut rom = vec![0u8; 12288];
        for bank in 0..3 {
            rom[bank * 4096..(bank + 1) * 4096].fill(bank as u8);
        }
        rom
    }

    #[test]
    fn detect_fa_rom() {
        let cart = Cartridge::from_rom(&banked_12k()).expect("12K");
        assert_eq!(cart.scheme(), BankingScheme::Fa);
        assert_eq!(cart.bank(), 2, "power-on bank is the last (2)");
    }

    #[test]
    fn fa_banks_switch_on_their_hotspots() {
        let mut cart = Cartridge::from_rom(&banked_12k()).expect("FA");

        // A plain ROM read (not a hotspot, not the RAM window) reflects the
        // current bank's fill byte.
        assert_eq!(cart.read(0x1F00), 2, "starts in bank 2");
        cart.read(0x1FF8);
        assert_eq!(cart.read(0x1F00), 0, "$1FF8 → bank 0");
        cart.read(0x1FF9);
        assert_eq!(cart.read(0x1F00), 1, "$1FF9 → bank 1");
        cart.read(0x1FFA);
        assert_eq!(cart.read(0x1F00), 2, "$1FFA → bank 2");
    }

    #[test]
    fn fa_ram_round_trips_through_its_ports() {
        let mut cart = Cartridge::from_rom(&banked_12k()).expect("FA");

        // Write port $1000-$10FF in, read port $1100-$11FF out (same offset).
        cart.write(0x1005, 0xAB);
        cart.write(0x10FF, 0x42);
        assert_eq!(cart.read(0x1105), 0xAB, "RAM offset 5 reads back");
        assert_eq!(cart.read(0x11FF), 0x42, "RAM offset 255 reads back");

        // RAM survives a bank switch (it's separate from the ROM banks).
        cart.read(0x1FF8); // → bank 0
        assert_eq!(cart.read(0x1105), 0xAB, "RAM persists across banking");
    }

    /// Build a 64 KB EF image whose sixteen 4 KB banks are each filled with
    /// the bank index.
    fn banked_64k() -> Vec<u8> {
        let mut rom = vec![0u8; 65536];
        for bank in 0..16 {
            rom[bank * 4096..(bank + 1) * 4096].fill(bank as u8);
        }
        rom
    }

    #[test]
    fn detect_ef_rom() {
        let cart = Cartridge::from_rom(&banked_64k()).expect("64K");
        assert_eq!(cart.scheme(), BankingScheme::Ef);
        assert_eq!(cart.bank(), 1, "EF resets to bank 1 (Stella getStartBank)");
    }

    #[test]
    fn ef_banks_switch_across_the_full_hotspot_window() {
        let mut cart = Cartridge::from_rom(&banked_64k()).expect("EF");
        assert_eq!(cart.read(0x1F00), 1, "EF resets to bank 1");
        // Every hotspot $1FE0-$1FEF selects its bank 0-15.
        for bank in 0..16u16 {
            cart.read(0x1FE0 + bank);
            assert_eq!(
                cart.read(0x1F00),
                bank as u8,
                "$1F{:02X} → bank {bank}",
                0xE0 + bank
            );
        }
        // An address just outside the window leaves the bank alone.
        cart.read(0x1FE5);
        cart.read(0x1FDF);
        assert_eq!(cart.read(0x1F00), 5, "$1FDF is not a hotspot");
    }

    /// Build an 8 KB UA image: bank 0 filled `0xA0`, bank 1 `0xA1`, carrying a
    /// `STA $240` UA hotspot signature.
    fn ua_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 8192];
        rom[0..4096].fill(0xA0);
        rom[4096..8192].fill(0xA1);
        rom[0x20..0x23].copy_from_slice(&[0x8D, 0x40, 0x02]); // STA $240
        rom
    }

    #[test]
    fn detect_ua_vs_f8() {
        // Plain 8K (no UA/E0 signature) stays F8.
        assert_eq!(
            Cartridge::from_rom(&vec![0xEA; 8192]).expect("F8").scheme(),
            BankingScheme::F8
        );
        // 8K with a UA hotspot signature → UA, power-on bank 0.
        let cart = Cartridge::from_rom(&ua_rom()).expect("UA");
        assert_eq!(cart.scheme(), BankingScheme::Ua);
        assert_eq!(cart.bank(), 0, "UA resets to bank 0 (Stella default)");
    }

    #[test]
    fn ua_snoops_its_out_of_window_hotspots() {
        let mut cart = Cartridge::from_rom(&ua_rom()).expect("UA");
        assert_eq!(cart.read(0x1F00), 0xA0, "starts in bank 0");

        cart.snoop(0x0240); // → bank 1
        assert_eq!(cart.read(0x1F00), 0xA1, "$0240 → bank 1");
        cart.snoop(0x0220); // → bank 0
        assert_eq!(cart.read(0x1F00), 0xA0, "$0220 → bank 0");

        // The address mirror real titles use ($02C0) folds onto the bank-1 case.
        cart.snoop(0x02C0);
        assert_eq!(cart.read(0x1F00), 0xA1, "$02C0 mirror → bank 1");

        // An unrelated access leaves the bank alone.
        cart.snoop(0x1F00);
        assert_eq!(cart.read(0x1F00), 0xA1, "non-hotspot access is inert");
    }

    #[test]
    fn detect_2k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 2048]).expect("2K");
        assert_eq!(cart.scheme(), BankingScheme::None);
    }

    #[test]
    fn detect_4k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 4096]).expect("4K");
        assert_eq!(cart.scheme(), BankingScheme::None);
    }

    #[test]
    fn detect_f8_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 8192]).expect("F8");
        assert_eq!(cart.scheme(), BankingScheme::F8);
        assert_eq!(cart.bank(), 1);
    }

    #[test]
    fn detect_f6_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 16384]).expect("F6");
        assert_eq!(cart.scheme(), BankingScheme::F6);
        assert_eq!(cart.bank(), 3);
    }

    #[test]
    fn detect_f4_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 32768]).expect("F4");
        assert_eq!(cart.scheme(), BankingScheme::F4);
        assert_eq!(cart.bank(), 7);
    }

    #[test]
    fn reject_invalid_size() {
        assert!(Cartridge::from_rom(&vec![0u8; 5000]).is_err());
    }

    #[test]
    fn f8_bank_switching() {
        let mut rom = vec![0u8; 8192];
        rom[..4096].fill(0xAA);
        rom[4096..].fill(0xBB);
        let mut cart = Cartridge::from_rom(&rom).expect("F8");
        assert_eq!(cart.read(0x1000), 0xBB);
        cart.read(0x1FF8);
        assert_eq!(cart.read(0x1000), 0xAA);
        cart.read(0x1FF9);
        assert_eq!(cart.read(0x1000), 0xBB);
    }

    #[test]
    fn two_kb_rom_mirrors() {
        let mut rom = vec![0u8; 2048];
        rom[0] = 0x42;
        let mut cart = Cartridge::from_rom(&rom).expect("2K");
        assert_eq!(cart.read(0x1000), 0x42);
        assert_eq!(cart.read(0x1800), 0x42);
    }
}
