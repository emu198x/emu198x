//! Remember the chord chosen on key-down until its physical key is released.
use emu198x_shell::InputEvent;
use std::collections::{BTreeSet, HashMap};
use winit::keyboard::{Key, KeyCode, ModifiersState};

pub(crate) fn character(key: &Key) -> Option<char> {
    let Key::Character(text) = key else {
        return None;
    };
    let mut chars = text.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

/// A host key reserved for deliberate target-key chords in character mode.
/// Other modifiers remain available for layout-produced characters.
#[derive(Clone, Copy)]
pub struct KeywordModifier {
    /// Physical host key that selects target-key chords.
    pub key: KeyCode,
    /// Target contacts held by that key alone.
    pub keys: &'static [&'static str],
    /// Target contacts held while host Shift is also down.
    pub shifted_keys: &'static [&'static str],
}

impl KeywordModifier {
    pub(crate) fn keys(self, shift: bool) -> Vec<String> {
        if shift { self.shifted_keys } else { self.keys }
            .iter()
            .map(|name| (*name).to_owned())
            .collect()
    }
}

pub(crate) enum HostKey {
    Release,
    Ignore,
    Keys(Vec<String>),
    Character(char),
    Control,
    Physical,
}

pub(crate) fn route_host_key(
    code: KeyCode,
    logical: &Key,
    pressed: bool,
    modifiers: ModifiersState,
    keyword: Option<KeywordModifier>,
    held: &HeldKeys,
) -> HostKey {
    if !pressed {
        return HostKey::Release;
    }
    // Let application shortcuts and layout-produced AltGr characters retain
    // their existing meanings. The configured physical modifier alone selects
    // target chords; an aggregate ALT flag must not capture right Alt/AltGr.
    if modifiers.super_key() || modifiers.control_key() && !modifiers.alt_key() {
        return HostKey::Ignore;
    }
    if let Some(keyword) = keyword {
        if code == keyword.key {
            return HostKey::Keys(keyword.keys(modifiers.shift_key()));
        }
        if held.contains(keyword.key)
            && !matches!(
                code,
                KeyCode::ShiftLeft
                    | KeyCode::ShiftRight
                    | KeyCode::AltLeft
                    | KeyCode::AltRight
                    | KeyCode::ControlLeft
                    | KeyCode::ControlRight
                    | KeyCode::SuperLeft
                    | KeyCode::SuperRight
                    | KeyCode::CapsLock
            )
        {
            return HostKey::Physical;
        }
    }
    if matches!(
        code,
        KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
            | KeyCode::CapsLock
    ) {
        HostKey::Ignore
    } else if let Some(ch) = character(logical) {
        HostKey::Character(ch)
    } else if matches!(logical, Key::Character(_) | Key::Dead(_)) {
        HostKey::Ignore
    } else {
        HostKey::Control
    }
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
        Self::changes(&before, &after)
    }

    fn changes(before: &BTreeSet<String>, after: &BTreeSet<String>) -> Vec<InputEvent> {
        let mut events = Vec::new();
        for name in before.difference(after) {
            events.push(InputEvent::Key {
                name: name.clone().into(),
                pressed: false,
            });
        }
        for name in after.difference(before) {
            events.push(InputEvent::Key {
                name: name.clone().into(),
                pressed: true,
            });
        }
        events
    }

    pub(crate) fn contains(&self, code: KeyCode) -> bool {
        self.chords.contains_key(&code)
    }

    /// Unlike a character chosen on key-down, an explicit modifier follows
    /// Shift changes while it is held, in either press/release order.
    pub(crate) fn refresh_keyword(
        &mut self,
        keyword: KeywordModifier,
        shift: bool,
    ) -> Vec<InputEvent> {
        if !self.contains(keyword.key) {
            return Vec::new();
        }
        let before = self.names();
        self.chords.insert(keyword.key, keyword.keys(shift));
        let after = self.names();
        Self::changes(&before, &after)
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
    fn spectrum_keyword() -> KeywordModifier {
        KeywordModifier {
            key: KeyCode::AltLeft,
            keys: &["symbol"],
            shifted_keys: &["caps", "symbol"],
        }
    }

    #[test]
    fn keyword_modifier_uses_physical_letters_then_restores_layout_typing() {
        let keyword = spectrum_keyword();
        let mut held = HeldKeys::default();
        let route = route_host_key(
            KeyCode::AltLeft,
            &Key::Named(winit::keyboard::NamedKey::Alt),
            true,
            ModifiersState::empty(),
            Some(keyword),
            &held,
        );
        let HostKey::Keys(keys) = route else {
            panic!("Left Alt must hold the target modifier")
        };
        assert_eq!(
            changes(held.update(KeyCode::AltLeft, Some(keys))),
            vec![("symbol".into(), true)]
        );
        // The logical character can differ from the physical keyword key.
        assert!(matches!(
            route_host_key(
                KeyCode::KeyG,
                &Key::Character("©".into()),
                true,
                ModifiersState::empty(),
                Some(keyword),
                &held
            ),
            HostKey::Physical
        ));
        held.update(KeyCode::KeyG, chord(&["g"]));
        assert!(matches!(
            route_host_key(
                KeyCode::AltLeft,
                &Key::Named(winit::keyboard::NamedKey::Alt),
                false,
                ModifiersState::empty(),
                Some(keyword),
                &held
            ),
            HostKey::Release
        ));
        assert_eq!(
            changes(held.update(KeyCode::AltLeft, None)),
            vec![("symbol".into(), false)]
        );
        assert_eq!(
            changes(held.update(KeyCode::KeyG, None)),
            vec![("g".into(), false)]
        );
        assert!(matches!(
            route_host_key(
                KeyCode::Digit2,
                &Key::Character("@".into()),
                true,
                ModifiersState::ALT,
                Some(keyword),
                &held
            ),
            HostKey::Character('@')
        ));
        assert!(matches!(
            route_host_key(
                KeyCode::KeyQ,
                &Key::Character("@".into()),
                true,
                ModifiersState::ALT | ModifiersState::CONTROL,
                Some(keyword),
                &held
            ),
            HostKey::Character('@')
        ));
    }

    #[test]
    fn shifted_keyword_tracks_both_shift_orders_and_focus_loss() {
        let keyword = spectrum_keyword();
        let mut held = HeldKeys::default();
        held.update(KeyCode::AltLeft, Some(keyword.keys(false)));
        assert_eq!(
            changes(held.refresh_keyword(keyword, true)),
            vec![("caps".into(), true)]
        );
        assert_eq!(
            changes(held.refresh_keyword(keyword, false)),
            vec![("caps".into(), false)]
        );
        assert_eq!(
            changes(held.update(KeyCode::AltLeft, None)),
            vec![("symbol".into(), false)]
        );
        // Shift first, then Left Alt; release Left Alt before Shift.
        let route = route_host_key(
            KeyCode::AltLeft,
            &Key::Named(winit::keyboard::NamedKey::Alt),
            true,
            ModifiersState::SHIFT,
            Some(keyword),
            &held,
        );
        let HostKey::Keys(keys) = route else {
            panic!("Shift+Left Alt must hold both contacts")
        };
        assert_eq!(keys, vec!["caps", "symbol"]);
        held.update(KeyCode::AltLeft, Some(keys));
        assert_eq!(
            changes(held.release_all()),
            vec![("caps".into(), false), ("symbol".into(), false)]
        );
        assert!(held.refresh_keyword(keyword, false).is_empty());
        assert!(!held.contains(KeyCode::AltLeft));
    }

    #[test]
    fn explicit_and_character_symbol_chords_share_contact_ownership() {
        let mut held = HeldKeys::default();
        held.update(KeyCode::Quote, chord(&["symbol", "p"]));
        assert!(
            held.update(KeyCode::AltLeft, Some(spectrum_keyword().keys(false)))
                .is_empty()
        );
        assert_eq!(
            changes(held.update(KeyCode::Quote, None)),
            vec![("p".into(), false)]
        );
        assert_eq!(
            changes(held.update(KeyCode::AltLeft, None)),
            vec![("symbol".into(), false)]
        );
    }

    #[test]
    fn unconfigured_systems_and_application_shortcuts_keep_existing_routes() {
        let held = HeldKeys::default();
        assert!(matches!(
            route_host_key(
                KeyCode::AltLeft,
                &Key::Named(winit::keyboard::NamedKey::Alt),
                true,
                ModifiersState::empty(),
                None,
                &held
            ),
            HostKey::Ignore
        ));
        for modifiers in [ModifiersState::SUPER, ModifiersState::CONTROL] {
            assert!(matches!(
                route_host_key(
                    KeyCode::AltLeft,
                    &Key::Named(winit::keyboard::NamedKey::Alt),
                    true,
                    modifiers,
                    Some(spectrum_keyword()),
                    &held
                ),
                HostKey::Ignore
            ));
        }
        assert!(matches!(
            route_host_key(
                KeyCode::KeyG,
                &Key::Character("g".into()),
                false,
                ModifiersState::CONTROL,
                Some(spectrum_keyword()),
                &held
            ),
            HostKey::Release
        ));
    }
    #[test]
    fn right_alt_keeps_layout_input_and_left_alt_overrides_option_characters() {
        let keyword = spectrum_keyword();
        let mut held = HeldKeys::default();
        assert!(matches!(
            route_host_key(
                KeyCode::AltRight,
                &Key::Named(winit::keyboard::NamedKey::Alt),
                true,
                ModifiersState::ALT,
                Some(keyword),
                &held
            ),
            HostKey::Ignore
        ));
        assert!(matches!(
            route_host_key(
                KeyCode::KeyG,
                &Key::Character("©".into()),
                true,
                ModifiersState::ALT,
                Some(keyword),
                &held
            ),
            HostKey::Character('©')
        ));
        let HostKey::Keys(keys) = route_host_key(
            KeyCode::AltLeft,
            &Key::Named(winit::keyboard::NamedKey::Alt),
            true,
            ModifiersState::ALT,
            Some(keyword),
            &held,
        ) else {
            panic!("left Alt modifier")
        };
        held.update(KeyCode::AltLeft, Some(keys));
        for (code, logical) in [
            (KeyCode::KeyG, Key::Character("©".into())),
            (KeyCode::KeyH, Key::Character("˙".into())),
            (KeyCode::KeyE, Key::Dead(Some('´'))),
        ] {
            assert!(matches!(
                route_host_key(
                    code,
                    &logical,
                    true,
                    ModifiersState::ALT,
                    Some(keyword),
                    &held
                ),
                HostKey::Physical
            ));
        }
        held.update(KeyCode::AltLeft, None);
        assert!(matches!(
            route_host_key(
                KeyCode::KeyQ,
                &Key::Character("@".into()),
                true,
                ModifiersState::ALT | ModifiersState::CONTROL,
                Some(keyword),
                &held
            ),
            HostKey::Character('@')
        ));
        assert!(matches!(
            route_host_key(
                KeyCode::Tab,
                &Key::Named(winit::keyboard::NamedKey::Tab),
                true,
                ModifiersState::empty(),
                Some(keyword),
                &held
            ),
            HostKey::Control
        ));
    }
}
