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
        (HostControl::South, ButtonTarget::new(2, "fire")),
    ]);

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
    fn button_map_ignores_unmapped_controls() {
        assert_eq!(TEST_MAP.event(HostControl::East, true), None);
    }
}
