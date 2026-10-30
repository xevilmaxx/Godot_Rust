# godot_core — `core/` value layer (module 01, COMPLETE)

Port of Godot 4.7 `core/variant/*`, `core/object/*`, `core/math/*`,
`core/string/*`, `core/templates/*`, `core/error/*`.

## What’s inside

- **`variant`**: all 37 `Variant::Type` in header order, conversions
  (`to_int/float/bool/string`), deep equality, cross-type compare, `in`,
  indexing (negative wrap), `duplicate_deep`, stable hash, JSON bridge.
- **`variant_op`**: operator tables (`+ - * / % ** << >> & | ^`, `and/or/xor/not`,
  comparisons) with GDScript semantics (int/int→int, div-by-zero errors, float inf).
- **`math`**: Vector2/2i/3/3i/4/4i, Rect2/2i, Color (html/blend/lerp), Plane,
  Quaternion (YXZ euler, slerp), Basis (rows convention, xform/inverse/mul verified
  against `basis.cpp`), Transform2D/3D, AABB, Projection (ortho/perspective).
- **`ustring`/`node_path`/`containers`**: `GodotString` (substr/capitalize/path_join),
  interned `StringName`, `NodePath` (names + `:subnames`), `GodotArray`
  (typed/readonly/slice/sort), `GodotDictionary`, `Callable`, `Signal`, `Rid`.
- **`object`**: `Object` trait, `SignalBus`, `MessageQueue` (`message_queue.h`),
  `GroupRegistry`, `RefCounted`, `Notification` constants.
- **`class_db`**: register/inheritance/singletons/property-method-signal-enum
  metadata, `to_json` + `to_ai_summary` for agent discovery.
- **`error`**: full Godot `Error` enum values + `GodotError`/`GodotResult`.

Out of scope: `core/io`, `core/os` (→ `godot_host`).

## Example

```rust
let v = Variant::Int(2) + Variant::Float(0.5); // Float(2.5) via variant_op
let t = Transform2D::from_rotation_position(0.7, Vector2::new(3.0, 4.0));
assert!(t.xform_inv(t.xform(p)).is_equal_approx(p));
```

## Verify

`cargo test -p godot_core` — 19 tests.
