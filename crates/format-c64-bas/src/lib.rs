//! Commodore 64 BASIC V2 tokeniser.
//!
//! Converts plain-text `.bas` files (with line numbers) into the tokenised
//! PRG format the C64 stores in memory. The output includes the $0801 load
//! address header and can be loaded directly with `load_prg`.
//!
//! # Input format
//!
//! ```text
//! 10 PRINT "HELLO, WORLD!"
//! 20 GOTO 10
//! ```
//!
//! Each line must start with a line number (1–63999).

mod tokens;

use tokens::KEYWORDS;

/// Default BASIC start address on the C64.
const BASIC_START: u16 = 0x0801;

/// A tokenised BASIC program in PRG format (load address + program bytes).
#[derive(Debug, Clone)]
pub struct BasicProgram {
    /// PRG bytes: 2-byte load address (little-endian) followed by the
    /// tokenised program and a $00 $00 end marker.
    pub bytes: Vec<u8>,
}

/// Tokenise a text BASIC program into C64 PRG format.
///
/// The output starts with the load address ($0801) followed by the tokenised
/// lines and a two-byte zero end marker. This can be fed directly to
/// `load_prg`.
///
/// # Errors
///
/// Returns an error if a line has no line number or the line number is out of range.
pub fn tokenise(source: &str) -> Result<BasicProgram, String> {
    // First pass: tokenise each line into (line_number, content_bytes).
    let mut lines: Vec<(u16, Vec<u8>)> = Vec::new();

    for (line_idx, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (line_num, rest) = parse_line_number(line, line_idx)?;
        let tokenised = tokenise_line(rest);
        lines.push((line_num, tokenised));
    }

    // Second pass: build PRG with next-line pointers.
    let mut output = Vec::new();

    // PRG load address
    output.push(BASIC_START as u8);
    output.push((BASIC_START >> 8) as u8);

    // Current address tracks where each line starts in C64 memory.
    let mut addr = BASIC_START;

    for (line_num, content) in &lines {
        // Line size: 2 (next ptr) + 2 (line num) + content + 1 (null terminator)
        let line_size = 2 + 2 + content.len() + 1;
        let next_addr = addr + line_size as u16;

        // Next-line pointer (little-endian)
        output.push(next_addr as u8);
        output.push((next_addr >> 8) as u8);

        // Line number (little-endian)
        output.push(*line_num as u8);
        output.push((line_num >> 8) as u8);

        // Tokenised content
        output.extend_from_slice(content);

        // Null terminator
        output.push(0x00);

        addr = next_addr;
    }

    // End-of-program marker: two zero bytes (null next-line pointer)
    output.push(0x00);
    output.push(0x00);

    Ok(BasicProgram { bytes: output })
}

/// Extract the line number from the start of a line.
fn parse_line_number(line: &str, line_idx: usize) -> Result<(u16, &str), String> {
    let num_end = line.find(|c: char| !c.is_ascii_digit()).unwrap_or(line.len());
    if num_end == 0 {
        return Err(format!(
            "Line {}: expected a line number, got: {line}",
            line_idx + 1
        ));
    }

    let num: u32 = line[..num_end].parse().map_err(|e| {
        format!("Line {}: invalid line number: {e}", line_idx + 1)
    })?;

    if num == 0 || num > 63999 {
        return Err(format!(
            "Line {}: line number {num} out of range (1–63999)",
            line_idx + 1
        ));
    }

    let rest_start = if line[num_end..].starts_with(' ') {
        num_end + 1
    } else {
        num_end
    };

    Ok((num as u16, &line[rest_start..]))
}

/// Tokenise the content of a single line (after the line number).
///
/// The C64 ROM tokeniser converts keywords to single-byte tokens ($80–$CB)
/// but leaves everything else — including operators — as literal PETSCII.
/// Content inside string literals and after REM is never tokenised.
fn tokenise_line(line: &str) -> Vec<u8> {
    let bytes = line.as_bytes();
    let mut output = Vec::new();
    let mut pos = 0;
    let mut in_string = false;
    let mut after_rem = false;

    while pos < bytes.len() {
        let ch = bytes[pos];

        // Inside a string literal or after REM — emit raw bytes
        if in_string {
            output.push(ascii_to_petscii(ch));
            if ch == b'"' {
                in_string = false;
            }
            pos += 1;
            continue;
        }

        if after_rem {
            output.push(ascii_to_petscii(ch));
            pos += 1;
            continue;
        }

        // Opening quote
        if ch == b'"' {
            in_string = true;
            output.push(ch);
            pos += 1;
            continue;
        }

        // Try to match a keyword at the current position
        if let Some((token, keyword_len)) = match_keyword(&bytes[pos..]) {
            output.push(token);
            if token == 0x8F {
                // REM — rest of line is literal
                after_rem = true;
            }
            pos += keyword_len;
            continue;
        }

        // Anything else — convert to PETSCII
        output.push(ascii_to_petscii(ch));
        pos += 1;
    }

    output
}

