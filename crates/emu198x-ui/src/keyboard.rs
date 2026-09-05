//! Remember the chord chosen on key-down until its physical key is released.
use emu198x_shell::InputEvent;
use std::collections::{BTreeSet, HashMap};
use winit::keyboard::{Key, KeyCode};

pub(crate) fn character(key: &Key) -> Option<char> {
    let Key::Character(text) = key else {
        return None;
    };
    let mut chars = text.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

#[derive(Default)]
pub(crate) struct HeldKeys {
    chords: HashMap<KeyCode, Vec<String>>,
}

impl HeldKeys {
    pub(crate) fn update(&mut self, code: KeyCode, keys: Option<Vec<String>>) -> Vec<InputEvent> {
        let before = self.names();
        if let Some(keys) = keys {
            self.chords.entry(code).or_insert(keys);
        } else {
            self.chords.remove(&code);
        }
        let after = self.names();
        let mut events = Vec::new();
        for name in before.difference(&after) {
            events.push(InputEvent::Key {
                name: name.clone().into(),
                pressed: false,
            });
        }
        for name in after.difference(&before) {
            events.push(InputEvent::Key {
                name: name.clone().into(),
                pressed: true,
            });
        }
        events
    }

    fn names(&self) -> BTreeSet<String> {
        self.chords.values().flatten().cloned().collect()
    }

    pub(crate) fn release_all(&mut self) -> Vec<InputEvent> {
        let events = self
            .names()
            .into_iter()
            .map(|name| InputEvent::Key {
                name: name.into(),
                pressed: false,
            })
            .collect();
        self.chords.clear();
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chord(names: &[&str]) -> Option<Vec<String>> {
        Some(names.iter().map(|s| (*s).to_owned()).collect())
    }
    fn changes(events: Vec<InputEvent>) -> Vec<(String, bool)> {
        events
            .into_iter()
            .filter_map(|e| {
                if let InputEvent::Key { name, pressed } = e {
                    Some((name.to_string(), pressed))
                } else {
                    None
                }
            })
            .collect()
    }
    #[test]
    fn overlapping_chords_keep_shared_shift_until_last_release() {
        let mut held = HeldKeys::default();
        held.update(KeyCode::Quote, chord(&["symbol", "p"]));
        assert_eq!(
            changes(held.update(KeyCode::Semicolon, chord(&["symbol", "o"]))),
            vec![("o".into(), true)]
        );
        assert_eq!(
            changes(held.update(KeyCode::Quote, None)),
            vec![("p".into(), false)]
        );
        assert_eq!(
            changes(held.update(KeyCode::Semicolon, None)),
            vec![("o".into(), false), ("symbol".into(), false)]
        );
    }
    #[test]
    fn release_uses_original_chord_and_focus_loss_releases_everything() {
        let mut held = HeldKeys::default();
        held.update(KeyCode::Digit2, chord(&["symbol", "p"]));
        assert!(held.update(KeyCode::Digit2, chord(&["2"])).is_empty());
        assert_eq!(
            changes(held.release_all()),
            vec![("p".into(), false), ("symbol".into(), false)]
        );
        assert!(held.update(KeyCode::Digit2, None).is_empty());
    }
    #[test]
    fn host_character_comes_from_layout_not_physical_position() {
        assert_eq!(character(&Key::Character("\"".into())), Some('"'));
        assert_eq!(character(&Key::Character("@".into())), Some('@'));
        assert_eq!(character(&Key::Character("é".into())), Some('é'));
        assert_eq!(character(&Key::Character("ab".into())), None);
        assert_eq!(character(&Key::Dead(Some('^'))), None);
    }
}
