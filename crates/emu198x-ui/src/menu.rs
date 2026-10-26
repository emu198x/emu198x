//! Native menu shell for the shared harness: NSMenu on macOS and the Win32
//! menu bar on Windows, both via `muda`.
//!
//! Generalises the Spectrum runner's menu (see
//! `knowledge/decisions/native-menu-shell.md`) into the harness so every system
//! gets the same native bar. This first cut carries the menus that need no
//! per-system knowledge: an **App** menu (About / Quit), **Machine → Reset**,
//! **State → Save / Load** (the quick-slot save-states), and **View** (window
//! scale + video filter). Per-system menus that the decision doc designs for —
//! File (open media), Machine variant switching — land later once the harness
//! grows media + live-variant capabilities.
//!
//! Every menu click becomes an [`AppCommand`] pushed onto the App's command
//! channel, the *same* channel the keyboard shortcuts feed, so the two never
//! drift into separate logic paths.
//!
//! On Linux the menu is a no-op stub (bottom of this file): muda with
//! `default-features = false` doesn't build there (the GTK feature would reopen
//! a Dependabot alert) and Linux menu attachment isn't wired yet. The
//! keyboard-shortcut path still drives every command the menu would expose.

#![cfg_attr(target_os = "linux", allow(dead_code))]

use std::borrow::Cow;
#[cfg(not(target_os = "linux"))]
use std::collections::HashMap;

use crate::{DrivePortInfo, VariantInfo};

use emu198x_native_video::VideoFilter;
#[cfg(not(target_os = "linux"))]
use emu198x_shell::commands::expect_command;
use emu198x_shell::{MediaKind, MediaSlot, MediaTransportAction};
#[cfg(not(target_os = "linux"))]
use muda::{AboutMetadata, CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use winit::window::Window;

/// The dynamic Machine-menu model: the variant radio and the per-port drive
/// radios, bundled so [`AppMenu::new`] takes one machine-config argument.
pub struct MachineMenu<'a> {
    /// Switchable machine variants (empty ⇒ no variant radio).
    pub variants: &'a [VariantInfo],
    /// The currently-running variant's id, for the radio check.
    pub current_variant: Option<&'a str>,
    /// Offer host-layout and original keyboard modes.
    pub host_keyboard: bool,
    /// Configurable drive ports (empty ⇒ no Drives menu).
    pub drive_ports: &'a [DrivePortInfo],
}

/// Window-scale values offered in the View menu's radio group.
pub const SCALE_OPTIONS: &[u32] = &[1, 2, 3, 4];

/// Video-filter values offered in the View menu's radio group, in order.
pub const FILTER_OPTIONS: &[VideoFilter] = &[
    VideoFilter::Raw,
    VideoFilter::Lcd,
    VideoFilter::Crt,
    VideoFilter::Signal,
    VideoFilter::Monitor,
    VideoFilter::Modern,
    VideoFilter::ModernMonitor,
];

