//! Memotech MTX `.mtx` tape images and `.RUN` programs.
//!
//! Layouts and provenance:
//! `reference/by-system/memotech-mtx/memotech-mtx-tape-and-run-formats.md`.
//!
//! A `.mtx` file is a tape image — the bytes an MTX cassette carries, in order:
//! an `$FF` byte, a 15-byte space-padded name, a little-endian
//! `system_variables_base`, then `$FB4B - base` bytes of system variables and
//! the BASIC program loading from `$4000`. A common variant drops the `$FF`
//! byte and the name, starting straight at the base word.
//!
//! The extension is not the format. A quarter of the files carrying `.mtx` in
//! the TOSEC set are something else, so these parsers validate rather than
//! trust — see the reference document §3.

/// Address the BASIC program loads at.
pub const BASIC_BASE: u16 = 0x4000;
/// The system-variable block ends here, inclusive, so its length falls out of
/// the base rather than being stored.
pub const SYSTEM_VARIABLES_END: u16 = 0xFB4A;
/// Leading byte of a full tape image.
const TAPE_MARKER: u8 = 0xFF;
/// Length of the space-padded tape name.
const NAME_LEN: usize = 15;
/// Longest system-variable block a well-formed image can name.
const MAX_SYSTEM_VARIABLES: usize = 0x1000;

/// A parsed `.mtx` tape image.
#[derive(Debug)]
pub struct TapeImage {
    /// Name a game's own loader matches against; `None` for the headerless
    /// variant, which does not carry one.
    pub name: Option<String>,
    /// Address the system-variable block restores to.
    pub system_variables_base: u16,
    /// The system-variable block.
    pub system_variables: Vec<u8>,
    /// The BASIC program, loading from [`BASIC_BASE`].
    pub basic: Vec<u8>,
}

impl TapeImage {
    /// Parse a `.mtx` image, accepting both the full and headerless forms.
    ///
    /// # Errors
    ///
    /// Returns an error when the file is too short, or when
    /// `system_variables_base` does not describe a block that fits — which is
    /// how a mislabelled file is caught, since the extension does not prove
    /// the format.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let (name, body) = if bytes.first() == Some(&TAPE_MARKER) {
            if bytes.len() < 1 + NAME_LEN + 2 {
                return Err("tape image too short to hold its header".to_owned());
            }
            let raw = &bytes[1..=NAME_LEN];
            let name = String::from_utf8_lossy(raw).trim_end().to_owned();
            (Some(name), &bytes[1 + NAME_LEN..])
        } else {
            (None, bytes)
        };

        if body.len() < 2 {
            return Err("tape image too short to hold a system-variables base".to_owned());
        }
        let base = u16::from_le_bytes([body[0], body[1]]);
        let length = usize::from(SYSTEM_VARIABLES_END.wrapping_sub(base)).wrapping_add(1);
        if base > SYSTEM_VARIABLES_END || length == 0 || length > MAX_SYSTEM_VARIABLES {
            return Err(format!(
                "system-variables base ${base:04X} is not a plausible one; a .mtx image bases \
                 its block near $F8F2, and a quarter of files carrying this extension are a \
                 different format"
            ));
        }
        let rest = &body[2..];
        if rest.len() < length {
            return Err(format!(
                "system-variables base ${base:04X} wants {length} bytes but only {} remain",
                rest.len()
            ));
        }
        let (system_variables, basic) = rest.split_at(length);
        Ok(Self {
            name,
            system_variables_base: base,
            system_variables: system_variables.to_vec(),
            basic: basic.to_vec(),
        })
    }
}

/// A parsed `.RUN` program.
///
/// Documented by MEMU's author but not checked against a real file — the TOSEC
/// set carries none. See the reference document §2.
#[derive(Debug)]
pub struct RunProgram {
    /// Address the code loads at, and is entered by jumping to.
    pub code_base: u16,
    /// The machine code.
    pub code: Vec<u8>,
}

