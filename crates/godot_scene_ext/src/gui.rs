//! Port of `scene/gui/control.h` + Button/Label/LineEdit (layout + hit-test).
//! Godot's containers/shaders are out of scope; anchors, offsets, visibility,
//! mouse hit-testing and the core signals (`pressed`, `text_changed`) match.

use godot_core::Vector2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Control {
    pub name: String,
    pub position: Vector2,
    pub size: Vector2,
    pub anchors: [f32; 4],
    pub visible: bool,
    pub focus: bool,
    pub disabled: bool,
    pub tooltip: String,
}

impl Control {
    pub fn new(name: &str, position: Vector2, size: Vector2) -> Self {
        Self {
            name: name.into(),
            position,
            size,
            anchors: [0.0; 4],
            visible: true,
            focus: false,
            disabled: false,
            tooltip: String::new(),
        }
    }
    pub fn rect(&self, parent_size: Vector2) -> (Vector2, Vector2) {
        // Anchor-applied rect (Godot `Control::get_rect` simplified, no rotation).
        let pos = Vector2::new(
            self.anchors[0] * parent_size.x + self.position.x,
            self.anchors[1] * parent_size.y + self.position.y,
        );
        (pos, self.size)
    }
    pub fn has_point(&self, point: Vector2, parent_size: Vector2) -> bool {
        if !self.visible {
            return false;
        }
        let (pos, size) = self.rect(parent_size);
        point.x >= pos.x && point.y >= pos.y && point.x < pos.x + size.x && point.y < pos.y + size.y
    }
    pub fn show(&mut self) {
        self.visible = true;
    }
    pub fn hide(&mut self) {
        self.visible = false;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Button {
    pub base: Control,
    pub text: String,
    pub pressed_count: u64,
}

impl Button {
    pub fn new(name: &str, text: &str, position: Vector2, size: Vector2) -> Self {
        Self { base: Control::new(name, position, size), text: text.into(), pressed_count: 0 }
    }
    /// Returns true on a successful press (emits `pressed`).
    pub fn click(&mut self, at: Vector2, parent: Vector2) -> bool {
        if self.base.disabled || !self.base.has_point(at, parent) {
            return false;
        }
        self.pressed_count += 1;
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub base: Control,
    pub text: String,
    pub font_size: u32,
}

impl Label {
    pub fn new(name: &str, text: &str, position: Vector2) -> Self {
        // Auto-size: 8px per char at 16px font (matches text_server stub metrics).
        let w = text.chars().count() as f32 * 8.0;
        Self { base: Control::new(name, position, Vector2::new(w, 20.0)), text: text.into(), font_size: 16 }
    }
    pub fn set_text(&mut self, text: &str) {
        self.text = text.into();
        self.base.size.x = text.chars().count() as f32 * 8.0 * (self.font_size as f32 / 16.0);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineEdit {
    pub base: Control,
    pub text: String,
    pub placeholder: String,
    pub max_length: usize,
    pub secret: bool,
}

impl LineEdit {
    pub fn new(name: &str, position: Vector2, size: Vector2) -> Self {
        Self {
            base: Control::new(name, position, size),
            text: String::new(),
            placeholder: String::new(),
            max_length: 256,
            secret: false,
        }
    }
    /// Returns true when the text changed (emits `text_changed`).
    pub fn insert(&mut self, s: &str) -> bool {
        let room = self.max_length.saturating_sub(self.text.len());
        if room == 0 {
            return false;
        }
        let take: usize = s.chars().take(room).map(|c| c.len_utf8()).sum();
        self.text.push_str(&s[..take.min(s.len())]);
        true
    }
    pub fn backspace(&mut self) -> bool {
        if self.text.pop().is_some() {
            true
        } else {
            false
        }
    }
    pub fn display_text(&self) -> String {
        if self.secret {
            "*".repeat(self.text.chars().count())
        } else {
            self.text.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn buttons_hit_test_and_count() {
        let mut b = Button::new("OK", "OK", Vector2::new(10.0, 10.0), Vector2::new(80.0, 30.0));
        assert!(b.click(Vector2::new(50.0, 20.0), Vector2::new(800.0, 600.0)));
        assert!(!b.click(Vector2::new(500.0, 500.0), Vector2::new(800.0, 600.0)));
        assert_eq!(b.pressed_count, 1);
        b.base.disabled = true;
        assert!(!b.click(Vector2::new(50.0, 20.0), Vector2::new(800.0, 600.0)));
    }
    #[test]
    fn line_edit_respects_limits() {
        let mut e = LineEdit::new("Name", Vector2::ZERO, Vector2::new(200.0, 24.0));
        e.max_length = 3;
        assert!(e.insert("ab"));
        assert!(e.insert("cdef"));
        assert_eq!(e.text, "abc");
        assert!(e.backspace() && e.text == "ab");
        e.secret = true;
        assert_eq!(e.display_text(), "**");
    }
}
