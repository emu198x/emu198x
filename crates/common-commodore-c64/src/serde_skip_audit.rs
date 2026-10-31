//! Compile-locked audit of `#[serde(skip)]` fields in the C64 runtime stack.
//!
//! A skipped field is reset to its default value during deserialisation. That
//! is safe only for state with no observable future effect, or when a typed
//! restore hook reconstructs it from serialised state. The electrical colour
//! cache is the sole reviewed exception: `Vic::rebuild_signal_codes` recovers
//! it from the saved RGB framebuffer at `C64::restore_snapshot_state`, before
//! any output is exposed. See `knowledge/decisions/electrical-video-receiver.md`
//! for this exception.
//!
//! The 6502's `variant` is the second: it names the part fitted, NMOS or the
//! 65C12's CMOS, and is set by the constructor. A skipped field restores to its
//! default, `Variant::Nmos`, which is the part every CPU in this stack is (the
//! 6510 and the drives' 6502s), so a C64 restore cannot change it. Every other
//! field affecting chip execution is serialised.
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
    "crates/emu198x-mos-6502/src",
    "crates/emu198x-mos-vic-ii/src",
    "crates/emu198x-mos-sid-6581/src",
    "crates/emu198x-mos-cia-6526/src",
    "crates/common-commodore-c64/src",
    "crates/common-commodore-iec/src",
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

    /// The directory a `crates/<name>/src` entry names. While the crate lives
    /// in this workspace that is the in-tree copy. Once it has its own
    /// repository and comes from crates.io, it is Cargo's unpacked copy of the
    /// version `Cargo.lock` pins, which is the source this build compiled.
    fn source_root(root: &Path, relative: &str) -> PathBuf {
        let in_tree = root.join(relative);
        if in_tree.is_dir() {
            return in_tree;
        }
        let mut parts = relative.splitn(3, '/');
        let (Some("crates"), Some(name), Some(rest)) = (parts.next(), parts.next(), parts.next())
        else {
            panic!("audit path {relative} is not of the form crates/<name>/...");
        };
        let lock = std::fs::read_to_string(root.join("Cargo.lock"))
            .unwrap_or_else(|err| panic!("audit cannot read Cargo.lock: {err}"));
        let version = locked_version(&lock, name)
            .unwrap_or_else(|| panic!("{name} is neither in crates/ nor in Cargo.lock"));
        let cargo_home = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .map(|home| PathBuf::from(home).join(".cargo"))
            })
            .expect("audit needs CARGO_HOME or a home directory");
        let registry = cargo_home.join("registry").join("src");
        let unpacked = format!("{name}-{version}");
        std::fs::read_dir(&registry)
            .unwrap_or_else(|err| panic!("audit cannot read {}: {err}", registry.display()))
            .flatten()
            .map(|index| index.path().join(&unpacked).join(rest))
            .find(|dir| dir.is_dir())
            .unwrap_or_else(|| panic!("audit cannot find {unpacked} below {}", registry.display()))
    }

    /// The version `Cargo.lock` pins for the package `name`.
    fn locked_version<'a>(lock: &'a str, name: &str) -> Option<&'a str> {
        let wanted = format!("name = \"{name}\"");
        let mut lines = lock.lines();
        while let Some(line) = lines.next() {
            if line == wanted {
                return lines
                    .next()?
                    .strip_prefix("version = \"")?
                    .strip_suffix('"');
            }
        }
        None
    }

    #[test]
    fn locked_version_reads_the_line_after_the_name() {
        let lock = "[[package]]\nname = \"a-b\"\nversion = \"0.2.0\"\n\n\
                    [[package]]\nname = \"a\"\nversion = \"1.0.0\"\n";
        assert_eq!(locked_version(lock, "a"), Some("1.0.0"));
        assert_eq!(locked_version(lock, "a-b"), Some("0.2.0"));
        assert_eq!(locked_version(lock, "c"), None);
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

    fn is_reviewed_signal_cache(path: &Path, annotation: &str, next_line: Option<&str>) -> bool {
        path == Path::new("crates/emu198x-mos-vic-ii/src/lib.rs")
            && annotation == "#[serde(skip)]"
            && next_line.map(str::trim) == Some("signal_codes: Vec<u16>,")
    }

    /// The 6502's part selector, which restores to the NMOS part this stack
    /// fits (see the module documentation).
    fn is_reviewed_cpu_variant(path: &Path, annotation: &str, next_line: Option<&str>) -> bool {
        path == Path::new("crates/emu198x-mos-6502/src/lib.rs")
            && annotation == "#[serde(skip)]"
            && next_line.map(str::trim) == Some("pub(crate) variant: Variant,")
    }

    #[test]
    fn c64_runtime_stack_has_no_unreviewed_serde_skips() {
        let root = workspace_root();
        // Each source is paired with its `crates/<name>/src/...` label, so the
        // reviewed exceptions match wherever the crate is read from.
        let mut sources = Vec::new();
        for relative in C64_STACK_SOURCE_ROOTS {
            let dir = source_root(&root, relative);
            let mut files = Vec::new();
            collect_rust_sources(&dir, &mut files);
            for file in files {
                let below = file
                    .strip_prefix(&dir)
                    .expect("collected sources sit below their root");
                sources.push((Path::new(relative).join(below), file));
            }
        }
        sources.sort();

        let mut skips = Vec::new();
        let mut reviewed_caches = 0;
        let mut reviewed_variants = 0;
        for (label, path) in sources {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("audit cannot read {}: {err}", path.display()));
            for (line, annotation) in serde_skip_lines(&source) {
                let relative = label.as_path();
                let next_line = source.lines().nth(line);
                if is_reviewed_signal_cache(relative, annotation, next_line) {
                    reviewed_caches += 1;
                } else if is_reviewed_cpu_variant(relative, annotation, next_line) {
                    reviewed_variants += 1;
                } else {
                    skips.push(format!("  {}:{line}: {annotation}", relative.display()));
                }
            }
        }

        assert_eq!(
            reviewed_caches, 1,
            "the reviewed VIC cache must occur exactly once"
        );
        assert_eq!(
            reviewed_variants, 1,
            "the reviewed 6502 variant must occur exactly once"
        );
        assert!(
            skips.is_empty(),
            "\nUnreviewed #[serde(skip)] in the C64 runtime stack:\n{}\n\n\
             Preserve the field directly, or document its typed restore hook and\n\
             narrow this audit to that reviewed exception.",
            skips.join("\n")
        );
    }

    #[test]
    fn reviewed_exception_is_exactly_the_derived_vic_colour_cache() {
        let vic = Path::new("crates/emu198x-mos-vic-ii/src/lib.rs");
        assert!(is_reviewed_signal_cache(
            vic,
            "#[serde(skip)]",
            Some("    signal_codes: Vec<u16>,")
        ));
        assert!(!is_reviewed_signal_cache(
            vic,
            "#[serde(skip)]",
            Some("    framebuffer: Vec<u32>,")
        ));
        assert!(!is_reviewed_signal_cache(
            Path::new("crates/other/src/lib.rs"),
            "#[serde(skip)]",
            Some("signal_codes: Vec<u16>,")
        ));
        assert!(!is_reviewed_signal_cache(vic, "#[serde(skip)]", None));
        assert!(!is_reviewed_signal_cache(
            vic,
            "#[serde(skip, default)]",
            Some("signal_codes: Vec<u16>,")
        ));
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
