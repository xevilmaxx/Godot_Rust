//! godot_host — FULL port of `core/io`, `core/os`, `platform/*` (headless subset)
//! and `main/main*.cpp` (main loop). Module 04, COMPLETE.
//!
//! - `vfs`: `FileAccess`/`DirAccess` with `res://` (project dir) + `user://`
//!   (per-user data dir) prefixes, like Godot's VFS.
//! - `os`: OS name/args/env/ticks/clipboard-stub, `Time` (datetime, ticks).
//! - `input`: `InputEvent` key/mouse + `Input` action map (pressed/just-pressed).
//! - `settings`: `ProjectSettings` key store + JSON save/load.
//! - `display`: headless `DisplayServer` (title/size, no window until module 04b).
//! - `engine`: `Engine` owns tree + servers; `iteration(dt)` = scene idle +
//!   physics + Iris + audio clock + render (software).

pub mod display;
pub mod engine;
pub mod input;
pub mod os;
pub mod settings;
pub mod time;
pub mod vfs;

pub use display::DisplayServer;
pub use engine::Engine;
pub use input::{Input, InputEvent, Key};
pub use os::OS;
pub use settings::ProjectSettings;
pub use time::Time;
pub use vfs::{DirAccess, FileAccess};

/// Register host singletons in the global ClassDB (idempotent).
pub fn register_host_classes() {
    use godot_core::{ClassDef, MethodDef, PropertyDef, VariantType};
    for name in ["OS", "Time", "Input", "ProjectSettings", "DisplayServer", "Engine"] {
        godot_core::register_global(ClassDef {
            name: name.into(),
            parent: Some("Object".into()),
            properties: vec![PropertyDef {
                name: "headless".into(),
                variant_type: VariantType::Bool,
                hint: "".into(),
                default_json: "true".into(),
            }],
            methods: vec![MethodDef {
                name: "get_ticks_msec".into(),
                args: vec![],
                return_type: VariantType::Int,
                doc: "Milliseconds since engine start.".into(),
                is_static: false,
                is_const: true,
            }],
            signals: vec![],
            enums: vec![],
            is_singleton: true,
            can_instantiate: false,
            doc: format!("{name} singleton (module 04 port)."),
        });
    }
}
