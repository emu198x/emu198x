//! The command catalogue: every action a host offers, described once.
//!
//! Each host builds its menus, shortcut help, toolbar and pause menu from this
//! list, so a command means the same thing on every host. The catalogue is
//! data only. Each host's own command type still does the work: `AppCommand`
//! in `emu198x-ui`, `HostCommand` in the Apple hosts. See
//! `knowledge/decisions/host-ui-model.md` § 3.

use serde::Serialize;

/// The menu a command sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum CommandGroup {
    /// The application menu: About, Quit.
    App,
    /// Opening and ejecting media, opening state files.
    File,
    /// The machine itself: reset, variants, drives, keyboard mode.
    Machine,
    /// Tape transport.
    Tape,
    /// Save states.
    State,
    /// How the picture is shown.
    View,
}

/// What a command needs before a host may offer it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Availability {
    /// Always available.
    Always,
    /// A machine is running.
    Running,
    /// The machine declares at least one media slot.
    MediaSlot,
    /// The machine has a tape slot.
    TapeSlot,
    /// The machine can record a tape and export it.
    TapeExport,
    /// The machine declares switchable variants.
    Variants,
    /// The machine has drive ports whose drive can be changed.
    DrivePorts,
    /// The machine offers a host-layout keyboard mode.
    HostKeyboard,
    /// The machine can open a state or snapshot file itself.
    StateFile,
}

/// The value a parameterised command takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum CommandParam {
    /// A media slot, named by its display name in the label.
    MediaSlot,
    /// A machine variant id.
    Variant,
    /// A drive model for one device port.
    PortDrive,
    /// A whole-number window scale.
    Scale,
    /// A video filter.
    Filter,
    /// On or off.
    Toggle,
}

/// A key a shortcut uses, independent of any windowing library.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[non_exhaustive]
pub enum ShortcutKey {
    /// A letter key, `'A'` to `'Z'`.
    Letter(char),
    /// A function key, `1` to `12`.
    Function(u8),
}

/// A keyboard shortcut.
///
/// `host` is the host modifier: Command on macOS, Control elsewhere. A
/// shortcut without it would be a key the guest also uses, so every shortcut
/// in the catalogue holds it. The Mac app's keys set the catalogue's
/// (`knowledge/decisions/host-ui-model.md` § 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Shortcut {
    /// The key.
    pub key: ShortcutKey,
    /// Whether the host modifier is held.
    pub host: bool,
    /// Whether Shift is held.
    pub shift: bool,
}

impl Shortcut {
    const fn host(letter: char) -> Self {
        Self {
            key: ShortcutKey::Letter(letter),
            host: true,
            shift: false,
        }
    }

    const fn host_shift(letter: char) -> Self {
        Self {
            key: ShortcutKey::Letter(letter),
            host: true,
            shift: true,
        }
    }
}

/// One entry in the catalogue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CommandSpec {
    /// Stable id, lower case and dotted, such as `media.tape.play`.
    pub id: &'static str,
    /// The menu label. A parameterised command's label holds `{}` where the
    /// parameter's name goes; use [`CommandSpec::label_for`].
    pub label: &'static str,
    /// The menu the command sits in.
    pub group: CommandGroup,
    /// The shortcut, if it has one.
    pub shortcut: Option<Shortcut>,
    /// What the command needs before a host offers it.
    pub needs: Availability,
    /// The value the command takes, if any.
    pub param: Option<CommandParam>,
    /// The MCP tool that does the same thing, if there is one.
    pub mcp_verb: Option<&'static str>,
}

impl CommandSpec {
    /// The label with `name` in place of `{}`. A label without `{}` is
    /// returned unchanged.
    #[must_use]
    pub fn label_for(&self, name: &str) -> String {
        self.label.replacen("{}", name, 1)
    }
}

