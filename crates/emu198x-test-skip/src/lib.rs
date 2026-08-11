//! Declare a test skipped for a missing fixture, so it cannot pass silently.
//!
//! ## The problem this exists for
//!
//! A test that needs a ROM, a corpus or a disk image typically guards on it
//! and returns early:
//!
//! ```ignore
//! let Some(session) = booted_dragon_session() else {
//!     return;
//! };
//! ```
//!
//! `libtest` then prints `ok`. Nothing distinguishes that from a test that
//! ran and passed. There are ~396 such guards in this workspace, and the
//! consequence is not hypothetical: the Dragon golden-frame test compared
//! encoded PNG bytes rather than pixels and broke when `png` 0.17 -> 0.18
//! changed its deflate settings on 2026-05-18. CI reported it `ok` for
//! nearly three months, because CI has no Dragon ROM and the test returned
//! early. It only failed on a machine that had the ROM.
//!
//! Counting skips is not enough on its own — a number in a log is the same
//! class of instrument that already went unread four times. What makes the
//! difference is that a skip is an *error* wherever the fixture is supposed
//! to exist.
//!
//! ## Use
//!
//! ```ignore
//! let Some(session) = booted_dragon_session() else {
//!     emu198x_test_skip::skip!("Dragon 32 ROM not staged (EMU198X_DRAGON32_ROM)");
//! };
//! ```
//!
//! ## Environment
//!
//! - `EMU198X_STRICT_FIXTURES` — when set and non-empty, a skip **panics**.
//!   Set it where the fixtures are supposed to be present: a development
//!   machine, and the nightly accuracy run that provisions them. A test
//!   that quietly stopped running then fails on the day it stops, not
//!   whenever someone next looks.
//! - `EMU198X_SKIP_LOG` — a path to append one line per skip to. `libtest`
//!   captures stdout and stderr for passing tests, which is why grepping a
//!   CI log for "skipping" returns nothing despite 158 tests printing it.
//!   A file survives that capture. CI sets this and publishes the tally.
//!
//! With neither set, a skip only writes to stderr, visible under
//! `--nocapture` or on failure. That is the local-developer default and
//! costs nothing.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::PathBuf;

/// Marker prefix on the stderr line. Grep-stable — do not reword it
/// without updating `.github/workflows/ci.yml`.
pub const MARKER: &str = "EMU198X-SKIP";

const STRICT_VAR: &str = "EMU198X_STRICT_FIXTURES";
const LOG_VAR: &str = "EMU198X_SKIP_LOG";

/// Declare the current test skipped and return from it.
///
/// Takes `format!` arguments. Say which fixture is missing and name the
/// environment variable that would supply it — the reason is what someone
/// reads when the tally says a system stopped being tested.
#[macro_export]
macro_rules! skip {
    ($($arg:tt)*) => {{
        $crate::record(&::std::format!($($arg)*));
        return;
    }};
}

/// What to do about a skip. Resolved from the environment once per call,
/// but kept separate from it so the behaviour is testable without mutating
/// process-global state — this workspace forbids `unsafe`, and
/// `std::env::set_var` is `unsafe` in edition 2024.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    /// Treat a skip as a failure.
    pub strict: bool,
    /// Append one line per skip here.
    pub log: Option<PathBuf>,
}

impl Policy {
    /// Read the policy from the two environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_vars(std::env::var_os(STRICT_VAR), std::env::var_os(LOG_VAR))
    }

    /// The pure half of [`Policy::from_env`].
    ///
    /// A set-but-empty variable means off. `FOO=` in a shell is set, and
    /// reading that as "on" would turn an unset-looking variable into a
    /// workspace-wide failure.
    #[must_use]
    pub fn from_vars(strict: Option<OsString>, log: Option<OsString>) -> Self {
        Self {
            strict: strict.is_some_and(|v| !v.is_empty()),
            log: log.filter(|v| !v.is_empty()).map(PathBuf::from),
        }
    }
}

