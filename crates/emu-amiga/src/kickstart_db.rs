//! Compiled-in SHA-1 database of known Kickstart ROMs.
//!
//! Scans a directory for ROM files, identifies them by hash, and picks the
//! best match for a given Amiga model.

use machine_amiga::AmigaModel;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};

/// A known Kickstart ROM entry.
struct KickstartEntry {
    /// SHA-1 hash as 40-char hex string.
    sha1: &'static str,
    /// Human-readable version string.
    version: &'static str,
    /// Description (model names, revision).
    description: &'static str,
    /// Models this ROM is compatible with.
    compatible_models: &'static [AmigaModel],
    /// Priority: higher wins when multiple ROMs match a model.
    /// Typical ROM for that model = 10, fallback = 5, beta = 1.
    priority: u8,
}

/// A scanned and identified ROM file.
#[allow(dead_code)]
pub struct ScannedRom {
    pub path: PathBuf,
    pub data: Vec<u8>,
    pub version: &'static str,
    pub description: &'static str,
    pub compatible_models: &'static [AmigaModel],
    pub priority: u8,
}

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

use AmigaModel::*;

const DB: &[KickstartEntry] = &[
    // KS 1.0 — A1000 only
    KickstartEntry {
        sha1: "00c15406beb4b8ab1a16aa66c05860e1a7c1ad79",
        version: "1.0",
        description: "KS 1.0, A1000",
        compatible_models: &[A1000],
        priority: 5,
    },
    // KS 1.2 r33.166 — A1000
    KickstartEntry {
        sha1: "6a7bfb5dbd6b8f179f03da84d8d9528267b6273b",
        version: "1.2",
        description: "KS 1.2 r33.166, A1000",
        compatible_models: &[A1000],
        priority: 8,
    },
    // KS 1.2 r33.180 — A500/A1000/A2000
    KickstartEntry {
        sha1: "11f9e62cf299f72184835b7b2a70a16333fc0d88",
        version: "1.2",
        description: "KS 1.2 r33.180, A500/A1000/A2000",
        compatible_models: &[A500, A1000, A2000],
        priority: 8,
    },
    // KS 1.3 r34.5 — A500/A1000/A2000/CDTV
    KickstartEntry {
        sha1: "891e9a547772fe0c6c19b610baf8bc4ea7fcb785",
        version: "1.3",
        description: "KS 1.3 r34.5, A500/A1000/A2000",
        compatible_models: &[A500, A1000, A2000],
        priority: 10,
    },
    // KS 1.3 r34.5 — A3000
    KickstartEntry {
        sha1: "c39bd9094d4e5f4e28c1411f3086950406062e87",
        version: "1.3",
        description: "KS 1.3 r34.5, A3000",
        compatible_models: &[A3000],
        priority: 5,
    },
    // KS 2.0 beta — A3000
    KickstartEntry {
        sha1: "4fcf8f55c27465f3c00b034a61b2680799ebc27b",
        version: "2.0 beta",
        description: "KS 2.0 r36.028, A3000 beta",
        compatible_models: &[A3000],
        priority: 1,
    },
    // KS 2.02 r36.207 — A3000
    KickstartEntry {
        sha1: "f2cc0cc8bf9321df63456c439884c15cda0fe752",
        version: "2.02",
        description: "KS 2.02 r36.207, A3000",
        compatible_models: &[A3000],
        priority: 8,
    },
    // KS 2.04 r37.175 — A500+
    KickstartEntry {
        sha1: "c5839f5cb98a7a8947065c3ed2f14f5f42e334a1",
        version: "2.04",
        description: "KS 2.04 r37.175, A500+",
        compatible_models: &[A500Plus],
        priority: 10,
    },
    // KS 2.05 r37.300 — A600
    KickstartEntry {
        sha1: "f72d89148dac39c696e30b10859ebc859226637b",
        version: "2.05",
        description: "KS 2.05 r37.300, A600",
        compatible_models: &[A600],
        priority: 10,
    },
    // KS 2.05 r37.350 — A600
    KickstartEntry {
        sha1: "02843c4253bbd29aba535b0aa3bd9a85034ecde4",
        version: "2.05",
        description: "KS 2.05 r37.350, A600",
        compatible_models: &[A600],
        priority: 9,
    },
    // KS 3.0 beta — A600
    KickstartEntry {
        sha1: "68e14fc9df3272da6aacec8df2408d346f82c9e3",
        version: "3.0 beta",
        description: "KS 3.0 r39.092, A600 beta",
        compatible_models: &[A600],
        priority: 1,
    },
    // KS 3.0 r39.106 — A1200
    KickstartEntry {
        sha1: "70033828182fffc7ed106e5373a8b89dda76faa5",
        version: "3.0",
        description: "KS 3.0 r39.106, A1200",
        compatible_models: &[A1200],
        priority: 8,
    },
    // KS 3.0 r39.106 — A4000
    KickstartEntry {
        sha1: "f0b4e9e29e12218c2d5bd7020e4e785297d91fd7",
        version: "3.0",
        description: "KS 3.0 r39.106, A4000",
        compatible_models: &[A4000],
        priority: 8,
    },
    // KS 3.1 r40.63 — A500/A500+/A600/A2000
    KickstartEntry {
        sha1: "3b7f1493b27e212830f989f26ca76c02049f09ca",
        version: "3.1",
        description: "KS 3.1 r40.63, A500/A500+/A600/A2000",
        compatible_models: &[A500, A500Plus, A600, A2000],
        priority: 10,
    },
    // KS 3.1 r40.68 — A1200
    KickstartEntry {
        sha1: "e21545723fe8374e91342617604f1b3d703094f1",
        version: "3.1",
        description: "KS 3.1 r40.68, A1200",
        compatible_models: &[A1200],
        priority: 10,
    },
    // KS 3.1 r40.68 — A3000
    KickstartEntry {
        sha1: "f8e210d72b4c4853e0c9b85d223ba20e3d1b36ee",
        version: "3.1",
        description: "KS 3.1 r40.68, A3000",
        compatible_models: &[A3000],
        priority: 10,
    },
    // KS 3.1 r40.68 — A4000
    KickstartEntry {
        sha1: "5fe04842d04a489720f0f4bb0e46948199406f49",
        version: "3.1",
        description: "KS 3.1 r40.68, A4000",
        compatible_models: &[A4000],
        priority: 10,
    },
    // KS 3.1 r40.68 — A600 beta
    KickstartEntry {
        sha1: "b4c5cb8620c86a93dceeb543866ea6ab005a3d41",
        version: "3.1 beta",
        description: "KS 3.1 r40.68, A600 beta",
        compatible_models: &[A600],
        priority: 1,
    },
    // KS 3.1 r40.70 — A4000 beta
    KickstartEntry {
        sha1: "81c631dd096bbb31d2af90299c76b774db74076c",
        version: "3.1 beta",
        description: "KS 3.1 r40.70, A4000 beta",
        compatible_models: &[A4000],
        priority: 1,
    },
    // KS 3.1 r40.70 — A4000T
    KickstartEntry {
        sha1: "b0ec8b84d6768321e01209f11e6248f2f5281a21",
        version: "3.1",
        description: "KS 3.1 r40.70, A4000T",
        compatible_models: &[A4000],
        priority: 9,
    },
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Scan a directory for Kickstart ROM files, hashing each and matching
/// against the compiled-in database.
pub fn scan_roms(dir: &Path) -> Vec<ScannedRom> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        // Only consider .rom files.
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ext.eq_ignore_ascii_case("rom") {
            continue;
        }

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Kickstart ROMs are 256 KB or 512 KB.
        if data.len() != 256 * 1024 && data.len() != 512 * 1024 {
            continue;
        }

        let hash = hex_sha1(&data);

        if let Some(entry) = DB.iter().find(|e| e.sha1 == hash) {
            eprintln!(
                "  Found {} — {} ({})",
                path.display(),
                entry.description,
                entry.version
            );
            results.push(ScannedRom {
                path,
                data,
                version: entry.version,
                description: entry.description,
                compatible_models: entry.compatible_models,
                priority: entry.priority,
            });
        }
    }

    results
}

