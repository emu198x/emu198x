//! Starpath / Arcadia Supercharger (AR) support — the fast-load path.
//!
//! The Supercharger (1982) is not a bankswitch cartridge: it's a RAM-expansion
//! peripheral that loads game code from cassette into 6 KB of RAM behind a small
//! BIOS, with a control register that pages the RAM and selects RAM/ROM/write
//! modes. The `.a26` images in the TOSEC set are the *already-decoded* tape, in
//! `LOAD_SIZE` (8448-byte) slots — one slot per tape "load".
//!
//! This module implements the **fast-load** path used by every emulator's
//! `.a26` handling: a 294-byte dummy BIOS (`DUMMY_ROM`) boots, hits the `$1850`
//! hotspot, and copies the load straight into the 6 KB RAM via the slot header's
//! page table. The real 2 KB BIOS / PCM tape streaming (the tape-accurate path)
//! is a separate future feature; the game runs at full accuracy after load
//! either way.
//!
//! Adapted from Stella `CartAR.cxx` / `CartAR.hxx` (GPL-2.0-or-later), at
//! `198x/emulators/atari/stella/src/emucore/`. See
//! `docs/plans/atari-2600-supercharger-ar-546.md`.

/// One RAM/ROM bank: 2 KB.
const BANK_SIZE: usize = 2048;
/// RAM working set: three 2 KB banks (banks 0, 1, 2).
const RAM_SIZE: usize = 3 * BANK_SIZE; // 6144
/// The 8 KB working image: 6 KB RAM + a 2 KB ROM region (the dummy BIOS).
const IMAGE_SIZE: usize = RAM_SIZE + BANK_SIZE; // 8192
/// One tape "load" in the `.a26` file: 6 KB RAM pages + 2 KB ROM placeholder +
/// a 256-byte header.
pub(crate) const LOAD_SIZE: usize = 8448;

/// Whether a ROM image is a Supercharger `.a26`: one or more 8448-byte loads.
/// Unambiguous by size — 8448×N collides with no other 2600 scheme.
#[must_use]
pub(crate) fn is_supercharger(len: usize) -> bool {
    len >= LOAD_SIZE && len.is_multiple_of(LOAD_SIZE)
}

/// Deterministic accumulator value left in A when the dummy BIOS exits. The real
/// BIOS leaves a random value here (Stella seeds it from its RNG); we use a
/// fixed-seed LCG step so power-up is reproducible, mirroring the RIOT timer
/// power-up approach. Faithful Supercharger code never relies on it.
const POWERUP_ACCUMULATOR: u8 = {
    let v = 0x1982_0546_u32
        .wrapping_mul(1_664_525)
        .wrapping_add(1_013_904_223)
        >> 16;
    v as u8
};

/// An effect the cart asks the machine to apply after a read/write. The AR
/// scheme is the only one that needs to reach back into machine state (the RIOT
/// RAM the dummy BIOS reads its load parameters from).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArEffect {
    /// Nothing for the machine to do.
    None,
    /// The cart mutated its mapped image (Stella marks the page dirty). The
    /// 2600 machine has no dirty-page tracking, so this is informational.
    Dirty,
    /// Stage `(riot_addr, value)` pokes into the RIOT RAM for the dummy BIOS:
    /// `$fe` = header[0] (bank-switch byte), `$ff` = header[1] (start address),
    /// `$80` = header[2] (next-load number).
    RamPokes([(u8, u8); 3]),
}

/// The Supercharger's mapped state and 8 KB working image.
pub struct Supercharger {
    /// The raw `.a26` file: `num_loads` × `LOAD_SIZE` bytes.
    file: Vec<u8>,
    /// The 8 KB working set (6 KB RAM + 2 KB dummy-BIOS ROM).
    image: [u8; IMAGE_SIZE],
    /// Number of tape loads in `file`.
    num_loads: usize,
    /// Image offset for each 2 KB cart slot: `[0]` = `$1000-$17FF`, `[1]` =
    /// `$1800-$1FFF`. An offset of `RAM_SIZE` means the slot maps the ROM region.
    image_offset: [usize; 2],
    /// Whether writes to `$1000-$10FF` commit into RAM (control bit D1).
    write_enabled: bool,
    /// ROM power (control bit D0 inverted): `true` = ROM powered. Tracked for
    /// fidelity; the fast-load path does not gate on it.
    power: bool,
    /// The low 5 bits of the last bank configuration, for the debug/`scheme`
    /// view.
    current_bank: u8,
    /// The data-hold register — loaded from the low byte of an access to
    /// `$1000-$10FF`, then committed to a bank config (`$1FF8`) or a RAM write.
    data_hold: u8,
    /// A RAM write is armed and waiting for its 5-distinct-access window.
    write_pending: bool,
    /// The distinct-access count captured when `data_hold` was last loaded.
    num_distinct_at_hold: u32,
    /// Whether the most recent `load_into_ram` passed every header/page
    /// checksum. A soft mismatch still loads (real dumps run with soft errors),
    /// so this is advisory — exposed for diagnostics.
    last_load_ok: bool,
}

