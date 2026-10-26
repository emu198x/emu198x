//! Romantic Robot's Multiface 128 and Multiface 3.
//!
//! A Multiface holds 8 KB of ROM and 8 KB of RAM. When its red button is
//! pressed it pulls the Z80's `/NMI` line, and when the CPU then fetches
//! the NMI vector at `$0066` it pages both in over the bottom 16 KB: the
//! ROM at `$0000-$1FFF` and the RAM at `$2000-$3FFF`. The fetch at `$0066`
//! already comes from the Multiface ROM. The software inside saves,
//! inspects or pokes the frozen program, then pages itself out and
//! returns.
//!
//! The ROM is Romantic Robot's firmware. It is never part of this
//! repository: [`Multiface::attach`] takes the image from the user.
//!
//! # Ports
//!
//! Both models answer on a partially decoded port, mask `$0072`, value
//! `$0032`: A6 low, A5, A4 and A1 high. A7 chooses the action.
//!
//! | Model | `IN` with A7 high (`$BF`) | `IN` with A7 low (`$3F`) |
//! |---|---|---|
//! | 128 | page in, when enabled | page out |
//! | 3 | page out | page in, when enabled |
//!
//! The Multiface 128 answers its page-in read with `$FF` or `$7F`: bit 7
//! is bit 3 of the last `$7FFD` write, so its software can tell which
//! screen the program was showing. The Multiface 3 answers every read with
//! the low nibble of the last write to `$1FFD`, `$3FFD`, `$5FFD` or
//! `$7FFD`, chosen by A13 and A14, over `$F0`. Those registers are
//! write-only on the +3, and the Multiface 3 keeps its own copy so its
//! software can restore the paging.
//!
//! Any `OUT` to the port re-arms the red button, which a press disables
//! until then.
//!
//! # Stealth
//!
//! Each model hides itself from software that probes for it. An `OUT`
//! while paged in sets whether a later `IN` may page it in again: A7 high
//! enables it, A7 low hides it. A hidden Multiface still answers its
//! button, and paging in through the button enables it again. After a
//! reset it starts hidden, so a program cannot find it until the button
//! has been pressed once.
//!
//! # Sources
//!
//! The port decoding, the paging and the stealth rules follow Fuse's
//! `peripherals/multiface.c` (GPL-2, used for facts only), which cites
//! the Multiface 128 user guide for `IN A,($BF)` and `IN A,($3F)` and
//! x128.speccy.cz for the Multiface 3's reversed sense. Fuse's paging
//! trap at `$0066` sits before the opcode fetch in `z80/z80_ops.c`.

use emu198x_bus_sinclair_zx_spectrum::{MemoryIntercept, Peripheral};

/// The size of the ROM and of the RAM: 8 KB each.
pub const MULTIFACE_MEMORY_BYTES: usize = 0x2000;

/// The NMI vector, whose fetch pages a pressed Multiface in.
const NMI_VECTOR: u16 = 0x0066;

/// Which Multiface is plugged in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MultifaceModel {
    /// The Multiface 128, for the 48K, 128K and grey +2.
    #[default]
    M128,
    /// The Multiface 3, for the +2A, +2B and +3.
    Three,
}

impl MultifaceModel {
    /// The model's name as Romantic Robot sold it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::M128 => "Multiface 128",
            Self::Three => "Multiface 3",
        }
    }
}

/// A ROM image that is not 8 KB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MultifaceRomError {
    /// The length that was supplied.
    pub actual: usize,
}

impl std::fmt::Display for MultifaceRomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "expected an {MULTIFACE_MEMORY_BYTES}-byte Multiface ROM image, got {} bytes",
            self.actual
        )
    }
}

impl std::error::Error for MultifaceRomError {}

/// The interface's state.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Multiface {
    model: MultifaceModel,
    attached: bool,
    rom: Vec<u8>,
    ram: Vec<u8>,
    paged: bool,
    /// Whether an `IN` may page the interface in: Fuse's `J2`.
    enabled: bool,
    /// Whether the red button works: the `/Q` output of Fuse's `IC8b`.
    button_armed: bool,
    /// The button was pressed and the next fetch from `$0066` pages in.
    nmi_pending: bool,
    /// The last `$7FFD` write, for the Multiface 128's page-in read.
    last_7ffd: u8,
    /// The Multiface 3's copies of `$1FFD`, `$3FFD`, `$5FFD` and `$7FFD`.
    paging: [u8; 4],
}

