//! Compile-locked inventory of every `#[serde(skip)]` field in the
//! Spectrum chip + class crates.
//!
//! Every `#[serde(skip)]` annotation breaks the serde graph for that
//! field — the field is reset to `Default::default()` (or the named
//! default function) on deserialise, not the value held before
//! snapshot. That's only acceptable when either:
//!
//! 1. The default produces correct behaviour (transient state that
//!    drains within one tick / one frame), or
//! 2. The field is rehydrated by a typed `after_restore` hook the
//!    runtime layer calls right after deserialise.
//!
//! Both cases are fine; what's not fine is *adding a new
//! `#[serde(skip)]` without consciously picking one*. The audit
//! below counts annotations per source file and locks the totals.
//! Any change forces an explicit decision and an update to the
//! `EXPECTED_SERDE_SKIPS` table — at which point the author is
//! looking at this comment and choosing 1 or 2 deliberately.
//!
//! See Seam 3 of `knowledge/decisions/spectrum-architecture-review.md`
//! for the rationale and full per-field justification trail.

/// One audit entry. `expected` is the count of `#[serde(skip)]`
/// annotation lines (matching `^\s*#\[serde\(skip`) in `path`. The
/// `justification` is a short prose blurb pointing at the
/// rehydration mechanism so a reader can trace the decision without
/// leaving this file.
#[cfg(test)]
struct SerdeSkipAudit {
    /// Path from the workspace root.
    path: &'static str,
    /// Expected number of `#[serde(skip)]` annotation lines.
    expected: usize,
    /// Why each one is safe — a short pointer to the rehydration
    /// mechanism. Plural-friendly even when there's only one. Read
    /// by humans reviewing the audit, not by the assertion — that's
    /// why the `#[allow(dead_code)]`.
    #[allow(dead_code)]
    justification: &'static str,
}

