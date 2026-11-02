//! Exact comparisons for the approved version/RTC-presence migration.
pub fn assert_migrated_save(old: &[u8], current: &[u8], rtc_added: bool) {
    assert_eq!(old.first(), Some(&66), "immutable v66 envelope");
    assert_eq!(current.first(), Some(&68), "new writer must emit v68");
    assert_eq!(old.len(), current.len(), "saved field layout is unchanged");
    let changes: Vec<_> = old
        .iter()
        .zip(current)
        .enumerate()
        .filter(|(_, (old, new))| old != new)
        .collect();
    assert_eq!(
        changes.len(),
        if rtc_added { 2 } else { 1 },
        "only the version and, for original stock A500+, the RTC-presence flag may change"
    );
    assert_eq!(changes[0].0, 0);
    if rtc_added {
        assert_eq!(*changes[1].1.0, 0, "legacy clock was not decoded");
        assert_eq!(*changes[1].1.1, 1, "fitted clock is now decoded");
    }
}
