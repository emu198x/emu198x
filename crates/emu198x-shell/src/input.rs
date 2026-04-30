//! Host-side input mapping helpers shared by native verifier shells.
//!
//! Machines consume stable [`InputEvent`] values. Native frontends can map
//! keyboards, physical gamepads, or any other host device into those same
//! events without teaching the emulated machine about host hardware.

use std::borrow::Cow;
use std::collections::HashMap;

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};

use crate::host::InputEvent;

const DEFAULT_AXIS_THRESHOLD: f32 = 0.5;

/// A frontend-neutral host control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HostControl {
    /// Directional up.
    Up,
    /// Directional down.
    Down,
    /// Directional left.
    Left,
    /// Directional right.
    Right,
    /// Primary south face button.
    South,
    /// East face button.
    East,
    /// West face button.
    West,
    /// North face button.
    North,
    /// Start/menu button.
    Start,
    /// Select/back button.
    Select,
}

/// Target emulated button for one host control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonTarget {
    /// Target emulated port.
    pub port: u8,
    /// Stable emulated control name.
    pub name: &'static str,
}

impl ButtonTarget {
    /// Creates a target button mapping.
    #[must_use]
    pub const fn new(port: u8, name: &'static str) -> Self {
        Self { port, name }
    }
}

/// Converts frontend-neutral host controls into emulated button events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ButtonInputMap {
    entries: &'static [(HostControl, ButtonTarget)],
}

impl ButtonInputMap {
    /// Creates a static button mapping.
    #[must_use]
    pub const fn new(entries: &'static [(HostControl, ButtonTarget)]) -> Self {
        Self { entries }
    }

    /// Returns the emulated target for a host control.
    #[must_use]
    pub fn target(&self, control: HostControl) -> Option<ButtonTarget> {
        self.entries
            .iter()
            .find_map(|(candidate, target)| (*candidate == control).then_some(*target))
    }

    /// Converts a host control state change into an emulated button event.
    #[must_use]
    pub fn event(&self, control: HostControl, pressed: bool) -> Option<InputEvent> {
        let target = self.target(control)?;
        Some(InputEvent::Button {
            port: target.port,
            name: Cow::Borrowed(target.name),
            pressed,
        })
    }
}

/// Polls physical gamepads and emits mapped button events.
pub struct NativeGamepadInput {
    gilrs: Option<Gilrs>,
    axis_states: HashMap<(GamepadId, HostControl), bool>,
    axis_threshold: f32,
}

impl NativeGamepadInput {
    /// Creates a gamepad poller. If the platform backend is unavailable, the
    /// poller becomes a no-op rather than failing shell startup.
    #[must_use]
    pub fn new() -> Self {
        Self {
            gilrs: Gilrs::new().ok(),
            axis_states: HashMap::new(),
            axis_threshold: DEFAULT_AXIS_THRESHOLD,
        }
    }

    /// Returns `true` when a physical gamepad backend is available.
    #[must_use]
    pub const fn is_available(&self) -> bool {
        self.gilrs.is_some()
    }