/// Every command a host may offer, in menu order.
pub const CATALOGUE: &[CommandSpec] = &[
    CommandSpec {
        id: "app.about",
        label: "About",
        group: CommandGroup::App,
        shortcut: None,
        needs: Availability::Always,
        param: None,
        mcp_verb: None,
    },
    CommandSpec {
        id: "app.quit",
        label: "Quit",
        group: CommandGroup::App,
        shortcut: None,
        needs: Availability::Always,
        param: None,
        mcp_verb: None,
    },
    CommandSpec {
        id: "media.open",
        label: "Open {}…",
        group: CommandGroup::File,
        shortcut: None,
        needs: Availability::MediaSlot,
        param: Some(CommandParam::MediaSlot),
        mcp_verb: Some("load_media"),
    },
    CommandSpec {
        id: "media.eject",
        label: "Eject {}",
        group: CommandGroup::File,
        shortcut: None,
        needs: Availability::MediaSlot,
        param: Some(CommandParam::MediaSlot),
        mcp_verb: None,
    },
    CommandSpec {
        id: "state.open",
        label: "Open State…",
        group: CommandGroup::File,
        shortcut: None,
        needs: Availability::StateFile,
        param: None,
        mcp_verb: Some("load_snapshot"),
    },
    CommandSpec {
        id: "machine.variant",
        label: "{}",
        group: CommandGroup::Machine,
        shortcut: None,
        needs: Availability::Variants,
        param: Some(CommandParam::Variant),
        mcp_verb: Some("set_machine"),
    },
    CommandSpec {
        id: "machine.drive",
        label: "{}",
        group: CommandGroup::Machine,
        shortcut: None,
        needs: Availability::DrivePorts,
        param: Some(CommandParam::PortDrive),
        mcp_verb: None,
    },
    CommandSpec {
        id: "machine.pause",
        label: "Pause",
        group: CommandGroup::Machine,
        shortcut: Some(Shortcut::host('P')),
        needs: Availability::Running,
        param: Some(CommandParam::Toggle),
        mcp_verb: None,
    },
    CommandSpec {
        id: "machine.reset",
        label: "Reset",
        group: CommandGroup::Machine,
        shortcut: Some(Shortcut::host_shift('R')),
        needs: Availability::Running,
        param: None,
        mcp_verb: Some("reset"),
    },
    // The two keyboard modes are one radio group. The shortcut sits on the
    // host mode and switches between the two.
    CommandSpec {
        id: "machine.keyboard.host",
        label: "Host Keyboard",
        group: CommandGroup::Machine,
        shortcut: Some(Shortcut::host_shift('K')),
        needs: Availability::HostKeyboard,
        param: None,
        mcp_verb: None,
    },
    CommandSpec {
        id: "machine.keyboard.original",
        label: "Original Keyboard",
        group: CommandGroup::Machine,
        shortcut: None,
        needs: Availability::HostKeyboard,
        param: None,
        mcp_verb: None,
    },
    // Play and Stop share one shortcut, which sits on Play: it plays a stopped
    // tape and stops a playing one.
    CommandSpec {
        id: "media.tape.play",
        label: "Play",
        group: CommandGroup::Tape,
        shortcut: Some(Shortcut::host('T')),
        needs: Availability::TapeSlot,
        param: None,
        mcp_verb: Some("media_transport"),
    },
    CommandSpec {
        id: "media.tape.stop",
        label: "Stop",
        group: CommandGroup::Tape,
        shortcut: None,
        needs: Availability::TapeSlot,
        param: None,
        mcp_verb: Some("media_transport"),
    },
    CommandSpec {
        id: "media.tape.fast-forward",
        label: "Fast-Forward While Playing",
        group: CommandGroup::Tape,
        shortcut: Some(Shortcut::host_shift('F')),
        needs: Availability::TapeSlot,
        param: Some(CommandParam::Toggle),
        mcp_verb: None,
    },
    CommandSpec {
        id: "media.tape.export",
        label: "Export Recording…",
        group: CommandGroup::Tape,
        shortcut: Some(Shortcut::host_shift('E')),
        needs: Availability::TapeExport,
        param: None,
        mcp_verb: Some("export_media"),
    },
    CommandSpec {
        id: "state.quick-save",
        label: "Save State",
        group: CommandGroup::State,
        shortcut: Some(Shortcut::host('S')),
        needs: Availability::Running,
        param: None,
        mcp_verb: Some("save_snapshot"),
    },
    CommandSpec {
        id: "state.quick-load",
        label: "Load State",
        group: CommandGroup::State,
        shortcut: Some(Shortcut::host('L')),
        needs: Availability::Running,
        param: None,
        mcp_verb: Some("load_snapshot"),
    },
    CommandSpec {
        id: "state.save-as",
        label: "Save State As…",
        group: CommandGroup::State,
        shortcut: None,
        needs: Availability::Running,
        param: None,
        mcp_verb: Some("save_snapshot"),
    },
    CommandSpec {
        id: "view.scale",
        label: "Window Scale {}×",
        group: CommandGroup::View,
        shortcut: None,
        needs: Availability::Always,
        param: Some(CommandParam::Scale),
        mcp_verb: None,
    },
    CommandSpec {
        id: "view.filter",
        label: "Video Filter: {}",
        group: CommandGroup::View,
        shortcut: None,
        needs: Availability::Always,
        param: Some(CommandParam::Filter),
        mcp_verb: None,
    },
];

