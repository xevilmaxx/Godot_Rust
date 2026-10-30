//! Port of `core/input/input.h` + `core/input/input_event.h` (action-map subset).
//! Real device polling arrives with a windowed build; the action map + injected
//! events already drive gameplay code and agent tests deterministically.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    Escape,
    Space,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Key(char),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    Key { key: Key, pressed: bool, echo: bool },
    MouseButton { button: u8, pressed: bool, position: (f32, f32) },
    MouseMotion { position: (f32, f32), relative: (f32, f32) },
}

#[derive(Debug, Default)]
pub struct Input {
    actions: BTreeMap<String, BTreeSet<char>>,
    pressed: BTreeSet<String>,
    just_pressed: BTreeSet<String>,
    just_released: BTreeSet<String>,
    mouse_pos: (f32, f32),
}

impl Input {
    pub fn new() -> Self {
        Self::default()
    }
    /// `InputMap::add_action` + bind physical keys (chars, e.g. ' ' or 'w').
    pub fn add_action(&mut self, action: &str, keys: &[char]) {
        self.actions.insert(action.to_owned(), keys.iter().copied().collect());
    }
    pub fn has_action(&self, action: &str) -> bool {
        self.actions.contains_key(action)
    }
    /// Inject an event (device driver or test harness).
    pub fn inject(&mut self, event: InputEvent) {
        match event {
            InputEvent::Key { key: Key::Key(c), pressed, .. } => {
                for (action, keys) in &self.actions {
                    if keys.contains(&c) {
                        if pressed {
                            if !self.pressed.contains(action) {
                                self.just_pressed.insert(action.clone());
                            }
                            self.pressed.insert(action.clone());
                        } else {
                            self.pressed.remove(action);
                            self.just_released.insert(action.clone());
                        }
                    }
                }
            }
            InputEvent::MouseMotion { position, .. } => self.mouse_pos = position,
            _ => {}
        }
    }
    pub fn is_pressed(&self, action: &str) -> bool {
        self.pressed.contains(action)
    }
    pub fn is_just_pressed(&self, action: &str) -> bool {
        self.just_pressed.contains(action)
    }
    pub fn is_just_released(&self, action: &str) -> bool {
        self.just_released.contains(action)
    }
    pub fn mouse_position(&self) -> (f32, f32) {
        self.mouse_pos
    }
    /// End-of-frame edge reset (Godot clears just-pressed/released per iteration).
    pub fn end_frame(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_track_edges_like_godot() {
        let mut i = Input::new();
        i.add_action("jump", &[' ']);
        i.inject(InputEvent::Key { key: Key::Key(' '), pressed: true, echo: false });
        assert!(i.is_pressed("jump") && i.is_just_pressed("jump"));
        i.end_frame();
        assert!(i.is_pressed("jump") && !i.is_just_pressed("jump"));
        i.inject(InputEvent::Key { key: Key::Key(' '), pressed: false, echo: false });
        assert!(!i.is_pressed("jump") && i.is_just_released("jump"));
    }
}
