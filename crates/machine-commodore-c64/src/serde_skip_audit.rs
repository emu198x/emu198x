//! Compile-locked audit of `#[serde(skip)]` fields in the C64 runtime stack.
//!
//! A skipped field is reset to its default value during deserialisation. That
//! is safe only for state with no observable future effect, or when a typed
//! restore hook reconstructs it from serialised state. The C64 stack currently
//! needs neither exception: every chip, bus, drive, expansion and runtime field
//! that can affect subsequent execution is directly serialisable.
//!
//! The previous audit named only the SID source file. It therefore missed seven
//! skipped VIC-II sprite-pipeline fields and treated queued SID audio as
//! disposable even though the runtime drains it only at frame boundaries. This
//! audit walks every Rust source below the complete C64 runtime dependency set,
//! so adding a skip anywhere fails until the exception and its reconstruction
//! mechanism are made explicit here.
//!
//! See `knowledge/decisions/c64-architecture-review.md` for the snapshot
//! boundary and the accuracy consequences of arbitrary-phase restore.

#[cfg(test)]
const C64_STACK_SOURCE_ROOTS: &[&str] = &[
    "crates/mos-6502/src",
    "crates/mos-vic-ii/src",
    "crates/mos-sid-6581/src",
    "crates/mos-cia-6526/src",
    "crates/common-commodore-c64/src",
    "crates/common-commodore-iec/src",
    "crates/machine-commodore-c64/src",
    "crates/machine-commodore-1541/src",
    "crates/machine-commodore-1571/src",
    "crates/machine-commodore-1581/src",
    "crates/runtime-commodore-c64/src",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("CARGO_MANIFEST_DIR should have a workspace ancestor")
            .to_path_buf()
    }

    fn collect_rust_sources(dir: &Path, sources: &mut Vec<PathBuf>) {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|err| panic!("audit cannot read {}: {err}", dir.display()));
        for entry in entries {
            let path = entry
                .expect("audit directory entry should be readable")
                .path();
            if path.is_dir() {
                collect_rust_sources(&path, sources);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                sources.push(path);
            }
        }
    }

    fn serde_skip_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
        source.lines().enumerate().filter_map(|(index, line)| {
            let trimmed = line.trim();
            let suffix = trimmed.strip_prefix("#[serde(skip");
            let is_skip = suffix
                .is_some_and(|suffix| matches!(suffix.bytes().next(), Some(b')' | b',' | b' ')))
                && trimmed
                    .find(']')
                    .is_some_and(|end| trimmed[end + 1..].trim().is_empty());
            is_skip.then_some((index + 1, trimmed))
        })
    }

    #[test]
    fn c64_runtime_stack_has_no_unreviewed_serde_skips() {
        let root = workspace_root();
        let mut sources = Vec::new();
        for relative in C64_STACK_SOURCE_ROOTS {
            collect_rust_sources(&root.join(relative), &mut sources);
        }
        sources.sort();

        let mut skips = Vec::new();
        for path in sources {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("audit cannot read {}: {err}", path.display()));
            for (line, annotation) in serde_skip_lines(&source) {
                let relative = path.strip_prefix(&root).unwrap_or(&path);
                skips.push(format!("  {}:{line}: {annotation}", relative.display()));
            }
        }

        assert!(
            skips.is_empty(),
            "\nUnreviewed #[serde(skip)] in the C64 runtime stack:\n{}\n\n\
             Preserve the field directly, or document its typed restore hook and\n\
             narrow this audit to that reviewed exception.",
            skips.join("\n")
        );
    }

    #[test]
    fn matcher_only_accepts_standalone_skip_attributes() {
        assert_eq!(serde_skip_lines("    #[serde(skip)]\n").count(), 1);
        assert_eq!(
            serde_skip_lines("#[serde(skip, default = \"foo\")]\n").count(),
            1
        );
        assert_eq!(
            serde_skip_lines("/// `#[serde(skip)]` keeps the field local.\n").count(),
            0
        );
        assert_eq!(serde_skip_lines("#[serde(skip_serializing)]\n").count(), 0);
        assert_eq!(
            serde_skip_lines("#[serde(skip)] inventory drift\n").count(),
            0
        );
    }
}