/// Pick the best ROM for a given model from the scanned set.
/// Returns the ROM with the highest priority that lists the model
/// in its `compatible_models`.
pub fn best_rom_for_model<'a>(
    scanned: &'a [ScannedRom],
    model: AmigaModel,
) -> Option<&'a ScannedRom> {
    scanned
        .iter()
        .filter(|r| r.compatible_models.contains(&model))
        .max_by_key(|r| r.priority)
}

fn hex_sha1(data: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hex = String::with_capacity(40);
    for byte in result {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_sha1_produces_40_char_lowercase_hex() {
        let hash = hex_sha1(b"hello");
        assert_eq!(hash.len(), 40);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        // Known SHA-1 of "hello"
        assert_eq!(hash, "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d");
    }

    #[test]
    fn best_rom_prefers_higher_priority() {
        let roms = vec![
            ScannedRom {
                path: PathBuf::from("low.rom"),
                data: vec![],
                version: "1.2",
                description: "low priority",
                compatible_models: &[A500],
                priority: 5,
            },
            ScannedRom {
                path: PathBuf::from("high.rom"),
                data: vec![],
                version: "3.1",
                description: "high priority",
                compatible_models: &[A500],
                priority: 10,
            },
        ];
        let best = best_rom_for_model(&roms, A500).unwrap();
        assert_eq!(best.path, PathBuf::from("high.rom"));
    }

    #[test]
    fn best_rom_returns_none_for_unmatched_model() {
        let roms = vec![ScannedRom {
            path: PathBuf::from("a500.rom"),
            data: vec![],
            version: "1.3",
            description: "A500 only",
            compatible_models: &[A500],
            priority: 10,
        }];
        assert!(best_rom_for_model(&roms, A1200).is_none());
    }
}
