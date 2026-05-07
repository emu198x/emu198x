//! Native menu shell — macOS NSMenu via `muda`.
//!
//! Phase 1 scope: a single Machine submenu with eight check items, one
//! per in-scope variant, current variant indicated. Selecting an item
//! emits an `AppCommand::SwitchMachine(kind)` into the App's command
//! channel; Phase 2 will wire that command to actual runtime swaps.
//!
//! See `wiki/decisions/native-menu-shell.md` for the broader design
//! and how File / State / View slot in later.

use std::collections::HashMap;

use muda::{AboutMetadata, CheckMenuItem, Menu, MenuId, PredefinedMenuItem, Submenu};

/// The eight in-scope October-public variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MachineKind {
    Spectrum16K,
    Spectrum48K,
    SpectrumPlus,
    Spectrum128K,
    SpectrumPlus2,
    SpectrumPlus2A,
    SpectrumPlus2B,
    SpectrumPlus3,
}

impl MachineKind {
    /// Display label used in the Machine menu.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Spectrum16K => "ZX Spectrum 16K",
            Self::Spectrum48K => "ZX Spectrum 48K",
            Self::SpectrumPlus => "ZX Spectrum+",
            Self::Spectrum128K => "ZX Spectrum 128",
            Self::SpectrumPlus2 => "ZX Spectrum +2",
            Self::SpectrumPlus2A => "ZX Spectrum +2A",
            Self::SpectrumPlus2B => "ZX Spectrum +2B",
            Self::SpectrumPlus3 => "ZX Spectrum +3",
        }
    }

    /// All eight variants in catalogue order (16K → 48K → Plus → 128K
    /// → +2 → +2A → +2B → +3). Stable iteration order matters for the
    /// menu layout and the radio-style "current" indicator.
    pub const fn all() -> [Self; 8] {
        [
            Self::Spectrum16K,
            Self::Spectrum48K,
            Self::SpectrumPlus,
            Self::Spectrum128K,
            Self::SpectrumPlus2,
            Self::SpectrumPlus2A,
            Self::SpectrumPlus2B,
            Self::SpectrumPlus3,
        ]
    }
}

/// Commands the App processes at frame boundaries. The same enum is
/// pushed by every event source (muda menu clicks today; winit
/// keyboard shortcuts, rfd file-dialog replies, MCP commands later).
#[derive(Clone, Debug)]
pub enum AppCommand {
    SwitchMachine(MachineKind),
}

/// The constructed muda menu plus the data the App needs to translate
/// menu-event IDs into commands and to keep the radio indicator
/// pointing at the current variant.
pub struct AppMenu {
    /// Owned root menu so it stays alive for the duration of the app.
    /// On macOS `install_for_nsapp` reads it to attach to NSApp. On
    /// Linux / Windows nothing reads it yet (the install path is a
    /// no-op TODO), but the field still has to own the `Menu` so its
    /// items don't get dropped — hence the lint suppression.
    #[allow(dead_code)]
    pub root: Menu,
    /// Per-variant check items, parallel to `MachineKind::all()`.
    pub machine_items: Vec<(MachineKind, CheckMenuItem)>,
    /// Maps a clicked item's ID back to the command it represents.
    pub action_map: HashMap<MenuId, AppCommand>,
}

impl AppMenu {
    /// Builds the menu structure with `current` checked.
    pub fn new(current: MachineKind) -> Self {
        let root = Menu::new();

        // macOS treats the first submenu as the application menu and
        // replaces its title with the bundle/app name. Convention is to
        // put About / Hide / Quit there so the system places them where
        // users expect. On other platforms these predefined items still
        // render in their submenu — they just don't get the special
        // treatment.
        let about_metadata = AboutMetadata {
            name: Some("Emu198x Spectrum".to_owned()),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            copyright: Some("© 2026 Steve Hill".to_owned()),
            website: Some("https://code198x.com".to_owned()),
            ..Default::default()
        };
        let app_submenu = Submenu::new("Emu198x Spectrum", true);
        app_submenu
            .append_items(&[
                &PredefinedMenuItem::about(Some("About Emu198x Spectrum"), Some(about_metadata)),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ])
            .expect("append app menu items");
        root.append(&app_submenu).expect("append app submenu");

        let machine_submenu = Submenu::new("Machine", true);
        let mut machine_items = Vec::with_capacity(8);
        let mut action_map = HashMap::new();

        for kind in MachineKind::all() {
            let item = CheckMenuItem::new(kind.label(), true, kind == current, None);
            action_map.insert(item.id().clone(), AppCommand::SwitchMachine(kind));
            machine_submenu
                .append(&item)
                .expect("append machine menu item");
            machine_items.push((kind, item));
        }

        root.append(&machine_submenu)
            .expect("append Machine submenu");

        Self {
            root,
            machine_items,
            action_map,
        }
    }

    /// On macOS, install the menu as the application's NSMenu. Other
    /// platforms (Windows HMENU, Linux GTK) use different
    /// initialisation paths; for now this is macOS-first per
    /// `wiki/decisions/native-menu-shell.md`.
    #[cfg(target_os = "macos")]
    pub fn install_for_nsapp(&self) {
        self.root.init_for_nsapp();
    }

    #[cfg(not(target_os = "macos"))]
    pub fn install_for_nsapp(&self) {
        // TODO(track-1c): Wire muda::Menu::init_for_hwnd / init_for_gtk_window
        // when we expand beyond macOS. The command channel stays the same;
        // only the menu attachment differs per platform.
    }

    /// Updates the radio indicator so only `current` is checked.
    pub fn set_current_machine(&self, current: MachineKind) {
        for (kind, item) in &self.machine_items {
            item.set_checked(*kind == current);
        }
    }
}