impl Supercharger {
    /// Build the cart from a Supercharger `.a26` image (`is_supercharger(len)`
    /// must hold). Initializes the dummy BIOS and resets to the power-on bank
    /// configuration, exactly as Stella's `reset()`.
    #[must_use]
    pub fn new(data: &[u8]) -> Self {
        let mut sc = Self {
            file: data.to_vec(),
            image: [0u8; IMAGE_SIZE],
            num_loads: data.len() / LOAD_SIZE,
            image_offset: [0, 0],
            write_enabled: false,
            power: true,
            current_bank: 0,
            data_hold: 0,
            write_pending: false,
            num_distinct_at_hold: 0,
            last_load_ok: true,
        };
        sc.reset();
        sc
    }

    /// Power-on reset: zero the RAM, install the dummy BIOS, and select bank
    /// configuration 0 (RAM bank 2 low, ROM high, write disabled, ROM powered)
    /// so the CPU resets through the BIOS entry vector.
    fn reset(&mut self) {
        self.image[..RAM_SIZE].fill(0);
        self.initialize_rom();
        self.write_enabled = false;
        self.power = true;
        self.data_hold = 0;
        self.num_distinct_at_hold = 0;
        self.write_pending = false;
        self.bank_configuration(0);
    }

    /// Install the 294-byte dummy BIOS into the ROM region: jam-fill, copy the
    /// BIOS, patch the fast-BIOS flag (skip the progress-bar code) and the
    /// power-up accumulator, then point the 6502 vectors at the entry (`$F80A`).
    fn initialize_rom(&mut self) {
        // Fill the 2 KB ROM region with an illegal opcode that jams a real 6502.
        self.image[RAM_SIZE..IMAGE_SIZE].fill(0x02);
        self.image[RAM_SIZE..RAM_SIZE + DUMMY_ROM.len()].copy_from_slice(&DUMMY_ROM);
        // Offset 109: 0xFF → skip the SC BIOS progress bars (fast load).
        self.image[RAM_SIZE + 109] = 0xFF;
        // Offset 281: the accumulator value on BIOS exit.
        self.image[RAM_SIZE + 281] = POWERUP_ACCUMULATOR;
        // 6502 vectors (NMI/RESET/IRQ low+high) → BIOS load entry at $F80A.
        self.image[IMAGE_SIZE - 4] = 0x0A;
        self.image[IMAGE_SIZE - 3] = 0xF8;
        self.image[IMAGE_SIZE - 2] = 0x0A;
        self.image[IMAGE_SIZE - 1] = 0xF8;
    }

    /// Apply a bank-configuration byte (control register write via `$1FF8`).
    /// D4-D2 select the RAM/ROM slot mapping, D1 enables RAM writes, D0 inverts
    /// to ROM power.
    fn bank_configuration(&mut self, cfg: u8) {
        // Per-slot 2 KB-bank index for each of the 8 configurations. An index of
        // 3 means the ROM region (`3 * BANK_SIZE == RAM_SIZE`).
        const OFFSET_0: [usize; 8] = [2, 0, 2, 0, 2, 1, 2, 1];
        const OFFSET_1: [usize; 8] = [3, 3, 0, 2, 3, 3, 1, 2];
        let bank_cfg = ((cfg & 0b1_1100) >> 2) as usize;
        self.current_bank = cfg & 0b1_1111;
        self.power = cfg & 0b0_0001 == 0;
        self.write_enabled = cfg & 0b0_0010 != 0;
        self.image_offset[0] = OFFSET_0[bank_cfg] * BANK_SIZE;
        self.image_offset[1] = OFFSET_1[bank_cfg] * BANK_SIZE;
    }

