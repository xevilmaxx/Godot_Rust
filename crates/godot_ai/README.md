# godot_ai — ACP agents + model registry + MCP server (AI bridge)

SolidCraft-style integration (`agent-chat` + `mcp`), ArtCraft-style model catalog.
No Godot original — this is the new AI surface over the ported engine.

## What’s inside

- **`acp`**: agent table (Claude Code, Codex, OpenCode, Grok, Copilot, custom) with
  install commands, spawn argv, `is_installed()` (`editor agents` = AI → Refresh).
- **`models`**: endpoint presets (Ollama `localhost:11434/v1`, LM Studio
  `localhost:1234/v1`, OpenAI, OpenCode Go) + `ModelRegistry` with last-choice
  memory (model dropdown; live `/models` fetch = next step).
- **`mcp`**: `McpServer` over stdio JSON-RPC 2.0 (`initialize`, `tools/list`,
  `inspect`, `execute`, `set_parameter`, `export`, `render`, `render_demo`,
  `render_gpu` (needs `godot_servers/wgpu`, else honest error),
  `physics3d_step`, `physics2d_step`, `audio_sine`, `iris_doc`, `nav_path`,
  `nav_bake`, `import_gltf`, `gd_transpile`, `shutdown`) against a **live**
  `SceneTree` + servers — agents mutate the same state the editor shows.
- **`control`**: `ControlChannel` (`127.0.0.1:port` + token, bridged sessions in 03b).

## Connect an agent (Claude Code example)

```sh
cargo run -p editor -- mcp        # stdio server; or:
claude mcp add godot_rs -- cargo run -p editor -- mcp
```

## Verify

`cargo test -p godot_ai` — live tree roundtrip, server tools, new-tool answers.
