//! The GTK menu bar, built from the command catalogue.
//!
//! Every item is a GTK action on the application. Activating one sends the
//! matching [`AppCommand`] to the core's channel, the same one the keyboard
//! shortcuts use. Radio and check items are stateful actions; the core moves
//! their marks through [`set_state`] once a command has taken effect, so a
//! refused command leaves the mark where it was.

use std::borrow::Cow;
use std::sync::mpsc::Sender;

use emu198x_shell::commands::{Shortcut, ShortcutKey, expect_command};
use emu198x_shell::{MediaKind, MediaSlot, MediaTransportAction};
use gtk4::prelude::*;
use gtk4::{gio, glib};

use crate::VideoFilter;
use crate::menu::{AppCommand, FILTER_OPTIONS, SCALE_OPTIONS, filter_name};
use crate::{DrivePortInfo, VariantInfo};

/// What the menu needs to know about the machine.
pub(super) struct MachineInfo<'a> {
    pub title: &'a str,
    pub media_slots: &'a [MediaSlot],
    pub variants: &'a [VariantInfo],
    pub current_variant: Option<&'a str>,
    pub drive_ports: &'a [DrivePortInfo],
    pub host_keyboard: Option<bool>,
    pub state_open: bool,
    pub tape_export: bool,
    pub scale: u32,
    pub filter: VideoFilter,
}

/// The action name for device `device`'s drive radio.
pub(super) fn drive_action(device: u8) -> String {
    format!("drive-{device}")
}

