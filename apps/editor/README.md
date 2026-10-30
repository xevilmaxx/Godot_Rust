# editor — Godot-style GUI editor + headless shell (`apps/editor`)

Port of `editor/` (docks + inspector + run) and `main/` boot, built on
eframe/egui (OpenGL). Build the GUI with `--features gui`
(`launch_engine.bat` does this for you).

## GUI layout (like Godot, headless-powered)

- **Scene dock** (left): live tree, add Node/Node2D/Node3D, click to select.
- **Viewport** (center): software-rendered frame buffer, draw stats + coverage.
- **Inspector** (right): rename, edit `position`/`visible`, groups, queue-free.
- **Output / Iris** (bottom): script editor + Check / Attach-to-selected,
  live `print` output, engine log.
- **Toolbar**: Play/Stop, Step, frame + node + class counters.

## Commands

| Command | Does |
|---|---|
| `gui` | Open the Godot-style GUI editor (needs `--features gui`) |
| `launch [n]` | Boot engine, run demo scene n frames (default 120), print summary |
| `doc [dir]` | Dump `classdb.json`, `classes.txt`, `iris.json`, `mcp_tools.json` |
| `agents` | List ACP agents + install commands (SolidCraft AI → Refresh) |
| `models` | Model dropdown presets |
| `mcp` | Serve MCP over stdio (Claude Code / MCP clients) |
| `tools` | Print MCP tool schemas |
| `check [f]` | `iris check` — parse + diagnostics |
| `run [f]` | `iris run` — execute script |
| `new-scene`, `scene-demo`, `servers-demo` | Smoke/end-to-end demos |

## Examples

```sh
cargo run -p editor --features gui -- gui   # the Godot-style window
cargo run -p editor -- launch 240           # headless run
cargo run -p editor -- doc docs/api
cargo run -p editor -- mcp
```