impl RunProgram {
    /// Parse a `.RUN` program.
    ///
    /// # Errors
    ///
    /// Returns an error when the header is incomplete or the declared length
    /// runs past the end of the file.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 4 {
            return Err("RUN file too short to hold its header".to_owned());
        }
        let code_base = u16::from_le_bytes([bytes[0], bytes[1]]);
        let code_length = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
        let code = bytes
            .get(4..4 + code_length)
            .ok_or_else(|| {
                format!(
                    "RUN file declares {code_length} bytes of code but carries {}",
                    bytes.len() - 4
                )
            })?
            .to_vec();
        Ok(Self { code_base, code })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USUAL_BASE: u16 = 0xF8F2;

    fn tape(name: Option<&str>, base: u16, basic: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(name) = name {
            out.push(TAPE_MARKER);
            let mut padded = format!("{name:<15}").into_bytes();
            padded.truncate(NAME_LEN);
            out.extend_from_slice(&padded);
        }
        out.extend_from_slice(&base.to_le_bytes());
        let length = usize::from(SYSTEM_VARIABLES_END - base) + 1;
        out.extend(std::iter::repeat_n(0xAAu8, length));
        out.extend_from_slice(basic);
        out
    }

    #[test]
    fn a_full_tape_image_splits_into_its_three_parts() {
        let image = tape(Some("TACHYON"), USUAL_BASE, &[0x01, 0x02, 0x03]);
        let parsed = TapeImage::parse(&image).expect("well-formed");
        assert_eq!(parsed.name.as_deref(), Some("TACHYON"));
        assert_eq!(parsed.system_variables_base, USUAL_BASE);
        assert_eq!(parsed.system_variables.len(), 0x0259, "601 bytes at $F8F2");
        assert_eq!(parsed.basic, [0x01, 0x02, 0x03]);
    }

    #[test]
    fn the_headerless_variant_parses_without_a_name() {
        let image = tape(None, USUAL_BASE, &[0x09]);
        let parsed = TapeImage::parse(&image).expect("headerless variant");
        assert_eq!(parsed.name, None);
        assert_eq!(parsed.system_variables_base, USUAL_BASE);
        assert_eq!(parsed.basic, [0x09]);
    }

    #[test]
    fn a_mislabelled_file_is_rejected_rather_than_half_loaded() {
        // $3E8A is one of the bases seen on files that carry the extension but
        // are a different format; it wants a 49 KB block.
        let mut image = tape(Some("BOGUS"), USUAL_BASE, &[0x00]);
        image[16..18].copy_from_slice(&0x3E8Au16.to_le_bytes());
        let error = TapeImage::parse(&image).expect_err("implausible base");
        assert!(error.contains("$3E8A"), "{error}");
    }

    #[test]
    fn a_truncated_system_variable_block_is_rejected() {
        let mut image = tape(Some("SHORT"), USUAL_BASE, &[]);
        image.truncate(image.len() - 10);
        let error = TapeImage::parse(&image).expect_err("truncated");
        assert!(error.contains("only"), "{error}");
    }

    #[test]
    fn a_run_program_carries_its_own_load_address() {
        let mut file = 0x8000u16.to_le_bytes().to_vec();
        file.extend_from_slice(&3u16.to_le_bytes());
        file.extend_from_slice(&[0xC3, 0x00, 0x80]);
        let parsed = RunProgram::parse(&file).expect("well-formed");
        assert_eq!(parsed.code_base, 0x8000);
        assert_eq!(parsed.code, [0xC3, 0x00, 0x80]);
    }

    #[test]
    fn a_run_program_shorter_than_it_claims_is_rejected() {
        let mut file = 0x8000u16.to_le_bytes().to_vec();
        file.extend_from_slice(&99u16.to_le_bytes());
        file.extend_from_slice(&[0x00, 0x01]);
        let error = RunProgram::parse(&file).expect_err("short");
        assert!(error.contains("declares 99 bytes"), "{error}");
    }
}