    /// Drains pending host gamepad events into the supplied emulated event queue.
    pub fn drain_events(&mut self, map: &ButtonInputMap, output: &mut Vec<InputEvent>) {
        loop {
            let event = {
                let Some(gilrs) = self.gilrs.as_mut() else {
                    return;
                };
                gilrs.next_event()
            };
            let Some(event) = event else {
                return;
            };

            match event.event {
                EventType::ButtonPressed(button, _) => {
                    if let Some(control) = map_gamepad_button(button)
                        && let Some(input) = map.event(control, true)
                    {
                        output.push(input);
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    if let Some(control) = map_gamepad_button(button)
                        && let Some(input) = map.event(control, false)
                    {
                        output.push(input);
                    }
                }
                EventType::AxisChanged(axis, value, _) => {
                    self.update_axis(event.id, axis, value, map, output);
                }
                _ => {}
            }
        }
    }

    fn update_axis(
        &mut self,
        gamepad_id: GamepadId,
        axis: Axis,
        value: f32,
        map: &ButtonInputMap,
        output: &mut Vec<InputEvent>,
    ) {
        let Some((negative, positive)) = map_gamepad_axis(axis) else {
            return;
        };

        self.update_axis_control(
            gamepad_id,
            negative,
            value <= -self.axis_threshold,
            map,
            output,
        );
        self.update_axis_control(
            gamepad_id,
            positive,
            value >= self.axis_threshold,
            map,
            output,
        );
    }

    fn update_axis_control(
        &mut self,
        gamepad_id: GamepadId,
        control: HostControl,
        pressed: bool,
        map: &ButtonInputMap,
        output: &mut Vec<InputEvent>,
    ) {
        let key = (gamepad_id, control);
        let was_pressed = self.axis_states.get(&key).copied().unwrap_or(false);
        if was_pressed == pressed {
            return;
        }

        if pressed {
            self.axis_states.insert(key, true);
        } else {
            self.axis_states.remove(&key);
        }

        if let Some(input) = map.event(control, pressed) {
            output.push(input);
        }
    }
}

impl Default for NativeGamepadInput {
    fn default() -> Self {
        Self::new()
    }
}

fn map_gamepad_button(button: Button) -> Option<HostControl> {
    Some(match button {
        Button::South => HostControl::South,
        Button::East => HostControl::East,
        Button::West => HostControl::West,
        Button::North => HostControl::North,
        Button::Start => HostControl::Start,
        Button::Select => HostControl::Select,
        Button::DPadUp => HostControl::Up,
        Button::DPadDown => HostControl::Down,
        Button::DPadLeft => HostControl::Left,
        Button::DPadRight => HostControl::Right,
        _ => return None,
    })
}

fn map_gamepad_axis(axis: Axis) -> Option<(HostControl, HostControl)> {
    Some(match axis {
        Axis::LeftStickX => (HostControl::Left, HostControl::Right),
        Axis::LeftStickY => (HostControl::Up, HostControl::Down),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MAP: ButtonInputMap = ButtonInputMap::new(&[
        (HostControl::Up, ButtonTarget::new(2, "up")),
        (HostControl::Down, ButtonTarget::new(2, "down")),
        (HostControl::Left, ButtonTarget::new(2, "left")),
        (HostControl::Right, ButtonTarget::new(2, "right")),
        (HostControl::South, ButtonTarget::new(2, "fire")),
        (HostControl::East, ButtonTarget::new(2, "east")),
        (HostControl::West, ButtonTarget::new(2, "west")),
        (HostControl::North, ButtonTarget::new(2, "north")),
        (HostControl::Start, ButtonTarget::new(1, "start")),
        (HostControl::Select, ButtonTarget::new(1, "select")),
    ]);

    #[test]
    fn button_target_new_records_port_and_name() {
        let target = ButtonTarget::new(7, "thrust");
        assert_eq!(target.port, 7);
        assert_eq!(target.name, "thrust");
    }

    #[test]
    fn button_input_map_new_stores_entries() {
        const ENTRIES: &[(HostControl, ButtonTarget)] =
            &[(HostControl::South, ButtonTarget::new(0, "a"))];
        let map = ButtonInputMap::new(ENTRIES);
        assert_eq!(map.target(HostControl::South), Some(ButtonTarget::new(0, "a")));
    }

    #[test]
    fn button_input_map_target_returns_each_mapped_control() {
        // Hits the find_map success path for every HostControl variant.
        for &(control, target) in [
            (HostControl::Up, ButtonTarget::new(2, "up")),
            (HostControl::Down, ButtonTarget::new(2, "down")),
            (HostControl::Left, ButtonTarget::new(2, "left")),
            (HostControl::Right, ButtonTarget::new(2, "right")),
            (HostControl::South, ButtonTarget::new(2, "fire")),
            (HostControl::East, ButtonTarget::new(2, "east")),
            (HostControl::West, ButtonTarget::new(2, "west")),
            (HostControl::North, ButtonTarget::new(2, "north")),
            (HostControl::Start, ButtonTarget::new(1, "start")),
            (HostControl::Select, ButtonTarget::new(1, "select")),
        ]
        .iter()
        {
            assert_eq!(TEST_MAP.target(control), Some(target));
        }
    }

    #[test]
    fn button_input_map_target_returns_none_for_empty_map() {
        const EMPTY: ButtonInputMap = ButtonInputMap::new(&[]);
        assert_eq!(EMPTY.target(HostControl::South), None);
    }

    #[test]
    fn button_map_emits_internal_button_event() {
        assert_eq!(
            TEST_MAP.event(HostControl::South, true),
            Some(InputEvent::Button {
                port: 2,
                name: Cow::Borrowed("fire"),
                pressed: true,
            })
        );
    }

    #[test]
    fn button_map_event_propagates_release_state() {
        assert_eq!(
            TEST_MAP.event(HostControl::Start, false),
            Some(InputEvent::Button {
                port: 1,
                name: Cow::Borrowed("start"),
                pressed: false,
            })
        );
    }

    #[test]
    fn button_map_ignores_unmapped_controls() {
        const SPARSE: ButtonInputMap =
            ButtonInputMap::new(&[(HostControl::Up, ButtonTarget::new(2, "up"))]);
        assert_eq!(SPARSE.event(HostControl::East, true), None);
    }

    #[test]
    fn host_control_is_hashable_and_eq() {
        // HostControl is used as a HashMap key inside NativeGamepadInput; cover
        // the derived traits here so the public API surface is exercised.
        use std::collections::HashSet;
        let mut set: HashSet<HostControl> = HashSet::new();
        set.insert(HostControl::Up);
        set.insert(HostControl::Up);
        set.insert(HostControl::Down);
        assert_eq!(set.len(), 2);
        assert!(set.contains(&HostControl::Up));
    }

    #[test]
    fn native_gamepad_input_new_constructs() {
        // Whether or not a backend is available depends on the host; both
        // branches return a usable value, so we just verify construction
        // does not panic and exposes the configured threshold.
        let input = NativeGamepadInput::new();
        assert_eq!(input.axis_threshold, DEFAULT_AXIS_THRESHOLD);
        assert!(input.axis_states.is_empty());
        // is_available reflects whatever Gilrs::new() returned.
        let _avail = input.is_available();
    }

    #[test]
    fn native_gamepad_input_default_matches_new() {
        let default = NativeGamepadInput::default();
        assert_eq!(default.axis_threshold, DEFAULT_AXIS_THRESHOLD);
        assert!(default.axis_states.is_empty());
    }

    #[test]
    fn native_gamepad_input_is_available_false_when_backend_missing() {
        let mut input = NativeGamepadInput::new();
        // Force the no-backend path regardless of platform.
        input.gilrs = None;
        assert!(!input.is_available());
    }

    #[test]
    fn native_gamepad_input_is_available_true_when_backend_present() {
        // We can't construct a real Gilrs, but we can detect when one is
        // available on this host. If it is, exercise the true branch.
        let input = NativeGamepadInput::new();
        if input.gilrs.is_some() {
            assert!(input.is_available());
        }
    }

    #[test]
    fn drain_events_returns_immediately_without_backend() {
        let mut input = NativeGamepadInput::new();
        input.gilrs = None;
        let mut output = Vec::new();
        input.drain_events(&TEST_MAP, &mut output);
        assert!(output.is_empty());
    }

    #[test]
    fn drain_events_empty_queue_returns_when_backend_present() {
        // If a backend exists on the host, the inner gilrs.next_event() returns
        // None on an empty queue, hitting the second early-return arm. This
        // test is a no-op on hosts without a gamepad backend.
        let mut input = NativeGamepadInput::new();
        if input.gilrs.is_some() {
            let mut output = Vec::new();
            input.drain_events(&TEST_MAP, &mut output);
            // Output may legitimately contain events the host generated; we
            // only assert that drain_events returned without panicking.
            let _ = output.len();
        }
    }

    #[test]
    fn map_gamepad_button_covers_every_named_control() {
        assert_eq!(map_gamepad_button(Button::South), Some(HostControl::South));
        assert_eq!(map_gamepad_button(Button::East), Some(HostControl::East));
        assert_eq!(map_gamepad_button(Button::West), Some(HostControl::West));
        assert_eq!(map_gamepad_button(Button::North), Some(HostControl::North));
        assert_eq!(map_gamepad_button(Button::Start), Some(HostControl::Start));
        assert_eq!(map_gamepad_button(Button::Select), Some(HostControl::Select));
        assert_eq!(map_gamepad_button(Button::DPadUp), Some(HostControl::Up));
        assert_eq!(map_gamepad_button(Button::DPadDown), Some(HostControl::Down));
        assert_eq!(map_gamepad_button(Button::DPadLeft), Some(HostControl::Left));
        assert_eq!(
            map_gamepad_button(Button::DPadRight),
            Some(HostControl::Right)
        );
    }

    #[test]
    fn map_gamepad_button_returns_none_for_unmapped_inputs() {
        // Cover the wildcard arm. Mode/Z/shoulder buttons are intentionally
        // unmapped because the abstract HostControl set doesn't include them.
        assert_eq!(map_gamepad_button(Button::Mode), None);
        assert_eq!(map_gamepad_button(Button::LeftTrigger), None);
        assert_eq!(map_gamepad_button(Button::LeftTrigger2), None);
        assert_eq!(map_gamepad_button(Button::RightTrigger), None);
        assert_eq!(map_gamepad_button(Button::RightTrigger2), None);
        assert_eq!(map_gamepad_button(Button::LeftThumb), None);
        assert_eq!(map_gamepad_button(Button::RightThumb), None);
        assert_eq!(map_gamepad_button(Button::C), None);
        assert_eq!(map_gamepad_button(Button::Z), None);
        assert_eq!(map_gamepad_button(Button::Unknown), None);
    }

    #[test]
    fn map_gamepad_axis_returns_pairs_for_left_stick() {
        assert_eq!(
            map_gamepad_axis(Axis::LeftStickX),
            Some((HostControl::Left, HostControl::Right))
        );
        assert_eq!(
            map_gamepad_axis(Axis::LeftStickY),
            Some((HostControl::Up, HostControl::Down))
        );
    }

    #[test]
    fn map_gamepad_axis_returns_none_for_unmapped_axes() {
        // Wildcard arm — right stick, triggers, and DPad axes are unmapped
        // because directional input flows through the dedicated DPad buttons.
        assert_eq!(map_gamepad_axis(Axis::RightStickX), None);
        assert_eq!(map_gamepad_axis(Axis::RightStickY), None);
        assert_eq!(map_gamepad_axis(Axis::LeftZ), None);
        assert_eq!(map_gamepad_axis(Axis::RightZ), None);
        assert_eq!(map_gamepad_axis(Axis::DPadX), None);
        assert_eq!(map_gamepad_axis(Axis::DPadY), None);
        assert_eq!(map_gamepad_axis(Axis::Unknown), None);
    }
}
