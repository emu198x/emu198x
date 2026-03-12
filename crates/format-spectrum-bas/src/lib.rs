//! ZX Spectrum BASIC tokeniser.
//!
//! Converts plain-text `.bas` files (with line numbers) into the tokenised
//! format the Spectrum stores in memory. Pair with `format-spectrum-tap`
//! to produce loadable TAP files.
//!
//! # Input format
//!
//! ```text
//! 10 PRINT "Hello, world!"
//! 20 GOTO 10
//! ```
//!
//! Each line must start with a line number (1–9999).

mod tokens;

use tokens::KEYWORDS;

/// A tokenised BASIC program, ready to be wrapped in a TAP file.
#[derive(Debug, Clone)]
pub struct BasicProgram {
    /// The raw tokenised bytes (all lines concatenated).
    pub bytes: Vec<u8>,
}

/// Tokenise a text BASIC program.
///
/// # Errors
///
/// Returns an error if a line has no line number or the line number is out of range.
pub fn tokenise(source: &str) -> Result<BasicProgram, String> {
    let mut output = Vec::new();

    for (line_idx, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue; // skip blank lines and comments
        }

        let (line_num, rest) = parse_line_number(line, line_idx)?;
        let tokenised = tokenise_line(rest);

        // Line format: 2-byte line number (big-endian), 2-byte length (little-endian),
        // tokenised content, $0D terminator.
        let line_len = (tokenised.len() + 1) as u16; // +1 for the $0D
        output.push((line_num >> 8) as u8);
        output.push(line_num as u8);
        output.push(line_len as u8);
        output.push((line_len >> 8) as u8);
        output.extend_from_slice(&tokenised);
        output.push(0x0D);
    }

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

    if num == 0 || num > 9999 {
        return Err(format!(
            "Line {}: line number {num} out of range (1–9999)",
            line_idx + 1
        ));
    }

    // Skip a single space after the line number if present
    let rest_start = if line[num_end..].starts_with(' ') {
        num_end + 1
    } else {
        num_end
    };

    Ok((num as u16, &line[rest_start..]))
}

/// Tokenise the content of a single line (after the line number).
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
            output.push(ch);
            if ch == b'"' {
                in_string = false;
            }
            pos += 1;
            continue;
        }

        if after_rem {
            output.push(ch);
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
            if token == 0xEA {
                // REM — rest of line is literal
                after_rem = true;
            }
            pos += keyword_len;
            continue;
        }

        // Number literal — emit ASCII digits then the 5-byte hidden representation
        if ch.is_ascii_digit() || (ch == b'.' && pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_digit()) {
            let num_start = pos;
            // Consume the number: digits, optional decimal point, more digits,
            // optional E/e exponent
            while pos < bytes.len() && (bytes[pos].is_ascii_digit() || bytes[pos] == b'.') {
                pos += 1;
            }
            // Exponent
            if pos < bytes.len() && (bytes[pos] == b'E' || bytes[pos] == b'e') {
                pos += 1;
                if pos < bytes.len() && (bytes[pos] == b'+' || bytes[pos] == b'-') {
                    pos += 1;
                }
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
            }

            let num_str = &line[num_start..pos];
            // Emit the ASCII representation
            output.extend_from_slice(num_str.as_bytes());
            // Emit the hidden 5-byte float
            output.push(0x0E);
            output.extend_from_slice(&number_to_float5(num_str));
            continue;
        }

        // Anything else — emit as-is
        output.push(ch);
        pos += 1;
    }

    output
}

