//! Load media named on the command line into an already-built session.
//!
//! `--rom cart.nes --mcp` used to start a server whose machine had no
//! cartridge. The flag was accepted, nothing complained, and the failure
//! surfaced later as `reached: 0` and `WaitingForInput` — which reads as
//! a script bug rather than a startup one (#1180).
//!
//! The same flag was honoured in the other two modes, so `--rom` worked,
//! `--rom --script` worked, and `--rom --mcp` silently did not. A caller
//! reasonably assumes a flag means the same thing in all three.
//!
//! Slots are resolved from the machine profile rather than from a table
//! here, so a machine that declares `cartridge-1` gets `--rom` routed to
//! it without this module knowing the machine exists. That also lets a
//! rejection list the slots the profile actually has, which is the other
//! half of the complaint: the valid names were not enumerable from
//! anywhere (#1175).

use std::path::{Path, PathBuf};

use crate::media::{MediaImage, MediaKind, MediaSet, MediaSlot};

/// One `FLAG PATH` pair from the command line, resolved to a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupMedia {
    /// Slot the image will be loaded into.
    pub slot: String,
    /// Kind the slot accepts.
    pub kind: MediaKind,
    /// Image path as given on the command line.
    pub path: PathBuf,
}

/// Flags that name a media kind rather than a slot, in the spelling the
/// binaries use.
const KIND_FLAGS: &[(&str, MediaKind)] = &[
    ("--rom", MediaKind::Cartridge),
    ("--cart", MediaKind::Cartridge),
    ("--cartridge", MediaKind::Cartridge),
    ("--tape", MediaKind::Tape),
    ("--disk", MediaKind::Disk),
];

/// Resolve the media flags in `args` against the slots a profile
/// declares.
///
/// Unknown flags are ignored: this runs beside each binary's own parser,
/// which owns everything else.
///
/// # Errors
///
/// Returns a message naming the declared slots when a flag has no slot
/// to go to, or names a path that is not there.
pub fn resolve(slots: &[MediaSlot], args: &[String]) -> Result<Vec<StartupMedia>, String> {
    let mut resolved = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if let Some((_, kind)) = KIND_FLAGS.iter().find(|(flag, _)| *flag == arg) {
            let Some(path) = args.get(i + 1) else {
                return Err(format!("{arg} expects a path"));
            };
            let slot = slots
                .iter()
                .find(|slot| slot.kind == *kind)
                .ok_or_else(|| describe_slots(arg, slots))?;
            resolved.push(StartupMedia {
                slot: slot.id.to_string(),
                kind: *kind,
                path: PathBuf::from(path),
            });
            i += 2;
            continue;
        }
        i += 1;
    }
    Ok(resolved)
}

fn describe_slots(flag: &str, slots: &[MediaSlot]) -> String {
    if slots.is_empty() {
        return format!("{flag} was given, but this machine declares no media slots");
    }
    let declared = slots
        .iter()
        .map(|slot| format!("{} ({:?})", slot.id, slot.kind))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{flag} has no matching slot on this machine; it declares {declared}")
}

/// Read each resolved image so it can be pushed into a [`MediaSet`].
///
/// # Errors
///
/// Returns a message naming the path when a file cannot be read.
pub fn read_all(resolved: &[StartupMedia]) -> Result<Vec<(String, MediaKind, Vec<u8>)>, String> {
    resolved
        .iter()
        .map(|item| {
            let bytes = read_file(&item.path)?;
            Ok((item.slot.clone(), item.kind, bytes))
        })
        .collect()
}

fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))
}

/// Build a [`MediaSet`] borrowing the bytes read by [`read_all`].
#[must_use]
pub fn media_set(loaded: &[(String, MediaKind, Vec<u8>)]) -> MediaSet<'_> {
    let mut media = MediaSet::new();
    for (slot, kind, bytes) in loaded {
        media.push(MediaImage::new(slot.clone(), *kind, bytes));
    }
    media
}

/// Resolve, read and load the media flags in `args` into `session`.
///
/// The one call each binary's MCP mode makes, so `--rom` means the same
/// thing there as it does in windowed and `--script` modes.
///
/// # Errors
///
/// Returns a message when a flag has no matching slot, names a path that
/// cannot be read, or the machine rejects the image.
pub fn load_into<M, Q>(
    session: &mut crate::session::HeadlessSession<M, Q>,
    args: &[String],
) -> Result<(), String>
where
    M: crate::machine::MachineCore,
    Q: crate::query::SessionQueryProvider<M>,
{
    let slots = session.machine().profile().media_slots.clone();
    let resolved = resolve(&slots, args)?;
    if resolved.is_empty() {
        return Ok(());
    }
    let loaded = read_all(&resolved)?;
    let media = media_set(&loaded);
    session
        .load_media(&media)
        .map_err(|err| format!("failed to load startup media: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::WritebackPolicy;

    fn slot(id: &'static str, kind: MediaKind) -> MediaSlot {
        MediaSlot {
            id: id.into(),
            display_name: id.into(),
            kind,
            required: false,
            writeback: WritebackPolicy::InMemoryOnly,
        }
    }

    #[test]
    fn a_cartridge_flag_finds_the_cartridge_slot() {
        let slots = [slot("cartridge-1", MediaKind::Cartridge)];
        let args = ["--rom".to_owned(), "game.nes".to_owned()];
        let resolved = resolve(&slots, &args).expect("resolves");
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].slot, "cartridge-1");
        assert_eq!(resolved[0].path, PathBuf::from("game.nes"));
    }

    #[test]
    fn each_kind_goes_to_its_own_slot() {
        let slots = [
            slot("cartridge-1", MediaKind::Cartridge),
            slot("tape-1", MediaKind::Tape),
            slot("disk-a", MediaKind::Disk),
        ];
        let args = [
            "--tape".to_owned(),
            "manic.tzx".to_owned(),
            "--disk".to_owned(),
            "work.adf".to_owned(),
        ];
        let resolved = resolve(&slots, &args).expect("resolves");
        assert_eq!(resolved[0].slot, "tape-1");
        assert_eq!(resolved[1].slot, "disk-a");
    }

    /// The other half of the complaint: the valid slot names were not
    /// enumerable from anywhere, so a rejection has to list them.
    #[test]
    fn a_flag_with_no_slot_names_what_the_machine_has() {
        let slots = [slot("tape-1", MediaKind::Tape)];
        let args = ["--rom".to_owned(), "game.nes".to_owned()];
        let err = resolve(&slots, &args).expect_err("no cartridge slot");
        assert!(err.contains("tape-1"), "{err}");
        assert!(err.contains("--rom"), "{err}");
    }

    #[test]
    fn a_flag_without_a_path_is_refused() {
        let slots = [slot("cartridge-1", MediaKind::Cartridge)];
        let err = resolve(&slots, &["--rom".to_owned()]).expect_err("no path");
        assert!(err.contains("expects a path"), "{err}");
    }

    #[test]
    fn flags_this_module_does_not_own_are_left_alone() {
        let slots = [slot("cartridge-1", MediaKind::Cartridge)];
        let args = [
            "--scale".to_owned(),
            "4".to_owned(),
            "--mcp".to_owned(),
            "--rom".to_owned(),
            "game.nes".to_owned(),
        ];
        let resolved = resolve(&slots, &args).expect("resolves");
        assert_eq!(resolved.len(), 1, "only the media flag is claimed");
        assert_eq!(resolved[0].path, PathBuf::from("game.nes"));
    }
}
