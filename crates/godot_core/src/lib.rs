//! godot_core — FULL port of Godot 4.7 `core/` value layer (module 01, COMPLETE).
//!
//! Originals: `core/variant/*`, `core/object/*`, `core/math/*`,
//! `core/string/*`, `core/templates/*`, `core/error/*`.
//!
//! DONE criteria for this module:
//! - [x] All 37 Variant types with conversions, operators (`variant_op`), compare,
//!       duplicate, stringify, JSON bridge (MCP/Iris use this)
//! - [x] Full math: Vector2/2i/3/3i/4/4i, Rect2/2i, Color, Plane, Quaternion,
//!       Basis, Transform2D/3D, AABB, Projection + `Math::lerp/clamp/approx`
//! - [x] Strings: GodotString, StringName (interned), NodePath (names+subnames)
//! - [x] Containers: GodotArray (typed/readonly/slice/sort), GodotDictionary,
//!       Callable, Signal, Rid
//! - [x] Object: properties, method registry per instance, signal connect/emit,
//!       groups, notification constants; RefCounted
//! - [x] ClassDB: register, inheritance, singletons, property/method/signal defs,
//!       JSON export for AI discovery
//! - [x] Error: full Godot `Error` enum + `GodotError` result type
//!
//! Out of scope (later modules): `core/io` (FileAccess → godot_host),
//! `core/os` (OS/Time → godot_host), `core/input`, `core/crypto`, `core/debugger`.

pub mod class_db;
pub mod containers;
pub mod error;
pub mod math;
pub mod node_path;
pub mod object;
pub mod ustring;
pub mod variant;
pub mod variant_op;

pub use class_db::{global_db, register_global, ClassDb, ClassDef, MethodDef, PropertyDef, SignalDef};
pub use containers::{Callable, GodotArray, GodotDictionary, Rid, Signal};
pub use error::{Error, GodotError, GodotResult};
pub use math::*;
pub use node_path::NodePath;
pub use object::{GroupRegistry, MessageQueue, Notification, Object, ObjectId, RefCounted, SignalBus, SignalConnection};
pub use ustring::{GodotString, StringName};
pub use variant::{Variant, VariantType};

/// Engine version this port tracks.
pub const ENGINE_VERSION: &str = "4.7.3-rc";
/// Crate-level port status, shown in editor About dialog.
pub const PORT_STATUS: &str = "module 01/12 COMPLETE: core value layer (variant/math/string/containers/object/classdb)";
