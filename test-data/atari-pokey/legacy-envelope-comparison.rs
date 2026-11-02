//! Compare frozen pre-scanner envelopes without claiming they remain loadable.
//! Unattached scanners serialize as one new Option::None byte in POKEY.
//! Everything else, including all chip history, must still match byte for byte.
pub fn matches_without_unattached_scanner(current: &[u8], legacy: &[u8], old: u8, new: u8) -> bool {
    if current.first() != Some(&new)
        || legacy.first() != Some(&old)
        || current.len() != legacy.len() + 1
    {
        return false;
    }
    let current = &current[1..];
    let legacy = &legacy[1..];
    let insertion = current
        .iter()
        .zip(legacy)
        .position(|(a, b)| a != b)
        .unwrap_or(legacy.len());
    current.get(insertion) == Some(&0) && current[insertion + 1..] == legacy[insertion..]
}

#[test]
fn comparison_rejects_changed_history_and_missing_or_extra_scanner_bytes() {
    assert!(matches_without_unattached_scanner(
        &[7, 10, 0, 20],
        &[6, 10, 20],
        6,
        7
    ));
    assert!(!matches_without_unattached_scanner(
        &[7, 11, 0, 20],
        &[6, 10, 20],
        6,
        7
    ));
    assert!(!matches_without_unattached_scanner(
        &[7, 10, 1, 20],
        &[6, 10, 20],
        6,
        7
    ));
    assert!(!matches_without_unattached_scanner(
        &[7, 10, 20],
        &[6, 10, 20],
        6,
        7
    ));
    assert!(!matches_without_unattached_scanner(
        &[7, 10, 0, 0, 20],
        &[6, 10, 20],
        6,
        7
    ));
}