/// The catalogue entry with this id.
#[must_use]
pub fn command(id: &str) -> Option<&'static CommandSpec> {
    CATALOGUE.iter().find(|spec| spec.id == id)
}

/// The entry with this id, for ids a host names in its own code.
///
/// # Panics
///
/// Panics if `id` is not in the catalogue. Every host's tests call this for
/// each id it uses, so a missing id fails a test before it can ship.
#[must_use]
pub fn expect_command(id: &str) -> &'static CommandSpec {
    match command(id) {
        Some(spec) => spec,
        None => panic!("`{id}` is not in the command catalogue"),
    }
}

/// The whole catalogue as JSON, for hosts outside Rust.
#[must_use]
pub fn catalogue_json() -> String {
    // Serialising plain data with no maps or non-string keys cannot fail.
    serde_json::to_string(CATALOGUE).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique() {
        let mut seen = HashSet::new();
        for spec in CATALOGUE {
            assert!(seen.insert(spec.id), "duplicate id `{}`", spec.id);
        }
    }

    #[test]
    fn ids_are_lower_case_and_dotted() {
        for spec in CATALOGUE {
            assert!(
                spec.id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '-'),
                "`{}` must be lower case, dots and hyphens",
                spec.id
            );
            assert!(spec.id.contains('.'), "`{}` needs a group prefix", spec.id);
        }
    }

    #[test]
    fn shortcuts_are_unique() {
        let mut seen = HashSet::new();
        for spec in CATALOGUE {
            if let Some(shortcut) = spec.shortcut {
                assert!(
                    seen.insert(shortcut),
                    "`{}` reuses a shortcut already taken",
                    spec.id
                );
            }
        }
    }

    #[test]
    fn every_shortcut_holds_the_host_modifier() {
        // A key without it would be swallowed from the guest's keyboard:
        // some machines use F9 to F12 as keys of their own.
        for spec in CATALOGUE {
            if let Some(shortcut) = spec.shortcut {
                assert!(shortcut.host, "`{}` has a shortcut without ⌘/Ctrl", spec.id);
            }
        }
    }

    #[test]
    fn parameterised_labels_have_a_placeholder() {
        for spec in CATALOGUE {
            let named = matches!(
                spec.param,
                Some(
                    CommandParam::MediaSlot
                        | CommandParam::Variant
                        | CommandParam::PortDrive
                        | CommandParam::Scale
                        | CommandParam::Filter
                )
            );
            assert_eq!(
                named,
                spec.label.contains("{}"),
                "`{}`'s label and parameter disagree",
                spec.id
            );
        }
    }

    #[test]
    fn label_for_fills_the_placeholder() {
        assert_eq!(expect_command("media.open").label_for("Tape"), "Open Tape…");
        assert_eq!(
            expect_command("view.scale").label_for("2"),
            "Window Scale 2×"
        );
        assert_eq!(expect_command("machine.reset").label_for("x"), "Reset");
    }

    #[test]
    fn json_round_trips_every_id() {
        let value: serde_json::Value =
            serde_json::from_str(&catalogue_json()).expect("catalogue JSON parses");
        let ids: Vec<&str> = value
            .as_array()
            .expect("an array")
            .iter()
            .filter_map(|entry| entry["id"].as_str())
            .collect();
        let expected: Vec<&str> = CATALOGUE.iter().map(|spec| spec.id).collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn mcp_verbs_name_shared_tools() {
        // Each verb named here is registered by `mcp_tools`.
        let source = include_str!("mcp_tools.rs");
        for spec in CATALOGUE {
            if let Some(verb) = spec.mcp_verb {
                assert!(
                    source.contains(&format!("\"{verb}\"")),
                    "`{}` names MCP verb `{verb}`, which mcp_tools does not register",
                    spec.id
                );
            }
        }
    }
}
