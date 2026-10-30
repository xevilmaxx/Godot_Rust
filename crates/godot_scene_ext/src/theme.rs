//! Port of `scene/theme/theme.h` + `scene/resources/style_box_flat.h` (flat subset).

use godot_core::Color;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StyleBoxFlat {
    pub bg: Color,
    pub border: Color,
    pub border_width: [u32; 4],
    pub corner_radius: [u32; 4],
    pub content_margin: [f32; 4],
}

impl StyleBoxFlat {
    pub fn filled(bg: Color) -> Self {
        Self { bg, ..Default::default() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Theme {
    colors: BTreeMap<(String, String), Color>,
    constants: BTreeMap<(String, String), i32>,
    font_sizes: BTreeMap<(String, String), u32>,
}

impl Theme {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_color(&mut self, kind: &str, name: &str, c: Color) {
        self.colors.insert((kind.into(), name.into()), c);
    }
    pub fn get_color(&self, kind: &str, name: &str) -> Option<Color> {
        self.colors.get(&(kind.to_owned(), name.to_owned())).copied()
    }
    pub fn set_constant(&mut self, kind: &str, name: &str, v: i32) {
        self.constants.insert((kind.into(), name.into()), v);
    }
    pub fn get_constant(&self, kind: &str, name: &str) -> Option<i32> {
        self.constants.get(&(kind.to_owned(), name.to_owned())).copied()
    }
    pub fn set_font_size(&mut self, kind: &str, name: &str, px: u32) {
        self.font_sizes.insert((kind.into(), name.into()), px);
    }
    pub fn get_font_size(&self, kind: &str, name: &str) -> Option<u32> {
        self.font_sizes.get(&(kind.to_owned(), name.to_owned())).copied()
    }
    pub fn default_theme() -> Self {
        let mut t = Self::new();
        t.set_color("Button", "font_color", Color::WHITE);
        t.set_color("Button", "bg", Color::rgb(0.2, 0.25, 0.35));
        t.set_constant("Button", "h_separation", 4);
        t.set_font_size("Label", "font_size", 16);
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_roundtrips() {
        let mut t = Theme::default_theme();
        assert_eq!(t.get_font_size("Label", "font_size"), Some(16));
        t.set_color("X", "y", Color::RED);
        assert_eq!(t.get_color("X", "y"), Some(Color::RED));
    }
}
