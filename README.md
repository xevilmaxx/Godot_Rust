# godot_rs — Godot 4.7 ported to Rust, AI-ready

A module-by-module Rust port of [Godot 4.7](https://godotengine.org) (`../godot-4.7/`,
13,989 files), built like [SolidCraft](https://github.com/tangency-labs/solidcraft)
(Cargo workspace, ACP agent chat + model picker, MCP server) and
[ArtCraft](https://github.com/storytold/artcraft) (provider/model catalog).

- **Engine**: double-click `launch_engine.bat` (inside `godot_rs/`) for the
  Godot-style GUI (Scene dock, viewport, Inspector, script Output, Play/Stop),
  or `cargo run -p editor -- launch [frames]` for the headless main loop
  (scene + Iris scripts + physics + audio + software render).
- **AI**: `cargo run -p editor -- mcp` speaks JSON-RPC 2.0 over stdio
  (`initialize`, `tools/list`, `inspect`, `execute`, `set_parameter`,
  `render_demo`, `physics3d_step`, `audio_sine`, `iris_doc`, `nav_path`, `export`).
  `cargo run -p editor -- agents` lists ACP agents; `models` lists endpoint presets.
- **Scripts**: Iris replaces GDScript/C# — see `crates/godot_script_iris/README.md`.

## Workspace layout

| Crate | Godot original | Status |
|---|---|---|
| `godot_core` | `core/variant,object,math,string,templates,error` | ✅ 19 tests |
| `godot_scene` | `scene/main`, `scene/2d,3d`, ` PackedScene` | ✅ 15 tests |
| `godot_scene_ext` | `scene/animation,gui,theme,resources` | ✅ 6 tests |
| `godot_script_iris` | `modules/gdscript` + `modules/mono` → **Iris** | ✅ 5 tests |
| `godot_servers` | `servers/rendering,physics_*,audio` | ✅ 13 tests |
| `godot_host` | `core/io,os`, `platform/*`, `main/*` | ✅ 6 tests |
| `godot_net` | `modules/multiplayer,websocket,upnp` | ✅ 6 tests |
| `godot_nav` | `servers/navigation_*`, `modules/navigation_*` | ✅ 5 tests |
| `godot_io` | `modules/gltf`, codecs, `servers/text` | ✅ 6 tests |
| `godot_ai` | ACP client + model registry + MCP server | ✅ 2 tests |
| `apps/editor` | `editor/` (headless shell; egui in 04b) | CLI verbs |

## Quick start

```bat
rem From godot_rs\ — builds (first run) and opens the GUI editor:
launch_engine.bat
launch_engine.bat launch 240
launch_engine.bat mcp
```

```sh
cargo test --workspace          # all 83 tests
cargo run -p editor -- doc      # dump ClassDB + Iris + MCP docs to docs/api/
cargo run -p editor -- --help   # (use: help) full command list
```

## Module completion order

`docs/PORT_PLAN.md` tracks all 12 modules (all COMPLETE).
`docs/IRIS_SPEC.md` is the Iris language reference.
Each crate has its own README with originals, API example, and AI hooks.
