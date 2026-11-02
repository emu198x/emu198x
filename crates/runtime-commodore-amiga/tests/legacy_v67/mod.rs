//! Immutable original v67 storage; decompression never rewrites a specimen.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;
use zip::ZipArchive;
const ORIGINAL_HEAD: &str = "c06321677cde6b4aea0211197eb8436ceae0a1a7";

#[derive(Deserialize)]
struct Identity {
    bytes: usize,
    sha256: String,
}
#[derive(Deserialize)]
struct Manifest {
    baseline: String,
    cases: usize,
    archive_sha256: String,
    files: BTreeMap<String, Identity>,
}
pub struct Originals {
    archive: ZipArchive<Cursor<Vec<u8>>>,
    manifest: Manifest,
}
fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut hex = String::with_capacity(64);
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    for byte in digest {
        write!(&mut hex, "{byte:02x}").expect("write SHA-256 to a String");
    }
    hex
}
impl Originals {
    pub fn open(root: &Path) -> Self {
        let manifest: Manifest = serde_json::from_slice(
            &std::fs::read(root.join("manifest.json")).expect("original manifest"),
        )
        .expect("manifest fields");
        assert_eq!(manifest.baseline, ORIGINAL_HEAD);
        assert_eq!(manifest.cases, 56);
        assert_eq!(manifest.files.len(), 177);
        let bytes =
            std::fs::read(root.join("original-v67.zip")).expect("immutable original archive");
        assert_eq!(hex_digest(&bytes), manifest.archive_sha256);
        let archive = ZipArchive::new(Cursor::new(bytes)).expect("original ZIP");
        assert_eq!(archive.len(), manifest.files.len());
        Self { archive, manifest }
    }
    pub fn read(&mut self, name: &str) -> Vec<u8> {
        let identity = self
            .manifest
            .files
            .get(name)
            .expect("named immutable original");
        let mut file = self.archive.by_name(name).expect("original archive member");
        assert_eq!(file.size(), identity.bytes as u64);
        let mut bytes = Vec::with_capacity(identity.bytes);
        file.read_to_end(&mut bytes)
            .expect("complete original bytes");
        assert_eq!(bytes.len(), identity.bytes);
        assert_eq!(
            hex_digest(&bytes),
            identity.sha256,
            "{name}: immutable identity"
        );
        bytes
    }
}
