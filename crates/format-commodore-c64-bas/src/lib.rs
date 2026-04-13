//! Commodore 64 BASIC V2 tokeniser.
//!
//! Converts plain-text `.bas` files with line numbers into tokenised PRG bytes
//! suitable for direct PRG import.

mod tokens;

use tokens::KEYWORDS;

const BASIC_START: u16 = 0x0801;

/// A tokenised BASIC program in PRG format.
#[derive(Debug, Clone)]
pub struct BasicProgram {
    /// PRG bytes: load address, tokenised lines, end marker.
    pub bytes: Vec<u8>,
}

/// Tokenises text BASIC source into C64 PRG format.
///
/// # Errors
///
/// Returns an error if any line number is missing or out of range.
pub fn tokenise(source: &str) -> Result<BasicProgram, String> {
    let mut lines: Vec<(u16, Vec<u8>)> = Vec::new();

    for (line_idx, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (line_num, rest) = parse_line_number(line, line_idx)?;
        lines.push((line_num, tokenise_line(rest)));
    }

    let mut output = vec![BASIC_START as u8, (BASIC_START >> 8) as u8];
    let mut addr = BASIC_START;

    for (line_num, content) in &lines {
        let line_size = 2 + 2 + content.len() + 1;
        let next_addr = addr + line_size as u16;
        output.push(next_addr as u8);
        output.push((next_addr >> 8) as u8);
        output.push(*line_num as u8);
        output.push((line_num >> 8) as u8);
        output.extend_from_slice(content);
        output.push(0x00);
        addr = next_addr;
    }

    output.push(0x00);
    output.push(0x00);

    Ok(BasicProgram { bytes: output })
}

fn parse_line_number(line: &str, line_idx: usize) -> Result<(u16, &str), String> {
    let num_end = line
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(line.len());
    if num_end == 0 {
        return Err(format!(
            "Line {}: expected a line number, got: {line}",
            line_idx + 1
        ));
    }

    let num: u32 = line[..num_end]
        .parse()
        .map_err(|err| format!("Line {}: invalid line number: {err}", line_idx + 1))?;
    if num == 0 || num > 63_999 {
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

fn tokenise_line(line: &str) -> Vec<u8> {
    let bytes = line.as_bytes();
    let mut output = Vec::new();
    let mut pos = 0usize;
    let mut in_string = false;
    let mut after_rem = false;

    while pos < bytes.len() {
        let ch = bytes[pos];

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

        if ch == b'"' {
            in_string = true;
            output.push(ch);
            pos += 1;
            continue;
        }

        if let Some((token, keyword_len)) = match_keyword(&bytes[pos..]) {
            output.push(token);
            if token == 0x8F {
                after_rem = true;
            }
            pos += keyword_len;
            continue;
        }

        output.push(ascii_to_petscii(ch));
        pos += 1;
    }

    output
}

fn match_keyword(text: &[u8]) -> Option<(u8, usize)> {
    for &(keyword, token) in KEYWORDS {
        if text.len() >= keyword.len()
            && text[..keyword.len()].eq_ignore_ascii_case(keyword.as_bytes())
        {
            let last_kw = keyword.as_bytes()[keyword.len() - 1];
            if (last_kw.is_ascii_alphabetic() || last_kw == b'$')
                && let Some(&next) = text.get(keyword.len())
                && (next.is_ascii_alphanumeric() || next == b'$')
            {
                continue;
            }

            return Some((token, keyword.len()));
        }
    }

    None
}

fn ascii_to_petscii(ch: u8) -> u8 {
    match ch {
        b'a'..=b'z' => ch - 0x20,
        _ => ch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenise_simple_print() {
        let prog = tokenise("10 PRINT \"HELLO\"").expect("should tokenise");
        assert_eq!(prog.bytes[0], 0x01);
        assert_eq!(prog.bytes[1], 0x08);
        assert_eq!(prog.bytes[6], 0x99);
    }

    #[test]
    fn tokenise_goto() {
        let prog = tokenise("20 GOTO 10").expect("should tokenise");
        assert_eq!(prog.bytes[6], 0x89);
    }

    #[test]
    fn next_line_pointers() {
        let prog = tokenise("10 PRINT \"A\"\n20 GOTO 10").expect("should tokenise");
        let next_ptr = u16::from(prog.bytes[2]) | (u16::from(prog.bytes[3]) << 8);
        assert_eq!(next_ptr, 0x080B);
    }

    #[test]
    fn program_ends_with_zero_marker() {
        let prog = tokenise("10 END").expect("should tokenise");
        let len = prog.bytes.len();
        assert_eq!(prog.bytes[len - 2], 0x00);
        assert_eq!(prog.bytes[len - 1], 0x00);
    }

    #[test]
    fn rem_preserves_content() {
        let prog = tokenise("10 REM PRINT IS NOT TOKENISED").expect("should tokenise");
        assert_eq!(prog.bytes[6], 0x8F);
        let after_rem = &prog.bytes[7..prog.bytes.len() - 3];
        assert!(!after_rem.contains(&0x99));
    }

    #[test]
    fn string_preserves_keywords() {
        let prog = tokenise("10 PRINT \"GOTO\"").expect("should tokenise");
        let quote_pos = prog.bytes.iter().position(|&b| b == b'"').expect("quote");
        assert_eq!(&prog.bytes[quote_pos + 1..quote_pos + 5], b"GOTO");
    }

    #[test]
    fn keyword_not_matched_as_prefix() {
        let prog = tokenise("10 LET PRINTER=1").expect("should tokenise");
        let after_header = &prog.bytes[6..];
        assert!(after_header.contains(&0x88));
        assert!(!after_header.contains(&0x99));
    }

    #[test]
    fn lowercase_converted_to_petscii() {
        let prog = tokenise("10 PRINT \"hello\"").expect("should tokenise");
        let quote_pos = prog.bytes.iter().position(|&b| b == b'"').expect("quote");
        assert_eq!(&prog.bytes[quote_pos + 1..quote_pos + 6], b"HELLO");
    }

    #[test]
    fn skip_blank_and_comment_lines() {
        let prog = tokenise("# comment\n\n10 END\n").expect("should tokenise");
        assert_eq!(prog.bytes[4], 10);
        assert_eq!(prog.bytes[5], 0);
    }

    #[test]
    fn line_number_validation() {
        assert!(tokenise("0 PRINT \"BAD\"").is_err());
        assert!(tokenise("64000 PRINT \"BAD\"").is_err());
        assert!(tokenise("PRINT \"BAD\"").is_err());
    }
}