impl Default for Multiface {
    fn default() -> Self {
        Self::new(MultifaceModel::default())
    }
}

impl Multiface {
    /// An unplugged interface of the given model.
    #[must_use]
    pub fn new(model: MultifaceModel) -> Self {
        Self {
            model,
            attached: false,
            rom: vec![0; MULTIFACE_MEMORY_BYTES],
            ram: vec![0; MULTIFACE_MEMORY_BYTES],
            paged: false,
            enabled: false,
            button_armed: true,
            nmi_pending: false,
            last_7ffd: 0,
            paging: [0; 4],
        }
    }

    /// Plugs the interface in with its ROM image, clearing its RAM.
    ///
    /// # Errors
    ///
    /// Returns an error, and leaves the interface as it was, if the image
    /// is not 8 KB.
    pub fn attach(&mut self, model: MultifaceModel, rom: &[u8]) -> Result<(), MultifaceRomError> {
        if rom.len() != MULTIFACE_MEMORY_BYTES {
            return Err(MultifaceRomError { actual: rom.len() });
        }
        self.model = model;
        self.rom.copy_from_slice(rom);
        self.ram.fill(0);
        self.attached = true;
        self.reset();
        Ok(())
    }

    /// Unplugs the interface.
    pub fn detach(&mut self) {
        self.attached = false;
        self.reset();
    }

    /// Returns the model.
    #[must_use]
    pub const fn model(&self) -> MultifaceModel {
        self.model
    }

    /// Whether the interface is plugged in.
    #[must_use]
    pub const fn is_attached(&self) -> bool {
        self.attached
    }

    /// Whether the ROM and RAM are paged in over the bottom 16 KB.
    #[must_use]
    pub const fn is_paged(&self) -> bool {
        self.paged
    }

    /// Whether an `IN` may page the interface in, or it is hidden.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The 8 KB of RAM.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Returns the interface to its power-on state, keeping its RAM: paged
    /// out, hidden, and with the button armed.
    pub fn reset(&mut self) {
        self.paged = false;
        self.enabled = false;
        self.button_armed = true;
        self.nmi_pending = false;
        self.last_7ffd = 0;
        self.paging = [0; 4];
    }

    /// Presses the red button. Returns `true` when the interface pulls
    /// `/NMI`, which the machine must then pulse on the CPU.
    ///
    /// A press does nothing while the button is disarmed: from one press
    /// until the Multiface software writes to the port.
    pub fn press_button(&mut self) -> bool {
        if !self.attached || !self.button_armed {
            return false;
        }
        self.button_armed = false;
        self.nmi_pending = true;
        true
    }

    /// Whether the interface is holding `/NMI` low: from a press until the
    /// fetch from `$0066` pages it in. The Z80 latches the falling edge,
    /// so the machine can feed this level straight to its NMI pin.
    #[must_use]
    pub const fn nmi_asserted(&self) -> bool {
        self.nmi_pending
    }

    /// Watches an `OUT` the machine performed, whether or not the port is
    /// the Multiface's, for the paging registers the interface copies.
    pub fn observe_io_write(&mut self, port: u16, value: u8) {
        if !self.attached {
            return;
        }
        match self.model {
            // The 128K's own partial decode of $7FFD: A15 and A1 low.
            MultifaceModel::M128 => {
                if port & 0x8002 == 0 {
                    self.last_7ffd = value;
                }
            }
            MultifaceModel::Three => {
                if port & 0x90FF == 0x10FD {
                    self.paging[Self::paging_index(port)] = value & 0x0F;
                }
            }
        }
    }

    fn paging_index(port: u16) -> usize {
        usize::from((port & 0x6000) >> 13)
    }

    fn page_in(&mut self) {
        self.paged = true;
        self.enabled = true;
    }

    const fn claims(port: u16) -> bool {
        port & 0x0072 == 0x0032
    }
}

impl Peripheral for Multiface {
    fn claims_port(&self, port: u16) -> bool {
        self.attached && Self::claims(port)
    }

    fn read(&mut self, port: u16) -> u8 {
        let a7 = port & 0x0080 != 0;
        match self.model {
            MultifaceModel::M128 => {
                if !a7 {
                    self.paged = false;
                    0xFF
                } else if self.enabled {
                    self.page_in();
                    if self.last_7ffd & 0x08 != 0 {
                        0xFF
                    } else {
                        0x7F
                    }
                } else {
                    0xFF
                }
            }
            MultifaceModel::Three => {
                if a7 {
                    self.paged = false;
                } else if self.enabled {
                    self.page_in();
                }
                if self.enabled {
                    self.paging[Self::paging_index(port)] | 0xF0
                } else {
                    0xFF
                }
            }
        }
    }