    /// The image byte mapped at a cart-window address. The 4 KB window is two
    /// 2 KB slots: lower (`$1000-$17FF`) and upper (`$1800-$1FFF`).
    fn image_index(&self, addr: u16) -> usize {
        let slot = if addr & 0x0800 != 0 { 1 } else { 0 };
        (addr as usize & 0x07FF) + self.image_offset[slot]
    }

    /// Find the load whose header `load`-number matches, validate it, copy its
    /// pages into RAM, and return the RIOT-RAM pokes the dummy BIOS expects.
    /// Returns [`ArEffect::None`] if no matching load exists.
    fn load_into_ram(&mut self, load: u8) -> ArEffect {
        for image in 0..self.num_loads {
            let base = image * LOAD_SIZE;
            let header_base = base + IMAGE_SIZE; // header follows the 8 KB body
            if self.file[header_base + 5] != load {
                continue;
            }
            let header: [u8; 256] = self.file[header_base..header_base + 256]
                .try_into()
                .expect("256-byte header");

            // Header checksum (sum of header[0..8] == 0x55) is advisory: a soft
            // mismatch still loads on real hardware.
            let mut ok = checksum(&header[0..8]) == 0x55;

            let pages = header[3] as usize;
            for j in 0..pages {
                let desc = header[16 + j];
                let bank = (desc & 0b011) as usize;
                let page = ((desc & 0b1_1100) >> 2) as usize;
                let src_off = base + j * 256;
                let src: [u8; 256] = self.file[src_off..src_off + 256]
                    .try_into()
                    .expect("256-byte page");
                // Page checksum: sum(src) + descriptor + per-page check == 0x55.
                let page_sum = checksum(&src)
                    .wrapping_add(desc)
                    .wrapping_add(header[64 + j]);
                ok &= page_sum == 0x55;
                // Copy into Supercharger RAM (never into the ROM region).
                if bank < 3 {
                    let dst = bank * BANK_SIZE + page * 256;
                    self.image[dst..dst + 256].copy_from_slice(&src);
                }
            }
            self.last_load_ok = ok;
            // Hand the BIOS its bank-switch byte, start address, and next-load
            // number via the RIOT RAM.
            return ArEffect::RamPokes([(0xfe, header[0]), (0xff, header[1]), (0x80, header[2])]);
        }
        ArEffect::None
    }

    /// The control/write mechanism, fired on every cart read *and* write to
    /// `$1000-$1FFF`. Returns whether it mutated the mapped RAM (Stella's dirty
    /// signal). `distinct_accesses` is the machine's address-changed counter.
    fn handle_hotspot(&mut self, addr: u16, distinct_accesses: u32) -> bool {
        // A pending write expires once more than 5 distinct accesses have passed.
        if self.write_pending && distinct_accesses > self.num_distinct_at_hold + 5 {
            self.write_pending = false;
        }
        // (1) Load the data-hold register: any access to $1000-$10FF — the value
        //     is the low byte of the address.
        if addr & 0x0F00 == 0 && (!self.write_enabled || !self.write_pending) {
            self.data_hold = addr as u8;
            self.num_distinct_at_hold = distinct_accesses;
            self.write_pending = true;
        }
        // (2) Commit a bank configuration: access to $1FF8.
        else if addr & 0x1FFF == 0x1FF8 {
            self.write_pending = false;
            self.bank_configuration(self.data_hold);
        }
        // (3) Commit a RAM write: exactly 5 distinct accesses after the hold.
        else if self.write_enabled
            && self.write_pending
            && distinct_accesses == self.num_distinct_at_hold + 5
        {
            let slot = if addr & 0x0800 == 0 { 0 } else { 1 };
            self.write_pending = false;
            // The ROM slot can't be poked.
            if slot == 0 || self.image_offset[1] != RAM_SIZE {
                let off = (addr as usize & 0x07FF) + self.image_offset[slot];
                self.image[off] = self.data_hold;
                return true;
            }
        }
        false
    }

    /// A cart read at `$1000-$1FFF`. `distinct_accesses` is the machine's
    /// address-changed counter; `ram_80` is RIOT RAM `$80` (the BIOS's load
    /// number). Returns the byte and any effect the machine must apply.
    pub fn read(&mut self, addr: u16, distinct_accesses: u32, ram_80: u8) -> (u8, ArEffect) {
        // Fast-load hotspot: the dummy BIOS reaches $1850 with the ROM mapped in
        // the upper slot. Load the block named by RIOT $80, then return the BIOS
        // byte at that address.
        if addr & 0x1FFF == 0x1850 && self.image_offset[1] == RAM_SIZE {
            let effect = self.load_into_ram(ram_80);
            let byte = self.image[(addr as usize & 0x07FF) + self.image_offset[1]];
            return (byte, effect);
        }
        let mutated = self.handle_hotspot(addr, distinct_accesses);
        let byte = self.image[self.image_index(addr)];
        (
            byte,
            if mutated {
                ArEffect::Dirty
            } else {
                ArEffect::None
            },
        )
    }