#[cfg(test)]
const EXPECTED_SERDE_SKIPS: &[SerdeSkipAudit] = &[
    SerdeSkipAudit {
        path: "crates/emu198x-sinclair-zx-spectrum-ula/src/lib.rs",
        expected: 8,
        justification: "Two-stage shifter pipeline + AOLatch border + \
                        config + M-cycle fall counter: pipeline state drains \
                        within one 16-pixel cycle; AOLatch within one \
                        character cell; config is `&'static`, reattached by \
                        `reattach_config()`. `mcycle_fall`/`prev_fall_addr` \
                        default to a shut counter and rebuild on the first \
                        address change, which is the next M-cycle — a \
                        restored state resumes at an instruction boundary, so \
                        there is no I/O M-cycle in flight to be mid-count.",
    },
    SerdeSkipAudit {
        path: "crates/emu198x-nec-upd765a/src/lib.rs",
        expected: 0,
        justification: "Spectrum snapshot v5 carries live mounted disks, \
                        protection and in-flight writes in the FDC. Replaying \
                        original cached source bytes would discard writes. \
                        Keep this zero-count entry to reject any future skip; \
                        see the +3 amendment in savestate-live-machine-serde.md.",
    },
    SerdeSkipAudit {
        path: "crates/emu198x-zilog-z80/src/walker.rs",
        expected: 1,
        justification: "Walker sequence is `&'static`. Rehydrated by \
                        `Z80::rehydrate_walker_sequence` from the preserved \
                        sequence identity and opcode.",
    },
    SerdeSkipAudit {
        path: "crates/common-sinclair-zx-spectrum-48k-class/src/core.rs",
        expected: 2,
        justification: "An `IoTrace` capture buffer. Case 1: the default is \
                        not-tracing, which is the honest state for a \
                        machine restored from a snapshot — a debugger \
                        buffer is host-side and belongs to the session \
                        that opened it, not to the machine (#1183). Also: `PhantomData<V>` is a ZST — the default IS the \
                        full state; serde skip has no observable effect.",
    },
    SerdeSkipAudit {
        path: "crates/common-sinclair-zx-spectrum-128k-class/src/core.rs",
        expected: 3,
        justification: "An `IoTrace` capture buffer. Case 1: the default is \
                        not-tracing, which is the honest state for a \
                        machine restored from a snapshot — a debugger \
                        buffer is host-side and belongs to the session \
                        that opened it, not to the machine (#1183). Also: `PhantomData<V>` (ZST, no state) + `&'static \
                        UlaConfig` (reattached by `restore_volatile_refs`).",
    },
    SerdeSkipAudit {
        path: "crates/common-sinclair-zx-spectrum-amstrad-class/src/core.rs",
        expected: 3,
        justification: "An `IoTrace` capture buffer. Case 1: the default is \
                        not-tracing, which is the honest state for a \
                        machine restored from a snapshot — a debugger \
                        buffer is host-side and belongs to the session \
                        that opened it, not to the machine (#1183). Also: `PhantomData<V>` (ZST, no state) + `&'static \
                        UlaConfig` (reattached by `restore_volatile_refs`).",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// Workspace root, computed from this crate's manifest dir. The
    /// audit grep needs to read files across the workspace; the test
    /// only runs in-tree, never from a published artifact, so this
    /// is safe.
    fn workspace_root() -> PathBuf {
        // CARGO_MANIFEST_DIR = .../crates/common-sinclair-zx-spectrum
        // Workspace root = parent of `crates/`.
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

    /// Counts `#[serde(skip)]` and `#[serde(skip, …)]` annotations in
    /// `source` — matches the regex `^\s*#\[serde\(skip[\)\,\s]` with
    /// a simple state machine (no regex crate dep). The audit only
    /// cares about top-level annotations; doc-comment mentions don't
    /// count.
    fn count_serde_skip_annotations(source: &str) -> usize {
        source
            .lines()
            .map(str::trim_start)
            .filter(|line| {
                if !line.starts_with("#[serde(skip") {
                    return false;
                }
                // Reject `#[serde(skip_serializing)]` etc. — must be
                // followed by `)`, `,`, or whitespace.
                let suffix = &line["#[serde(skip".len()..];
                matches!(suffix.bytes().next(), Some(b')' | b',' | b' '))
            })
            .count()
    }

    #[test]
    fn every_serde_skip_in_spectrum_stack_is_locked() {
        let root = workspace_root();
        let mut mismatches = Vec::new();

        for audit in EXPECTED_SERDE_SKIPS {
            let (dir, file) = audit
                .path
                .split_once("/src/")
                .expect("audit paths name a file below a crate's src/");
            let path = source_root(&root, &format!("{dir}/src")).join(file);
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("audit cannot read {}: {e}", path.display()));
            let actual = count_serde_skip_annotations(&source);
            if actual != audit.expected {
                mismatches.push(format!(
                    "  {} : expected {} #[serde(skip)] but found {}",
                    audit.path, audit.expected, actual
                ));
            }
        }

        assert!(
            mismatches.is_empty(),
            "\n\
             #[serde(skip)] inventory drift detected:\n\
             {}\n\n\
             A new #[serde(skip)] on a Spectrum-stack struct is a deliberate\n\
             decision: either the default value produces correct behaviour, or\n\
             a typed `after_restore` rehydrates the field. Update the entry in\n\
             crates/common-sinclair-zx-spectrum/src/serde_skip_audit.rs and\n\
             document the chosen mechanism in its `justification`. See Seam 3\n\
             of knowledge/decisions/spectrum-architecture-review.md.",
            mismatches.join("\n")
        );
    }

    /// Sanity check on the line-matcher — the regex-free state
    /// machine is small enough to test directly. Catches accidental
    /// regressions where the matcher starts under- or over-counting.
    #[test]
    fn matcher_only_counts_annotation_lines() {
        // True positives.
        assert_eq!(count_serde_skip_annotations("    #[serde(skip)]\n"), 1);
        assert_eq!(
            count_serde_skip_annotations("#[serde(skip, default = \"foo\")]\n"),
            1
        );
        assert_eq!(
            count_serde_skip_annotations(
                "    #[serde(skip)]\n    pub a: u8,\n    #[serde(skip)]\n    pub b: u8,\n"
            ),
            2
        );

        // False positives the audit must reject.
        // Doc comment mentioning the annotation in prose.
        assert_eq!(
            count_serde_skip_annotations("/// `#[serde(skip)]` keeps the field local.\n"),
            0
        );
        // Different serde directive that starts with "skip".
        assert_eq!(
            count_serde_skip_annotations("#[serde(skip_serializing)]\n"),
            0
        );
        // No serde at all.
        assert_eq!(count_serde_skip_annotations("pub a: u8,\n"), 0);
    }
}
