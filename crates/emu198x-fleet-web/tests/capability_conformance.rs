//! Fleet capability conformance: what a profile declares agrees with what the
//! runtime does.
//!
//! A `CapabilitySet` is only worth reading if it is true. Nothing in the type
//! makes it true — `known_capability` wraps any string — so this test derives
//! the truth from behaviour instead of trusting the declaration. Every variant
//! of every family is built from synthetic firmware (zero-filled images sized
//! until the runtime accepts them; no ROM is needed, so this runs in CI), given
//! a synthetic cartridge if it will not run without one, and run for a few
//! frames. A runtime that pushed non-empty audio packets must declare
//! `audio-output`; one that ran and pushed none must not. Both the profile
//! (`FamilyRuntime::profile_for`) and the live machine
//! (`MachineCore::capabilities`) are checked, since they are separate code.
//!
//! Synthetic firmware executes garbage, which is the point: whether a runtime
//! *emits samples* does not depend on what the ROM plays. Silence is still
//! samples; only a machine with no sound path emits none.
//!
//! The fleet list below is checked against both the compiled catalogue and
//! `docs/status/systems.toml`, so a new system cannot join the fleet without
//! being probed here.
//!
//! See emu198x/emu198x#1369 and `knowledge/decisions/tools-follow-the-machine-spec.md`.

use std::collections::{BTreeMap, BTreeSet};

use emu198x_shell::capability::ids::AUDIO_OUTPUT;
use emu198x_shell::{
    AudioPacket, AudioSink, FamilyRuntime, FirmwareImage, FirmwareSet, FramePacket, FrameSink,
    HostIo, MachineError, MachineTime, MediaImage, MediaKind, MediaSet, NullTraceSink,
    known_capability,
};

/// Frames each variant runs before its output is judged.
const FRAMES: usize = 5;

/// Firmware sizes tried, smallest first, until the runtime stops rejecting
/// the image. Covers every ROM in the fleet from a 2 KiB character ROM to a
/// 512 KiB Kickstart.
const FIRMWARE_SIZES: &[usize] = &[
    256, 512, 1024, 2048, 4096, 6144, 8192, 10240, 12288, 16384, 20480, 24576, 32768, 40960, 49152,
    65536, 98304, 131_072, 262_144, 524_288, 1_048_576,
];

/// What one variant declared and did.
struct Observation {
    variant: String,
    machine_id: String,
    profile_declares: bool,
    live_declares: bool,
    frames: usize,
    samples: usize,
}

#[derive(Default)]
struct Counter {
    frames: usize,
    samples: usize,
}

impl FrameSink for Counter {
    fn push_frame(&mut self, _frame: FramePacket<'_>) -> Result<(), MachineError> {
        self.frames += 1;
        Ok(())
    }
}

struct Samples<'a>(&'a mut usize);

impl AudioSink for Samples<'_> {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        *self.0 += packet.samples.len();
        Ok(())
    }
}

fn run_frames<R: FamilyRuntime>(runtime: &mut R, counter: &mut Counter) -> Result<(), String> {
    let ticks = runtime.native_frame_ticks();
    let mut samples = 0;
    let mut frames = Counter::default();
    for _ in 0..FRAMES {
        let target = MachineTime::new(runtime.time().get().saturating_add(ticks));
        let mut audio = Samples(&mut samples);
        let mut trace = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frames,
            audio_sink: &mut audio,
            trace_sink: &mut trace,
        };
        runtime
            .run_until(target, &mut host)
            .map_err(|e| format!("run failed: {e}"))?;
    }
    counter.frames += frames.frames;
    counter.samples += samples;
    Ok(())
}

/// Builds `model` from zero-filled firmware, growing each image the runtime
/// rejects until it is accepted. Each image's first byte is distinct, because
/// some families refuse a pair of identical ROMs.
fn build<R: FamilyRuntime>(model: R::Model) -> Result<R, String> {
    let profile = R::profile_for(model);
    let mut sizes: BTreeMap<String, usize> = profile
        .firmware
        .iter()
        .filter(|f| !f.optional)
        .map(|f| (f.id.to_string(), 0))
        .collect();
    for _ in 0..FIRMWARE_SIZES.len() * (sizes.len() + 2) {
        let images: Vec<(String, Vec<u8>)> = sizes
            .iter()
            .enumerate()
            .map(|(n, (id, size))| {
                let mut bytes = vec![0u8; FIRMWARE_SIZES[*size]];
                bytes[0] = u8::try_from(n + 1).unwrap_or(u8::MAX);
                (id.clone(), bytes)
            })
            .collect();
        let mut firmware = FirmwareSet::new();
        for (id, bytes) in &images {
            firmware.push(FirmwareImage::new(id.clone(), bytes.as_slice()));
        }
        match R::from_firmware(model, &firmware) {
            Ok(runtime) => return Ok(runtime),
            Err(MachineError::InvalidFirmware { id, reason }) => match sizes.get_mut(&id) {
                Some(size) if *size + 1 < FIRMWARE_SIZES.len() => *size += 1,
                _ => return Err(format!("firmware {id} rejected at every size: {reason}")),
            },
            Err(MachineError::MissingFirmware { id }) if !sizes.contains_key(&id) => {
                sizes.insert(id, 0);
            }
            Err(e) => return Err(format!("cannot build from synthetic firmware: {e}")),
        }
    }
    Err("firmware size search did not converge".into())
}

