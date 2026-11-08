//! Isolated process environments for firmware discovery tests.
use std::cell::Cell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CASE: &str = "EMU198X_SHELL_TEST_CASE";
const ROOT: &str = "EMU198X_SHELL_TEST_ROOT";

pub(crate) struct Environment {
    test: &'static str,
    executed: Cell<bool>,
    pub(crate) root: PathBuf,
}

impl Environment {
    pub(crate) fn new(test: &'static str) -> Self {
        let root = std::env::var_os(ROOT).map_or_else(
            || {
                std::env::temp_dir()
                    .join(format!("emu198x-home-{}-{test}", std::process::id()).replace("::", "-"))
            },
            PathBuf::from,
        );
        std::fs::create_dir_all(&root).expect("test root");
        Self {
            test,
            root,
            executed: Cell::new(false),
        }
    }

    pub(crate) fn case(
        &self,
        name: &str,
        variables: &[(&str, OsString)],
        check: impl FnOnce(&Path),
    ) {
        let marker = format!("{}:{name}", self.test);
        if let Ok(selected) = std::env::var(CASE) {
            if selected == marker {
                self.executed.set(true);
                check(&self.root);
                println!("verified {marker}");
            }
            return;
        }
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args(["--exact", self.test, "--nocapture"])
            .current_dir(&self.root)
            .env(CASE, &marker)
            .env(ROOT, &self.root)
            .env_remove("HOME")
            .env_remove("USERPROFILE")
            .env_remove("EMU198X_LAUNCH_TEST_ROM_THAT_NOBODY_SETS")
            .env_remove("EMU198X_FAM_TEST_ROM_DIR_THAT_NOBODY_SETS")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in variables {
            command.env(key, value);
        }
        let mut child = command.spawn().expect("isolated test child");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if child.try_wait().expect("child status").is_some() {
                break;
            }
            if Instant::now() >= deadline {
                child.kill().expect("kill timed out child");
                child.wait().expect("reap child");
                panic!("firmware environment case timed out: {marker}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().expect("child output");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{marker}: {stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains(&format!("verified {marker}")) && stdout.contains("1 passed;"),
            "case did not execute: {stdout}"
        );
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        if std::env::var_os(CASE).is_none() {
            std::fs::remove_dir_all(&self.root).expect("remove test root");
        } else {
            assert!(
                self.executed.get(),
                "selected firmware environment case never executed"
            );
        }
    }
}
