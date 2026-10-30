# godot_host — `core/io,os` + `platform/*` + `main/*` (module 04, COMPLETE)

Headless port: VFS, OS/Time, Input, ProjectSettings, DisplayServer stub, and the
`Engine` main loop. (Real winit/wgpu windowing = module 04b; the API shape is ready.)

## What’s inside

- **`vfs`**: `FileAccess` (read/write/seek/get_line) + `DirAccess` (list/mkdir/remove)
  with `res://` (project dir) and `user://` (per-user data dir) prefixes.
- **`os`/`time`**: OS name/args/env/ticks/processor count, Unix time, UTC datetime.
- **`input`**: `InputEvent` (key/mouse) + `Input` action map (pressed/just-pressed/
  just-released edges, end-of-frame reset) — injectable for tests/agents.
- **`settings`**: `ProjectSettings` typed store (app/display/physics/rendering keys)
  + JSON save/load, `physics_ticks()`, `viewport_size()`.
- **`display`**: headless `DisplayServer` (title/size, `is_headless()` true).
- **`engine`**: `Engine` owns tree + rendering + physics + audio + input + display;
  `iteration(dt)` = scene idle → input edges → fixed-step physics → audio clock → render.

## Example

```rust
let mut e = Engine::new(ProjectSettings::new());
e.iteration(1.0 / 60.0); // advances frames, timers, Iris, physics, audio, render
```

## Verify

`cargo test -p godot_host` — 6 tests.