    /// A cart write at `$1000-$1FFF`. AR ignores the data bus value — the value
    /// comes from the address — so only the hotspot mechanism runs.
    pub fn write(&mut self, addr: u16, distinct_accesses: u32) -> ArEffect {
        if self.handle_hotspot(addr, distinct_accesses) {
            ArEffect::Dirty
        } else {
            ArEffect::None
        }
    }

    /// The image byte mapped at `addr`, with no side effects (debugger view).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.image[self.image_index(addr)]
    }

    /// The low 5 bits of the current bank configuration (debug view).
    #[must_use]
    pub fn current_bank(&self) -> usize {
        self.current_bank as usize
    }
}

/// 8-bit checksum: the wrapping sum of the bytes.
fn checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

/// The 294-byte dummy Supercharger BIOS. Adapted verbatim from Stella
/// `CartAR.cxx` `ourDummyROMCode` (GPL-2.0-or-later). It boots, configures the
/// TIA/RIOT minimally, and triggers the `$1850` fast-load of load #0.
const DUMMY_ROM: [u8; 294] = [
    0xa5, 0xfa, 0x85, 0x80, 0x4c, 0x18, 0xf8, 0xff, 0xff, 0xff, 0x78, 0xd8, 0xa0, 0x00, 0xa2, 0x00,
    0x94, 0x00, 0xe8, 0xd0, 0xfb, 0x4c, 0x50, 0xf8, 0xa2, 0x00, 0xbd, 0x06, 0xf0, 0xad, 0xf8, 0xff,
    0xa2, 0x00, 0xad, 0x00, 0xf0, 0xea, 0xbd, 0x00, 0xf7, 0xca, 0xd0, 0xf6, 0x4c, 0x50, 0xf8, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xa2, 0x03, 0xbc, 0x22, 0xf9, 0x94, 0xfa, 0xca, 0x10, 0xf8, 0xa0, 0x00, 0xa2, 0x28, 0x94, 0x04,
    0xca, 0x10, 0xfb, 0xa2, 0x1c, 0x94, 0x81, 0xca, 0x10, 0xfb, 0xa9, 0xff, 0xc9, 0x00, 0xd0, 0x03,
    0x4c, 0x13, 0xf9, 0xa9, 0x00, 0x85, 0x1b, 0x85, 0x1c, 0x85, 0x1d, 0x85, 0x1e, 0x85, 0x1f, 0x85,
    0x19, 0x85, 0x1a, 0x85, 0x08, 0x85, 0x01, 0xa9, 0x10, 0x85, 0x21, 0x85, 0x02, 0xa2, 0x07, 0xca,
    0xca, 0xd0, 0xfd, 0xa9, 0x00, 0x85, 0x20, 0x85, 0x10, 0x85, 0x11, 0x85, 0x02, 0x85, 0x2a, 0xa9,
    0x05, 0x85, 0x0a, 0xa9, 0xff, 0x85, 0x0d, 0x85, 0x0e, 0x85, 0x0f, 0x85, 0x84, 0x85, 0x85, 0xa9,
    0xf0, 0x85, 0x83, 0xa9, 0x74, 0x85, 0x09, 0xa9, 0x0c, 0x85, 0x15, 0xa9, 0x1f, 0x85, 0x17, 0x85,
    0x82, 0xa9, 0x07, 0x85, 0x19, 0xa2, 0x08, 0xa0, 0x00, 0x85, 0x02, 0x88, 0xd0, 0xfb, 0x85, 0x02,
    0x85, 0x02, 0xa9, 0x02, 0x85, 0x02, 0x85, 0x00, 0x85, 0x02, 0x85, 0x02, 0x85, 0x02, 0xa9, 0x00,
    0x85, 0x00, 0xca, 0x10, 0xe4, 0x06, 0x83, 0x66, 0x84, 0x26, 0x85, 0xa5, 0x83, 0x85, 0x0d, 0xa5,
    0x84, 0x85, 0x0e, 0xa5, 0x85, 0x85, 0x0f, 0xa6, 0x82, 0xca, 0x86, 0x82, 0x86, 0x17, 0xe0, 0x0a,
    0xd0, 0xc3, 0xa9, 0x02, 0x85, 0x01, 0xa2, 0x1c, 0xa0, 0x00, 0x84, 0x19, 0x84, 0x09, 0x94, 0x81,
    0xca, 0x10, 0xfb, 0xa6, 0x80, 0xdd, 0x00, 0xf0, 0xa9, 0x9a, 0xa2, 0xff, 0xa0, 0x00, 0x9a, 0x4c,
    0xfa, 0x00, 0xcd, 0xf8, 0xff, 0x4c,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic single-load AR image with a valid header and 24 pages
    /// (the full 6 KB), each page filled with its page index so the post-load
    /// RAM layout is checkable. Page j maps to bank `j / 8`, page `j % 8`.
    fn synthetic_single_load() -> Vec<u8> {
        let mut file = vec![0u8; LOAD_SIZE];
        // 24 pages of body data: page j is 256 bytes all equal to j.
        for j in 0..24usize {
            file[j * 256..(j + 1) * 256].fill(j as u8);
        }
        let h = IMAGE_SIZE; // header offset
        file[h] = 0xAB; // header[0] → $fe (bank-switch byte)
        file[h + 1] = 0xCD; // header[1] → $ff (start address)
        file[h + 2] = 0x01; // header[2] → $80 (next-load number)
        file[h + 3] = 24; // header[3] page count
        file[h + 5] = 0; // header[5] this load's number
        // header[0..8] must sum to 0x55. Current sum without header[7]:
        // 0xAB+0xCD+0x01+24 = 401; header[7] = 0x55 - (401 & 0xff).
        let partial = (0xABu32 + 0xCD + 0x01 + 24) as u8;
        file[h + 7] = 0x55u8.wrapping_sub(partial);
        // Page descriptors + per-page checksums.
        for j in 0..24usize {
            let bank = (j / 8) as u8;
            let page = (j % 8) as u8;
            let desc = (page << 2) | bank;
            file[h + 16 + j] = desc;
            // A 256-byte uniform page sums to 0 (256 ≡ 0 mod 256), so the page
            // checksum byte just balances the descriptor to 0x55.
            file[h + 64 + j] = 0x55u8.wrapping_sub(desc);
        }
        file
    }

    #[test]
    fn detects_supercharger_sizes() {
        assert!(is_supercharger(8448));
        assert!(is_supercharger(2 * 8448));
        assert!(is_supercharger(4 * 8448));
        assert!(!is_supercharger(8192));
        assert!(!is_supercharger(8447));
        assert!(!is_supercharger(0));
    }

    #[test]
    fn dummy_bios_installed_with_entry_vector() {
        let sc = Supercharger::new(&synthetic_single_load());
        // The BIOS sits in the ROM region; its first bytes are LDA $FA / STA $80.
        assert_eq!(sc.image[RAM_SIZE], 0xa5);
        assert_eq!(sc.image[RAM_SIZE + 1], 0xfa);
        // Fast-BIOS flag patched.
        assert_eq!(sc.image[RAM_SIZE + 109], 0xFF);
        // Reset/IRQ vectors point at $F80A.
        assert_eq!(
            &sc.image[IMAGE_SIZE - 4..],
            &[0x0A, 0xF8, 0x0A, 0xF8],
            "entry vectors → $F80A"
        );
    }

    #[test]
    fn power_on_bank_config_maps_ram_low_rom_high() {
        let sc = Supercharger::new(&synthetic_single_load());
        // bankConfiguration(0): slot 0 → RAM bank 2, slot 1 → ROM region.
        assert_eq!(sc.image_offset[0], 2 * BANK_SIZE);
        assert_eq!(sc.image_offset[1], RAM_SIZE, "upper slot maps ROM");
        assert!(sc.power, "ROM powered at reset");
        assert!(!sc.write_enabled, "writes disabled at reset");
    }

    #[test]
    fn bank_configuration_table_matches_stella() {
        let mut sc = Supercharger::new(&synthetic_single_load());
        // 011: slot0 → bank 0, slot1 → bank 2 (Suicide Mission layout).
        sc.bank_configuration(0b011 << 2);
        assert_eq!(sc.image_offset[0], 0);
        assert_eq!(sc.image_offset[1], 2 * BANK_SIZE);
        // D1 set → write enabled; D0 set → ROM power off.
        sc.bank_configuration(0b011);
        assert!(sc.write_enabled);
        assert!(!sc.power);
    }

    #[test]
    fn load_into_ram_copies_pages_and_returns_pokes() {
        let mut sc = Supercharger::new(&synthetic_single_load());
        let effect = sc.load_into_ram(0);
        assert_eq!(
            effect,
            ArEffect::RamPokes([(0xfe, 0xAB), (0xff, 0xCD), (0x80, 0x01)]),
            "BIOS pokes from header[0..3]"
        );
        assert!(sc.last_load_ok, "synthetic header + pages checksum clean");
        // Page j (value j) landed at bank j/8, page j%8.
        for j in 0..24usize {
            let bank = j / 8;
            let page = j % 8;
            let cell = bank * BANK_SIZE + page * 256;
            assert_eq!(sc.image[cell], j as u8, "page {j} mapped to bank/page");
            assert_eq!(sc.image[cell + 255], j as u8);
        }
    }

    #[test]
    fn load_into_ram_missing_load_is_none() {
        let mut sc = Supercharger::new(&synthetic_single_load());
        assert_eq!(sc.load_into_ram(0x42), ArEffect::None);
    }

    #[test]
    fn fast_load_hotspot_triggers_on_1850() {
        let mut sc = Supercharger::new(&synthetic_single_load());
        // RIOT $80 names load 0; reading $1850 with ROM mapped high fires the load.
        let (_byte, effect) = sc.read(0x1850, 0, 0);
        assert_eq!(
            effect,
            ArEffect::RamPokes([(0xfe, 0xAB), (0xff, 0xCD), (0x80, 0x01)])
        );
        assert_eq!(sc.image[0], 0, "bank 0 page 0 = page index 0");
        assert_eq!(sc.image[BANK_SIZE], 8, "bank 1 page 0 = page index 8");
    }

    #[test]
    fn data_hold_then_1ff8_commits_bank_config() {
        let mut sc = Supercharger::new(&synthetic_single_load());
        // Access $10nn loads the data-hold register with nn (here 0b01111 = 0x0F).
        sc.read(0x100F, 1, 0);
        // $1FF8 commits it as the bank configuration.
        sc.read(0x1FF8, 2, 0);
        assert_eq!(sc.current_bank, 0x0F);
        // 0b01111: bank_cfg = 011 → slot0 bank 0, slot1 bank 2; write+no-power.
        assert_eq!(sc.image_offset[0], 0);
        assert_eq!(sc.image_offset[1], 2 * BANK_SIZE);
        assert!(sc.write_enabled);
    }

    #[test]
    fn ram_write_commits_after_five_distinct_accesses() {
        let mut sc = Supercharger::new(&synthetic_single_load());
        // Configure: write-enabled, both slots RAM. 0b01111 → slot0 bank0, slot1 bank2.
        sc.read(0x100F, 1, 0);
        sc.read(0x1FF8, 2, 0);
        assert!(sc.write_enabled);
        // Load the hold register with value 0x42 via $1042 at distinct=10.
        sc.write(0x1042, 10);
        // A commit lands at exactly hold+5 distinct accesses, to a RAM slot.
        // $1100 → slot 0, offset 0x100 + image_offset[0] (bank 0).
        sc.write(0x1100, 15);
        assert_eq!(sc.image[0x100], 0x42, "RAM write committed at +5");
    }

    /// Gated test against the real Phaser Patrol proto, if staged locally.
    fn phaser_patrol() -> Option<Vec<u8>> {
        let home = std::env::var("HOME").ok()?;
        let path =
            std::path::PathBuf::from(home).join(".emu198x/media/atari-2600/Phaser Patrol.a26");
        std::fs::read(path).ok()
    }

    #[test]
    fn real_phaser_patrol_loads_clean() {
        let Some(data) = phaser_patrol() else {
            eprintln!("skipping: Phaser Patrol.a26 not staged");
            return;
        };
        assert_eq!(data.len(), LOAD_SIZE, "single-load proto");
        let mut sc = Supercharger::new(&data);
        let (_byte, effect) = sc.read(0x1850, 0, 0);
        // Header[0..3] = ctrl 0x00, start 0xF6, next 0x0B.
        assert_eq!(
            effect,
            ArEffect::RamPokes([(0xfe, 0x00), (0xff, 0xF6), (0x80, 0x0B)])
        );
        assert!(sc.last_load_ok, "real proto header + pages checksum clean");
    }
}
