//! Native menu shell for the shared harness — macOS NSMenu via `muda`.
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

use crate::VariantInfo;

use emu198x_native_video::VideoFilter;
use emu198x_shell::{MediaKind, MediaSlot};
#[cfg(not(target_os = "linux"))]
use muda::{AboutMetadata, CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};

/// Window-scale values offered in the View menu's radio group.
pub const SCALE_OPTIONS: &[u32] = &[1, 2, 3, 4];

/// Video-filter values offered in the View menu's radio group, in order.
pub const FILTER_OPTIONS: &[VideoFilter] = &[VideoFilter::Raw, VideoFilter::Lcd, VideoFilter::Crt];

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
    /// Switch the live machine to the variant with this id (menu Machine →
    /// variant radio). The id round-trips through [`crate::UiSystem::switch_variant`].
    SwitchVariant(Cow<'static, str>),
}

#[cfg(not(target_os = "linux"))]
fn filter_label(filter: VideoFilter) -> &'static str {
    // VideoFilter is #[non_exhaustive]; a future addition shows a generic label
    // until it's named here.
    match filter {
        VideoFilter::Raw => "Video Filter: Raw",
        VideoFilter::Lcd => "Video Filter: LCD",
        VideoFilter::Crt => "Video Filter: CRT",
        _ => "Video Filter",
    }
}

/// The native menu bar plus the `MenuId → AppCommand` map and the check-item
/// handles needed to keep the radio groups in sync.
#[cfg(not(target_os = "linux"))]
pub struct AppMenu {
    /// Owns the menu tree so its items aren't dropped while installed.
    root: Menu,
    action_map: HashMap<MenuId, AppCommand>,
    scale_items: Vec<(u32, CheckMenuItem)>,
    filter_items: Vec<(VideoFilter, CheckMenuItem)>,
    variant_items: Vec<(Cow<'static, str>, CheckMenuItem)>,
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
        variants: &[VariantInfo],
        current_variant: Option<&str>,
    ) -> Self {
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
        let file_menu = (!media_slots.is_empty()).then(|| {
            let file_menu = Submenu::new("File", true);
            for slot in media_slots {
                let item = MenuItem::new(format!("Open {}…", slot.display_name), true, None);
                action_map.insert(
                    item.id().clone(),
                    AppCommand::OpenMedia {
                        slot: slot.id.clone(),
                        kind: slot.kind,
                    },
                );
                file_menu.append(&item).expect("append file item");
            }
            file_menu
        });

        // Machine → variant radio (when the system declares variants) + Reset.
        let machine_menu = Submenu::new("Machine", true);
        let mut variant_items = Vec::new();
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
            machine_menu.append(&item).expect("append variant item");
            variant_items.push((variant.id.clone(), item));
        }
        if !variant_items.is_empty() {
            machine_menu
                .append(&PredefinedMenuItem::separator())
                .expect("append machine separator");
        }
        let reset_item = MenuItem::new("Reset", true, None);
        action_map.insert(reset_item.id().clone(), AppCommand::Reset);
        machine_menu.append(&reset_item).expect("append reset");

        // State → Save / Load (the quick-slot save-states).
        let state_menu = Submenu::new("State", true);
        let save_item = MenuItem::new("Save State", true, None);
        let load_item = MenuItem::new("Load State", true, None);
        action_map.insert(save_item.id().clone(), AppCommand::QuickSave);
        action_map.insert(load_item.id().clone(), AppCommand::QuickLoad);
        state_menu
            .append_items(&[&save_item, &load_item])
            .expect("append state items");

        // View → Window Scale radio + Video Filter radio.
        let view_menu = Submenu::new("View", true);
        let mut scale_items = Vec::new();
        for &scale in SCALE_OPTIONS {
            let item = CheckMenuItem::new(
                format!("Window Scale {scale}×"),
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
            let item =
                CheckMenuItem::new(filter_label(filter), true, filter == current_filter, None);
            action_map.insert(item.id().clone(), AppCommand::SetFilter(filter));
            view_menu.append(&item).expect("append filter item");
            filter_items.push((filter, item));
        }

        root.append(&app_menu).expect("append app submenu");
        if let Some(file_menu) = &file_menu {
            root.append(file_menu).expect("append file submenu");
        }
        root.append(&machine_menu).expect("append machine submenu");
        root.append(&state_menu).expect("append state submenu");
        root.append(&view_menu).expect("append view submenu");

        Self {
            root,
            action_map,
            scale_items,
            filter_items,
            variant_items,
        }
    }

    /// Look up the command a menu-event id maps to, if any.
    pub fn command_for(&self, id: &MenuId) -> Option<AppCommand> {
        self.action_map.get(id).cloned()
    }

    /// Attach the menu to the process. macOS only for now; other platforms get
    /// the same command channel but no visible bar yet.
    #[cfg(target_os = "macos")]
    pub fn install(&self) {
        self.root.init_for_nsapp();
    }

    #[cfg(not(target_os = "macos"))]
    pub fn install(&self) {
        // TODO(#549): wire `init_for_hwnd` (Windows) when we expand past macOS.
        // The command channel is identical; only the attachment differs.
    }

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
        _variants: &[VariantInfo],
        _current_variant: Option<&str>,
    ) -> Self {
        Self
    }

    pub fn install(&self) {}

    pub fn set_current_scale(&self, _scale: u32) {}

    pub fn set_current_filter(&self, _filter: VideoFilter) {}

    pub fn set_current_variant(&self, _id: &str) {}
}
