//! Text-preserving conversion for editable listings. The ROM judges syntax;
//! unlike the analysis AST, this path never invents expressions or drops text.
use crate::tokens::KeywordRole;
use crate::{BasicProgram, serialize::number_to_float5, tokens::KEYWORDS};
use std::collections::BTreeMap;

/// Convert a numbered ASCII listing to stored BASIC, ordered by line number.
///
/// Keyword names need word boundaries; strings and REM text stay literal.
/// Numeric spellings are retained with their hidden five-byte values appended.
/// This bounded editor route excludes DEF FN (which needs parameter markers).
///
/// # Errors
/// Returns an error for unsupported characters, malformed numbers/strings,
/// missing, duplicate or empty lines, unsupported DEF FN, or oversized output.
/// BASIC grammar is intentionally left to the ROM; tokenisation is not validation.
pub fn tokenise_listing(source: &str) -> Result<BasicProgram, String> {
    if source.len() > 65536 {
        return Err("Source exceeds 64 KiB".into());
    }
    let mut lines = BTreeMap::new();
    for (index, raw) in source.lines().enumerate() {
        let text = raw.trim();
        if text.is_empty() {
            continue;
        }
        let context = |message: &str| format!("Source line {}: {message}", index + 1);
        if !text.bytes().all(|b| (32..=126).contains(&b)) {
            return Err(context(
                "use plain ASCII text; graphics and control codes are not supported here",
            ));
        }
        let end = text.bytes().take_while(u8::is_ascii_digit).count();
        let number: u16 = text[..end]
            .parse()
            .map_err(|_| context("start with a line number from 1 to 9999"))?;
        if !(1..=9999).contains(&number) {
            return Err(context("line number must be 1 to 9999"));
        }
        let body = text[end..].trim_start();
        if body.is_empty() {
            return Err(context("remove an entire line to delete it"));
        }
        let mut bytes = tokenise_body(body).map_err(|e| context(&e))?;
        bytes.push(13);
        let length = u16::try_from(bytes.len()).map_err(|_| context("line is too long"))?;
        let mut line = number.to_be_bytes().to_vec();
        line.extend_from_slice(&length.to_le_bytes());
        line.extend(bytes);
        if lines.insert(number, line).is_some() {
            return Err(context(&format!(
                "line {number} appears twice; edit its existing line"
            )));
        }
    }
    let bytes: Vec<u8> = lines.into_values().flatten().collect();
    if bytes.is_empty() {
        return Err("Enter at least one numbered BASIC line".into());
    }
    if bytes.len() > 0x9000 {
        return Err("Program exceeds this 48K editor's 36 KiB limit".into());
    }
    Ok(BasicProgram { bytes })
}

