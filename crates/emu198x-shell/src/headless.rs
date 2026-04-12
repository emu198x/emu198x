//! Shared helpers for thin headless runners and automation layers.

use crate::control::ControlCommand;
use crate::error::MachineError;
use crate::firmware::FirmwareSet;
use crate::machine::MachineCore;
use crate::media::MediaSet;

/// Shared boot artifacts for creating or restoring one machine runtime.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BootArtifacts<'a> {
    /// Firmware images for a cold boot.
    pub firmware: FirmwareSet<'a>,
    /// Optional snapshot to restore after construction.
    pub snapshot: Option<&'a [u8]>,
}

impl<'a> BootArtifacts<'a> {
    /// Creates an empty boot-artifact bundle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Creates one runtime from shared boot artifacts.
///
/// If firmware is supplied, `build_from_firmware` is used. If firmware is not
/// supplied, `blank` is used so a later snapshot restore still has a concrete
/// runtime to target.
///
/// # Errors
///
/// Returns an error if neither firmware nor a snapshot is supplied, if the
/// firmware-backed constructor rejects the images, or if snapshot restore
/// fails.
pub fn boot_machine<M, Build, Blank>(
    artifacts: &BootArtifacts<'_>,
    build_from_firmware: Build,
    blank: Blank,
) -> Result<M, MachineError>
where
    M: MachineCore,
    Build: FnOnce(&FirmwareSet<'_>) -> Result<M, MachineError>,
    Blank: FnOnce() -> M,
{
    if artifacts.firmware.is_empty() && artifacts.snapshot.is_none() {
        return Err(MachineError::InvalidRequest {
            reason: "either firmware or a snapshot must be supplied".into(),
        });
    }

    let mut machine = if artifacts.firmware.is_empty() {
        blank()
    } else {
        build_from_firmware(&artifacts.firmware)?
    };

    if let Some(snapshot) = artifacts.snapshot {
        machine.restore(snapshot)?;
    }

    Ok(machine)
}

/// Applies machine-local media inserts plus shared control commands.
///
/// # Errors
///
/// Returns an error if either the media set or any control command is rejected
/// by the target machine.
pub fn prepare_machine(
    machine: &mut dyn MachineCore,
    media: &MediaSet<'_>,
    commands: &[ControlCommand],
) -> Result<(), MachineError> {
    if !media.is_empty() {
        machine.load_media(media)?;
    }

    for command in commands {
        machine.command(command)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CapabilitySet, HostIo, MachineProfile, MachineTime, ResetKind, RunResult, StopReason,
    };

    #[derive(Default)]
    struct DummyMachine {
        restored: bool,
        media_loaded: bool,
        commands: usize,
    }

    impl MachineCore for DummyMachine {
        fn profile(&self) -> &MachineProfile {
            panic!("profile access is not needed in this test")
        }

        fn time(&self) -> MachineTime {
            MachineTime::default()
        }

        fn reset(&mut self, _kind: ResetKind) {}

        fn load_media(&mut self, _media: &MediaSet<'_>) -> Result<(), MachineError> {
            self.media_loaded = true;
            Ok(())
        }

        fn run_until(
            &mut self,
            target: MachineTime,
            _host: &mut HostIo<'_>,
        ) -> Result<RunResult, MachineError> {
            Ok(RunResult::new(target, StopReason::ReachedTarget))
        }

        fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
            Ok(vec![])
        }

        fn restore(&mut self, _bytes: &[u8]) -> Result<(), MachineError> {
            self.restored = true;
            Ok(())
        }

        fn command(&mut self, _command: &ControlCommand) -> Result<(), MachineError> {
            self.commands += 1;
            Ok(())
        }

        fn capabilities(&self) -> CapabilitySet {
            CapabilitySet::new()
        }
    }

    #[test]
    fn boot_machine_requires_firmware_or_snapshot() {
        let artifacts = BootArtifacts::new();
        let result = boot_machine(
            &artifacts,
            |_firmware| Ok(DummyMachine::default()),
            DummyMachine::default,
        );

        assert!(matches!(
            result,
            Err(MachineError::InvalidRequest { ref reason })
                if reason == "either firmware or a snapshot must be supplied"
        ));
    }

    #[test]
    fn boot_machine_uses_blank_runtime_for_snapshot_only_restore() {
        let artifacts = BootArtifacts {
            firmware: FirmwareSet::new(),
            snapshot: Some(&[1, 2, 3]),
        };
        let machine = boot_machine(
            &artifacts,
            |_firmware| Ok(DummyMachine::default()),
            DummyMachine::default,
        )
        .expect("snapshot-only boot should construct a blank runtime and restore it");

        assert!(machine.restored);
    }

    #[test]
    fn prepare_machine_loads_media_then_commands() {
        let mut machine = DummyMachine::default();
        let mut media = MediaSet::new();
        media.push(crate::MediaImage::new(
            "tape-1",
            crate::MediaKind::Tape,
            &[0x00],
        ));
        let commands = [ControlCommand::MediaTransport(
            crate::MediaTransportCommand::new("tape-1", crate::MediaTransportAction::Start),
        )];

        prepare_machine(&mut machine, &media, &commands)
            .expect("media and command preparation should succeed");

        assert!(machine.media_loaded);
        assert_eq!(machine.commands, 1);
    }
}
