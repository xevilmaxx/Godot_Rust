//! Port of `core/config/project_settings.h` — typed key store + JSON persistence.
//! Godot uses `project.godot` (INI-ish); v1 stores JSON with the same key paths
//! (`application/name`, `display/window/size/viewport_width`, …) so agents and
//! humans share one format. INI import is a documented non-goal (module 10b).

use godot_core::Variant;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSettings {
    values: BTreeMap<String, Variant>,
}

impl ProjectSettings {
    pub fn new() -> Self {
        let mut s = Self::default();
        s.set("application/name", Variant::String("godot_rs".into()));
        s.set("application/boot_splash/show_image", Variant::Bool(false));
        s.set("display/window/size/viewport_width", Variant::Int(1152));
        s.set("display/window/size/viewport_height", Variant::Int(648));
        s.set("physics/common/physics_ticks_per_second", Variant::Int(60));
        s.set("rendering/renderer/rendering_method", Variant::String("software".into()));
        s
    }
    pub fn set(&mut self, key: &str, value: Variant) {
        self.values.insert(key.to_owned(), value);
    }
    pub fn get(&self, key: &str) -> Option<&Variant> {
        self.values.get(key)
    }
    pub fn get_or(&self, key: &str, default: Variant) -> Variant {
        self.values.get(key).cloned().unwrap_or(default)
    }
    pub fn has(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }
    pub fn save_json(&self, path: &str) -> godot_core::GodotResult<()> {
        let mut f = super::vfs::FileAccess::open_write(path)?;
        f.write_text(&serde_json::to_string_pretty(&self.values).unwrap_or_else(|_| "{}".into()));
        f.close()
    }
    pub fn load_json(path: &str) -> godot_core::GodotResult<Self> {
        let mut f = super::vfs::FileAccess::open_read(path)?;
        let text = f.read_text();
        let values: BTreeMap<String, Variant> =
            serde_json::from_str(&text).map_err(|e| godot_core::GodotError::Parse(e.to_string()))?;
        Ok(Self { values })
    }
    pub fn physics_ticks(&self) -> u64 {
        self.get("physics/common/physics_ticks_per_second").map(|v| v.to_int().max(1) as u64).unwrap_or(60)
    }
    pub fn viewport_size(&self) -> (u32, u32) {
        let w = self.get("display/window/size/viewport_width").map(|v| v.to_int().max(1) as u32).unwrap_or(1152);
        let h = self.get("display/window/size/viewport_height").map(|v| v.to_int().max(1) as u32).unwrap_or(648);
        (w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_json_roundtrip() {
        let mut s = ProjectSettings::new();
        assert_eq!(s.viewport_size(), (1152, 648));
        s.set("application/name", Variant::String("demo".into()));
        s.save_json("user://_test_project.json").unwrap();
        let back = ProjectSettings::load_json("user://_test_project.json").unwrap();
        assert_eq!(back.get("application/name"), Some(&Variant::String("demo".into())));
        let _ = super::super::vfs::DirAccess::remove("user://_test_project.json");
    }
}