/// Commands the App processes at frame boundaries. Pushed by both menu clicks
/// and keyboard shortcuts, so the two converge on one handler. Defined on every
/// platform (the Linux stub menu emits none, but the shortcut path still does).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppCommand {
    /// Hard-reset the machine (menu Machine → Reset, or the F12 shortcut).
    Reset,
    /// Quick-save to the machine's slot (menu State → Save, or Cmd/Ctrl+S).
    QuickSave,
    /// Quick-load from the machine's slot (menu State → Load, or Cmd/Ctrl+L).
    QuickLoad,
    /// Resize the window to an integer multiple of the native frame.
    SetScale(u32),
    /// Switch the post-framebuffer video filter.
    SetFilter(VideoFilter),
    /// Open a file dialog and load the chosen image into the named media slot
    /// (menu File → Open …). The slot/kind come from the machine's profile.
    OpenMedia {
        slot: Cow<'static, str>,
        kind: MediaKind,
    },
    /// Eject whatever media occupies the named slot (menu File → Eject …). The
    /// counterpart to [`Self::OpenMedia`]; the slot comes from the machine's
    /// profile and is handed to [`emu198x_shell::MachineCore::eject_media`].
    Eject { slot: Cow<'static, str> },
    /// Switch the live machine to the variant with this id (menu Machine →
    /// variant radio). The id round-trips through [`crate::UiSystem::switch_variant`].
    SwitchVariant(Cow<'static, str>),
    /// Set the drive model on a device port (menu Machine → Drives → Device N
    /// radio). The `kind_id` round-trips through [`crate::UiSystem::set_port_drive`].
    SetPortDrive {
        device: u8,
        kind_id: Cow<'static, str>,
    },
    /// Start/stop the transport for a media slot (menu Tape → Play / Stop, or
    /// the F9 / F10 shortcuts).
    MediaTransport {
        slot: Cow<'static, str>,
        action: MediaTransportAction,
    },
    /// Toggle tape fast-load / turbo (menu Tape → Fast-Forward While
    /// Playing, or F11).
    ToggleTurbo,
    /// Open a file dialog and load an arbitrary state/snapshot file
    /// (menu File → Open State…), parsed by the system itself.
    OpenState,

    /// Open a save dialog and write the runtime's snapshot to the chosen path
    /// (menu State → Save State As…). The counterpart to [`Self::QuickSave`],
    /// which writes to the fixed per-machine slot.
    SaveState,
    /// Export recorded tape data to a new user-chosen file.
    ExportTape,
    /// Select host-character translation or original key mapping.
    SetHostKeyboard(bool),
}

// Only the tests read these so far. The GTK4 host will route by them.
#[cfg(test)]
impl AppCommand {
    /// The command-catalogue id this command carries out
    /// (`emu198x_shell::commands`). Exhaustive, so a new command cannot be
    /// added without naming its catalogue entry.
    #[must_use]
    pub const fn catalogue_id(&self) -> &'static str {
        match self {
            Self::Reset => "machine.reset",
            Self::QuickSave => "state.quick-save",
            Self::QuickLoad => "state.quick-load",
            Self::SetScale(_) => "view.scale",
            Self::SetFilter(_) => "view.filter",
            Self::OpenMedia { .. } => "media.open",
            Self::Eject { .. } => "media.eject",
            Self::SwitchVariant(_) => "machine.variant",
            Self::SetPortDrive { .. } => "machine.drive",
            Self::MediaTransport {
                action: MediaTransportAction::Stop,
                ..
            } => "media.tape.stop",
            // MediaTransportAction is non-exhaustive; Start is the only other.
            Self::MediaTransport { .. } => "media.tape.play",
            Self::ToggleTurbo => "media.tape.fast-forward",
            Self::OpenState => "state.open",
            Self::SaveState => "state.save-as",
            Self::ExportTape => "media.tape.export",
            Self::SetHostKeyboard(true) => "machine.keyboard.host",
            Self::SetHostKeyboard(false) => "machine.keyboard.original",
        }
    }
}

/// Catalogue ids the native menu provides itself, not through an
/// [`AppCommand`]: the predefined About and Quit items.
#[cfg(test)]
pub const MENU_PREDEFINED: &[&str] = &["app.about", "app.quit"];

/// The filter's name, which fills the catalogue's `view.filter` label.
pub(crate) fn filter_name(filter: VideoFilter) -> &'static str {
    // VideoFilter is #[non_exhaustive]; a future addition shows a generic name
    // until it's named here.
    match filter {
        VideoFilter::Raw => "Raw",
        VideoFilter::Lcd => "LCD",
        VideoFilter::Crt => "CRT",
        VideoFilter::Signal => "Signal",
        VideoFilter::Monitor => "Monitor",
        VideoFilter::Modern => "Modern display",
        VideoFilter::ModernMonitor => "Modern monitor",
        VideoFilter::ModernWeave => "Modern display",
        VideoFilter::ModernMonitorWeave => "Modern monitor",
        _ => "Other",
    }
}

