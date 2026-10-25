//! This crate is an exception to the workspace's `unsafe_code = "forbid"`
//! (knowledge/decisions/windows-menu-unsafe-exception.md). These checks keep
//! the exception as narrow as the decision allows: the crate's lint table is
//! the workspace's with `unsafe_code` lowered to `deny` and nothing else
//! weakened, and only the `attach` module is allowed unsafe code.

use std::collections::BTreeMap;
use std::path::Path;

const CRATE: &str = include_str!("../Cargo.toml");
const WORKSPACE: &str = include_str!("../../../Cargo.toml");

/// The `key = value` lines of one TOML table.
fn table<'a>(text: &'a str, header: &str) -> BTreeMap<&'a str, &'a str> {
    text.lines()
        .skip_while(|line| line.trim() != header)
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect()
}

#[test]
fn every_workspace_lint_still_applies() {
    for (workspace, local) in [
        ("[workspace.lints.rust]", "[lints.rust]"),
        ("[workspace.lints.clippy]", "[lints.clippy]"),
    ] {
        let workspace_lints = table(WORKSPACE, workspace);
        let local_lints = table(CRATE, local);
        assert!(!workspace_lints.is_empty(), "found {workspace}");
        for (lint, level) in workspace_lints {
            let expected = if lint == "unsafe_code" {
                "\"deny\""
            } else {
                level
            };
            assert_eq!(local_lints.get(lint), Some(&expected), "{lint} in {local}");
        }
    }
    assert_eq!(
        table(CRATE, "[lints.clippy]").get("undocumented_unsafe_blocks"),
        Some(&"\"deny\"")
    );
}

#[test]
fn only_the_attach_module_uses_unsafe() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut allows = 0;
    for entry in std::fs::read_dir(&src).expect("src") {
        let path = entry.expect("entry").path();
        let text = std::fs::read_to_string(&path).expect("source");
        allows += text.matches("allow(unsafe_code)").count();
        if path.file_name().is_some_and(|name| name != "attach.rs") {
            for form in ["unsafe {", "unsafe fn", "unsafe extern", "unsafe impl"] {
                assert!(!text.contains(form), "{} uses `{form}`", path.display());
            }
        }
    }
    let lib = std::fs::read_to_string(src.join("lib.rs")).expect("lib.rs");
    assert_eq!(allows, 1, "one allow, on the attach module");
    assert!(lib.contains("#[allow(unsafe_code)]\nmod attach;"));
    // The attach module holds exactly one unsafe block: the call to muda.
    let attach = std::fs::read_to_string(src.join("attach.rs")).expect("attach.rs");
    assert_eq!(attach.matches("unsafe {").count(), 1, "one unsafe block");
}
