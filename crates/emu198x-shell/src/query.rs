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
    "session.display.kind",
    "session.display.pixel_clock_hz",
    "session.display.lines_per_tv_height",
    "session.framebuffer.height",
    "session.framebuffer.width",
    "session.profile.summary",
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

/// The session state the shared query paths read.
///
/// A struct rather than eight parameters: the display was the eighth, and a
/// list that long stops being readable at the call site.
#[derive(Clone, Copy)]
pub struct SessionView<'a> {
    /// Profile of the machine as currently configured.
    pub profile: &'a MachineProfile,
    /// What the machine's video output reaches, if it has said.
    pub display: Option<crate::display::Display>,
    /// Machine time reached.
    pub time: MachineTime,
    /// Authoritative ticks in one native frame.
    pub native_frame_ticks: u64,
    /// Whether a frame has been captured.
    pub has_frame: bool,
    /// Extent of the last captured frame, if there has been one.
    pub framebuffer: Option<(u32, u32)>,
    /// Whether audio has been captured.
    pub has_audio: bool,
    /// Result of the last run step.
    pub last_run_result: Option<RunResult>,
}

/// Resolves one shared session query path.
pub fn query_value(view: &SessionView<'_>, path: &str) -> Result<QueryResult, QueryError> {
    let SessionView {
        profile,
        display,
        time,
        native_frame_ticks,
        has_frame,
        framebuffer,
        has_audio,
        last_run_result,
    } = *view;
    let value = match path {
        "session.time" => json!(time.get()),
        "session.native_frame_ticks" => json!(native_frame_ticks),
        "session.profile.machine_id" => json!(profile.machine_id.as_str()),
        "session.profile.profile_id" => json!(profile.profile_id.as_str()),
        "session.profile.display_name" => json!(profile.display_name.as_ref()),
        "session.profile.family" => json!(family_name(profile)),
        "session.profile.region" => json!(region_name(profile)),
        "session.profile.release_year" => json!(profile.release_year),
        "session.profile.summary" => json!(profile.summary.as_ref()),
        // The display is the machine's, not the profile's, because it can move
        // under a running machine. A core that has not stated one answers null
        // rather than guessing a television.
        "session.display.kind" => match display {
            Some(crate::display::Display::Television { .. }) => json!("television"),
            Some(crate::display::Display::Lcd) => json!("lcd"),
            Some(crate::display::Display::Monitor { .. }) => json!("monitor"),
            _ => json!(null),
        },
        "session.display.pixel_clock_hz" => match display {
            Some(crate::display::Display::Television { pixel_clock_hz, .. }) => {
                json!(pixel_clock_hz)
            }
            _ => json!(null),
        },
        "session.display.lines_per_tv_height" => match display {
            Some(crate::display::Display::Television {
                lines_per_tv_height,
                ..
            }) => json!(lines_per_tv_height),
            _ => json!(null),
        },
        // Read off the last frame the machine emitted rather than stated
        // anywhere, so it cannot disagree with what the core actually draws.
        // Null until a frame exists: the extent is a fact about output, and a
        // machine that has not run has produced none.
        //
        // Paired with the display, this is the instrument for #1054 — a
        // television's window is `pixel_clock × active_line_seconds` wide by
        // `lines_per_tv_height` tall, so the two together say how much of the
        // raster a core keeps.
        "session.framebuffer.width" => match framebuffer {
            Some((width, _)) => json!(width),
            None => json!(null),
        },
        "session.framebuffer.height" => match framebuffer {
            Some((_, height)) => json!(height),
            None => json!(null),
        },
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
        crate::Family::Other => "other",
    }
}

fn region_name(profile: &MachineProfile) -> &'static str {
    match profile.region {
        crate::Region::Pal => "pal",
        crate::Region::Ntsc => "ntsc",
        crate::Region::Other => "other",
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
        Family, MachineId, MachineProfile, ProfileId, Region, RunResult, StopReason,
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

        let view = SessionView {
            profile: &profile,
            display: None,
            time,
            native_frame_ticks: 69888,
            has_frame: true,
            framebuffer: Some((256, 192)),
            has_audio: false,
            last_run_result: last_run,
        };

        let profile_id =
            query_value(&view, "session.profile.profile_id").expect("profile id should resolve");
        let stop_reason =
            query_value(&view, "run.last.stop_reason").expect("stop reason should resolve");

        assert_eq!(profile_id.value, json!("sinclair-zx-spectrum-48k-pal"));
        assert_eq!(stop_reason.value, json!("reached_target"));
    }

    #[test]
    fn the_framebuffer_extent_is_null_until_a_frame_exists() {
        let profile = test_profile();
        let view = SessionView {
            profile: &profile,
            display: None,
            time: MachineTime::new(0),
            native_frame_ticks: 69888,
            has_frame: false,
            framebuffer: None,
            has_audio: false,
            last_run_result: None,
        };

        // Null rather than an error or a zero. The extent is a fact about
        // output, and a machine that has not drawn has not stated one — which
        // an audit needs to be able to tell apart from a core that draws
        // nothing.
        assert_eq!(
            query_value(&view, "session.framebuffer.width")
                .expect("the path exists")
                .value,
            json!(null)
        );
    }

    #[test]
    fn the_framebuffer_extent_comes_from_the_frame() {
        let profile = test_profile();
        let view = SessionView {
            profile: &profile,
            display: None,
            time: MachineTime::new(0),
            native_frame_ticks: 69888,
            has_frame: true,
            framebuffer: Some((352, 296)),
            has_audio: false,
            last_run_result: None,
        };

        assert_eq!(
            query_value(&view, "session.framebuffer.width")
                .expect("the path exists")
                .value,
            json!(352)
        );
        assert_eq!(
            query_value(&view, "session.framebuffer.height")
                .expect("the path exists")
                .value,
            json!(296)
        );
    }

    #[test]
    fn query_value_rejects_missing_run_state() {
        let profile = test_profile();
        let view = SessionView {
            profile: &profile,
            display: None,
            time: MachineTime::new(0),
            native_frame_ticks: 69888,
            has_frame: false,
            framebuffer: None,
            has_audio: false,
            last_run_result: None,
        };
        let result = query_value(&view, "run.last.reached");

        assert!(matches!(
            result,
            Err(QueryError::UnavailablePath { ref path, .. }) if path == "run.last.reached"
        ));
    }
}
