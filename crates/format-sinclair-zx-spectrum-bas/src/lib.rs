//! ZX Spectrum BASIC parser and tokeniser.
//!
//! Converts plain-text `.bas` files (with line numbers) into the tokenised
//! format the Spectrum stores in memory.
//!
//! Internally, the pipeline is: **text → AST → tokenised bytes**.
//! The AST is available for educational verification and analysis.
//!
//! # Input format
//!
//! ```text
//! 10 PRINT "Hello, world!"
//! 20 GO TO 10
//! ```
//!
//! Each line must start with a line number (1–9999).

pub mod ast;
mod parser;
mod serialize;
mod tokens;

/// A tokenised BASIC program, ready to be poked into Spectrum RAM.
#[derive(Debug, Clone)]
pub struct BasicProgram {
    /// The raw tokenised bytes (all lines concatenated).
    pub bytes: Vec<u8>,
}

/// Parse and tokenise a text BASIC program.
///
/// # Errors
///
/// Returns an error if a line has no line number or the line number is out of range.
pub fn tokenise(source: &str) -> Result<BasicProgram, String> {
    let program = parser::parse_program(source)?;
    let bytes = serialize::serialize(&program);
    Ok(BasicProgram { bytes })
}

/// Parse a text BASIC program into an AST without tokenising.
pub fn parse(source: &str) -> Result<ast::Program, String> {
    parser::parse_program(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenise_simple_print() {
        let prog = tokenise("10 PRINT \"Hello\"").expect("should tokenise");
        assert_eq!(prog.bytes[0], 0x00);
        assert_eq!(prog.bytes[1], 0x0A);
        assert_eq!(prog.bytes[4], 0xF5);
        assert_eq!(prog.bytes[5], b'"');
        assert_eq!(&prog.bytes[6..11], b"Hello");
        assert_eq!(prog.bytes[11], b'"');
        assert_eq!(*prog.bytes.last().expect("non-empty"), 0x0D);
    }

    #[test]
    fn tokenise_goto() {
        let prog = tokenise("20 GO TO 10").expect("should tokenise");
        assert_eq!(prog.bytes[4], 0xEC);
    }

    #[test]
    fn tokenise_rem_preserves_content() {
        let prog = tokenise("10 REM PRINT is not tokenised here").expect("should tokenise");
        assert_eq!(prog.bytes[4], 0xEA);
        let content = &prog.bytes[5..prog.bytes.len() - 1];
        assert!(
            !content.contains(&0xF5),
            "PRINT inside REM should not be tokenised"
        );
    }

    #[test]
    fn tokenise_string_preserves_keywords() {
        let prog = tokenise("10 PRINT \"GOTO\"").expect("should tokenise");
        let bytes = &prog.bytes;
        let quote_pos = bytes
            .iter()
            .position(|&b| b == b'"')
            .expect("quote present");
        let inner = &bytes[quote_pos + 1..quote_pos + 5];
        assert_eq!(inner, b"GOTO");
    }

    #[test]
    fn integer_encoding() {
        let prog = tokenise("10 LET a=42").expect("should tokenise");
        let pos_4 = prog
            .bytes
            .iter()
            .position(|&b| b == b'4')
            .expect("'4' present");
        assert_eq!(prog.bytes[pos_4 + 2], 0x0E);
        assert_eq!(prog.bytes[pos_4 + 3], 0x00); // integer short form
        assert_eq!(prog.bytes[pos_4 + 4], 0x00); // sign = positive
        assert_eq!(prog.bytes[pos_4 + 5], 42); // low byte
        assert_eq!(prog.bytes[pos_4 + 6], 0); // high byte
        assert_eq!(prog.bytes[pos_4 + 7], 0x00);
    }

    #[test]
    fn zero_encoding() {
        let prog = tokenise("10 LET a=0").expect("should tokenise");
        let pos_0 = prog
            .bytes
            .iter()
            .position(|&b| b == b'0')
            .expect("'0' present");
        assert_eq!(prog.bytes[pos_0 + 1], 0x0E);
        assert_eq!(&prog.bytes[pos_0 + 2..pos_0 + 7], &[0, 0, 0, 0, 0]);
    }

    #[test]
    fn number_literal_has_hidden_float() {
        let prog = tokenise("10 LET a=42").expect("should tokenise");
        let bytes = &prog.bytes;
        let pos_4 = bytes
            .iter()
            .position(|&b| b == b'4')
            .expect("digit 4 present");
        assert_eq!(bytes[pos_4], b'4');
        assert_eq!(bytes[pos_4 + 1], b'2');
        assert_eq!(bytes[pos_4 + 2], 0x0E);
        assert_eq!(bytes[pos_4 + 3], 0x00);
    }

    #[test]
    fn skip_blank_and_comment_lines() {
        let prog = tokenise("# This is a comment\n\n10 CLS\n").expect("should tokenise");
        assert_eq!(prog.bytes[0], 0x00);
        assert_eq!(prog.bytes[1], 0x0A);
        assert_eq!(prog.bytes[4], 0xFB);
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
    fn fractional_number_encoding() {
        let prog = tokenise("10 BEEP 0.3,5").expect("should tokenise");
        assert!(!prog.bytes.is_empty());
    }

    #[test]
    fn keyword_not_matched_as_prefix_of_identifier() {
        let prog = tokenise("10 LET printer=1").expect("should tokenise");
        let after_header = &prog.bytes[4..];
        assert!(
            !after_header.contains(&0xF5),
            "PRINT should not match inside 'printer'"
        );
    }

    #[test]
    fn for_variable_not_tokenised_as_keyword() {
        let prog = tokenise("10 FOR cat = 1 TO 4").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        assert!(
            !after_header.contains(&0xCF),
            "CAT keyword should not appear in FOR cat"
        );
        assert_eq!(after_header[0], 0xEB);
        assert_eq!(after_header[1], b'c');
        assert_eq!(after_header[2], b'a');
        assert_eq!(after_header[3], b't');
    }

    #[test]
    fn let_variable_not_tokenised_as_keyword() {
        let prog = tokenise("10 LET ink = 5").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        assert!(
            !after_header.contains(&0xD9),
            "INK keyword should not appear in LET ink"
        );
    }

    #[test]
    fn read_variables_not_tokenised_as_keywords() {
        let prog = tokenise("10 READ cat$,ink").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        assert!(
            !after_header.contains(&0xCF),
            "CAT keyword should not appear in READ cat$"
        );
        assert!(
            !after_header.contains(&0xD9),
            "INK keyword should not appear in READ ink"
        );
    }

    #[test]
    fn next_variable_not_tokenised_as_keyword() {
        let prog = tokenise("10 NEXT cat").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        assert!(
            !after_header.contains(&0xCF),
            "CAT keyword should not appear in NEXT cat"
        );
    }

    #[test]
    fn dim_variable_not_tokenised_as_keyword() {
        let prog = tokenise("10 DIM cat(4)").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        assert!(
            !after_header.contains(&0xCF),
            "CAT keyword should not appear in DIM cat"
        );
    }

    #[test]
    fn keyword_still_works_in_expressions() {
        let prog = tokenise("10 CAT 1").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        assert_eq!(after_header[0], 0xCF);
    }

    // ── New tests: expression context ───────────────────────────────

    #[test]
    fn paper_ink_variable_not_tokenised() {
        let prog = tokenise("10 PAPER ink").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        assert_eq!(after_header[0], 0xDA); // PAPER token
        assert!(
            !after_header[1..].contains(&0xD9),
            "INK keyword should not appear when ink is a variable after PAPER"
        );
    }

    #[test]
    fn paper_ink_colon_ink_keyword() {
        let prog = tokenise("10 PAPER ink: INK 7").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        assert_eq!(after_header[0], 0xDA); // PAPER token
        // Find the colon — after it, INK should be a keyword
        let colon_pos = after_header
            .iter()
            .position(|&b| b == b':')
            .expect("colon present");
        assert_eq!(after_header[colon_pos + 1], 0xD9); // INK keyword
    }

    #[test]
    fn print_ink_as_print_item() {
        let prog = tokenise("10 PRINT INK 2;\"text\"").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        assert_eq!(after_header[0], 0xF5); // PRINT
        assert!(
            after_header[1..].contains(&0xD9),
            "INK should be tokenised as a keyword within PRINT"
        );
    }

    #[test]
    fn let_expression_ink_not_keyword() {
        let prog = tokenise("10 LET a = ink + 1").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        assert!(
            !after_header.contains(&0xD9),
            "INK should not be tokenised in expression context"
        );
    }

    #[test]
    fn cat_in_expression_not_keyword() {
        let prog = tokenise("10 IF cat > 3 THEN PRINT cat").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        // CAT token $CF should not appear anywhere
        assert!(
            !after_header.contains(&0xCF),
            "CAT should not be tokenised when cat is a variable"
        );
    }

    #[test]
    fn data_no_keyword_matching() {
        let prog = tokenise("10 DATA cat,3,ink").expect("should tokenise");
        let after_header = &prog.bytes[4..prog.bytes.len() - 1];
        assert_eq!(after_header[0], 0xE4); // DATA token
        assert!(
            !after_header[1..].contains(&0xCF),
            "CAT keyword should not appear in DATA values"
        );
        assert!(
            !after_header[1..].contains(&0xD9),
            "INK keyword should not appear in DATA values"
        );
    }

    #[test]
    fn parse_produces_ast() {
        let program = parse("10 LET cat = 5\n20 PRINT cat").expect("should parse");
        assert_eq!(program.lines.len(), 2);
        assert_eq!(program.lines[0].number, 10);
        assert_eq!(program.lines[1].number, 20);
        assert!(matches!(
            program.lines[0].statements[0],
            ast::Statement::Let { .. }
        ));
    }
}
