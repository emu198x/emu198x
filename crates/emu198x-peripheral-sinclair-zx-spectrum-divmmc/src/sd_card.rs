//! An SDHC card on the DivMMC's SPI bus.
//!
//! The card answers the SPI-mode commands esxDOS uses to bring a card up
//! and move single 512-byte blocks: `CMD0`, `CMD8`, `CMD9`, `CMD10`,
//! `CMD17`, `CMD24`, `CMD55`, `ACMD41` and `CMD58`. Anything else is an
//! illegal command. Command arguments are block numbers, as on any SDHC
//! card, and CRCs are ignored.
//!
//! Each byte written to the SPI port shifts one byte of a command in;
//! each byte read shifts one byte of the response out, or `$FF` when the
//! card has nothing to say. Responses are ready at once, with none of a
//! real card's wait bytes, as in libspectrum's `mmc.c` (GPL-2, used for
//! facts only), whose state machine and register contents this follows.
//!
//! The card's contents are host media, not machine state: they are kept
//! out of snapshots, so loading a state leaves the inserted card as it
//! is, and every byte written stays in memory until the host exports it.

/// Bytes in a block.
pub const SD_BLOCK_BYTES: usize = 512;

/// Images are whole multiples of this: libspectrum's smallest card, and
/// the unit of the CSD's `C_SIZE`.
pub const SD_IMAGE_UNIT: usize = 512 * 1024;

/// One more `C_SIZE` than an SDHC card allows: 32 GB less 80 MB.
const C_SIZE_LIMIT: usize = 65_375;

const IN_IDLE_STATE: u8 = 0x01;
const ILLEGAL_COMMAND: u8 = 0x04;
const PARAMETER_ERROR: u8 = 0x40;

/// The data token before a block, in either direction.
const DATA_TOKEN: u8 = 0xFE;

/// Why bytes cannot be a card image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdImageError {
    /// Empty, or not a whole number of 512 KB units.
    BadSize {
        /// The length supplied.
        len: usize,
    },
    /// Larger than an SDHC card can be.
    TooLarge {
        /// The length supplied.
        len: usize,
    },
}

impl std::fmt::Display for SdImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadSize { len } => write!(
                f,
                "a {len}-byte SD card image is not a whole number of 512 KB"
            ),
            Self::TooLarge { len } => {
                write!(f, "a {len}-byte SD card image is larger than SDHC allows")
            }
        }
    }
}

impl std::error::Error for SdImageError {}

/// Where the card is in receiving a command or a block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum CommandState {
    #[default]
    Command,
    Argument(u8),
    Crc,
    DataToken,
    Data,
    DataCrc(u8),
}

/// The card in the slot, if any, and its SPI state.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SdCard {
    #[serde(skip)]
    image: Vec<u8>,
    #[serde(skip)]
    modified: bool,
    r1: u8,
    state: CommandState,
    command: u8,
    argument: [u8; 4],
    block: Vec<u8>,
    response: Vec<u8>,
    response_pos: usize,
    cmd8_issued: bool,
    cmd55_issued: bool,
}

impl SdCard {
    /// An empty slot.
    #[must_use]
    pub fn new() -> Self {
        let mut card = Self::default();
        card.reset();
        card
    }

    /// Puts a card in the slot with these contents, replacing any there.
    ///
    /// # Errors
    ///
    /// Returns an error, and leaves the slot as it was, if the image is
    /// empty, not a whole number of 512 KB, or larger than SDHC allows.
    pub fn insert(&mut self, image: Vec<u8>) -> Result<(), SdImageError> {
        let len = image.len();
        if len == 0 || !len.is_multiple_of(SD_IMAGE_UNIT) {
            return Err(SdImageError::BadSize { len });
        }
        if len / SD_IMAGE_UNIT > C_SIZE_LIMIT {
            return Err(SdImageError::TooLarge { len });
        }
        self.image = image;
        self.modified = false;
        self.reset();
        Ok(())
    }

    /// Puts contents back into the slot after a state load, keeping the
    /// SPI state the snapshot restored. Unlike [`Self::insert`] it does
    /// not check the size: the contents came out of a card.
    pub fn restore_contents(&mut self, image: Vec<u8>) {
        self.image = image;
    }

