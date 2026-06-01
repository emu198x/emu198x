//! Shared query surface above one headless session.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::machine::{MachineProfile, RunResult, StopReason};
use crate::time::MachineTime;

/// Stable generic session query paths.
pub const SESSION_QUERY_PATHS: &[&str] = &[
    "capture.has_audio",
    "capture.has_frame",
    "run.last.reached",
    "run.last.stop_reason",
    "session.native_frame_ticks",
    "session.profile.capabilities",
    "session.profile.clock.rate.denominator_hz",
    "session.profile.clock.rate.numerator_hz",
    "session.profile.clock.unit",
    "session.profile.display_name",
    "session.profile.family",
    "session.profile.firmware.ids",
    "session.profile.machine_id",
    "session.profile.media_slots.ids",
    "session.profile.profile_id",
    "session.profile.region",
    "session.profile.release_year",
    "session.profile.summary",
    "session.profile.support_tier",
    "session.time",
];

/// Optional family-owned query surface that can extend one headless session.
pub trait SessionQueryProvider<M> {
    /// Returns additional query paths owned by this provider.
    #[must_use]
    fn query_paths(&self, _machine: &M, _prefix: Option<&str>) -> Vec<String> {
        Vec::new()
    }

    /// Resolves one provider-owned query path.
    ///
    /// Returns `Ok(None)` when the provider does not own the path.
    fn query(&self, _machine: &M, _path: &str) -> Result<Option<QueryResult>, QueryError> {
        Ok(None)
    }
}

/// Default query provider with no family-owned paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoAdditionalQueries;

impl<M> SessionQueryProvider<M> for NoAdditionalQueries {}

/// Error surfaced by the shared session query layer.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum QueryError {
    /// The requested query path is not part of the shared session surface.
    #[error("query path {path} is not known")]
    UnknownPath {
        /// The unknown path.
        path: String,
    },

    /// The requested query path exists but has no value yet.
    #[error("query path {path} is unavailable: {reason}")]
    UnavailablePath {
        /// The query path that could not be served.
        path: String,
        /// Human-readable availability note.
        reason: &'static str,
    },
}

/// One resolved query response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueryResult {
    /// The query path that was resolved.
    pub path: String,
    /// JSON value for the resolved path.
    pub value: Value,
}

/// One query-path listing response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryPathsResult {
    /// Optional prefix filter that was applied.
    pub prefix: Option<String>,
    /// Matching query paths in sorted order.
    pub paths: Vec<String>,
}

/// Returns the shared session query paths matching one optional prefix.
#[must_use]
pub fn query_paths(prefix: Option<&str>) -> QueryPathsResult {
    let mut paths: Vec<String> = SESSION_QUERY_PATHS
        .iter()
        .copied()
        .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
        .map(str::to_owned)
        .collect();
    paths.sort_unstable();

    QueryPathsResult {
        prefix: prefix.map(str::to_owned),
        paths,
    }
}

/// Resolves one shared session query path.
pub fn query_value(
    profile: &MachineProfile,
    time: MachineTime,
    native_frame_ticks: u64,
    has_frame: bool,
    has_audio: bool,
    last_run_result: Option<RunResult>,
    path: &str,
) -> Result<QueryResult, QueryError> {
    let value = match path {
        "session.time" => json!(time.get()),
        "session.native_frame_ticks" => json!(native_frame_ticks),
        "session.profile.machine_id" => json!(profile.machine_id.as_str()),
        "session.profile.profile_id" => json!(profile.profile_id.as_str()),
        "session.profile.display_name" => json!(profile.display_name.as_ref()),
        "session.profile.family" => json!(family_name(profile)),
        "session.profile.region" => json!(region_name(profile)),
        "session.profile.support_tier" => json!(support_tier_name(profile)),
        "session.profile.release_year" => json!(profile.release_year),
        "session.profile.summary" => json!(profile.summary.as_ref()),
        "session.profile.clock.unit" => json!(profile.clock.unit.as_ref()),
        "session.profile.clock.rate.numerator_hz" => json!(profile.clock.rate.numerator_hz),
        "session.profile.clock.rate.denominator_hz" => json!(profile.clock.rate.denominator_hz),
        "session.profile.capabilities" => json!(
            profile
                .capabilities
                .iter()
                .map(|capability| capability.as_str())
                .collect::<Vec<_>>()
        ),
        "session.profile.firmware.ids" => json!(
            profile
                .firmware
                .iter()
                .map(|firmware| firmware.id.as_ref())
                .collect::<Vec<_>>()
        ),
        "session.profile.media_slots.ids" => json!(
            profile
                .media_slots
                .iter()
                .map(|slot| slot.id.as_ref())
                .collect::<Vec<_>>()
        ),
        "capture.has_frame" => json!(has_frame),
        "capture.has_audio" => json!(has_audio),
        "run.last.reached" => json!(
            last_run_result
                .ok_or_else(|| QueryError::UnavailablePath {
                    path: path.to_owned(),
                    reason: "no run result is available yet",
                })?
                .reached
                .get()
        ),
        "run.last.stop_reason" => json!(stop_reason_name(
            last_run_result
                .ok_or_else(|| QueryError::UnavailablePath {
                    path: path.to_owned(),
                    reason: "no run result is available yet",
                })?
                .stop_reason,
        )),
        _ => {
            return Err(QueryError::UnknownPath {
                path: path.to_owned(),
            });
        }
    };

    Ok(QueryResult {
        path: path.to_owned(),
        value,
    })
}