/// Synthetic cartridges, tried in order until a slot accepts one: an iNES
/// image, a Game Boy image with a valid header checksum, then bare
/// zero-filled ROMs.
fn synthetic_cartridges() -> Vec<Vec<u8>> {
    let mut ines = b"NES\x1a\x01\x01".to_vec();
    ines.resize(16, 0);
    ines.resize(16 + 16384 + 8192, 0);

    let mut game_boy = vec![0u8; 32768];
    let checksum = game_boy[0x134..=0x14C]
        .iter()
        .fold(0u8, |x, b| x.wrapping_sub(*b).wrapping_sub(1));
    game_boy[0x14D] = checksum;

    let mut images = vec![ines, game_boy];
    images.extend(
        FIRMWARE_SIZES
            .iter()
            .filter(|size| **size >= 2048)
            .map(|size| vec![0u8; *size]),
    );
    images
}

fn insert_cartridge<R: FamilyRuntime>(runtime: &mut R) -> bool {
    let slots: Vec<_> = runtime
        .profile()
        .media_slots
        .iter()
        .filter(|slot| slot.kind == MediaKind::Cartridge)
        .map(|slot| slot.id.clone())
        .collect();
    for slot in slots {
        for image in synthetic_cartridges() {
            let mut media = MediaSet::new();
            media.push(MediaImage::new(
                slot.clone(),
                MediaKind::Cartridge,
                image.as_slice(),
            ));
            if runtime.load_media(&media).is_ok() {
                return true;
            }
        }
    }
    false
}

fn observe<R: FamilyRuntime>() -> Vec<Result<Observation, String>> {
    let audio = known_capability(AUDIO_OUTPUT);
    R::variant_ids()
        .iter()
        .map(|id| {
            let model = R::model_from_id(id).ok_or_else(|| format!("{id}: no model"))?;
            let profile = R::profile_for(model);
            let mut runtime = build::<R>(model).map_err(|e| format!("{id}: {e}"))?;
            let mut counter = Counter::default();
            run_frames(&mut runtime, &mut counter).map_err(|e| format!("{id}: {e}"))?;
            if counter.frames == 0 && insert_cartridge(&mut runtime) {
                run_frames(&mut runtime, &mut counter).map_err(|e| format!("{id}: {e}"))?;
            }
            Ok(Observation {
                variant: (*id).to_owned(),
                machine_id: profile.machine_id.as_str().to_owned(),
                profile_declares: profile.capabilities.contains(&audio),
                live_declares: runtime.capabilities().contains(&audio),
                frames: counter.frames,
                samples: counter.samples,
            })
        })
        .collect()
}

