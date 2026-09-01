//! Jupiter Ace `.ace` snapshot decoder.
//!
//! `.ace` is the snapshot container written by ACE32 and read by Blaze and the
//! multi-system emulators. It has no header and no magic number: byte 0 is the
//! first byte of a run-length-encoded stream that decodes to address `$2000`
//! upward.
//!
//! Format and provenance:
//! `reference/by-system/jupiter-ace/jupiter-ace-ace-snapshot-format.md`.
//!
//! | Sequence | Meaning |
//! |---|---|
//! | `ED 00` | End of stream |
//! | `ED xx yy` | Byte `yy` repeated `xx` times |
//! | any other byte | Itself, unchanged |
//!
//! A literal `$ED` is always written as `ED 01 ED`, so an `$ED` in the stream is
//! always an escape. This is **not** the `.z80` scheme (`ED ED count byte`);
//! decoding it that way finds no runs and returns the stream as literals, at a
//! length that looks plausible until it is checked.

/// Address the decoded image starts at.
pub const LOAD_ADDRESS: u16 = 0x2000;
/// The escape byte.
const ESCAPE: u8 = 0xED;
/// Offset of the Z80 register block within the decoded image (`$2100`).
const REGISTER_BLOCK: usize = 0x0100;
/// Decoded lengths the format allows, one per RAM configuration.
const LEGAL_LENGTHS: [usize; 3] = [0x2000, 0x6000, 0xA000];

/// Z80 state recovered from a snapshot's register block.
///
/// ACE32 stores each register in a 32-bit little-endian slot. Only the low 16
/// bits are the register: the published notes describe the upper half as `00,
/// 00` padding, but real files carry unrelated bytes there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Registers {
    pub af: u16,
    pub bc: u16,
    pub de: u16,
    pub hl: u16,
    pub ix: u16,
    pub iy: u16,
    pub sp: u16,
    pub pc: u16,
    pub af_alt: u16,
    pub bc_alt: u16,
    pub de_alt: u16,
    pub hl_alt: u16,
}

/// A decoded `.ace` snapshot.
#[derive(Debug)]
pub struct Snapshot {
    /// RAM image starting at [`LOAD_ADDRESS`].
    pub memory: Vec<u8>,
    /// Z80 state from the register block.
    pub registers: Registers,
}

impl Snapshot {
    /// Decode one `.ace` file.
    ///
    /// # Errors
    ///
    /// Returns an error for a truncated escape sequence, a stream with no
    /// terminator, or a decoded length that is not one of the three the format
    /// allows (`$2000`, `$6000`, `$A000`).
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let memory = decode(bytes)?;
        if !LEGAL_LENGTHS.contains(&memory.len()) {
            return Err(format!(
                "decoded to {} bytes; a .ace image covers $2000-$3FFF, $2000-$7FFF or \
                 $2000-$BFFF ({:?})",
                memory.len(),
                LEGAL_LENGTHS
            ));
        }
        let registers = read_registers(&memory);
        Ok(Self { memory, registers })
    }

    /// Address the image's last byte occupies.
    #[must_use]
    pub fn top_address(&self) -> u16 {
        LOAD_ADDRESS + (self.memory.len() as u16) - 1
    }
}

/// Expand the RLE stream.
///
/// # Errors
///
/// Returns an error for a truncated escape sequence or a stream that ends
/// without the `ED 00` terminator.
pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != ESCAPE {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        match bytes.get(i + 1) {
            None => return Err(format!("truncated escape at offset {i}")),
            Some(0) => return Ok(out),
            Some(&count) => {
                let value = *bytes
                    .get(i + 2)
                    .ok_or_else(|| format!("truncated run at offset {i}"))?;
                out.extend(std::iter::repeat_n(value, count as usize));
                i += 3;
            }
        }
    }
    Err("stream ended without an ED 00 terminator".to_owned())
}