/// The native menu bar plus the `MenuId → AppCommand` map and the check-item
/// handles needed to keep the radio groups in sync.
#[cfg(not(target_os = "linux"))]
pub struct AppMenu {
    /// Owns the menu tree so its items aren't dropped while installed.
    root: Menu,
    keyboard_items: Vec<(bool, CheckMenuItem)>,
    action_map: HashMap<MenuId, AppCommand>,
    scale_items: Vec<(u32, CheckMenuItem)>,
    filter_items: Vec<(VideoFilter, CheckMenuItem)>,
    variant_items: Vec<(Cow<'static, str>, CheckMenuItem)>,
    /// (device, option id, item) for each Machine → Drives radio, kept so a
    /// selection can re-check the right item per port.
    port_drive_items: Vec<(u8, Cow<'static, str>, CheckMenuItem)>,
    /// The Tape → Fast-Forward While Playing check item, kept so its tick
    /// can follow the turbo state. `None` when the machine has no tape slot
    /// (no Tape menu).
    turbo_item: Option<CheckMenuItem>,
}

#[cfg(not(target_os = "linux"))]
impl AppMenu {
    /// Build the menu bar for a window titled `title`, with `current_scale` /
    /// `current_filter` initially checked in the View radios. A File menu is
    /// added with one "Open …" item per media slot the machine declares.
    pub fn new(
        title: &str,
        current_scale: u32,
        current_filter: VideoFilter,
        media_slots: &[MediaSlot],
        machine: &MachineMenu<'_>,
        state_open: bool,
        tape_export: bool,
    ) -> Self {
        let MachineMenu {
            variants,
            current_variant,
            drive_ports,
            host_keyboard,
        } = *machine;
        let root = Menu::new();
        let mut action_map = HashMap::new();

        // App menu (first submenu → the macOS application menu).
        let about = AboutMetadata {
            name: Some(title.to_owned()),
            ..AboutMetadata::default()
        };
        let app_menu = Submenu::new(title, true);
        app_menu
            .append_items(&[
                &PredefinedMenuItem::about(Some("About"), Some(about)),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ])
            .expect("append app menu");

        // File → one "Open …" per declared media slot. Built from the machine's
        // profile, so a tapeless console gets no File menu and a disk machine
        // gets the right slots — no per-system code.
        let file_menu = (!media_slots.is_empty() || state_open).then(|| {
            let file_menu = Submenu::new("File", true);
            for slot in media_slots {
                let item = MenuItem::new(
                    expect_command("media.open").label_for(&slot.display_name),
                    true,
                    None,
                );
                action_map.insert(
                    item.id().clone(),
                    AppCommand::OpenMedia {
                        slot: slot.id.clone(),
                        kind: slot.kind,
                    },
                );
                file_menu.append(&item).expect("append file item");

                // Eject sits right under its slot's Open item. The handler
                // reports gracefully on a slot whose runtime has no eject wired,
                // so every slot gets the item without per-system knowledge here.
                let eject = MenuItem::new(
                    expect_command("media.eject").label_for(&slot.display_name),
                    true,
                    None,
                );
                action_map.insert(
                    eject.id().clone(),
                    AppCommand::Eject {
                        slot: slot.id.clone(),
                    },
                );
                file_menu.append(&eject).expect("append eject item");
            }
            if state_open {
                if !media_slots.is_empty() {
                    file_menu
                        .append(&PredefinedMenuItem::separator())
                        .expect("append file separator");
                }
                let item = MenuItem::new(expect_command("state.open").label, true, None);
                action_map.insert(item.id().clone(), AppCommand::OpenState);
                file_menu.append(&item).expect("append open state");
            }
            file_menu
        });

        // Machine → variant radio (when the system declares variants) + Reset.
        let machine_menu = Submenu::new("Machine", true);
        let mut variant_items = Vec::new();
        let mut variant_groups = HashMap::new();
        for variant in variants {
            let item = CheckMenuItem::new(
                variant.label.as_ref(),
                true,
                Some(variant.id.as_ref()) == current_variant,
                None,
            );
            action_map.insert(
                item.id().clone(),
                AppCommand::SwitchVariant(variant.id.clone()),
            );
            if let Some(group) = &variant.group {
                let submenu = variant_groups.entry(group.clone()).or_insert_with(|| {
                    let submenu = Submenu::new(group.as_ref(), true);
                    machine_menu.append(&submenu).expect("append variant group");
                    submenu
                });
                submenu.append(&item).expect("append grouped variant");
            } else {
                machine_menu.append(&item).expect("append variant item");
            }
            variant_items.push((variant.id.clone(), item));
        }

        // Machine → Drives → Device N radios (when the system has swappable
        // drive ports). Each port is a submenu of model radios; unavailable
        // models (firmware absent) are shown disabled.
        let mut port_drive_items = Vec::new();
        if !drive_ports.is_empty() {
            let drives_menu = Submenu::new("Drives", true);
            for port in drive_ports {
                let port_menu = Submenu::new(port.label.as_ref(), true);
                for option in &port.options {
                    let item = CheckMenuItem::new(
                        option.label.as_ref(),
                        option.available,
                        option.id == port.current,
                        None,
                    );
                    action_map.insert(
                        item.id().clone(),
                        AppCommand::SetPortDrive {
                            device: port.device,
                            kind_id: option.id.clone(),
                        },
                    );
                    port_menu.append(&item).expect("append port drive item");
                    port_drive_items.push((port.device, option.id.clone(), item));
                }
                drives_menu.append(&port_menu).expect("append port submenu");
            }
            machine_menu
                .append(&drives_menu)
                .expect("append drives submenu");
        }

        if !variant_items.is_empty() || !port_drive_items.is_empty() {
            machine_menu
                .append(&PredefinedMenuItem::separator())
                .expect("append machine separator");
        }
        let reset_item = MenuItem::new(expect_command("machine.reset").label, true, None);
        action_map.insert(reset_item.id().clone(), AppCommand::Reset);
        machine_menu.append(&reset_item).expect("append reset");

        let mut keyboard_items = Vec::new();
        if host_keyboard {
            let keyboard = Submenu::new("Keyboard", true);
            for (host, id) in [
                (true, "machine.keyboard.host"),
                (false, "machine.keyboard.original"),
            ] {
                let item = CheckMenuItem::new(expect_command(id).label, true, host, None);
                action_map.insert(item.id().clone(), AppCommand::SetHostKeyboard(host));
                keyboard.append(&item).expect("append keyboard mode");
                keyboard_items.push((host, item));
            }
            machine_menu
                .append(&keyboard)
                .expect("append keyboard menu");
        }

        // Tape → Play / Stop / Fast-Forward While Playing, when the machine has a tape slot. The
        // transport commands target that slot; the same AppCommands the F9/F10/F11
        // shortcuts emit, so menu and keys share one path.
        let tape_slot = media_slots
            .iter()
            .find(|slot| slot.kind == MediaKind::Tape)
            .map(|slot| slot.id.clone());
        let (tape_menu, turbo_item) = if let Some(slot) = tape_slot {
            let tape_menu = Submenu::new("Tape", true);
            let play = MenuItem::new(expect_command("media.tape.play").label, true, None);
            let stop = MenuItem::new(expect_command("media.tape.stop").label, true, None);
            action_map.insert(
                play.id().clone(),
                AppCommand::MediaTransport {
                    slot: slot.clone(),
                    action: MediaTransportAction::Start,
                },
            );
            action_map.insert(
                stop.id().clone(),
                AppCommand::MediaTransport {
                    slot,
                    action: MediaTransportAction::Stop,
                },
            );
            let fast = CheckMenuItem::new(
                expect_command("media.tape.fast-forward").label,
                true,
                false,
                None,
            );
            action_map.insert(fast.id().clone(), AppCommand::ToggleTurbo);
            tape_menu
                .append_items(&[&play, &stop, &PredefinedMenuItem::separator(), &fast])
                .expect("append tape items");
            if tape_export {
                let export = MenuItem::new(expect_command("media.tape.export").label, true, None);
                action_map.insert(export.id().clone(), AppCommand::ExportTape);
                tape_menu
                    .append_items(&[&PredefinedMenuItem::separator(), &export])
                    .expect("append tape export");
            }
            (Some(tape_menu), Some(fast))
        } else {
            (None, None)
        };

        // State → Save / Load (the quick-slot save-states) + Save State As…
        // (a save dialog, the counterpart to File → Open State…).
        let state_menu = Submenu::new("State", true);
        let save_item = MenuItem::new(expect_command("state.quick-save").label, true, None);
        let load_item = MenuItem::new(expect_command("state.quick-load").label, true, None);
        let save_as_item = MenuItem::new(expect_command("state.save-as").label, true, None);
        action_map.insert(save_item.id().clone(), AppCommand::QuickSave);
        action_map.insert(load_item.id().clone(), AppCommand::QuickLoad);
        action_map.insert(save_as_item.id().clone(), AppCommand::SaveState);
        state_menu
            .append_items(&[
                &save_item,
                &load_item,
                &PredefinedMenuItem::separator(),
                &save_as_item,
            ])
            .expect("append state items");

        // View → Window Scale radio + Video Filter radio.
        let view_menu = Submenu::new("View", true);
        let mut scale_items = Vec::new();
        for &scale in SCALE_OPTIONS {
            let item = CheckMenuItem::new(
                expect_command("view.scale").label_for(&scale.to_string()),
                true,
                scale == current_scale,
                None,
            );
            action_map.insert(item.id().clone(), AppCommand::SetScale(scale));
            view_menu.append(&item).expect("append scale item");
            scale_items.push((scale, item));
        }
        view_menu
            .append(&PredefinedMenuItem::separator())
            .expect("append view separator");
        let mut filter_items = Vec::new();
        for &filter in FILTER_OPTIONS {
            let item = CheckMenuItem::new(
                expect_command("view.filter").label_for(filter_name(filter)),
                true,
                filter == current_filter,
                None,
            );
            action_map.insert(item.id().clone(), AppCommand::SetFilter(filter));
            view_menu.append(&item).expect("append filter item");
            filter_items.push((filter, item));
        }

        root.append(&app_menu).expect("append app submenu");
        if let Some(file_menu) = &file_menu {
            root.append(file_menu).expect("append file submenu");
        }
        root.append(&machine_menu).expect("append machine submenu");
        if let Some(tape_menu) = &tape_menu {
            root.append(tape_menu).expect("append tape submenu");
        }
        root.append(&state_menu).expect("append state submenu");
        root.append(&view_menu).expect("append view submenu");

        Self {
            root,
            keyboard_items,
            action_map,
            scale_items,
            filter_items,
            variant_items,
            port_drive_items,
            turbo_item,
        }
    }

    pub fn set_host_keyboard(&self, enabled: bool) {
        for (host, item) in &self.keyboard_items {
            item.set_checked(*host == enabled);
        }
    }

    /// Look up the command a menu-event id maps to, if any.
    pub fn command_for(&self, id: &MenuId) -> Option<AppCommand> {
        self.action_map.get(id).cloned()
    }

    /// Attach the menu: to the application on macOS, and to `window` on
    /// Windows, where each window carries its own menu bar.
    #[cfg(target_os = "macos")]
    pub fn install(&self, _window: &Window) {
        self.root.init_for_nsapp();
    }

    /// Attach the menu: to the application on macOS, and to `window` on
    /// Windows, where each window carries its own menu bar. A failure is
    /// reported and the app runs on without a menu; the keyboard shortcuts
    /// still reach every command.
    #[cfg(target_os = "windows")]
    pub fn install(&self, window: &Window) {
        if let Err(err) = emu198x_win_menu::attach(&self.root, window) {
            eprintln!("warning: no menu bar: {err}");
        }
    }

    /// No other platform has a menu attachment wired yet.
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn install(&self, _window: &Window) {}

    /// Refresh the View → Window Scale radio so only `scale` is checked.
    pub fn set_current_scale(&self, scale: u32) {
        for (option, item) in &self.scale_items {
            item.set_checked(*option == scale);
        }
    }

    /// Refresh the View → Video Filter radio so only `filter` is checked.
    pub fn set_current_filter(&self, filter: VideoFilter) {
        for (option, item) in &self.filter_items {
            item.set_checked(*option == filter);
        }
    }

    /// Refresh the Machine → variant radio so only the item for `id` is checked.
    pub fn set_current_variant(&self, id: &str) {
        for (option, item) in &self.variant_items {
            item.set_checked(option.as_ref() == id);
        }
    }

    /// Refresh one device port's drive radios so only `kind_id` is checked.
    pub fn set_current_port_drive(&self, device: u8, kind_id: &str) {
        for (item_device, option, item) in &self.port_drive_items {
            if *item_device == device {
                item.set_checked(option.as_ref() == kind_id);
            }
        }
    }

    /// Tick/untick the Tape → Fast-Forward While Playing item to match the turbo state. No-op
    /// when the machine has no tape menu.
    pub fn set_turbo_armed(&self, armed: bool) {
        if let Some(item) = &self.turbo_item {
            item.set_checked(armed);
        }
    }
}

/// Linux stub: a no-op menu. muda isn't a Linux dependency (see the module
/// docs), so the bar is absent there; the keyboard shortcuts still drive every
/// command it would expose.
#[cfg(target_os = "linux")]
pub struct AppMenu;

#[cfg(target_os = "linux")]
impl AppMenu {
    pub fn new(
        _title: &str,
        _current_scale: u32,
        _current_filter: VideoFilter,
        _media_slots: &[MediaSlot],
        _machine: &MachineMenu<'_>,
        _state_open: bool,
        _tape_export: bool,
    ) -> Self {
        Self
    }

    pub fn install(&self, _window: &Window) {}

    pub fn set_host_keyboard(&self, _enabled: bool) {}

    pub fn set_current_scale(&self, _scale: u32) {}

    pub fn set_current_filter(&self, _filter: VideoFilter) {}

    pub fn set_current_variant(&self, _id: &str) {}

    pub fn set_current_port_drive(&self, _device: u8, _kind_id: &str) {}

    pub fn set_turbo_armed(&self, _armed: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::commands::{CATALOGUE, command};
    use std::collections::BTreeSet;

    /// One of every command, so each catalogue id is reached.
    fn every_command() -> Vec<AppCommand> {
        vec![
            AppCommand::Reset,
            AppCommand::QuickSave,
            AppCommand::QuickLoad,
            AppCommand::SetScale(2),
            AppCommand::SetFilter(VideoFilter::Raw),
            AppCommand::OpenMedia {
                slot: Cow::Borrowed("tape-1"),
                kind: MediaKind::Tape,
            },
            AppCommand::Eject {
                slot: Cow::Borrowed("tape-1"),
            },
            AppCommand::SwitchVariant(Cow::Borrowed("zx-48k")),
            AppCommand::SetPortDrive {
                device: 8,
                kind_id: Cow::Borrowed("1541"),
            },
            AppCommand::MediaTransport {
                slot: Cow::Borrowed("tape-1"),
                action: MediaTransportAction::Start,
            },
            AppCommand::MediaTransport {
                slot: Cow::Borrowed("tape-1"),
                action: MediaTransportAction::Stop,
            },
            AppCommand::ToggleTurbo,
            AppCommand::OpenState,
            AppCommand::SaveState,
            AppCommand::ExportTape,
            AppCommand::SetHostKeyboard(true),
            AppCommand::SetHostKeyboard(false),
        ]
    }

    #[test]
    fn every_command_names_a_catalogue_entry() {
        for cmd in every_command() {
            assert!(
                command(cmd.catalogue_id()).is_some(),
                "{cmd:?} names `{}`, which is not in the catalogue",
                cmd.catalogue_id()
            );
        }
    }

    #[test]
    fn the_baseline_covers_the_whole_catalogue() {
        let covered: BTreeSet<&str> = every_command()
            .iter()
            .map(AppCommand::catalogue_id)
            .chain(MENU_PREDEFINED.iter().copied())
            .collect();
        let catalogue: BTreeSet<&str> = CATALOGUE.iter().map(|spec| spec.id).collect();
        assert_eq!(
            covered, catalogue,
            "catalogue entries and winit commands must match"
        );
    }
}