    fn write(&mut self, port: u16, _val: u8) {
        if self.paged {
            self.enabled = port & 0x0080 != 0;
        }
        self.button_armed = true;
    }

    fn on_m1(&mut self, addr: u16) {
        if self.nmi_pending && addr == NMI_VECTOR {
            self.nmi_pending = false;
            self.page_in();
        }
    }
}

impl MemoryIntercept for Multiface {
    fn read_memory(&self, addr: u16) -> Option<u8> {
        if !self.paged {
            return None;
        }
        let offset = usize::from(addr);
        if offset < MULTIFACE_MEMORY_BYTES {
            Some(self.rom[offset])
        } else if offset < 2 * MULTIFACE_MEMORY_BYTES {
            Some(self.ram[offset - MULTIFACE_MEMORY_BYTES])
        } else {
            None
        }
    }

    fn write_memory(&mut self, addr: u16, val: u8) -> bool {
        if !self.paged {
            return false;
        }
        let offset = usize::from(addr);
        if offset < MULTIFACE_MEMORY_BYTES {
            true
        } else if offset < 2 * MULTIFACE_MEMORY_BYTES {
            self.ram[offset - MULTIFACE_MEMORY_BYTES] = val;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the firmware: each byte is the low byte of its
    /// address, so a read shows where it came from.
    fn rom() -> Vec<u8> {
        (0..MULTIFACE_MEMORY_BYTES).map(|i| i as u8).collect()
    }

    fn attached(model: MultifaceModel) -> Multiface {
        let mut mf = Multiface::new(model);
        mf.attach(model, &rom()).expect("an 8 KB ROM attaches");
        mf
    }

    /// Presses the button and fetches the NMI vector.
    fn freeze(mf: &mut Multiface) {
        assert!(mf.press_button());
        mf.on_m1(NMI_VECTOR);
    }

    #[test]
    fn the_rom_must_be_8k() {
        let mut mf = Multiface::new(MultifaceModel::M128);
        assert_eq!(
            mf.attach(MultifaceModel::M128, &[0; 100]),
            Err(MultifaceRomError { actual: 100 })
        );
        assert!(!mf.is_attached());
    }

    #[test]
    fn an_unplugged_interface_claims_nothing_and_ignores_the_button() {
        let mut mf = Multiface::new(MultifaceModel::M128);
        assert!(!mf.claims_port(0x00BF));
        assert!(!mf.press_button());
    }

    #[test]
    fn the_port_decode_is_partial() {
        let mf = attached(MultifaceModel::M128);
        assert!(mf.claims_port(0x00BF));
        assert!(mf.claims_port(0x003F));
        assert!(mf.claims_port(0xFF33));
        // A6 high, A5 low or A1 low is someone else's.
        assert!(!mf.claims_port(0x00FF));
        assert!(!mf.claims_port(0x001F));
        assert!(!mf.claims_port(0x00BD));
    }

    #[test]
    fn the_button_pages_in_on_the_nmi_vector_fetch() {
        let mut mf = attached(MultifaceModel::M128);
        assert!(mf.press_button());
        assert!(mf.nmi_asserted());
        mf.on_m1(0x1234);
        assert!(!mf.is_paged(), "only the fetch from $0066 pages in");
        mf.on_m1(NMI_VECTOR);
        assert!(mf.is_paged());
        assert!(!mf.nmi_asserted(), "paging in releases /NMI");
        assert_eq!(mf.read_memory(0x0066), Some(0x66));
        assert_eq!(mf.read_memory(0x1FFF), Some(0xFF));
        assert_eq!(mf.read_memory(0x4000), None);
    }

    #[test]
    fn the_ram_takes_writes_and_the_rom_drops_them() {
        let mut mf = attached(MultifaceModel::M128);
        freeze(&mut mf);
        assert!(mf.write_memory(0x2000, 0xAA));
        assert!(mf.write_memory(0x0000, 0x55));
        assert!(!mf.write_memory(0x4000, 0x11));
        assert_eq!(mf.read_memory(0x2000), Some(0xAA));
        assert_eq!(mf.read_memory(0x0000), Some(0x00));
        assert_eq!(mf.ram()[0], 0xAA);
    }

    #[test]
    fn nothing_is_overlaid_while_paged_out() {
        let mut mf = attached(MultifaceModel::M128);
        assert_eq!(mf.read_memory(0x0000), None);
        assert!(!mf.write_memory(0x2000, 1));
    }

    #[test]
    fn the_button_stays_disarmed_until_an_out() {
        let mut mf = attached(MultifaceModel::M128);
        freeze(&mut mf);
        assert!(!mf.press_button(), "a second press does nothing");
        mf.write(0x00BF, 0);
        assert!(mf.press_button(), "an OUT to the port re-arms it");
    }

    #[test]
    fn a_multiface_128_pages_in_on_a7_high_and_out_on_a7_low() {
        let mut mf = attached(MultifaceModel::M128);
        freeze(&mut mf);
        mf.read(0x003F);
        assert!(!mf.is_paged());
        mf.read(0x00BF);
        assert!(mf.is_paged());
    }

    #[test]
    fn a_multiface_128_reports_the_shown_screen_on_page_in() {
        let mut mf = attached(MultifaceModel::M128);
        freeze(&mut mf);
        mf.observe_io_write(0x7FFD, 0x08);
        assert_eq!(mf.read(0x00BF), 0xFF);
        mf.observe_io_write(0x7FFD, 0x00);
        assert_eq!(mf.read(0x00BF), 0x7F);
    }

    #[test]
    fn a_multiface_3_pages_the_other_way_round() {
        let mut mf = attached(MultifaceModel::Three);
        freeze(&mut mf);
        mf.read(0x00BF);
        assert!(!mf.is_paged());
        mf.read(0x003F);
        assert!(mf.is_paged());
    }

    #[test]
    fn a_multiface_3_reads_back_the_paging_registers() {
        let mut mf = attached(MultifaceModel::Three);
        mf.observe_io_write(0x1FFD, 0x05);
        mf.observe_io_write(0x7FFD, 0x17);
        freeze(&mut mf);
        assert_eq!(mf.read(0x1F3F), 0xF5);
        assert_eq!(mf.read(0x7F3F), 0xF7);
        assert_eq!(mf.read(0x3F3F), 0xF0);
    }

    #[test]
    fn both_models_start_hidden_until_the_button() {
        for model in [MultifaceModel::M128, MultifaceModel::Three] {
            let mut mf = attached(model);
            let page_in = if model == MultifaceModel::M128 {
                0x00BF
            } else {
                0x003F
            };
            assert_eq!(mf.read(page_in), 0xFF, "{model:?}");
            assert!(!mf.is_paged(), "{model:?}: hidden after reset");
            freeze(&mut mf);
            assert!(mf.is_enabled(), "{model:?}: the button enables it");
        }
    }

    #[test]
    fn an_out_while_paged_hides_or_shows_it() {
        let mut mf = attached(MultifaceModel::M128);
        freeze(&mut mf);
        mf.write(0x003F, 0);
        assert!(!mf.is_enabled(), "A7 low hides it");
        mf.read(0x003F);
        mf.read(0x00BF);
        assert!(
            !mf.is_paged(),
            "a hidden Multiface ignores the page-in read"
        );

        // The button still works, and paging in shows it again.
        freeze(&mut mf);
        assert!(mf.is_enabled());
        mf.write(0x00BF, 0);
        assert!(mf.is_enabled(), "A7 high keeps it visible");
    }

    #[test]
    fn an_out_while_paged_out_leaves_stealth_alone() {
        let mut mf = attached(MultifaceModel::M128);
        freeze(&mut mf);
        mf.read(0x003F);
        mf.write(0x003F, 0);
        assert!(mf.is_enabled());
    }

    #[test]
    fn reset_pages_out_and_hides_but_keeps_ram() {
        let mut mf = attached(MultifaceModel::M128);
        freeze(&mut mf);
        mf.write_memory(0x2001, 0x42);
        mf.reset();
        assert!(!mf.is_paged());
        assert!(!mf.is_enabled());
        assert_eq!(mf.ram()[1], 0x42);
        assert!(mf.press_button());
    }

    #[test]
    fn model_names() {
        assert_eq!(MultifaceModel::M128.name(), "Multiface 128");
        assert_eq!(MultifaceModel::Three.name(), "Multiface 3");
        assert_eq!(
            MultifaceRomError { actual: 3 }.to_string(),
            "expected an 8192-byte Multiface ROM image, got 3 bytes"
        );
    }
}