/// Build the menu bar and register its actions on `app`.
pub(super) fn build(
    app: &gtk4::Application,
    machine: &MachineInfo<'_>,
    commands: &Sender<AppCommand>,
) -> gio::Menu {
    let bar = gio::Menu::new();

    // File: one Open and Eject per media slot, then Open State and Quit.
    let file = gio::Menu::new();
    if !machine.media_slots.is_empty() {
        let slots = gio::Menu::new();
        for slot in machine.media_slots {
            slots.append(
                Some(&expect_command("media.open").label_for(&slot.display_name)),
                Some(&detailed("app.open-media", &slot.id)),
            );
            slots.append(
                Some(&expect_command("media.eject").label_for(&slot.display_name)),
                Some(&detailed("app.eject", &slot.id)),
            );
        }
        file.append_section(None, &slots);
        let kinds: Vec<(Cow<'static, str>, MediaKind)> = machine
            .media_slots
            .iter()
            .map(|slot| (slot.id.clone(), slot.kind))
            .collect();
        add_string_action(app, "open-media", commands, move |slot| {
            kinds
                .iter()
                .find(|(id, _)| id == slot)
                .map(|(id, kind)| AppCommand::OpenMedia {
                    slot: id.clone(),
                    kind: *kind,
                })
        });
        add_string_action(app, "eject", commands, |slot| {
            Some(AppCommand::Eject {
                slot: Cow::Owned(slot.to_owned()),
            })
        });
    }
    if machine.state_open {
        let section = gio::Menu::new();
        section.append_item(&command_item("state.open", "app.open-state"));
        file.append_section(None, &section);
        add_action(app, "open-state", commands, AppCommand::OpenState);
    }
    let quit = gio::Menu::new();
    quit.append_item(&command_item("app.quit", "app.quit"));
    file.append_section(None, &quit);
    let quit_action = gio::SimpleAction::new("quit", None);
    let quit_app = app.downgrade();
    quit_action.connect_activate(move |_, _| {
        if let Some(app) = quit_app.upgrade() {
            app.quit();
        }
    });
    app.add_action(&quit_action);
    bar.append_submenu(Some("File"), &file);

    // Machine: variants, drives, Reset, keyboard mode.
    let machine_menu = gio::Menu::new();
    if !machine.variants.is_empty() {
        let section = gio::Menu::new();
        let mut groups: Vec<(Cow<'static, str>, gio::Menu)> = Vec::new();
        for variant in machine.variants {
            let item = gio::MenuItem::new(Some(&variant.label), None);
            item.set_action_and_target_value(
                Some("app.variant"),
                Some(&variant.id.as_ref().to_variant()),
            );
            match &variant.group {
                Some(group) => {
                    if let Some((_, menu)) = groups.iter().find(|(name, _)| name == group) {
                        menu.append_item(&item);
                    } else {
                        let menu = gio::Menu::new();
                        menu.append_item(&item);
                        section.append_submenu(Some(group), &menu);
                        groups.push((group.clone(), menu));
                    }
                }
                None => section.append_item(&item),
            }
        }
        machine_menu.append_section(None, &section);
        add_radio_action(
            app,
            "variant",
            machine.current_variant.unwrap_or_default(),
            commands,
            |id| Some(AppCommand::SwitchVariant(Cow::Owned(id.to_owned()))),
        );
    }
    if !machine.drive_ports.is_empty() {
        let drives = gio::Menu::new();
        for port in machine.drive_ports {
            let options = gio::Menu::new();
            let name = drive_action(port.device);
            for option in &port.options {
                let item = gio::MenuItem::new(Some(&option.label), None);
                item.set_action_and_target_value(
                    Some(&format!("app.{name}")),
                    Some(&option.id.as_ref().to_variant()),
                );
                options.append_item(&item);
            }
            drives.append_submenu(Some(&port.label), &options);
            let device = port.device;
            let available: Vec<String> = port
                .options
                .iter()
                .filter(|option| option.available)
                .map(|option| option.id.to_string())
                .collect();
            add_radio_action(app, &name, &port.current, commands, move |kind_id| {
                available
                    .iter()
                    .any(|id| id == kind_id)
                    .then(|| AppCommand::SetPortDrive {
                        device,
                        kind_id: Cow::Owned(kind_id.to_owned()),
                    })
            });
        }
        machine_menu.append_submenu(Some("Drives"), &drives);
    }
    let reset = gio::Menu::new();
    reset.append_item(&command_item("machine.reset", "app.reset"));
    machine_menu.append_section(None, &reset);
    add_action(app, "reset", commands, AppCommand::Reset);
    if let Some(host) = machine.host_keyboard {
        let keyboard = gio::Menu::new();
        for (target, id) in [
            ("host", "machine.keyboard.host"),
            ("original", "machine.keyboard.original"),
        ] {
            let item = gio::MenuItem::new(Some(expect_command(id).label), None);
            item.set_action_and_target_value(Some("app.keyboard"), Some(&target.to_variant()));
            show_shortcut(&item, id);
            keyboard.append_item(&item);
        }
        machine_menu.append_section(None, &keyboard);
        add_radio_action(
            app,
            "keyboard",
            if host { "host" } else { "original" },
            commands,
            |target| Some(AppCommand::SetHostKeyboard(target == "host")),
        );
    }
    bar.append_submenu(Some("Machine"), &machine_menu);

    // Tape: Play, Stop and Fast-Forward on the first tape slot, then Export.
    if let Some(slot) = machine
        .media_slots
        .iter()
        .find(|slot| slot.kind == MediaKind::Tape)
    {
        let tape = gio::Menu::new();
        tape.append_item(&command_item("media.tape.play", "app.tape-play"));
        tape.append_item(&command_item("media.tape.stop", "app.tape-stop"));
        let fast = gio::Menu::new();
        fast.append_item(&command_item("media.tape.fast-forward", "app.fast-forward"));
        tape.append_section(None, &fast);
        add_action(
            app,
            "tape-play",
            commands,
            AppCommand::MediaTransport {
                slot: slot.id.clone(),
                action: MediaTransportAction::Start,
            },
        );
        add_action(
            app,
            "tape-stop",
            commands,
            AppCommand::MediaTransport {
                slot: slot.id.clone(),
                action: MediaTransportAction::Stop,
            },
        );
        let fast_action =
            gio::SimpleAction::new_stateful("fast-forward", None, &false.to_variant());
        let sender = commands.clone();
        fast_action.connect_activate(move |_, _| {
            let _ = sender.send(AppCommand::ToggleTurbo);
        });
        app.add_action(&fast_action);
        if machine.tape_export {
            let export = gio::Menu::new();
            export.append_item(&command_item("media.tape.export", "app.export-tape"));
            tape.append_section(None, &export);
            add_action(app, "export-tape", commands, AppCommand::ExportTape);
        }
        bar.append_submenu(Some("Tape"), &tape);
    }

    // State: the quick slot and Save State As.
    let state = gio::Menu::new();
    let quick = gio::Menu::new();
    quick.append_item(&command_item("state.quick-save", "app.quick-save"));
    quick.append_item(&command_item("state.quick-load", "app.quick-load"));
    state.append_section(None, &quick);
    let save_as = gio::Menu::new();
    save_as.append_item(&command_item("state.save-as", "app.save-as"));
    state.append_section(None, &save_as);
    add_action(app, "quick-save", commands, AppCommand::QuickSave);
    add_action(app, "quick-load", commands, AppCommand::QuickLoad);
    add_action(app, "save-as", commands, AppCommand::SaveState);
    bar.append_submenu(Some("State"), &state);

    // View: window scale, then the video filters.
    let view = gio::Menu::new();
    let scales = gio::Menu::new();
    for &scale in SCALE_OPTIONS {
        let item = gio::MenuItem::new(
            Some(&expect_command("view.scale").label_for(&scale.to_string())),
            None,
        );
        item.set_action_and_target_value(Some("app.scale"), Some(&scale.to_string().to_variant()));
        scales.append_item(&item);
    }
    view.append_section(None, &scales);
    let filters = gio::Menu::new();
    for &filter in FILTER_OPTIONS {
        let item = gio::MenuItem::new(
            Some(&expect_command("view.filter").label_for(filter_name(filter))),
            None,
        );
        item.set_action_and_target_value(
            Some("app.filter"),
            Some(&filter.to_string().to_variant()),
        );
        filters.append_item(&item);
    }
    view.append_section(None, &filters);
    add_radio_action(
        app,
        "filter",
        &machine.filter.to_string(),
        commands,
        |value| value.parse().ok().map(AppCommand::SetFilter),
    );
    add_radio_action(
        app,
        "scale",
        &machine.scale.to_string(),
        commands,
        |value| value.parse().ok().map(AppCommand::SetScale),
    );
    bar.append_submenu(Some("View"), &view);

    // Help: About.
    let help = gio::Menu::new();
    help.append_item(&command_item("app.about", "app.about"));
    let about = gio::SimpleAction::new("about", None);
    let title = machine.title.to_owned();
    let about_app = app.downgrade();
    about.connect_activate(move |_, _| {
        let parent = about_app.upgrade().and_then(|app| app.active_window());
        let dialog = gtk4::AboutDialog::builder()
            .program_name(&title)
            .version(env!("CARGO_PKG_VERSION"))
            .modal(true)
            .build();
        dialog.set_transient_for(parent.as_ref());
        dialog.present();
    });
    app.add_action(&about);
    bar.append_submenu(Some("Help"), &help);

    bar
}

/// A menu item for catalogue command `id`, firing `action`, showing the
/// command's shortcut.
fn command_item(id: &str, action: &str) -> gio::MenuItem {
    let item = gio::MenuItem::new(Some(expect_command(id).label), Some(action));
    show_shortcut(&item, id);
    item
}

/// Show command `id`'s shortcut beside its menu item. The `accel` attribute
/// only labels the item: the key itself still goes to the core, which
/// decides whether it is a shortcut or a key for the machine (F9 to F11
/// stay the machine's on systems that map them).
fn show_shortcut(item: &gio::MenuItem, id: &str) {
    if let Some(shortcut) = expect_command(id).shortcut {
        item.set_attribute_value("accel", Some(&accel_string(shortcut).to_variant()));
    }
}

/// A catalogue shortcut in GTK's accelerator syntax. The host modifier is
/// Control on Linux.
fn accel_string(shortcut: Shortcut) -> String {
    let mut accel = String::new();
    if shortcut.host {
        accel.push_str("<Control>");
    }
    if shortcut.shift {
        accel.push_str("<Shift>");
    }
    match shortcut.key {
        ShortcutKey::Letter(letter) => accel.push(letter.to_ascii_lowercase()),
        ShortcutKey::Function(n) => accel.push_str(&format!("F{n}")),
        _ => {}
    }
    accel
}

/// Move a stateful action's mark to `value`.
pub(super) fn set_state(app: &gtk4::Application, name: &str, value: glib::Variant) {
    if let Some(action) = app
        .lookup_action(name)
        .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
    {
        action.set_state(&value);
    }
}

fn detailed(action: &str, target: &str) -> String {
    gio::Action::print_detailed_name(action, Some(&target.to_variant())).to_string()
}

fn add_action(
    app: &gtk4::Application,
    name: &str,
    commands: &Sender<AppCommand>,
    command: AppCommand,
) {
    let action = gio::SimpleAction::new(name, None);
    let sender = commands.clone();
    action.connect_activate(move |_, _| {
        let _ = sender.send(command.clone());
    });
    app.add_action(&action);
}

fn add_string_action(
    app: &gtk4::Application,
    name: &str,
    commands: &Sender<AppCommand>,
    command: impl Fn(&str) -> Option<AppCommand> + 'static,
) {
    let action = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
    let sender = commands.clone();
    action.connect_activate(move |_, parameter| {
        if let Some(value) = parameter.and_then(|p| p.str())
            && let Some(command) = command(value)
        {
            let _ = sender.send(command);
        }
    });
    app.add_action(&action);
}

/// A radio group: a stateful string action whose state is the checked item.
/// Activating an item sends its command but leaves the mark alone; the core
/// moves it once the change has happened.
fn add_radio_action(
    app: &gtk4::Application,
    name: &str,
    initial: &str,
    commands: &Sender<AppCommand>,
    command: impl Fn(&str) -> Option<AppCommand> + 'static,
) {
    let action =
        gio::SimpleAction::new_stateful(name, Some(glib::VariantTy::STRING), &initial.to_variant());
    let sender = commands.clone();
    action.connect_activate(move |_, parameter| {
        if let Some(value) = parameter.and_then(|p| p.str())
            && let Some(command) = command(value)
        {
            let _ = sender.send(command);
        }
    });
    app.add_action(&action);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_targets_round_trip() {
        // The View menu's filter radio carries each filter as its text form
        // and parses it back into the command.
        for &filter in FILTER_OPTIONS {
            assert_eq!(filter.to_string().parse::<VideoFilter>(), Ok(filter));
        }
    }

    #[test]
    fn shortcuts_render_in_gtk_accelerator_syntax() {
        // Every shortcut in the catalogue, as GTK's accelerator syntax.
        let expected = [
            ("machine.reset", "F12"),
            ("machine.keyboard.host", "<Control><Shift>k"),
            ("media.tape.play", "F9"),
            ("media.tape.stop", "F10"),
            ("media.tape.fast-forward", "F11"),
            ("media.tape.export", "<Control><Shift>e"),
            ("state.quick-save", "<Control>s"),
            ("state.quick-load", "<Control>l"),
        ];
        let with_shortcuts = emu198x_shell::commands::CATALOGUE
            .iter()
            .filter(|spec| spec.shortcut.is_some())
            .count();
        assert_eq!(
            with_shortcuts,
            expected.len(),
            "a shortcut has no expected text"
        );
        for (id, text) in expected {
            let shortcut = expect_command(id).shortcut.expect("has a shortcut");
            assert_eq!(accel_string(shortcut), text, "{id}");
        }
    }

    #[test]
    fn drive_actions_are_named_by_device() {
        assert_eq!(drive_action(8), "drive-8");
    }
}
