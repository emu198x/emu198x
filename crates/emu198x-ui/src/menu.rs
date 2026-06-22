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

#[cfg(not(target_os = "linux"))]
use std::collections::HashMap;

use emu198x_native_video::VideoFilter;
#[cfg(not(target_os = "linux"))]
use muda::{AboutMetadata, CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};

/// Window-scale values offered in the View menu's radio group.
pub const SCALE_OPTIONS: &[u32] = &[1, 2, 3, 4];

/// Video-filter values offered in the View menu's radio group, in order.
pub const FILTER_OPTIONS: &[VideoFilter] = &[VideoFilter::Raw, VideoFilter::Lcd, VideoFilter::Crt];

/// Commands the App processes at frame boundaries. Pushed by both menu clicks
/// and keyboard shortcuts, so the two converge on one handler. Defined on every
/// platform (the Linux stub menu emits none, but the shortcut path still does).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

#[cfg(not(target_os = "linux"))]
impl AppMenu {
    /// Build the menu bar for a window titled `title`, with `current_scale` /
    /// `current_filter` initially checked in the View radios.
    pub fn new(title: &str, current_scale: u32, current_filter: VideoFilter) -> Self {
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

        // Machine → Reset.
        let machine_menu = Submenu::new("Machine", true);
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
        root.append(&machine_menu).expect("append machine submenu");
        root.append(&state_menu).expect("append state submenu");
        root.append(&view_menu).expect("append view submenu");

        Self {
            root,
            action_map,
            scale_items,
            filter_items,
        }
    }

    /// Look up the command a menu-event id maps to, if any.
    pub fn command_for(&self, id: &MenuId) -> Option<AppCommand> {
        self.action_map.get(id).copied()
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
}

/// Linux stub: a no-op menu. muda isn't a Linux dependency (see the module
/// docs), so the bar is absent there; the keyboard shortcuts still drive every
/// command it would expose.
#[cfg(target_os = "linux")]
pub struct AppMenu;

#[cfg(target_os = "linux")]
impl AppMenu {
    pub fn new(_title: &str, _current_scale: u32, _current_filter: VideoFilter) -> Self {
        Self
    }

    pub fn install(&self) {}

    pub fn set_current_scale(&self, _scale: u32) {}

    pub fn set_current_filter(&self, _filter: VideoFilter) {}
}