fn family_name(profile: &MachineProfile) -> &'static str {
    match profile.family {
        crate::Family::Spectrum => "spectrum",
        crate::Family::C64 => "c64",
        crate::Family::Nes => "nes",
        crate::Family::Amiga => "amiga",
        crate::Family::GameBoy => "game-boy",
        crate::Family::Dragon => "dragon",
        crate::Family::Msx => "msx",
    }
}

fn region_name(profile: &MachineProfile) -> &'static str {
    match profile.region {
        crate::Region::Pal => "pal",
        crate::Region::Ntsc => "ntsc",
        crate::Region::Other => "other",
    }
}

fn support_tier_name(profile: &MachineProfile) -> &'static str {
    match profile.support_tier {
        crate::SupportTier::Research => "research",
        crate::SupportTier::Boots => "boots",
        crate::SupportTier::Usable => "usable",
        crate::SupportTier::Teaching => "teaching",
        crate::SupportTier::Reference => "reference",
    }
}

fn stop_reason_name(stop_reason: StopReason) -> &'static str {
    match stop_reason {
        crate::StopReason::ReachedTarget => "reached_target",
        crate::StopReason::WaitingForInput => "waiting_for_input",
        crate::StopReason::Breakpoint => "breakpoint",
        crate::StopReason::Halted => "halted",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::machine::{
        Family, MachineId, MachineProfile, ProfileId, Region, RunResult, StopReason, SupportTier,
    };
    use crate::media::{FirmwareRequirement, MediaSlot, WritebackPolicy};
    use crate::time::{ClockDesc, ClockRate};
    use crate::{MediaKind, known_capability};

    fn test_profile() -> MachineProfile {
        MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from("sinclair-zx-spectrum-48k-pal"),
            display_name: "ZX Spectrum 48K (PAL)".into(),
            family: Family::Spectrum,
            region: Region::Pal,
            support_tier: SupportTier::Boots,
            release_year: 1982,
            summary: "test profile".into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
            firmware: vec![FirmwareRequirement::new("rom-0", "ROM 0", false)],
            media_slots: vec![MediaSlot::new(
                "tape-1",
                "Tape Deck",
                MediaKind::Tape,
                false,
                WritebackPolicy::InMemoryOnly,
            )],
            capabilities: CapabilitySet::with_all([known_capability("snapshot-export")]),
        }
    }

    #[test]
    fn query_paths_can_filter_by_prefix() {
        let paths = query_paths(Some("session.profile."));
        assert!(
            paths
                .paths
                .iter()
                .all(|path| path.starts_with("session.profile."))
        );
        assert!(
            paths
                .paths
                .contains(&"session.profile.profile_id".to_owned())
        );
    }

    #[test]
    fn query_value_reads_profile_and_run_state() {
        let profile = test_profile();
        let time = MachineTime::new(1234);
        let last_run = Some(RunResult::new(
            MachineTime::new(5678),
            StopReason::ReachedTarget,
        ));

        let profile_id = query_value(
            &profile,
            time,
            69888,
            true,
            false,
            last_run,
            "session.profile.profile_id",
        )
        .expect("profile id should resolve");
        let stop_reason = query_value(
            &profile,
            time,
            69888,
            true,
            false,
            last_run,
            "run.last.stop_reason",
        )
        .expect("stop reason should resolve");

        assert_eq!(profile_id.value, json!("sinclair-zx-spectrum-48k-pal"));
        assert_eq!(stop_reason.value, json!("reached_target"));
    }

    #[test]
    fn query_value_rejects_missing_run_state() {
        let result = query_value(
            &test_profile(),
            MachineTime::new(0),
            69888,
            false,
            false,
            None,
            "run.last.reached",
        );

        assert!(matches!(
            result,
            Err(QueryError::UnavailablePath { ref path, .. }) if path == "run.last.reached"
        ));
    }
}
