# Physics 3D — status, features, and known problems

`crates/godot_servers/src/physics3d.rs` — port of Godot's `servers/physics_3d`
(God tangent-space GodotPhysics model, not an exact replica).

## What works (verified by tests)

- **Bodies**: static / rigid / kinematic (`BodyMode3D`), mass, gravity scale,
  per-body linear + angular damping (`body_set_damping`, Godot `linear_damp` /
  `angular_damp`), sleep/wake with resting-contact detection.
- **Shapes**: sphere, box, plane, capsule with local offsets
  (`body_add_shape` + `Transform3D` offset, orientation-aware).
- **Integration**: semi-implicit Euler at fixed step; orientation integrated
  from angular velocity via quaternion (`dq * q`, renormalized each step).
- **Inertia**: exact diagonal inverse inertia for boxes/spheres/capsules
  (`update_inertia`), world-space application `R·I⁻¹·Rᵀ`
  (`inv_inertia_world`).
- **Narrowphase** (all orientation-aware):
  - sphere–sphere (exact), sphere–box (box-frame clamp + inside-case
    face push), box–box (**full 15-axis SAT with face clipping**, up to
    8-point manifolds), capsule vs sphere/box/plane (exact segment math) and
    capsule–capsule (segment–segment closest points). Capsule-vs-box uses a
    **documented 3-sphere approximation**.
  - Shape vs static plane with oriented support; static boxes honor body
    orientation + shape offsets; y=0 ground-plane fallback when no plane
    shapes exist.
- **Contact solver**: positional correction with slop (0.005) + manifold
  distribution (1/N per point, so N contacts remove ~one depth); velocity
  impulses with rotation (normal + Coulomb friction, μ clamped at 0.6·j).
  Impulses are computed from **pre-manifold snapshots (Jacobi-style)** so the
  result does not depend on contact processing order — sequential live reads
  made first-processed corners dominate and injected systematic torque
  (found empirically: symmetric stacks drifted diagonally).
- **Joints**: fixed weld (position + relative-orientation correction,
  `rest_rel` captured at creation) and hinge (ball-socket + axis alignment via
  corrective angular velocity, `axis_a`/`axis_b` per body), 2 relaxation
  passes per step.
- **CharacterBody3D**: kinematic `move_and_slide` (sphere approximation,
  oriented obstacles, floor/wall detection, floor snap).
- **Raycast**: closest-hit vs spheres/oriented boxes (ray transformed into box
  frame)/planes/capsules (3 sub-spheres).
- **Stability guards**: 50 rad/s angular-velocity clamp, finite-state
  assertions available (`violent_impact_stays_bounded`).

## Test status (last full run, `cargo test -p godot_servers --features wgpu`)

Passing (9): `ball_falls_and_rests_on_ground`, `impulse_wakes_and_static_planes_block`,
`ray_hits_closest_first`, `capsule_falls_upright`, `box_falls_flat_on_floor`,
`fixed_joint_carries_both_bodies`, `hinge_anchors_coincide`,
`character_slides_on_wall_and_lands`, `violent_impact_stays_bounded`
(finite positions/velocities, no floor tunneling after a 3 m off-center drop).

Failing (2, tracked — see problems below):

- `boxes_stack_on_static_floor` — bottom box ends corner-balanced at
  `y ≈ 0.8660254` (= 0.5·√3, space diagonal vertical) instead of flat at 0.5.
- `tilted_box_settles_flat_with_damping` — 0.18 rad tilted drop ends
  edge-balanced at `y ≈ 0.7071` (= 0.5·√2) instead of flat, even with
  damping (0.3 linear / 1.5 angular).

## Known problems / limitations (honest list)

1. **Metastable balances freeze.** A box arriving at an exact corner/edge
   balance with ~zero energy stays there: contacts are perfectly symmetric,
   damping + sleep remove the residual rocking that would topple it in
   reality. Deterministic symmetry + sleep = stuck at an unstable equilibrium.
   Real gameplay (varied impulses) rarely hits these states; the deterministic
   tests do. Same class of quirk exists in other single-iteration PBD solvers.
2. **Offset loads tip instead of settling.** A top box placed even 0.02 off
   center walks the stack to an edge/corner balance (see 1) instead of
   rocking back flat. Flat rest is effectively repelling for offset loads in
   the current solver; only centered stacks settle flat.
3. **Zero-damping perpetual spin is correct behavior, not a bug.** A violent
   corner-first impact (e.g. 3 m drop onto a corner) physically injects
   ~15+ rad/s; with zero damping there is no air drag, so the body keeps
   spinning and only slowly dissipates at grazing contacts. Scenes that need
   settling must set damping (`body_set_damping`), same as Godot.
4. **Single velocity iteration.** The solver does one contact pass per step
   (plus 2 joint passes). Residual approach velocity (~30% with 1/N scaling)
   persists into the next step; convergence relies on positional correction
   and damping rather than iteration. Deep high-speed penetration can pick a
   wrong SAT axis for a step (recovers next steps; bounded per
   `violent_impact_stays_bounded`).
5. **Capsule-vs-box is approximate** (3 sub-sphere samples, deepest kept).
   Documented in code; exact segment-vs-box clipping is deferred.
6. **Sleep can freeze tilted poses.** Sleeping triggers on contact + low
   linear/angular velocity with no uprightness check, so a frozen corner/edge
   balance never wakes (nothing perturbs it). Ramps/leaning rest is fine;
   exact balances are the failure mode (see 1).
7. **Ground fallback is a single point + hard snap.** The y=0 fallback
   (`bb.position.y = rest`) applies full correction, unlike the
   slop/distributed manifold path. Consistent for flat rest; can fight the
   manifold path during tilted contact (double support at the same plane).

## Tuning knobs (where to adjust behavior)

- `body_set_damping(space, body, linear, angular)` — primary settling control.
- Contact slop `0.005`, friction cap `0.6·j`, angular cap `50 rad/s`,
  sleep thresholds (`0.0025` lin², `0.01` ang², 60 frames), joint passes (2).
- Manifold distribution `1/N` for position + velocity (`iscale` in
  `apply_pair_impulse` / `resolve_env`); snapshot (Jacobi) velocity reads.

## Deferred / not covered by this change

Voxel navmesh baking, FBX/Collada importers, `.bin` streaming, MP3 decode,
WebSocket handshake, WebRTC signaling — requested separately, untouched here.
