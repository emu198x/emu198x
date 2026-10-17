//! Quick-slot I/O without a window, so failures can reach native UI feedback.

use std::path::Path;

use emu198x_shell::MachineError;

/// Read and restore a quick-slot, returning an explanation for the native dialog.
/// The runtime remains responsible for rejecting invalid snapshots atomically.
pub(crate) fn restore(
    path: Option<&Path>,
    apply: impl FnOnce(&[u8]) -> Result<(), MachineError>,
) -> Result<(), String> {
    let Some(path) = path else {
        eprintln!("load-state: no state directory (set EMU198X_STATE_DIR or HOME)");
        return Err("No save folder is available for this session.".to_owned());
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("load-state: cannot read {}: {err}", path.display());
            return Err(if err.kind() == std::io::ErrorKind::NotFound {
                "No saved state for this machine.\nSave a state first, then try again.".to_owned()
            } else {
                format!(
                    "The saved state could not be read.\n{err}\nCheck access to the save folder and try again."
                )
            });
        }
    };
    apply(&bytes).map_err(|err| {
        eprintln!(
            "load-state: {} could not be restored: {err}",
            path.display()
        );
        format!(
            "The saved state could not be loaded.\n{err}\nTry a valid saved state for this machine."
        )
    })
}

#[cfg(test)]
mod tests {
    use super::restore;
    use emu198x_shell::MachineError;
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Folder(PathBuf);

    impl Folder {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "emu198x-quick-load-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).expect("create isolated test folder");
            Self(path)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_folder_reports_failure_without_calling_restore() {
        let called = Cell::new(false);
        let result = restore(None, |_| {
            called.set(true);
            Ok(())
        });
        assert!(result.is_err());
        assert!(!called.get());
    }

    #[test]
    fn missing_slot_explains_how_to_recover_without_calling_restore() {
        let folder = Folder::new();
        let called = Cell::new(false);
        let result = restore(Some(&folder.0.join("absent.state")), |_| {
            called.set(true);
            Ok(())
        });
        let explanation = result.expect_err("missing slot must report a failure");
        assert!(explanation.contains("Save a state first"));
        assert!(!called.get());
    }

    #[test]
    fn read_failure_is_not_misreported_as_a_missing_save() {
        let folder = Folder::new();
        let called = Cell::new(false);
        let result = restore(Some(&folder.0), |_| {
            called.set(true);
            Ok(())
        });
        let explanation = result.expect_err("a directory is not a save file");
        assert!(explanation.contains("could not be read"));
        assert!(!explanation.contains("No saved state"));
        assert!(!called.get());
    }

    #[test]
    fn invalid_file_can_be_followed_by_a_valid_restore() {
        let folder = Folder::new();
        let slot = folder.0.join("machine.state");
        let current = Cell::new(23_u8);
        let apply = |bytes: &[u8]| {
            let [0xA5, value] = bytes else {
                return Err(MachineError::InvalidSnapshot {
                    reason: "test snapshot is truncated".to_owned(),
                });
            };
            current.set(*value);
            Ok(())
        };
        std::fs::write(&slot, [0xA5]).expect("write malformed fixture");
        let explanation = restore(Some(&slot), apply).expect_err("invalid file must fail");
        assert!(explanation.contains("could not be loaded"));
        assert_eq!(current.get(), 23);

        std::fs::write(&slot, [0xA5, 81]).expect("write valid fixture");
        restore(Some(&slot), apply).expect("a valid file still restores");
        assert_eq!(current.get(), 81);
        assert_eq!(
            std::fs::read(&slot).expect("read unchanged input"),
            [0xA5, 81]
        );
    }
}
