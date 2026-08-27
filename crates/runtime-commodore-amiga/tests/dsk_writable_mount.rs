//! #97 — `insert_floppy0_writable` honours the mount's writability, so an
//! archive mounted read-only authentically asserts `/DSKPROT` (and a SAVE
//! is rejected), while a writable work disk does not.

use peripheral_commodore_amiga_floppy::{Adf, DD};
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};

fn blank_adf() -> Adf {
    Adf::from_bytes(vec![0; DD.len()]).expect("valid blank ADF")
}

#[test]
fn read_only_mount_reports_dskprot() {
    let mut rt = AmigaRuntimeKind::blank(Model::A500OcsPal);
    rt.insert_floppy0_writable(blank_adf(), false, false);
    assert!(
        rt.drive().status().write_protect,
        "a read-only mount must assert /DSKPROT"
    );
}

#[test]
fn writable_mount_does_not_report_dskprot() {
    let mut rt = AmigaRuntimeKind::blank(Model::A500OcsPal);
    rt.insert_floppy0_writable(blank_adf(), false, true);
    assert!(
        !rt.drive().status().write_protect,
        "a writable mount must not assert /DSKPROT"
    );
}

#[test]
fn default_insert_floppy0_is_writable() {
    // The `insert_floppy0` convenience defaults to a writable mount.
    let mut rt = AmigaRuntimeKind::blank(Model::A500OcsPal);
    rt.insert_floppy0(blank_adf(), false);
    assert!(!rt.drive().status().write_protect);
}
