//! Read selected media and return actionable failures to the native UI.

use std::path::Path;

use emu198x_shell::{MachineError, MediaImage, MediaKind, MediaSet, read_media_asset};

pub(crate) fn load(
    path: &Path,
    slot: &str,
    kind: MediaKind,
    apply: impl FnOnce(&MediaSet<'_>) -> Result<(), MachineError>,
) -> Result<(), String> {
    let loaded = read_media_asset(path, kind).map_err(|err| {
        eprintln!("media: cannot read {}: {err}", path.display());
        format!(
            "The selected file could not be read.\n{err}\nCheck access to the file, or choose another media file."
        )
    })?;
    let mut set = MediaSet::new();
    set.push(MediaImage::new(slot.to_owned(), kind, &loaded.bytes));
    apply(&set).map_err(|err| {
        eprintln!("media: {} rejected by the machine: {err}", path.display());
        format!(
            "The selected media could not be loaded.\n{err}\nChoose a valid media file for this machine."
        )
    })
}

#[cfg(test)]
mod tests {
    use super::load;
    use emu198x_shell::{MachineError, MediaKind};
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Folder(PathBuf);

    impl Folder {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "emu198x-media-open-{}-{}",
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
    fn missing_file_reports_failure_without_calling_runtime() {
        let folder = Folder::new();
        let called = Cell::new(false);
        let result = load(
            &folder.0.join("absent.tap"),
            "tape-1",
            MediaKind::Tape,
            |_| {
                called.set(true);
                Ok(())
            },
        );
        let message = result.expect_err("missing input must fail");
        assert!(message.contains("Check access"));
        assert!(!called.get());
    }

    #[test]
    fn damaged_archive_is_not_passed_to_runtime() {
        let folder = Folder::new();
        let path = folder.0.join("damaged.zip");
        std::fs::write(&path, b"not a zip archive").expect("write damaged archive");
        let called = Cell::new(false);
        let result = load(&path, "tape-1", MediaKind::Tape, |_| {
            called.set(true);
            Ok(())
        });
        assert!(result.is_err());
        assert!(!called.get());
        assert_eq!(
            std::fs::read(&path).expect("read unchanged archive"),
            b"not a zip archive"
        );
    }

    #[test]
    fn runtime_rejection_explains_recovery_and_allows_a_later_load() {
        let folder = Folder::new();
        let path = folder.0.join("selected.tap");
        let bytes = [19, 0, 0, 0, 100];
        std::fs::write(&path, bytes).expect("write selected media");
        let message = load(&path, "tape-1", MediaKind::Tape, |set| {
            assert_eq!(set.images.len(), 1);
            assert_eq!(set.images[0].bytes, bytes);
            Err(MachineError::InvalidMedia {
                slot: "tape-1".to_owned(),
                reason: "TAP block at offset 0 claims 19 bytes but only 3 remain".to_owned(),
            })
        })
        .expect_err("runtime rejection must reach the user");
        assert!(message.contains("claims 19 bytes but only 3 remain"));
        assert!(message.contains("Choose a valid media file"));
        assert_eq!(std::fs::read(&path).expect("read rejected media"), bytes);

        let valid = [2, 0, 255, 255];
        std::fs::write(&path, valid).expect("write replacement media");
        let called = Cell::new(false);
        load(&path, "tape-1", MediaKind::Tape, |set| {
            assert_eq!(set.images.len(), 1);
            let image = &set.images[0];
            assert_eq!(image.slot, "tape-1");
            assert_eq!(image.kind, MediaKind::Tape);
            assert_eq!(image.bytes, valid);
            assert!(!image.writable);
            called.set(true);
            Ok(())
        })
        .expect("later valid load must still reach runtime");
        assert!(called.get());
        assert_eq!(std::fs::read(&path).expect("read accepted media"), valid);
    }
}