/// Every family the fleet compiles, observed. Adding a family to the
/// catalogue without adding it here fails `the_probe_covers_the_whole_fleet`.
fn fleet() -> Vec<(&'static str, Vec<Result<Observation, String>>)> {
    vec![
        ("acorn-atom", observe::<runtime_acorn_atom::AtomRuntime>()),
        (
            "acorn-bbc-micro",
            observe::<runtime_acorn_bbc_micro::BbcMicroRuntime>(),
        ),
        (
            "acorn-electron",
            observe::<runtime_acorn_electron::ElectronRuntime>(),
        ),
        (
            "amstrad-cpc",
            observe::<runtime_amstrad_cpc::AmstradCpcRuntime>(),
        ),
        (
            "atari-2600",
            observe::<runtime_atari_2600::Atari2600Runtime>(),
        ),
        (
            "atari-5200",
            observe::<runtime_atari_5200::Atari5200Runtime>(),
        ),
        (
            "atari-7800",
            observe::<runtime_atari_7800::Atari7800Runtime>(),
        ),
        (
            "atari-800xl",
            observe::<runtime_atari_800xl::Atari800xlRuntime>(),
        ),
        (
            "coleco-colecovision",
            observe::<runtime_coleco_colecovision::CvRuntime>(),
        ),
        (
            "commodore-amiga",
            observe::<runtime_commodore_amiga::AmigaRuntimeKind>(),
        ),
        (
            "commodore-c64",
            observe::<runtime_commodore_c64::C64Runtime>(),
        ),
        (
            "commodore-pet",
            observe::<runtime_commodore_pet::PetRuntime>(),
        ),
        (
            "commodore-vic-20",
            observe::<runtime_commodore_vic_20::Vic20Runtime>(),
        ),
        ("dragon", observe::<runtime_dragon::DragonRuntime>()),
        (
            "jupiter-ace",
            observe::<runtime_jupiter_ace::JupiterAceRuntime>(),
        ),
        (
            "mattel-aquarius",
            observe::<runtime_mattel_aquarius::AquariusRuntime>(),
        ),
        (
            "memotech-mtx",
            observe::<runtime_memotech_mtx::MtxRuntime>(),
        ),
        ("msx", observe::<runtime_msx::MsxRuntime>()),
        (
            "nintendo-game-boy",
            observe::<runtime_nintendo_game_boy::GameBoyRuntime>(),
        ),
        (
            "nintendo-nes",
            observe::<runtime_nintendo_nes::NesRuntime>(),
        ),
        ("oric-atmos", observe::<runtime_oric_atmos::OricRuntime>()),
        (
            "sega-game-gear",
            observe::<runtime_sega_game_gear::SmsRuntime>(),
        ),
        (
            "sega-master-system",
            observe::<runtime_sega_master_system::SmsRuntime>(),
        ),
        (
            "sega-sg-1000",
            observe::<runtime_sega_sg_1000::Sg1000Runtime>(),
        ),
        (
            "sinclair-zx-spectrum",
            observe::<runtime_sinclair_zx_spectrum::SpectrumRuntimeKind>(),
        ),
        (
            "sinclair-zx80",
            observe::<runtime_sinclair_zx80::Zx80Runtime>(),
        ),
        (
            "sinclair-zx81",
            observe::<runtime_sinclair_zx81::Zx81Runtime>(),
        ),
        ("sord-m5", observe::<runtime_sord_m5::M5Runtime>()),
        (
            "spectravideo-svi-328",
            observe::<runtime_spectravideo_svi_328::Svi328Runtime>(),
        ),
        (
            "tatung-einstein",
            observe::<runtime_tatung_einstein::EinsteinRuntime>(),
        ),
    ]
}

/// Machine ids in the system registry, the list CI already holds every
/// profile to.
fn registry_machine_ids() -> BTreeSet<String> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/status/systems.toml"
    );
    let text = std::fs::read_to_string(path).expect("the system registry is readable");
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("machine_id = \""))
        .filter_map(|rest| rest.strip_suffix('"'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn audio_output_is_declared_exactly_when_samples_are_emitted() {
    let mut problems = Vec::new();
    let mut machine_ids = BTreeSet::new();
    let mut families = BTreeSet::new();
    for (family, observations) in fleet() {
        families.insert(family.to_owned());
        for observation in observations {
            let o = match observation {
                Ok(o) => o,
                Err(e) => {
                    problems.push(format!("{family}/{e}"));
                    continue;
                }
            };
            machine_ids.insert(o.machine_id.clone());
            let name = format!("{family}/{}", o.variant);
            if o.frames == 0 {
                problems.push(format!(
                    "{name}: produced no frames, so its audio cannot be judged"
                ));
                continue;
            }
            if o.profile_declares != o.live_declares {
                problems.push(format!(
                    "{name}: profile_for says {AUDIO_OUTPUT}={} but the live machine says {}",
                    o.profile_declares, o.live_declares
                ));
            }
            let emits = o.samples > 0;
            if emits != o.profile_declares {
                problems.push(format!(
                    "{name}: emitted {} samples over {} frames but {} {AUDIO_OUTPUT}",
                    o.samples,
                    o.frames,
                    if o.profile_declares {
                        "declares"
                    } else {
                        "does not declare"
                    },
                ));
            }
        }
    }

    let catalogue: BTreeSet<String> = emu198x_fleet_web::catalogue()
        .iter()
        .filter_map(|entry| entry["family"].as_str().map(str::to_owned))
        .collect();
    if families != catalogue {
        problems.push(format!(
            "probed families differ from the compiled catalogue: probed-only {:?}, unprobed {:?}",
            families.difference(&catalogue).collect::<Vec<_>>(),
            catalogue.difference(&families).collect::<Vec<_>>(),
        ));
    }
    let registry = registry_machine_ids();
    if machine_ids != registry {
        problems.push(format!(
            "probed machines differ from docs/status/systems.toml: probed-only {:?}, unprobed {:?}",
            machine_ids.difference(&registry).collect::<Vec<_>>(),
            registry.difference(&machine_ids).collect::<Vec<_>>(),
        ));
    }

    assert!(
        problems.is_empty(),
        "capability declarations disagree with behaviour:\n  {}",
        problems.join("\n  ")
    );
}