/// Record a skip under the ambient policy. Prefer the [`skip!`] macro,
/// which also returns from the test.
///
/// # Panics
///
/// When `EMU198X_STRICT_FIXTURES` is set and non-empty. That is the point:
/// in an environment that claims to have the fixtures, a missing one is a
/// provisioning failure, not a reason to report success.
pub fn record(reason: &str) {
    record_under(&Policy::from_env(), reason);
}

/// Record a skip under an explicit policy.
///
/// # Panics
///
/// When `policy.strict` is set.
pub fn record_under(policy: &Policy, reason: &str) {
    let test = std::thread::current()
        .name()
        .unwrap_or("<unnamed test>")
        .to_owned();

    assert!(
        !policy.strict,
        "{MARKER}: {test} skipped for a missing fixture under \
         {STRICT_VAR}: {reason}"
    );

    eprintln!("{MARKER}: {test}: {reason}");

    let Some(path) = policy.log.as_ref() else {
        return;
    };

    // One `O_APPEND` write of one line. Test binaries run in parallel
    // processes, and short appends do not interleave; a lock file would
    // buy nothing and could deadlock a test run.
    let mut line = String::new();
    let _ = writeln!(line, "{test}\t{reason}");
    if let Ok(mut file) = OpenOptions::new().append(true).create(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_log(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("emu198x-skip-{}-{tag}.tsv", std::process::id()))
    }

    #[test]
    fn an_empty_variable_does_not_arm_strict_mode() {
        assert_eq!(
            Policy::from_vars(Some(OsString::new()), Some(OsString::new())),
            Policy {
                strict: false,
                log: None
            },
        );
    }

    #[test]
    fn a_set_variable_arms_strict_mode_and_the_log() {
        assert_eq!(
            Policy::from_vars(Some("1".into()), Some("/tmp/skips.tsv".into())),
            Policy {
                strict: true,
                log: Some(PathBuf::from("/tmp/skips.tsv")),
            },
        );
    }

    #[test]
    fn a_skip_is_quiet_by_default() {
        record_under(&Policy::default(), "no fixture");
    }

    #[test]
    fn strict_mode_turns_a_skip_into_a_failure() {
        let policy = Policy {
            strict: true,
            log: None,
        };
        let outcome = std::panic::catch_unwind(|| record_under(&policy, "Dragon ROM not staged"));
        let err = outcome.expect_err("a skip under strict mode must fail the test");
        let message = err
            .downcast_ref::<String>()
            .map(String::as_str)
            .unwrap_or_default();
        assert!(
            message.contains("Dragon ROM not staged"),
            "the panic must carry the reason, got {message:?}",
        );
    }

    #[test]
    fn skips_are_appended_to_the_log_with_their_reasons() {
        let path = temp_log("append");
        let _ = std::fs::remove_file(&path);
        let policy = Policy {
            strict: false,
            log: Some(path.clone()),
        };

        record_under(&policy, "Dragon ROM not staged");
        record_under(&policy, "Kickstart 1.3 ROM missing");

        let logged = std::fs::read_to_string(&path).expect("skip log should have been written");
        let lines: Vec<_> = logged.lines().collect();
        assert_eq!(lines.len(), 2, "one line per skip, appended not truncated");
        assert!(
            lines[0].ends_with("Dragon ROM not staged"),
            "first line should carry the first reason, got {:?}",
            lines[0],
        );
        assert!(
            lines[1].ends_with("Kickstart 1.3 ROM missing"),
            "second line should carry the second reason, got {:?}",
            lines[1],
        );
        assert!(
            lines[0].contains('\t'),
            "each line should be <test>\\t<reason>, got {:?}",
            lines[0],
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_unwritable_log_path_does_not_fail_the_test() {
        // Diagnostics must never be the reason a suite goes red.
        let policy = Policy {
            strict: false,
            log: Some(PathBuf::from("/nonexistent-directory/skips.tsv")),
        };
        record_under(&policy, "no fixture");
    }
}
