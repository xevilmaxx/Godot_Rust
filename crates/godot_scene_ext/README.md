# godot_scene_ext — animation/GUI/theme/audio nodes (module 05, COMPLETE)

Port of `scene/resources` (animation subset), `scene/animation`,
`scene/gui`, `scene/theme`.

## What’s inside

- **`animation`**: `Animation` (value tracks, keyframes, None/Linear/PingPong loops,
  linear float/vec/color sampling) + `AnimationPlayer` (play/stop/seek/advance →
  `(path, value)` pairs the caller applies via `SceneTree::set_prop`).
- **`gui`**: `Control` (anchors/offsets/visibility/hit-test), `Button` (press count),
  `Label` (auto-size), `LineEdit` (limit/secret/display text).
- **`theme`**: `Theme` (colors/constants/font sizes) + `StyleBoxFlat`, with
  `default_theme()`.
- **`audio_players`**: `AudioStreamPlayer` node-data (stream desc, bus, bind to
  `AudioServer` player id).

## Verify

`cargo test -p godot_scene_ext` — 6 tests.