/// Try to match a keyword at the start of `text`.
/// Returns the token byte and the number of source bytes consumed.
fn match_keyword(text: &[u8]) -> Option<(u8, usize)> {
    for &(keyword, token) in KEYWORDS {
        if text.len() >= keyword.len()
            && text[..keyword.len()].eq_ignore_ascii_case(keyword.as_bytes())
        {
            // Avoid matching keywords that are prefixes of identifiers.
            let last_kw = keyword.as_bytes()[keyword.len() - 1];
            if last_kw.is_ascii_alphabetic() || last_kw == b'$' {
                if let Some(&next) = text.get(keyword.len()) {
                    if next.is_ascii_alphanumeric() || next == b'$' {
                        continue;
                    }
                }
            }
            return Some((token, keyword.len()));
        }
    }
    None
}

/// Convert an ASCII byte to PETSCII (unshifted mode).
///
/// The main difference: lowercase ASCII letters map to uppercase PETSCII
/// ($41–$5A) and uppercase ASCII letters also map to $41–$5A. In BASIC
/// listings, the C64 displays uppercase by default.
fn ascii_to_petscii(ch: u8) -> u8 {
    match ch {
        b'a'..=b'z' => ch - 0x20, // lowercase → uppercase PETSCII
        _ => ch,                   // everything else passes through
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenise_simple_print() {
        let prog = tokenise("10 PRINT \"HELLO\"").expect("should tokenise");
        // PRG starts with load address $0801
        assert_eq!(prog.bytes[0], 0x01);
        assert_eq!(prog.bytes[1], 0x08);
        // Skip load address (2) + next-line pointer (2) + line number (2) = offset 6
        assert_eq!(prog.bytes[6], 0x99); // PRINT token
    }

    #[test]
    fn tokenise_goto() {
        let prog = tokenise("20 GOTO 10").expect("should tokenise");
        assert_eq!(prog.bytes[6], 0x89); // GOTO token
    }

    #[test]
    fn next_line_pointers() {
        let prog = tokenise("10 PRINT \"A\"\n20 GOTO 10").expect("should tokenise");
        // First line starts at $0801
        // Next-line pointer is at bytes 2-3 (after load address)
        let next_ptr = u16::from(prog.bytes[2]) | (u16::from(prog.bytes[3]) << 8);
        // Line content: PRINT token, ", A, ", null = 5 bytes
        // Line: 2 (next) + 2 (linenum) + 5 (content) + 1 (null) = 10 bytes
        // So next line at $0801 + 10 = $080B
        assert_eq!(next_ptr, 0x080B);
    }

    #[test]
    fn program_ends_with_zero_marker() {
        let prog = tokenise("10 END").expect("should tokenise");
        let len = prog.bytes.len();
        // Last two bytes should be $00 $00
        assert_eq!(prog.bytes[len - 2], 0x00);
        assert_eq!(prog.bytes[len - 1], 0x00);
    }

    #[test]
    fn rem_preserves_content() {
        let prog = tokenise("10 REM PRINT IS NOT TOKENISED").expect("should tokenise");
        assert_eq!(prog.bytes[6], 0x8F); // REM token
        // PRINT ($99) should not appear after REM
        let after_rem = &prog.bytes[7..prog.bytes.len() - 3]; // skip null + end marker
        assert!(
            !after_rem.contains(&0x99),
            "PRINT inside REM should not be tokenised"
        );
    }

    #[test]
    fn string_preserves_keywords() {
        let prog = tokenise("10 PRINT \"GOTO\"").expect("should tokenise");
        let bytes = &prog.bytes;
        let quote_pos = bytes.iter().position(|&b| b == b'"').unwrap();
        let inner = &bytes[quote_pos + 1..quote_pos + 5];
        assert_eq!(inner, b"GOTO");
    }

    #[test]
    fn keyword_not_matched_as_prefix() {
        let prog = tokenise("10 LET PRINTER=1").expect("should tokenise");
        let after_header = &prog.bytes[6..];
        // LET token ($88) should appear, but PRINT ($99) should not
        assert!(after_header.contains(&0x88), "LET should be tokenised");
        assert!(
            !after_header.iter().any(|&b| b == 0x99),
            "PRINT should not match inside PRINTER"
        );
    }

    #[test]
    fn lowercase_converted_to_petscii() {
        let prog = tokenise("10 PRINT \"hello\"").expect("should tokenise");
        let quote_pos = prog.bytes.iter().position(|&b| b == b'"').unwrap();
        // "hello" should become uppercase PETSCII
        assert_eq!(&prog.bytes[quote_pos + 1..quote_pos + 6], b"HELLO");
    }

    #[test]
    fn skip_blank_and_comment_lines() {
        let prog = tokenise("# comment\n\n10 END\n").expect("should tokenise");
        // Line number should be 10
        assert_eq!(prog.bytes[4], 10); // line num low
        assert_eq!(prog.bytes[5], 0);  // line num high
    }

    #[test]
    fn line_number_out_of_range() {
        assert!(tokenise("0 PRINT \"BAD\"").is_err());
        assert!(tokenise("64000 PRINT \"BAD\"").is_err());
    }

    #[test]
    fn no_line_number() {
        assert!(tokenise("PRINT \"BAD\"").is_err());
    }
}
