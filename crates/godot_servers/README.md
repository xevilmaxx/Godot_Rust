# godot_servers — `servers/` (module 03, COMPLETE)

Port of `servers/rendering` (API + `dummy/` rasterizer, upgraded to a
deterministic software rasterizer), `servers/physics_2d/3d` (GodotPhysics model),
`servers/audio` (+ dummy driver), camera/display stubs.

## What’s inside

- **`allocator`**: `RidOwner<T>` free-list ids (like `RID_Alloc`).
- **`rendering`**: textures/meshes (box, AABB, 12-tri box)/materials/lights/
  cameras/environments/viewports/instances; `render()` clears + draws each visible
  instance as its projected-AABB rect shaded by albedo×(ambient+key); culling,
  `capture`/`pixel`/`coverage`, stats. `RenderBackendKind::{Software,Wgpu}` —
  Wgpu is an explicit error until `godot_host` owns a device (module 04b).
- **`physics3d`/`physics2d`**: spaces, static/kinematic/rigid bodies, sphere/box/
  plane/capsule (3D) and circle/rect/segment (2D), fixed-step gravity+damping,
  plane/ground contacts with resting tolerance, friction, sleep, analytic raycasts.
- **`audio`**: dummy 44100 Hz clock, buses (volume/mute/solo/peaks), streams
  (sine/square/noise/PCM), players, `render_pcm` mix, WAV-16 `encode_wav`.
- **`sync`**: scene↔server helpers (body position → node, node transform → instance).

## Depth beyond the skeleton

- **Physics**: full rigid-body rotation (orientation, angular velocity, exact
  body-space inertia), oriented 15-axis SAT box-box with face-clipped
  manifolds, exact capsule segment math (+ documented 3-sphere approximation
  vs boxes), static-box obstacles, body–body impulse response with friction,
  fixed weld joints (position + relative orientation) + hinge joints
  (ball-socket + axis alignment), `CharacterBody3D::move_and_slide` (slide,
  floor/wall detection, snap), `body_set_damping` (linear + angular).
  Solver uses pre-manifold velocity snapshots (order-independent) with
  1/N-distributed slop correction. **Known limits (see
  `docs/PHYSICS_3D.md`)**: centered stacks settle flat; offset loads and
  tilted drops can freeze in edge/corner balances; zero-damping spin persists
  by design; 9/11 physics3d tests green, 2 edge cases tracked as failing.
- **Rendering**: `RenderBackendKind::Wgpu` renders offscreen to texture and
  reads pixels back into the same framebuffers the software path and GUI
  viewport display (`cargo test -p godot_servers --features wgpu`, verified on
  WARP fallback; graceful error without a device). Build default stays glow-only.

## Verify

`cargo test -p godot_servers` — software suite.
`cargo test -p godot_servers --features wgpu` — incl. live GPU render tests.
`cargo run -p editor -- servers-demo` — coverage 0.09, ball y=0.500, 4410 samples.
