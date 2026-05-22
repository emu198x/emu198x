//! Native menu shell — macOS NSMenu via `muda`.
//!
//! Phase 1 scope: a single Machine submenu with eight check items, one
//! per in-scope variant, current variant indicated. Selecting an item
//! emits an `AppCommand::SwitchMachine(kind)` into the App's command
//! channel; Phase 2 will wire that command to actual runtime swaps.
//!
//! See `knowledge/decisions/native-menu-shell.md` for the broader design
//! and how File / State / View slot in later.

use std::collections::HashMap;

use emu198x_native_video::VideoFilter;
use muda::{AboutMetadata, CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};

use crate::machine::MachineKind;

/// Window-scale values exposed in the View menu's radio group.
pub const SCALE_OPTIONS: &[u32] = &[1, 2, 3, 4];

/// Video-filter values exposed in the View menu's radio group, in the
/// order they appear.
pub const FILTER_OPTIONS: &[VideoFilter] = &[VideoFilter::Raw, VideoFilter::Lcd, VideoFilter::Crt];

/// Commands the App processes at frame boundaries. The same enum is
/// pushed by every event source (muda menu clicks; winit keyboard
/// shortcuts; rfd file-dialog replies; MCP commands later).
#[derive(Clone, Debug)]
pub enum AppCommand {
    /// Switch the live machine to one of the eight in-scope variants.
    SwitchMachine(MachineKind),
    /// Resize the window to one integer multiple of the native frame.
    SetWindowScale(u32),
    /// Switch the post-framebuffer video filter.
    SetVideoFilter(VideoFilter),
    /// Pop a file picker for a snapshot file (`.sna` / `.z80`) and
    /// restore it into the live machine.
    OpenSnapshot,
    /// Pop a file picker for a tape image (`.tap` / `.tzx`), load it,
    /// and start tape transport so the program begins loading.
    OpenTape,
    /// Pop a file picker for a disk image (`.dsk`) and insert it into
    /// the +3's drive.
    OpenDisk,
    /// Pop a save dialog and write the current snapshot to disk.
    SaveSnapshot,
    /// Pop a file picker for a snapshot and restore it. Same dispatch
    /// as `OpenSnapshot`; lifted to State for discoverability.
    LoadSnapshot,
    /// Open one URL in the system browser (Help menu items).
    OpenUrl(&'static str),
}

/// Documentation URL surfaced via Help menu.
pub const DOCUMENTATION_URL: &str = "https://github.com/code198x/emu198x";

/// "View on GitHub" URL surfaced via Help menu.
pub const GITHUB_URL: &str = "https://github.com/code198x/emu198x";

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
    /// File > Open Disk... — held so the App can enable / disable it
    /// when the live variant's disk-slot support changes (only +3
    /// today).
    pub open_disk_item: MenuItem,
    /// View > Window Scale radio items, parallel to [`SCALE_OPTIONS`].
    pub scale_items: Vec<(u32, CheckMenuItem)>,
    /// View > Video Filter radio items, parallel to [`FILTER_OPTIONS`].
    pub filter_items: Vec<(VideoFilter, CheckMenuItem)>,
    /// Maps a clicked item's ID back to the command it represents.
    pub action_map: HashMap<MenuId, AppCommand>,
}

