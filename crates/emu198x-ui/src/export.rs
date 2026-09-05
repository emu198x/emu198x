//! Host exports always create a new file, never overwrite preserved media.
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

pub(crate) fn write_new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // Atomic creation also rejects existing symlinks and hard links, regardless
    // of whether the source was loaded through the UI, CLI or a saved session.
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_preserves_existing_files_and_reports_bad_destinations() {
        let directory = std::env::temp_dir().join(format!("emu198x-export-{}", std::process::id()));
        std::fs::create_dir(&directory).expect("temporary directory");
        let path = directory.join("recording.tap");
        let result = std::panic::catch_unwind(|| {
            write_new_file(&path, b"first recording").expect("new export");
            assert_eq!(
                std::fs::read(&path).expect("read export"),
                b"first recording"
            );
            assert_eq!(
                write_new_file(&path, b"replacement")
                    .expect_err("existing destination")
                    .kind(),
                io::ErrorKind::AlreadyExists
            );
            assert_eq!(
                std::fs::read(&path).expect("read preserved export"),
                b"first recording"
            );
            assert!(write_new_file(&directory.join("missing/recording.tap"), b"data").is_err());
        });
        std::fs::remove_dir_all(directory).expect("remove temporary directory");
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}