    /// Takes the card out, returning its contents.
    pub fn eject(&mut self) -> Option<Vec<u8>> {
        self.reset();
        self.modified = false;
        if self.image.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.image))
        }
    }

    /// The card's contents, as they are now, or `None` with no card.
    #[must_use]
    pub fn image(&self) -> Option<&[u8]> {
        (!self.image.is_empty()).then_some(self.image.as_slice())
    }

    /// Whether a block has been written since the card went in.
    #[must_use]
    pub const fn is_modified(&self) -> bool {
        self.modified
    }

    /// Puts the card back to the state it powers up in: idle, waiting for
    /// a command, with nothing to say.
    pub fn reset(&mut self) {
        self.r1 = IN_IDLE_STATE;
        self.state = CommandState::Command;
        self.response.clear();
        self.response_pos = 0;
        self.cmd8_issued = false;
        self.cmd55_issued = false;
    }

    /// Shifts the next response byte out, `$FF` when there is none.
    pub fn read(&mut self) -> u8 {
        match self.response.get(self.response_pos) {
            Some(&byte) => {
                self.response_pos += 1;
                byte
            }
            None => 0xFF,
        }
    }

    /// Shifts one byte of a command or block in. With no card in the
    /// slot nothing changes.
    pub fn write(&mut self, byte: u8) {
        if self.image.is_empty() {
            return;
        }
        self.state = match self.state {
            // A command starts with bits 7 and 6 at 0 and 1.
            CommandState::Command if byte & 0xC0 == 0x40 => {
                self.command = byte & 0x3F;
                CommandState::Argument(0)
            }
            CommandState::Command => CommandState::Command,
            CommandState::Argument(n) => {
                self.argument[usize::from(n)] = byte;
                if n == 3 {
                    CommandState::Crc
                } else {
                    CommandState::Argument(n + 1)
                }
            }
            CommandState::Crc => {
                self.execute();
                // Error bits last for one response; idle stays.
                self.r1 &= IN_IDLE_STATE;
                if self.command == 24 {
                    CommandState::DataToken
                } else {
                    CommandState::Command
                }
            }
            CommandState::DataToken if byte == DATA_TOKEN => {
                self.block.clear();
                CommandState::Data
            }
            CommandState::DataToken => CommandState::DataToken,
            CommandState::Data => {
                self.block.push(byte);
                if self.block.len() == SD_BLOCK_BYTES {
                    CommandState::DataCrc(0)
                } else {
                    CommandState::Data
                }
            }
            CommandState::DataCrc(0) => CommandState::DataCrc(1),
            CommandState::DataCrc(_) => {
                self.write_block();
                CommandState::Command
            }
        };
    }

    fn respond(&mut self, bytes: &[u8]) {
        self.response.clear();
        self.response.extend_from_slice(bytes);
        self.response_pos = 0;
    }

    fn respond_r1(&mut self) {
        self.respond(&[self.r1]);
    }

    fn respond_r7(&mut self, value: u32) {
        let mut bytes = [self.r1, 0, 0, 0, 0];
        bytes[1..].copy_from_slice(&value.to_be_bytes());
        self.respond(&bytes);
    }

    fn block_number(&self) -> usize {
        u32::from_be_bytes(self.argument) as usize
    }

    fn block_count(&self) -> usize {
        self.image.len() / SD_BLOCK_BYTES
    }

    /// Whether the card has left the idle state; if not, the command is
    /// refused as illegal.
    fn ready(&mut self) -> bool {
        if self.r1 & IN_IDLE_STATE != 0 {
            self.r1 |= ILLEGAL_COMMAND;
            self.respond_r1();
            false
        } else {
            true
        }
    }

    fn execute(&mut self) {
        if self.cmd55_issued {
            self.cmd55_issued = false;
            if self.execute_application() {
                return;
            }
        }
        match self.command {
            0 => {
                self.r1 |= IN_IDLE_STATE;
                self.cmd8_issued = false;
                self.respond_r1();
            }
            8 => {
                self.cmd8_issued = true;
                // Voltage accepted, check pattern echoed.
                self.respond_r7(0x0000_0100 | u32::from(self.argument[3]));
            }
            9 => self.send_register(&self.csd()),
            10 => self.send_register(&Self::cid()),
            17 => self.read_block(),
            24 => self.respond_r1(),
            55 => {
                self.cmd55_issued = true;
                self.respond_r1();
            }
            // Card power up done and card capacity status: an SDHC card.
            58 => self.respond_r7(0xC000_0000),
            _ => {
                self.r1 |= ILLEGAL_COMMAND;
                self.respond_r1();
            }
        }
    }

    /// Runs an application command after `CMD55`. Returns `false` for a
    /// command that is a standard one after all.
    fn execute_application(&mut self) -> bool {
        match self.command {
            41 => {
                // ACMD41 with HCS set, after CMD8, finishes initialising.
                if self.cmd8_issued && self.argument[0] & 0x40 != 0 {
                    self.r1 &= !IN_IDLE_STATE;
                }
                self.respond_r1();
                true
            }
            13 | 22 | 23 | 42 | 51 => {
                self.r1 |= ILLEGAL_COMMAND;
                self.respond_r1();
                true
            }
            _ => false,
        }
    }

    fn send_register(&mut self, register: &[u8; 16]) {
        if !self.ready() {
            return;
        }
        let mut bytes = vec![self.r1, DATA_TOKEN];
        bytes.extend_from_slice(register);
        bytes.extend_from_slice(&[0, 0]);
        self.respond(&bytes);
    }

    /// A version 2.0 CSD: 512-byte blocks and the card's `C_SIZE`.
    fn csd(&self) -> [u8; 16] {
        let c_size = self.image.len() / SD_IMAGE_UNIT - 1;
        let mut csd = [0; 16];
        csd[0] = 0x40;
        csd[4] = 0x5B;
        csd[5] = 0x05 | 0x09;
        csd[8] = (c_size >> 8) as u8;
        csd[9] = c_size as u8;
        csd[10] = 0x40 | 0x3F;
        csd[11] = 0x80;
        csd[12] = 0x10;
        csd[13] = 0x01;
        csd[15] = 0x01;
        csd
    }

    /// A nearly empty CID, naming the product.
    fn cid() -> [u8; 16] {
        let mut cid = [0; 16];
        cid[1..3].copy_from_slice(b"  ");
        cid[3..8].copy_from_slice(b"EMU98");
        cid[15] = 0x01;
        cid
    }

    fn read_block(&mut self) {
        if !self.ready() {
            return;
        }
        let block = self.block_number();
        if block >= self.block_count() {
            self.r1 |= PARAMETER_ERROR;
            self.respond_r1();
            return;
        }
        let start = block * SD_BLOCK_BYTES;
        let mut bytes = Vec::with_capacity(SD_BLOCK_BYTES + 4);
        bytes.extend_from_slice(&[self.r1, DATA_TOKEN]);
        bytes.extend_from_slice(&self.image[start..start + SD_BLOCK_BYTES]);
        bytes.extend_from_slice(&[0, 0]);
        self.respond(&bytes);
    }

    fn write_block(&mut self) {
        if !self.ready() {
            return;
        }
        let block = self.block_number();
        if block >= self.block_count() {
            self.r1 |= PARAMETER_ERROR;
            self.respond_r1();
            return;
        }
        let start = block * SD_BLOCK_BYTES;
        self.image[start..start + SD_BLOCK_BYTES].copy_from_slice(&self.block);
        self.modified = true;
        // Data accepted, then busy until programmed.
        self.respond(&[0x05, 0x01]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> SdCard {
        let mut image = vec![0; SD_IMAGE_UNIT];
        for (i, byte) in image.iter_mut().enumerate() {
            *byte = (i / SD_BLOCK_BYTES) as u8 ^ i as u8;
        }
        let mut card = SdCard::new();
        card.insert(image).expect("512 KB");
        card
    }

    fn command(card: &mut SdCard, index: u8, argument: u32) {
        card.write(0x40 | index);
        for byte in argument.to_be_bytes() {
            card.write(byte);
        }
        card.write(0x01);
    }

    fn read(card: &mut SdCard, n: usize) -> Vec<u8> {
        (0..n).map(|_| card.read()).collect()
    }

    /// CMD0, CMD8, then CMD55 and ACMD41 until the card leaves idle.
    fn initialise(card: &mut SdCard) {
        command(card, 0, 0);
        assert_eq!(card.read(), IN_IDLE_STATE);
        command(card, 8, 0x1AA);
        assert_eq!(read(card, 5), [IN_IDLE_STATE, 0, 0, 0x01, 0xAA]);
        command(card, 55, 0);
        assert_eq!(card.read(), IN_IDLE_STATE);
        command(card, 41, 0x4000_0000);
        assert_eq!(card.read(), 0x00, "ready");
    }

    #[test]
    fn images_must_be_whole_512k_units() {
        let mut card = SdCard::new();
        assert_eq!(
            card.insert(vec![0; 1000]),
            Err(SdImageError::BadSize { len: 1000 })
        );
        assert_eq!(
            card.insert(Vec::new()),
            Err(SdImageError::BadSize { len: 0 })
        );
        assert!(card.image().is_none());
        assert!(
            SdImageError::TooLarge { len: 1 }
                .to_string()
                .contains("SDHC")
        );
    }

    #[test]
    fn an_empty_slot_ignores_commands_and_reads_high() {
        let mut card = SdCard::new();
        command(&mut card, 0, 0);
        assert_eq!(card.read(), 0xFF);
        assert!(card.eject().is_none());
    }

    #[test]
    fn reads_before_initialising_are_illegal() {
        let mut card = card();
        command(&mut card, 17, 0);
        assert_eq!(card.read(), IN_IDLE_STATE | ILLEGAL_COMMAND);
        command(&mut card, 0, 0);
        assert_eq!(card.read(), IN_IDLE_STATE, "error bits last one response");
    }

    #[test]
    fn acmd41_without_cmd8_leaves_the_card_idle() {
        let mut card = card();
        command(&mut card, 55, 0);
        card.read();
        command(&mut card, 41, 0x4000_0000);
        assert_eq!(card.read(), IN_IDLE_STATE);
    }

    #[test]
    fn the_card_reports_itself_as_sdhc() {
        let mut card = card();
        initialise(&mut card);
        command(&mut card, 58, 0);
        assert_eq!(read(&mut card, 5), [0, 0xC0, 0, 0, 0]);
        command(&mut card, 9, 0);
        let csd = read(&mut card, 20);
        assert_eq!(csd[..2], [0, DATA_TOKEN]);
        assert_eq!(csd[2], 0x40, "CSD version 2.0");
        assert_eq!((csd[10], csd[11]), (0, 0), "C_SIZE 0: one 512 KB unit");
        command(&mut card, 10, 0);
        assert_eq!(read(&mut card, 20)[5..10], *b"EMU98");
    }

    #[test]
    fn a_block_reads_with_its_token() {
        let mut card = card();
        initialise(&mut card);
        command(&mut card, 17, 3);
        let response = read(&mut card, SD_BLOCK_BYTES + 4);
        assert_eq!(response[..2], [0, DATA_TOKEN]);
        let expected: Vec<u8> = (0..SD_BLOCK_BYTES)
            .map(|i| 3 ^ (3 * SD_BLOCK_BYTES + i) as u8)
            .collect();
        assert_eq!(response[2..2 + SD_BLOCK_BYTES], expected);
        assert_eq!(card.read(), 0xFF, "nothing more to say");
    }

    #[test]
    fn a_block_beyond_the_card_is_a_parameter_error() {
        let mut card = card();
        initialise(&mut card);
        command(&mut card, 17, 1024);
        assert_eq!(card.read(), PARAMETER_ERROR);
    }

    #[test]
    fn a_block_writes_after_its_token() {
        let mut card = card();
        initialise(&mut card);
        command(&mut card, 24, 2);
        assert_eq!(card.read(), 0);
        card.write(0xFF);
        card.write(DATA_TOKEN);
        for i in 0..SD_BLOCK_BYTES {
            card.write(i as u8);
        }
        card.write(0);
        card.write(0);
        assert_eq!(read(&mut card, 2), [0x05, 0x01]);
        assert!(card.is_modified());
        let image = card.image().expect("a card");
        let block = &image[2 * SD_BLOCK_BYTES..3 * SD_BLOCK_BYTES];
        assert!(block.iter().enumerate().all(|(i, &b)| b == i as u8));
    }

    #[test]
    fn unknown_commands_are_illegal() {
        let mut card = card();
        initialise(&mut card);
        command(&mut card, 38, 0);
        assert_eq!(card.read(), ILLEGAL_COMMAND);
        command(&mut card, 55, 0);
        card.read();
        command(&mut card, 51, 0);
        assert_eq!(card.read(), ILLEGAL_COMMAND);
    }

    #[test]
    fn eject_returns_the_contents() {
        let mut card = card();
        let image = card.eject().expect("a card");
        assert_eq!(image.len(), SD_IMAGE_UNIT);
        assert!(card.image().is_none());
    }
}