impl AppMenu {
    /// Builds the menu structure with `current` machine, scale, and filter checked.
    pub fn new(
        current: MachineKind,
        supports_disk: bool,
        current_scale: u32,
        current_filter: VideoFilter,
    ) -> Self {
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

        let mut action_map = HashMap::new();

        let file_submenu = Submenu::new("File", true);
        let open_snapshot_item = MenuItem::new("Open Snapshot...", true, None);
        let open_tape_item = MenuItem::new("Open Tape...", true, None);
        let open_disk_item = MenuItem::new("Open Disk...", supports_disk, None);
        action_map.insert(open_snapshot_item.id().clone(), AppCommand::OpenSnapshot);
        action_map.insert(open_tape_item.id().clone(), AppCommand::OpenTape);
        action_map.insert(open_disk_item.id().clone(), AppCommand::OpenDisk);
        file_submenu
            .append_items(&[&open_snapshot_item, &open_tape_item, &open_disk_item])
            .expect("append file menu items");
        root.append(&file_submenu).expect("append File submenu");

        let machine_submenu = Submenu::new("Machine", true);
        let mut machine_items = Vec::with_capacity(8);

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

        let view_submenu = Submenu::new("View", true);
        let mut scale_items = Vec::with_capacity(SCALE_OPTIONS.len());
        for &scale in SCALE_OPTIONS {
            let label = format!("Window Scale {scale}×");
            let item = CheckMenuItem::new(label, true, scale == current_scale, None);
            action_map.insert(item.id().clone(), AppCommand::SetWindowScale(scale));
            view_submenu.append(&item).expect("append scale item");
            scale_items.push((scale, item));
        }
        view_submenu
            .append(&PredefinedMenuItem::separator())
            .expect("append view separator");
        let mut filter_items = Vec::with_capacity(FILTER_OPTIONS.len());
        for &filter in FILTER_OPTIONS {
            let item =
                CheckMenuItem::new(filter_label(filter), true, filter == current_filter, None);
            action_map.insert(item.id().clone(), AppCommand::SetVideoFilter(filter));
            view_submenu.append(&item).expect("append filter item");
            filter_items.push((filter, item));
        }
        root.append(&view_submenu).expect("append View submenu");

        let state_submenu = Submenu::new("State", true);
        let save_snapshot_item = MenuItem::new("Save State...", true, None);
        let load_snapshot_item = MenuItem::new("Load State...", true, None);
        action_map.insert(save_snapshot_item.id().clone(), AppCommand::SaveSnapshot);
        action_map.insert(load_snapshot_item.id().clone(), AppCommand::LoadSnapshot);
        state_submenu
            .append_items(&[&save_snapshot_item, &load_snapshot_item])
            .expect("append state menu items");
        root.append(&state_submenu).expect("append State submenu");

        let help_submenu = Submenu::new("Help", true);
        let github_item = MenuItem::new("View on GitHub", true, None);
        let docs_item = MenuItem::new("Documentation", true, None);
        action_map.insert(github_item.id().clone(), AppCommand::OpenUrl(GITHUB_URL));
        action_map.insert(
            docs_item.id().clone(),
            AppCommand::OpenUrl(DOCUMENTATION_URL),
        );
        help_submenu
            .append_items(&[&github_item, &docs_item])
            .expect("append help menu items");
        root.append(&help_submenu).expect("append Help submenu");

        Self {
            root,
            machine_items,
            open_disk_item,
            scale_items,
            filter_items,
            action_map,
        }
    }

    /// Enables or disables File > Open Disk... in response to a Machine
    /// menu switch. Only +3 supports disks today; non-disk variants
    /// grey the item out.
    pub fn set_disk_supported(&self, supported: bool) {
        self.open_disk_item.set_enabled(supported);
    }

    /// On macOS, install the menu as the application's NSMenu. Other
    /// platforms (Windows HMENU, Linux GTK) use different
    /// initialisation paths; for now this is macOS-first per
    /// `knowledge/decisions/native-menu-shell.md`.
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

    /// Refreshes the View > Window Scale radio so only `scale` is
    /// checked. `scale` outside [`SCALE_OPTIONS`] leaves no item
    /// checked — accurate for callers running at a non-menu scale.
    pub fn set_current_scale(&self, scale: u32) {
        for (option, item) in &self.scale_items {
            item.set_checked(*option == scale);
        }
    }

    /// Refreshes the View > Video Filter radio so only `filter` is
    /// checked.
    pub fn set_current_filter(&self, filter: VideoFilter) {
        for (option, item) in &self.filter_items {
            item.set_checked(*option == filter);
        }
    }
}

fn filter_label(filter: VideoFilter) -> &'static str {
    // VideoFilter is #[non_exhaustive] so the match needs a fallback;
    // future additions surface as a generic label until they're given
    // a proper one here.
    match filter {
        VideoFilter::Raw => "Video Filter: Raw",
        VideoFilter::Lcd => "Video Filter: LCD",
        VideoFilter::Crt => "Video Filter: CRT",
        _ => "Video Filter",
    }
}
