//! Commodore 64 BASIC V2 keyword-to-token mapping.
//!
//! Token values $80–$CB. Sorted longest-first so the tokeniser
//! performs greedy matching (e.g. "GOSUB" before "GO").
//!
//! The C64 ROM tokeniser only matches keywords — operators like +, -, *
//! are stored as literal PETSCII bytes, not as tokens $AA–$B3. We follow
//! the same convention here.

/// `(keyword_text, token_byte)` — sorted by keyword length descending.
pub const KEYWORDS: &[(&str, u8)] = &[
    // 7 characters
    ("RESTORE", 0x8C),
    ("INPUT#", 0x84),
    ("PRINT#", 0x98),
    ("RETURN", 0x8E),
    ("VERIFY", 0x95),
    // 6 characters
    ("RIGHT$", 0xC9),
    ("CLOSE", 0xA0),
    ("GOSUB", 0x8D),
    ("INPUT", 0x85),
    ("LEFT$", 0xC8),
    ("PRINT", 0x99),
    // 4 characters
    ("CONT", 0x9A),
    ("DATA", 0x83),
    ("GOTO", 0x89),
    ("LIST", 0x9B),
    ("LOAD", 0x93),
    ("MID$", 0xCA),
    ("NEXT", 0x82),
    ("OPEN", 0x9F),
    ("PEEK", 0xC2),
    ("POKE", 0x97),
    ("READ", 0x87),
    ("SAVE", 0x94),
    ("SPC(", 0xA6),
    ("STEP", 0xA9),
    ("STOP", 0x90),
    ("STR$", 0xC4),
    ("TAB(", 0xA3),
    ("THEN", 0xA7),
    ("WAIT", 0x92),
    ("CHR$", 0xC7),
    // 3 characters
    ("ABS", 0xB6),
    ("AND", 0xAF),
    ("ASC", 0xC6),
    ("ATN", 0xC1),
    ("CLR", 0x9C),
    ("CMD", 0x9D),
    ("COS", 0xBE),
    ("DEF", 0x96),
    ("DIM", 0x86),
    ("END", 0x80),
    ("EXP", 0xBD),
    ("FOR", 0x81),
    ("FRE", 0xB8),
    ("GET", 0xA1),
    ("INT", 0xB5),
    ("LEN", 0xC3),
    ("LET", 0x88),
    ("LOG", 0xBC),
    ("NEW", 0xA2),
    ("NOT", 0xA8),
    ("POS", 0xB9),
    ("REM", 0x8F),
    ("RND", 0xBB),
    ("RUN", 0x8A),
    ("SGN", 0xB4),
    ("SIN", 0xBF),
    ("SQR", 0xBA),
    ("SYS", 0x9E),
    ("TAN", 0xC0),
    ("USR", 0xB7),
    ("VAL", 0xC5),
    // 2 characters
    ("FN", 0xA5),
    ("GO", 0xCB),
    ("IF", 0x8B),
    ("ON", 0x91),
    ("OR", 0xB0),
    ("TO", 0xA4),
];
