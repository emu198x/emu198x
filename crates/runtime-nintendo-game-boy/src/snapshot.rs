//! Versioned postcard envelopes; version 3 uses its exact legacy machine layout.
use crate::runtime::GameBoyRuntime;
use emu198x_shell::{MachineError, MachineTime};
use machine_nintendo_game_boy::{GameBoy, GameBoyV3};
use serde::{Deserialize, Serialize};

const SNAPSHOT_VERSION: u32 = 4;

#[derive(Serialize)]
struct GameBoyRuntimeSnapshotRefV4<'a> {
    version: u32,
    profile_id: &'a str,
    time: MachineTime,
    cartridge_bytes: Option<&'a [u8]>,
    machine: Option<&'a GameBoy>,
    boot_rom: Option<&'a [u8]>,
}

#[derive(Deserialize)]
struct SnapshotV4 {
    version: u32,
    profile_id: String,
    time: MachineTime,
    cartridge_bytes: Option<Vec<u8>>,
    machine: Option<GameBoy>,
    boot_rom: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct SnapshotV3 {
    version: u32,
    profile_id: String,
    time: MachineTime,
    cartridge_bytes: Option<Vec<u8>>,
    machine: Option<GameBoyV3>,
}

fn invalid(reason: impl Into<String>) -> MachineError {
    MachineError::InvalidSnapshot {
        reason: reason.into(),
    }
}

fn decode_exact<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, MachineError> {
    let (value, tail) = postcard::take_from_bytes(bytes)
        .map_err(|error| invalid(format!("decode failed: {error}")))?;
    if !tail.is_empty() {
        return Err(invalid("snapshot has trailing bytes"));
    }
    Ok(value)
}

pub(crate) fn encode(runtime: &GameBoyRuntime) -> Result<Vec<u8>, MachineError> {
    postcard::to_allocvec(&GameBoyRuntimeSnapshotRefV4 {
        version: SNAPSHOT_VERSION,
        profile_id: runtime.profile().profile_id.as_str(),
        time: runtime.time_value(),
        cartridge_bytes: runtime.cartridge_bytes(),
        machine: runtime.machine(),
        boot_rom: runtime.boot_rom(),
    })
    .map_err(|error| invalid(format!("encode failed: {error}")))
}

pub(crate) fn decode(runtime: &mut GameBoyRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let (version, _) = postcard::take_from_bytes::<u32>(bytes)
        .map_err(|error| invalid(format!("decode failed: {error}")))?;
    let snapshot = match version {
        3 => {
            let old: SnapshotV3 = decode_exact(bytes)?;
            let machine = old.machine.map(GameBoy::from);
            let boot_rom = machine
                .as_ref()
                .and_then(GameBoy::boot_rom)
                .map(<[u8]>::to_vec);
            SnapshotV4 {
                version: old.version,
                profile_id: old.profile_id,
                time: old.time,
                cartridge_bytes: old.cartridge_bytes,
                machine,
                boot_rom,
            }
        }
        SNAPSHOT_VERSION => decode_exact(bytes)?,
        other => return Err(invalid(format!("unsupported snapshot version {other}"))),
    };
    if snapshot.version != version {
        return Err(invalid("snapshot version mismatch"));
    }
    if snapshot.profile_id != runtime.profile().profile_id.as_str() {
        return Err(invalid(format!(
            "snapshot profile {} does not match runtime profile {}",
            snapshot.profile_id,
            runtime.profile().profile_id.as_str()
        )));
    }
    if let Some(boot) = &snapshot.boot_rom {
        let Some((_, size)) = runtime.model().boot_firmware() else {
            return Err(invalid("boot firmware is not supported on this model"));
        };
        if boot.len() != size {
            return Err(invalid("snapshot boot firmware has the wrong size"));
        }
    }
    if let Some(machine) = &snapshot.machine {
        machine
            .validate_boot_state(runtime.model().boot_profile())
            .map_err(invalid)?;
        if machine.boot_rom() != snapshot.boot_rom.as_deref() {
            return Err(invalid("snapshot firmware and machine boot image disagree"));
        }
    }
    // All decoding and validation completes before replacing any live state.
    runtime.set_machine(snapshot.machine);
    runtime.set_cartridge_bytes(snapshot.cartridge_bytes);
    runtime.set_boot_rom(snapshot.boot_rom);
    runtime.set_time(snapshot.time);
    runtime.clear_audio_buffer();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{GameBoyRuntimeSnapshotRefV4, decode};
    use crate::{Model, runtime::GameBoyRuntime};
    use emu198x_shell::{MachineError, MachineTime};

    /// Decode rejects a snapshot whose version field is not the one
    /// the runtime knows. We can't reach this via the public encode
    /// path (which always writes `SNAPSHOT_VERSION`), so craft a
    /// future-version envelope here and confirm the version check
    /// fails before profile validation.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let bytes = postcard::to_allocvec(&GameBoyRuntimeSnapshotRefV4 {
            version: 999,
            profile_id: runtime.profile().profile_id.as_str(),
            time: MachineTime::default(),
            cartridge_bytes: None,
            machine: None,
            boot_rom: None,
        })
        .expect("synthetic envelope should encode");

        let err = decode(&mut runtime, &bytes).expect_err("future version should reject");
        match err {
            MachineError::InvalidSnapshot { reason } => {
                assert!(
                    reason.contains("unsupported snapshot version"),
                    "unexpected reason: {reason}",
                );
            }
            other => panic!("expected InvalidSnapshot, got {other:?}"),
        }
    }
    #[test]
    fn malformed_v4_firmware_and_trailing_data_do_not_replace_live_state() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let before = super::encode(&runtime).expect("before");
        let wrong_size = [0; 255];
        let invalid = postcard::to_allocvec(&GameBoyRuntimeSnapshotRefV4 {
            version: 4,
            profile_id: runtime.profile().profile_id.as_str(),
            time: MachineTime::new(123),
            cartridge_bytes: None,
            machine: None,
            boot_rom: Some(&wrong_size),
        })
        .expect("invalid firmware fixture");
        let mut trailing = before.clone();
        trailing.push(0);
        for bytes in [&invalid[..], &trailing[..], &before[..before.len() - 1]] {
            assert!(decode(&mut runtime, bytes).is_err());
            assert_eq!(super::encode(&runtime).expect("unchanged"), before);
        }
    }

    #[test]
    fn firmware_cache_and_machine_cannot_disagree_on_restore() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let before = super::encode(&runtime).expect("before");
        let rom =
            include_bytes!("../../../test-data/synthetic-cartridges/nintendo-game-boy-logo.gb")
                .to_vec();
        let (_, machine) = machine_nintendo_game_boy::GameBoy::try_from_rom_with_boot_rom(
            rom,
            Model::Dmg.boot_profile(),
            Some(vec![0; 256]),
        )
        .expect("synthetic boot machine");
        let other = [1; 256];
        let bytes = postcard::to_allocvec(&GameBoyRuntimeSnapshotRefV4 {
            version: 4,
            profile_id: runtime.profile().profile_id.as_str(),
            time: MachineTime::new(123),
            cartridge_bytes: Some(machine.cartridge().rom()),
            machine: Some(&machine),
            boot_rom: Some(&other),
        })
        .expect("inconsistent firmware fixture");
        assert!(decode(&mut runtime, &bytes).is_err());
        assert_eq!(super::encode(&runtime).expect("unchanged"), before);
    }
    #[test]
    fn machine_hardware_must_match_the_envelope_profile() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let before = super::encode(&runtime).expect("before");
        let rom =
            include_bytes!("../../../test-data/synthetic-cartridges/nintendo-game-boy-logo.gb")
                .to_vec();
        let (_, machine) = machine_nintendo_game_boy::GameBoy::try_from_rom_with_boot_rom(
            rom,
            Model::Cgb.boot_profile(),
            None,
        )
        .expect("colour machine");
        let bytes = postcard::to_allocvec(&GameBoyRuntimeSnapshotRefV4 {
            version: 4,
            profile_id: runtime.profile().profile_id.as_str(),
            time: MachineTime::new(123),
            cartridge_bytes: Some(machine.cartridge().rom()),
            machine: Some(&machine),
            boot_rom: None,
        })
        .expect("mislabelled hardware fixture");
        assert!(decode(&mut runtime, &bytes).is_err());
        assert_eq!(super::encode(&runtime).expect("unchanged"), before);
    }
}
