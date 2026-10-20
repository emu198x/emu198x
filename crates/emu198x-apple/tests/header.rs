//! `include/emu198x.h` is hand-written, so these checks keep it honest: it
//! declares exactly the functions `src/ffi.rs` exports, and the status
//! values Swift sees are the ones Rust returns.

use std::collections::BTreeSet;

use emu198x_apple::status;

const HEADER: &str = include_str!("../include/emu198x.h");
const FFI: &str = include_str!("../src/ffi.rs");

fn identifier_at(text: &str) -> &str {
    let end = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    &text[..end]
}

/// Every `emu198x_…(` in the header outside comments.
fn declared() -> BTreeSet<&'static str> {
    HEADER
        .lines()
        .map(str::trim_start)
        .filter(|line| !(line.starts_with('/') || line.starts_with('*') || line.starts_with('#')))
        .flat_map(|line| {
            line.match_indices("emu198x_")
                .map(move |(at, _)| &line[at..])
        })
        .map(identifier_at)
        .filter(|name| HEADER.contains(&format!("{name}(")))
        .collect()
}

fn exported() -> BTreeSet<&'static str> {
    FFI.lines()
        .filter_map(|line| line.split_once("extern \"C\" fn "))
        .map(|(_, rest)| identifier_at(rest))
        .collect()
}

#[test]
fn header_declares_every_export_and_nothing_else() {
    let exported = exported();
    assert!(exported.len() > 10, "found the exports: {exported:?}");
    assert_eq!(declared(), exported);
}

#[test]
fn header_status_values_match_rust() {
    let defines: BTreeSet<(&str, i32)> = HEADER
        .lines()
        .filter_map(|line| line.strip_prefix("#define EMU198X_"))
        .filter_map(|rest| rest.split_once(' '))
        .map(|(name, value)| (name, value.trim().parse().expect("numeric status")))
        .collect();
    let expected = BTreeSet::from([
        ("OK", status::OK),
        ("ERROR", status::ERROR),
        ("INVALID_ARGUMENT", status::INVALID_ARGUMENT),
        ("PANICKED", status::PANICKED),
        ("POISONED", status::POISONED),
    ]);
    assert_eq!(defines, expected);
}