fn tokenise_body(source: &str) -> Result<Vec<u8>, String> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut pos = 0;
    let mut binary = false;
    let mut statement_start = true;
    let mut printing = false;
    let mut variable = false;
    while pos < bytes.len() {
        let ch = bytes[pos];
        if variable && ch.is_ascii_alphabetic() {
            let start = pos;
            while pos < bytes.len() && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'$') {
                pos += 1;
            }
            out.extend_from_slice(&bytes[start..pos]);
            variable = false;
            statement_start = false;
        } else if ch == b'"' {
            let start = pos;
            pos += 1;
            while pos < bytes.len() && bytes[pos] != b'"' {
                pos += 1;
            }
            if pos == bytes.len() {
                return Err("missing closing quotation mark".into());
            }
            pos += 1;
            out.extend_from_slice(&bytes[start..pos]);
        } else if ch == b' ' {
            out.push(ch);
            pos += 1;
        } else if let Some((operator, token)) = [("<=", 0xC7), (">=", 0xC8), ("<>", 0xC9)]
            .iter()
            .find(|(operator, _)| source[pos..].starts_with(operator))
        {
            out.push(*token);
            pos += operator.len();
        } else if let Some(&(keyword, token, _)) = KEYWORDS.iter().find(|(keyword, _, role)| {
            let allowed = match role {
                KeywordRole::Statement | KeywordRole::RestOfLine => statement_start,
                KeywordRole::PrintItem => statement_start || printing,
                _ => true,
            };
            if !allowed {
                return false;
            }
            let word = keyword.trim_end();
            let rest = &bytes[pos..];
            rest.len() >= word.len()
                && rest[..word.len()].eq_ignore_ascii_case(word.as_bytes())
                && rest
                    .get(word.len())
                    .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'$')
        }) {
            if token == 0xCE {
                return Err("DEF FN is not supported by this editor yet".into());
            }
            out.push(token);
            if statement_start {
                printing = token == 0xF5 || token == 0xEE || token == 0xE0;
            }
            statement_start = token == 0xCB;
            variable = matches!(token, 0xF1 | 0xEB | 0xF3 | 0xE9 | 0xE3);
            pos += keyword.trim_end().len();
            // The ROM adds keyword display spacing. Do not duplicate it.
            if bytes.get(pos) == Some(&b' ') {
                pos += 1;
            }
            if token == 0xEA {
                out.extend_from_slice(&bytes[pos..]);
                break;
            }
            binary = token == 0xC4;
        } else if ch.is_ascii_alphabetic() {
            let start = pos;
            while pos < bytes.len() && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'$') {
                pos += 1;
            }
            out.extend_from_slice(&bytes[start..pos]);
            binary = false;
        } else if ch.is_ascii_digit()
            || (ch == b'.' && bytes.get(pos + 1).is_some_and(u8::is_ascii_digit))
        {
            let start = pos;
            while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                pos += 1;
            }
            if !binary && bytes.get(pos) == Some(&b'.') {
                pos += 1;
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
            }
            if !binary
                && bytes
                    .get(pos)
                    .is_some_and(|b| b.eq_ignore_ascii_case(&b'e'))
            {
                pos += 1;
                if bytes.get(pos).is_some_and(|b| *b == b'+' || *b == b'-') {
                    pos += 1;
                }
                let digits = pos;
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
                if pos == digits {
                    return Err("an exponent needs digits after E".into());
                }
            }
            let spelling = &source[start..pos];
            let value = if binary {
                u16::from_str_radix(spelling, 2)
                    .map(f64::from)
                    .map_err(|_| "BIN needs up to 16 binary digits")?
            } else {
                spelling.parse::<f64>().map_err(|_| "invalid number")?
            };
            if !value.is_finite()
                || value >= 2.0_f64.powi(127)
                || (value != 0.0 && value < 2.0_f64.powi(-128))
            {
                return Err("number is outside the Spectrum's range".into());
            }
            out.extend_from_slice(spelling.as_bytes());
            out.push(14);
            out.extend_from_slice(&number_to_float5(value));
            binary = false;
        } else {
            if ch == b':' {
                statement_start = true;
                printing = false;
            }
            // Retain punctuation and mistakes, so the ROM sees what was entered.
            out.push(ch);
            pos += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_text_and_unknown_input_are_not_rewritten() {
        let result = tokenise_listing("10 PRINT \"GO TO 12\": REM PRINT 99").expect("listing");
        assert_eq!(&result.bytes[4..], b"\xF5\"GO TO 12\": \xEAPRINT 99\r");
        assert!(result.bytes.windows(8).any(|w| w == b"GO TO 12"));
        assert!(result.bytes.windows(8).any(|w| w == b"PRINT 99"));
        assert!(!result.bytes.contains(&14));
        assert_eq!(
            &tokenise_listing("10 PRONT 2").expect("retain typo").bytes[4..9],
            b"PRONT"
        );
    }
    #[test]
    fn statement_words_can_be_variable_names() {
        let result = tokenise_listing("10 LET cat=2\n20 PRINT cat").expect("listing");
        assert!(!result.bytes.contains(&0xCF));
        assert_eq!(result.bytes.windows(3).filter(|w| *w == b"cat").count(), 2);
    }

    #[test]
    fn numbers_order_and_comparisons() {
        let result = tokenise_listing("20 IF x<>2 THEN PRINT x\n10 LET x=1.5").expect("listing");
        assert_eq!(&result.bytes[..2], &[0, 10]);
        assert!(result.bytes.contains(&0xC9));
        assert!(
            result
                .bytes
                .windows(9)
                .any(|w| w == [b'1', b'.', b'5', 14, 129, 64, 0, 0, 0])
        );
        assert!(tokenise_listing("10 PRINT 1\n10 PRINT 2").is_err());
        assert!(tokenise_listing("PRINT 1").is_err());
        assert!(tokenise_listing("10 PRINT \"oops").is_err());
        assert!(tokenise_listing("10 PRINT 1e999").is_err());
        assert!(tokenise_listing("10 PRINT BIN 102").is_err());
    }
}