/// Read the register block at `$2100`, taking the low half of each 32-bit slot.
fn read_registers(memory: &[u8]) -> Registers {
    let slot = |index: usize| -> u16 {
        let at = REGISTER_BLOCK + index * 4;
        memory
            .get(at..at + 2)
            .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
    };
    Registers {
        af: slot(0),
        bc: slot(1),
        de: slot(2),
        hl: slot(3),
        ix: slot(4),
        iy: slot(5),
        sp: slot(6),
        pc: slot(7),
        af_alt: slot(8),
        bc_alt: slot(9),
        de_alt: slot(10),
        hl_alt: slot(11),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a stream that decodes to `len` bytes, then terminate it.
    fn padded(prefix: &[u8], len: usize) -> Vec<u8> {
        let mut s = prefix.to_vec();
        let mut produced = decode_len(prefix);
        while produced + 240 <= len {
            s.extend_from_slice(&[ESCAPE, 240, 0x00]);
            produced += 240;
        }
        while produced < len {
            s.push(0x00);
            produced += 1;
        }
        s.extend_from_slice(&[ESCAPE, 0x00]);
        s
    }

    fn decode_len(prefix: &[u8]) -> usize {
        let mut n = 0;
        let mut i = 0;
        while i < prefix.len() {
            if prefix[i] == ESCAPE {
                n += prefix[i + 1] as usize;
                i += 3;
            } else {
                n += 1;
                i += 1;
            }
        }
        n
    }

    #[test]
    fn a_run_expands_and_a_literal_passes_through() {
        let stream = padded(&[0x01, 0x80, ESCAPE, 0x05, 0xAA, 0x42], 0x2000);
        let out = decode(&stream).expect("valid stream");
        assert_eq!(&out[..2], &[0x01, 0x80]);
        assert_eq!(&out[2..7], &[0xAA; 5], "ED 05 AA is five AA bytes");
        assert_eq!(out[7], 0x42);
        assert_eq!(out.len(), 0x2000);
    }

    #[test]
    fn a_literal_escape_byte_arrives_as_a_run_of_one() {
        // The encoder never emits a bare ED, so ED 01 ED is how one is carried.
        let stream = padded(&[ESCAPE, 0x01, ESCAPE], 0x2000);
        let out = decode(&stream).expect("valid stream");
        assert_eq!(out[0], ESCAPE);
    }

    #[test]
    fn the_z80_scheme_is_not_this_scheme() {
        // ED ED 05 AA is the .z80 encoding of five AA bytes. Read as .ace it is
        // "ED ED 05" — repeat $05 exactly $ED (237) times — then a literal AA.
        let stream = padded(&[ESCAPE, ESCAPE, 0x05, 0xAA], 0x2000);
        let out = decode(&stream).expect("decodes, wrongly, without complaint");
        assert_eq!(out[0], 0x05);
        assert_eq!(out[236], 0x05);
        assert_eq!(out[237], 0xAA);
    }

    #[test]
    fn every_legal_length_is_accepted_and_others_are_not() {
        for len in LEGAL_LENGTHS {
            let snap = Snapshot::parse(&padded(&[], len)).expect("legal length");
            assert_eq!(snap.memory.len(), len);
        }
        let error = Snapshot::parse(&padded(&[], 0x1234)).expect_err("illegal length");
        assert!(error.contains("decoded to 4660 bytes"), "{error}");
    }

    #[test]
    fn top_address_covers_the_configured_ram() {
        for (len, top) in [(0x2000usize, 0x3FFFu16), (0x6000, 0x7FFF), (0xA000, 0xBFFF)] {
            let snap = Snapshot::parse(&padded(&[], len)).expect("legal length");
            assert_eq!(snap.top_address(), top);
        }
    }

    #[test]
    fn registers_take_the_low_half_of_each_slot() {
        let mut image = vec![0u8; 0x6000];
        // IY at slot 5, PC at slot 7. High halves deliberately non-zero, as
        // real files have them.
        image[REGISTER_BLOCK + 5 * 4..REGISTER_BLOCK + 5 * 4 + 4]
            .copy_from_slice(&[0xC8, 0x04, 0x07, 0x07]);
        image[REGISTER_BLOCK + 7 * 4..REGISTER_BLOCK + 7 * 4 + 4]
            .copy_from_slice(&[0xBD, 0x04, 0x07, 0x07]);
        let regs = read_registers(&image);
        assert_eq!(regs.iy, 0x04C8, "the FORTH inner interpreter");
        assert_eq!(regs.pc, 0x04BD);
    }

    #[test]
    fn a_stream_without_a_terminator_is_rejected() {
        let error = decode(&[0x01, 0x80, 0x00]).expect_err("no ED 00");
        assert!(error.contains("without an ED 00 terminator"), "{error}");
    }

    #[test]
    fn a_truncated_run_is_rejected() {
        assert!(decode(&[0x01, ESCAPE, 0x05]).is_err());
        assert!(decode(&[0x01, ESCAPE]).is_err());
    }
}
