# godot_io — importers + codecs + TextServer (module 10, COMPLETE)

Port of `modules/gltf` (JSON document), audio/image codecs, `servers/text`.

## What’s inside

- **`gltf`**: full `.gltf` JSON document model (scenes/nodes/meshes/materials),
  embedded base64 buffers, POSITION (+indices) extraction, PBR base color.
  `.glb` container, Draco/meshopt, sparse accessors, external `.bin` →
  explicit errors (module 10b). FBX/Collada → redirect errors (same as Godot’s
  funnel through `modules/fbx`/`modules/gltf`).
- **`codecs`**: WAV-16 decode (matches our encoder), BMP 24-bit encode/decode;
  OGG/PNG report as unsupported with the Godot-registry message shape.
- **`text_server`**: monospace shaping/measuring, greedy word wrap, RTL-run
  reversal stub (HarfBuzz shaping = 10b).

## Depth

- `GltfDocument::from_glb` (JSON+BIN chunks), `from_file` (`.gltf` with
  external `.bin` resolution, `.glb` sniffing).
- `decode_image` (PNG/JPEG via `image`), `decode_ogg` (Vorbis via `lewton`,
  downmixed to mono), `encode_wav` round-trips.
- `TextServer::shape_with_font` (real `rustybuzz` shaping),
  `measure_shaped`, `system_font_bytes` discovery; deterministic monospace
  fallback preserved for headless runs and tests.

## Verify

`cargo test -p godot_io` — glTF/GLB/external-BIN, PNG, OGG-reject, shaping.