/// Try to match a keyword at the start of `text`.
/// Returns the token byte and the number of source bytes consumed.
fn match_keyword(text: &[u8]) -> Option<(u8, usize)> {
    // Keywords are sorted longest-first in the table to ensure greedy matching.
    for &(keyword, token) in KEYWORDS {
        if text.len() >= keyword.len()
            && text[..keyword.len()].eq_ignore_ascii_case(keyword.as_bytes())
        {
            // Avoid matching keywords that are prefixes of identifiers.
            // e.g. "PRINTER" should not match "PRINT" + "ER".
            // If the keyword ends with a letter and the next char is also a letter/digit,
            // don't match.
            let last_kw = keyword.as_bytes()[keyword.len() - 1];
            if last_kw.is_ascii_alphabetic() {
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

/// Convert a number string to the Spectrum's 5-byte floating point format.
///
/// For integers in the range -65535..=65535 the short form is used:
/// `[0x00, sign, low, high, 0x00]`.
///
/// For other values the full 5-byte float is used.
fn number_to_float5(s: &str) -> [u8; 5] {
    let val: f64 = s.parse().unwrap_or(0.0);

    // Use the integer short form if possible
    let int_val = val as i64;
    #[allow(clippy::float_cmp)]
    if val == int_val as f64 && (-65535..=65535).contains(&int_val) {
        let sign = if int_val < 0 { 0xFF } else { 0x00 };
        let abs_val = int_val.unsigned_abs() as u16;
        return [0x00, sign, abs_val as u8, (abs_val >> 8) as u8, 0x00];
    }

    // Full floating point
    float_to_spectrum5(val)
}

/// Convert an `f64` to the Spectrum's 5-byte floating point format.
///
/// Format: byte 0 = exponent + 0x80, bytes 1–4 = mantissa.
/// Bit 7 of byte 1 encodes the sign (0 = positive, 1 = negative).
/// The mantissa has an implicit leading 1 bit.
fn float_to_spectrum5(val: f64) -> [u8; 5] {
    if val == 0.0 {
        return [0x00, 0x00, 0x00, 0x00, 0x00];
    }

    let negative = val < 0.0;
    let val = val.abs();

    // Decompose: val = mantissa * 2^exp, where 0.5 <= mantissa < 1.0
    // (Spectrum convention: mantissa in [0.5, 1.0), stored with implicit leading 1)
    let mut exp = val.log2().floor() as i32 + 1;
    let mut mantissa = val / f64::from(2_i32.pow(exp as u32));

    // Normalise into [0.5, 1.0)
    while mantissa >= 1.0 {
        mantissa /= 2.0;
        exp += 1;
    }
    while mantissa < 0.5 && mantissa > 0.0 {
        mantissa *= 2.0;
        exp -= 1;
    }

    let exp_byte = (exp + 0x80) as u8;

    // Mantissa to 4 bytes (32 bits). The leading 1 bit is implicit,
    // so we strip it and shift left by 1 to make room for the sign bit.
    let m = ((mantissa * 2.0 - 1.0) * (1u64 << 31) as f64) as u32;
    let m_bytes = m.to_be_bytes();

    let mut result = [exp_byte, m_bytes[0], m_bytes[1], m_bytes[2], m_bytes[3]];

    // Sign bit goes in bit 7 of byte 1
    if negative {
        result[1] |= 0x80;
    } else {
        result[1] &= 0x7F;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenise_simple_print() {
        let prog = tokenise("10 PRINT \"Hello\"").expect("should tokenise");
        // Line number 10 = $00 $0A (big-endian)
        assert_eq!(prog.bytes[0], 0x00);
        assert_eq!(prog.bytes[1], 0x0A);
        // After the 4-byte header, first byte should be PRINT token ($F5)
        // Note: the keyword table entry "PRINT " includes the trailing space
        assert_eq!(prog.bytes[4], 0xF5);
        // Then quote, H, e, l, l, o, quote (PRINT consumed its trailing space)
        assert_eq!(prog.bytes[5], b'"');
        assert_eq!(&prog.bytes[6..11], b"Hello");
        assert_eq!(prog.bytes[11], b'"');
        // Last byte is $0D terminator
        assert_eq!(*prog.bytes.last().unwrap(), 0x0D);
    }

    #[test]
    fn tokenise_goto() {
        let prog = tokenise("20 GO TO 10").expect("should tokenise");
        assert_eq!(prog.bytes[4], 0xEC); // GO TO token
    }

    #[test]
    fn tokenise_rem_preserves_content() {
        let prog = tokenise("10 REM PRINT is not tokenised here").expect("should tokenise");
        assert_eq!(prog.bytes[4], 0xEA); // REM token
        // "PRINT" after REM should be literal ASCII, not the $F5 token
        let content = &prog.bytes[5..prog.bytes.len() - 1]; // skip REM and $0D
        assert!(
            !content.contains(&0xF5),
            "PRINT inside REM should not be tokenised"
        );
    }

    #[test]
    fn tokenise_string_preserves_keywords() {
        let prog = tokenise("10 PRINT \"GOTO\"").expect("should tokenise");
        // GOTO inside quotes should be literal ASCII
        let bytes = &prog.bytes;
        // Find the opening quote
        let quote_pos = bytes.iter().position(|&b| b == b'"').unwrap();
        let inner = &bytes[quote_pos + 1..quote_pos + 5];
        assert_eq!(inner, b"GOTO");
    }

    #[test]
    fn integer_encoding() {
        let result = number_to_float5("42");
        assert_eq!(result, [0x00, 0x00, 42, 0, 0x00]);
    }

    #[test]
    fn zero_encoding() {
        let result = number_to_float5("0");
        assert_eq!(result, [0x00, 0x00, 0, 0, 0x00]);
    }

    #[test]
    fn negative_integer_encoding() {
        // This won't appear in BASIC source (- is an operator, not part of the literal)
        // but test the function anyway
        let result = number_to_float5("-1");
        assert_eq!(result, [0x00, 0xFF, 1, 0, 0x00]);
    }

    #[test]
    fn number_literal_has_hidden_float() {
        let prog = tokenise("10 LET a=42").expect("should tokenise");
        // After "42" (ASCII $34 $32) there should be $0E + 5 bytes
        let bytes = &prog.bytes;
        let pos_4 = bytes.iter().position(|&b| b == b'4').unwrap();
        assert_eq!(bytes[pos_4], b'4');
        assert_eq!(bytes[pos_4 + 1], b'2');
        assert_eq!(bytes[pos_4 + 2], 0x0E); // hidden number marker
        assert_eq!(bytes[pos_4 + 3], 0x00); // integer short form
    }

    #[test]
    fn skip_blank_and_comment_lines() {
        let prog = tokenise("# This is a comment\n\n10 CLS\n").expect("should tokenise");
        // Should only produce one BASIC line
        assert_eq!(prog.bytes[0], 0x00);
        assert_eq!(prog.bytes[1], 0x0A); // line 10
        assert_eq!(prog.bytes[4], 0xFB); // CLS token
    }

    #[test]
    fn line_number_out_of_range() {
        assert!(tokenise("0 PRINT \"bad\"").is_err());
        assert!(tokenise("10000 PRINT \"bad\"").is_err());
    }

    #[test]
    fn no_line_number() {
        assert!(tokenise("PRINT \"bad\"").is_err());
    }

    #[test]
    fn keyword_not_matched_as_prefix_of_identifier() {
        // "printer" should not be tokenised as PRINT + "er"
        let prog = tokenise("10 LET printer=1").expect("should tokenise");
        let bytes = &prog.bytes;
        // PRINT token is $F5 — it should NOT appear
        // (LET is $F1, which will appear)
        let after_header = &bytes[4..];
        assert!(
            !after_header.iter().any(|&b| b == 0xF5),
            "PRINT should not match inside 'printer'"
        );
    }
}
