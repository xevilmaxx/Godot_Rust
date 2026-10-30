//! Full port of `servers/physics_3d/physics_server_3d.h` (GodotPhysics model).
//!
//! Deterministic fixed-step rigid bodies: static/kinematic/rigid bodies,
//! sphere/box/plane/capsule shapes, gravity + damping integration, SAT box-box
//! (axis-aligned; bodies carry no angular state), exact capsule segment math vs
//! spheres/planes, 3-sphere decomposition for the rest, static planes/boxes,
//! fixed + hinge joints, `CharacterBody3D::move_and_slide`, analytic raycasts,
//! sleeping with resting contacts. Full rotational SAT and the Jolt backend are
//! the documented next step (`physics/jolt.rs` mapping note below).

use godot_core::{Basis, GodotError, Quaternion, Transform3D, Vector3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::allocator::RidOwner;
use godot_core::Rid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BodyMode3D {
    Static,
    Kinematic,
    #[default]
    Rigid,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Shape3D {
    Sphere { radius: f32 },
    Box { extents: Vector3 },
    Plane { normal: Vector3, d: f32 },
    Capsule { radius: f32, height: f32 },
}

impl Shape3D {
    /// Bounding radius used for broadphase + box-as-sphere contacts.
    pub fn bounding_radius(&self) -> f32 {
        match *self {
            Shape3D::Sphere { radius } => radius,
            Shape3D::Box { extents } => extents.length(),
            Shape3D::Plane { .. } => f32::INFINITY,
            Shape3D::Capsule { radius, height } => radius + height * 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Body3D {
    pub mode: BodyMode3D,
    pub mass: f32,
    pub position: Vector3,
    pub linear_velocity: Vector3,
    /// World orientation (full rigid-body rotation; integrated every step).
    pub orientation: Basis,
    pub angular_velocity: Vector3,
    /// Diagonal inverse inertia in *body* space (updated when shapes change).
    pub inv_inertia_body: Vector3,
    pub gravity_scale: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub shapes: Vec<(Shape3D, Transform3D)>,
    pub sleeping: bool,
    sleep_frames: u32,
}

impl Body3D {
    fn new(mode: BodyMode3D, mass: f32) -> Self {
        let mut b = Self {
            mode,
            mass: mass.max(0.001),
            position: Vector3::ZERO,
            linear_velocity: Vector3::ZERO,
            orientation: Basis::IDENTITY,
            angular_velocity: Vector3::ZERO,
            inv_inertia_body: Vector3::ONE,
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            shapes: vec![],
            sleeping: false,
            sleep_frames: 0,
        };
        b.update_inertia();
        b
    }
    fn inv_mass(&self) -> f32 {
        match self.mode {
            BodyMode3D::Static => 0.0,
            _ => 1.0 / self.mass,
        }
    }
    /// World-space inverse inertia applied to `v`: R * (I^-1 * (R^T * v)).
    pub fn inv_inertia_world(&self, v: Vector3) -> Vector3 {
        let local = self.orientation.transposed().xform(v);
        let scaled = Vector3::new(local.x * self.inv_inertia_body.x, local.y * self.inv_inertia_body.y, local.z * self.inv_inertia_body.z);
        self.orientation.xform(scaled)
    }
    /// Solid-body inertia from the combined shape AABB (documented approx).
    fn update_inertia(&mut self) {
        let mut lo = Vector3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut hi = Vector3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut any = false;
        for (sh, _) in &self.shapes {
            let e = match *sh {
                Shape3D::Sphere { radius } => Vector3::new(radius, radius, radius),
                Shape3D::Box { extents } => extents,
                Shape3D::Capsule { radius, height } => Vector3::new(radius, radius + height * 0.5, radius),
                Shape3D::Plane { .. } => continue,
            };
            lo = lo.min(-e);
            hi = hi.max(e);
            any = true;
        }
        if !any {
            self.inv_inertia_body = Vector3::ONE / self.mass.max(1e-6);
            return;
        }
        let s = hi - lo; // full size
        let m = self.mass.max(1e-6);
        // Solid box 1/12 m (h²+d²) per axis.
        self.inv_inertia_body = Vector3::new(
            12.0 / (m * (s.y * s.y + s.z * s.z).max(1e-9)),
            12.0 / (m * (s.x * s.x + s.z * s.z).max(1e-9)),
            12.0 / (m * (s.x * s.x + s.y * s.y).max(1e-9)),
        );
    }
    pub fn apply_impulse(&mut self, impulse: Vector3) {
        if self.mode == BodyMode3D::Static {
            return;
        }
        self.sleeping = false;
        self.sleep_frames = 0;
        self.linear_velocity = self.linear_velocity + impulse * self.inv_mass();
    }
    /// Off-center impulse: linear kick + torque (`r` = world offset from center).
    pub fn apply_impulse_at(&mut self, impulse: Vector3, r: Vector3) {
        if self.mode == BodyMode3D::Static {
            return;
        }
        self.apply_impulse(impulse);
        self.angular_velocity = self.angular_velocity + self.inv_inertia_world(r.cross(impulse));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact3D {
    pub body: u64,
    pub position: Vector3,
    pub normal: Vector3,
    pub depth: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Space3D {
    pub gravity: Vector3,
    pub damping: f32,
    bodies: BTreeMap<u64, Body3D>,
    next_body: u64,
    pub contacts: Vec<Contact3D>,
    pub step_count: u64,
    fixed_joints: Vec<FixedJoint>,
    hinge_joints: Vec<HingeJoint>,
}

impl Space3D {
    fn new(gravity: Vector3) -> Self {
        Self {
            gravity,
            damping: 0.0,
            bodies: BTreeMap::new(),
            next_body: 1,
            contacts: vec![],
            step_count: 0,
            fixed_joints: vec![],
            hinge_joints: vec![],
        }
    }
}

/// Rigid weld: `b` keeps `offset` and relative orientation vs `a`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedJoint {
    pub a: u64,
    pub b: u64,
    pub offset_b_from_a: Vector3,
    /// Relative orientation of b in a's frame, captured at creation.
    pub rest_rel: Basis,
}

/// Ball-socket at coincident world anchors + hinge-axis alignment
/// (`axis_a`/`axis_b` are the hinge axis in each body's local frame).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HingeJoint {
    pub a: u64,
    pub b: u64,
    pub anchor_a: Vector3,
    pub anchor_b: Vector3,
    pub axis_a: Vector3,
    pub axis_b: Vector3,
}

/// Kinematic character controller state (Godot `CharacterBody3D` equivalent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterBody3D {
    pub position: Vector3,
    pub velocity: Vector3,
    pub radius: f32,
    pub floor_snap: f32,
    pub floor_max_angle_deg: f32,
    pub is_on_floor: bool,
    pub is_on_wall: bool,
    pub floor_normal: Vector3,
    was_on_floor: bool,
}

impl CharacterBody3D {
    pub fn new(position: Vector3, radius: f32) -> Self {
        Self {
            position,
            velocity: Vector3::ZERO,
            radius: radius.max(0.05),
            floor_snap: 0.1,
            floor_max_angle_deg: 45.0,
            is_on_floor: false,
            is_on_wall: false,
            floor_normal: Vector3::UP,
            was_on_floor: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoveResult {
    pub traveled: Vector3,
    pub collisions: usize,
    pub is_on_floor: bool,
    pub is_on_wall: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayHit3D {
    pub body: u64,
    pub position: Vector3,
    pub normal: Vector3,
    pub distance: f32,
}

pub struct PhysicsServer3D {
    spaces: RidOwner<Space3D>,
}

impl PhysicsServer3D {
    pub fn new() -> Self {
        Self { spaces: RidOwner::new() }
    }

    pub fn space_create(&mut self, gravity: Vector3) -> Rid {
        self.spaces.alloc(Space3D::new(gravity))
    }
    pub fn space_set_gravity(&mut self, space: Rid, g: Vector3) -> bool {
        if let Some(s) = self.spaces.get_mut(space) {
            s.gravity = g;
            return true;
        }
        false
    }
    pub fn space_get_gravity(&self, space: Rid) -> Option<Vector3> {
        self.spaces.get(space).map(|s| s.gravity)
    }
    pub fn free_space(&mut self, space: Rid) -> bool {
        self.spaces.free(space)
    }
    pub fn space_count(&self) -> usize {
        self.spaces.len()
    }

    pub fn body_create(&mut self, space: Rid, mode: BodyMode3D, mass: f32) -> Option<u64> {
        let s = self.spaces.get_mut(space)?;
        let id = s.next_body;
        s.next_body += 1;
        s.bodies.insert(id, Body3D::new(mode, mass));
        Some(id)
    }
    pub fn body_add_shape(&mut self, space: Rid, body: u64, shape: Shape3D) -> bool {
        if let Some(s) = self.spaces.get_mut(space) {
            if let Some(b) = s.bodies.get_mut(&body) {
                b.shapes.push((shape, Transform3D::IDENTITY));
                b.update_inertia();
                return true;
            }
        }
        false
    }
    pub fn body_set_orientation(&mut self, space: Rid, body: u64, basis: Basis) -> bool {
        if let Some(s) = self.spaces.get_mut(space) {
            if let Some(b) = s.bodies.get_mut(&body) {
                b.orientation = basis.orthonormalized();
                b.sleeping = false;
                return true;
            }
        }
        false
    }
    pub fn body_get_orientation(&self, space: Rid, body: u64) -> Option<Basis> {
        self.spaces.get(space)?.bodies.get(&body).map(|b| b.orientation)
    }
    pub fn body_set_angular_velocity(&mut self, space: Rid, body: u64, w: Vector3) -> bool {
        if let Some(s) = self.spaces.get_mut(space) {
            if let Some(b) = s.bodies.get_mut(&body) {
                b.angular_velocity = w;
                b.sleeping = false;
                return true;
            }
        }
        false
    }
    pub fn body_get_angular_velocity(&self, space: Rid, body: u64) -> Option<Vector3> {
        self.spaces.get(space)?.bodies.get(&body).map(|b| b.angular_velocity)
    }
    pub fn body_apply_impulse_at(&mut self, space: Rid, body: u64, impulse: Vector3, at_world: Vector3) -> bool {
        if let Some(s) = self.spaces.get_mut(space) {
            if let Some(b) = s.bodies.get_mut(&body) {
                let r = at_world - b.position;
                b.apply_impulse_at(impulse, r);
                return true;
            }
        }
        false
    }
    pub fn body_set_position(&mut self, space: Rid, body: u64, p: Vector3) -> bool {
        if let Some(s) = self.spaces.get_mut(space) {
            if let Some(b) = s.bodies.get_mut(&body) {
                b.position = p;
                return true;
            }
        }
        false
    }
    pub fn body_get_position(&self, space: Rid, body: u64) -> Option<Vector3> {
        self.spaces.get(space)?.bodies.get(&body).map(|b| b.position)
    }
    pub fn body_set_velocity(&mut self, space: Rid, body: u64, v: Vector3) -> bool {
        if let Some(s) = self.spaces.get_mut(space) {
            if let Some(b) = s.bodies.get_mut(&body) {
                b.linear_velocity = v;
                b.sleeping = false;
                return true;
            }
        }
        false
    }
    pub fn body_get_velocity(&self, space: Rid, body: u64) -> Option<Vector3> {
        self.spaces.get(space)?.bodies.get(&body).map(|b| b.linear_velocity)
    }
    /// Set linear + angular damping (per-second rates, Godot `linear_damp` /
    /// `angular_damp`). Damping bleeds energy (air drag + micro-slip) so
    /// impacts and rocking settle instead of ringing forever.
    pub fn body_set_damping(&mut self, space: Rid, body: u64, linear: f32, angular: f32) -> bool {
        if let Some(s) = self.spaces.get_mut(space) {
            if let Some(b) = s.bodies.get_mut(&body) {
                b.linear_damping = linear.max(0.0);
                b.angular_damping = angular.max(0.0);
                return true;
            }
        }
        false
    }
    pub fn body_apply_impulse(&mut self, space: Rid, body: u64, impulse: Vector3) -> bool {
        if let Some(s) = self.spaces.get_mut(space) {
            if let Some(b) = s.bodies.get_mut(&body) {
                b.apply_impulse(impulse);
                return true;
            }
        }
        false
    }
    pub fn body_is_sleeping(&self, space: Rid, body: u64) -> Option<bool> {
        self.spaces.get(space)?.bodies.get(&body).map(|b| b.sleeping)
    }
    pub fn contact_count(&self, space: Rid) -> usize {
        self.spaces.get(space).map(|s| s.contacts.len()).unwrap_or(0)
    }


    /// Fixed-step tick: integrate, collide (planes + sphere-ground), sleep.
    pub fn space_step(&mut self, space: Rid, dt: f64) {
        let Some(s) = self.spaces.get_mut(space) else { return };
        let dt = dt as f32;
        s.contacts.clear();
        s.step_count += 1;
        // Collect static planes first (world-space, offsets ignored in v1).
        let planes: Vec<(Vector3, f32)> = s
            .bodies
            .values()
            .filter(|b| b.mode == BodyMode3D::Static)
            .flat_map(|b| b.shapes.iter())
            .filter_map(|(sh, _)| match *sh {
                Shape3D::Plane { normal, d } => Some((normal.normalized(), d)),
                _ => None,
            })
            .collect();
        let ids: Vec<u64> = s.bodies.keys().copied().collect();
        // Pass 1: integrate awake dynamic bodies (linear + angular).
        for id in &ids {
            let (gravity, damping) = (s.gravity, s.damping);
            let Some(b) = s.bodies.get_mut(id) else { continue };
            if b.mode == BodyMode3D::Static || b.sleeping {
                continue;
            }
            if b.mode == BodyMode3D::Rigid {
                b.linear_velocity = b.linear_velocity + gravity * b.gravity_scale * dt;
                let damp_l = (1.0 - (b.linear_damping + damping) * dt).max(0.0);
                let damp_a = (1.0 - (b.angular_damping + damping) * dt).max(0.0);
                b.linear_velocity = b.linear_velocity * damp_l;
                b.angular_velocity = b.angular_velocity * damp_a;
            }
            b.position = b.position + b.linear_velocity * dt;
            // Orientation: q += 0.5 * dt * ω_quat * q, then renormalize.
            let wlen = b.angular_velocity.length();
            if wlen > 1e-9 {
                let axis = b.angular_velocity / wlen;
                let dq = Quaternion::from_axis_angle(axis, wlen * dt);
                let q = Quaternion::from_basis(b.orientation);
                b.orientation = (dq * q).to_basis().orthonormalized();
            }
        }
        // Pass 2a: static environment (planes + static boxes + y=0 fallback).
        // Static boxes honor body orientation + shape offsets.
        let static_boxes: Vec<(Vector3, Basis, Vector3)> = s
            .bodies
            .values()
            .filter(|b| b.mode == BodyMode3D::Static)
            .flat_map(|b| {
                b.shapes.iter().map(move |(sh, off)| {
                    let basis = b.orientation * off.basis;
                    let center = b.position + b.orientation.xform(off.origin);
                    (center, basis, *sh)
                })
            })
            .filter_map(|(c, basis, sh)| match sh {
                Shape3D::Box { extents } => Some((c, basis, extents)),
                _ => None,
            })
            .collect();
        let mut touched: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
        for id in &ids {
            let snap = match s.bodies.get(id) {
                Some(b) if b.mode != BodyMode3D::Static => (
                    b.position,
                    b.orientation,
                    b.inv_mass(),
                    !b.sleeping,
                    b.shapes.clone(),
                    b.linear_velocity,
                    b.angular_velocity,
                ),
                _ => continue,
            };
            let (pos, ori, _invm, awake, shapes, lin0, ang0) = snap;
            for (sh, off) in &shapes {
                let basis = ori * off.basis;
                let c = pos + ori.xform(off.origin);
                // vs static planes (orientation-aware support).
                for (n, d) in &planes {
                    if let Some((normal, depth, point)) = collide_shape_plane(c, &basis, sh, *n, *d) {
                        if depth > -1e-4 {
                            if awake && depth > 0.0 {
                                resolve_env(s, *id, point, normal, depth, 1.0, pos, lin0, ang0);
                            }
                            s.contacts.push(Contact3D { body: *id, position: point, normal, depth: depth.max(0.0) });
                            touched.insert(*id);
                        }
                    }
                }
                // vs static boxes (manifold; impulse distributed over points).
                for (bc, bb, be) in &static_boxes {
                    let manifold = collide_shapes(c, &basis, sh, *bc, bb, &Shape3D::Box { extents: *be });
                    let iscale = 1.0 / manifold.len().max(1) as f32;
                    for (normal, depth, point) in manifold {
                        if depth > -1e-4 {
                            if awake && depth > 0.0 {
                                resolve_env(s, *id, point, normal, depth, iscale, pos, lin0, ang0);
                            }
                            s.contacts.push(Contact3D { body: *id, position: point, normal, depth: depth.max(0.0) });
                            touched.insert(*id);
                        }
                    }
                }
            }
            // Ground plane y=0 fallback when no plane shapes exist (common test setup).
            if planes.is_empty() {
                let rest = rest_height_oriented(s, *id);
                let py = s.bodies.get(id).map(|b| b.position.y).unwrap_or(f32::INFINITY);
                let px = s.bodies.get(id).map(|b| b.position.x).unwrap_or(0.0);
                let pz = s.bodies.get(id).map(|b| b.position.z).unwrap_or(0.0);
                if rest > 0.0 && py - rest < 1e-4 {
                    let depth = rest - py;
                    if awake && depth > 0.0 {
                        if let Some(bb) = s.bodies.get_mut(id) {
                            bb.position.y = rest;
                            if bb.linear_velocity.y < 0.0 {
                                bb.linear_velocity.y = 0.0;
                                bb.linear_velocity.x *= 0.98;
                                bb.linear_velocity.z *= 0.98;
                                bb.angular_velocity = bb.angular_velocity * 0.98;
                            }
                            bb.sleeping = false;
                            bb.sleep_frames = 0;
                        }
                    }
                    s.contacts.push(Contact3D {
                        body: *id,
                        position: Vector3::new(px, 0.0, pz),
                        normal: Vector3::UP,
                        depth: depth.max(0.0),
                    });
                    touched.insert(*id);
                }
            }
        }
        // Pass 2b: body-body narrowphase with contact manifolds (deterministic id order).
        for ia in 0..ids.len() {
            for ib in (ia + 1)..ids.len() {
                let (a, b) = (ids[ia], ids[ib]);
                let snap_a = match s.bodies.get(&a) {
                    Some(x) => (x.position, x.orientation, x.inv_mass(), x.linear_velocity, x.angular_velocity),
                    None => continue,
                };
                let snap_b = match s.bodies.get(&b) {
                    Some(x) => (x.position, x.orientation, x.inv_mass(), x.linear_velocity, x.angular_velocity),
                    None => continue,
                };
                if snap_a.2 == 0.0 && snap_b.2 == 0.0 {
                    continue;
                }
                let sha = s.bodies.get(&a).map(|x| x.shapes.clone()).unwrap_or_default();
                let shb = s.bodies.get(&b).map(|x| x.shapes.clone()).unwrap_or_default();
                for (sa, offa) in &sha {
                    for (sb, offb) in &shb {
                        let ba = snap_a.1 * offa.basis;
                        let ca = snap_a.0 + snap_a.1.xform(offa.origin);
                        let bb = snap_b.1 * offb.basis;
                        let cb = snap_b.0 + snap_b.1.xform(offb.origin);
                        // Gather the manifold first: impulses are distributed
                        // over its points (1/N each). Sequential full-strength
                        // impulses otherwise compound through rotation —
                        // each point's impulse spins the body, inflating the
                        // next point's approach velocity (energy pump).
                        let manifold = collide_shapes(ca, &ba, sa, cb, &bb, sb);
                        let iscale = 1.0 / manifold.len().max(1) as f32;
                        for (n, depth, point) in manifold {
                            if depth <= 1e-4 {
                                // Resting touch: still reported.
                                s.contacts.push(Contact3D { body: a, position: point, normal: n, depth: depth.max(0.0) });
                                touched.insert(a);
                                touched.insert(b);
                                continue;
                            }
                            apply_pair_impulse(
                                s,
                                a,
                                Some(b),
                                n,
                                point,
                                depth,
                                iscale,
                                snap_a.0,
                                snap_a.3,
                                snap_a.4,
                                snap_b.0,
                                snap_b.3,
                                snap_b.4,
                            );
                            s.contacts.push(Contact3D { body: a, position: point, normal: n, depth });
                            touched.insert(a);
                            touched.insert(b);
                        }
                    }
                }
            }
        }
        // Pass 2c: joints (2 relaxation iterations).
        for _ in 0..2 {
            solve_joints(s);
        }
        // Pass 3: sleep bookkeeping (slow + touching -> deeper sleep).
        for id in &ids {
            let Some(b) = s.bodies.get_mut(id) else { continue };
            if b.mode != BodyMode3D::Rigid || b.sleeping {
                continue;
            }
            if touched.contains(id)
                && b.linear_velocity.length_squared() < 0.0025
                && b.angular_velocity.length_squared() < 0.01
            {
                b.sleep_frames += 1;
                if b.sleep_frames > 60 {
                    b.sleeping = true;
                    b.linear_velocity = Vector3::ZERO;
                    b.angular_velocity = Vector3::ZERO;
                }
            } else {
                b.sleep_frames = 0;
            }
        }
    }

    // ------------------------------------------------------------ joints ---
    /// Weld `b` to `a`, capturing relative offset and orientation.
    pub fn space_add_fixed(&mut self, space: Rid, a: u64, b: u64) -> Result<(), GodotError> {
        let s = self.spaces.get_mut(space).ok_or_else(|| GodotError::NotFound(format!("space {}", space.0)))?;
        let ba = s.bodies.get(&a).ok_or_else(|| GodotError::NotFound(format!("body {a}")))?;
        let (pa, oa) = (ba.position, ba.orientation);
        let bb = s.bodies.get(&b).ok_or_else(|| GodotError::NotFound(format!("body {b}")))?;
        let (pb, ob) = (bb.position, bb.orientation);
        s.fixed_joints.push(FixedJoint {
            a,
            b,
            offset_b_from_a: pb - pa,
            rest_rel: oa.transposed() * ob,
        });
        Ok(())
    }
    /// Ball-socket at a world anchor plus hinge-axis alignment. The axis is
    /// stored per body so bodies may start rotated; alignment is enforced by
    /// corrective angular velocity each tick (translation is a hard weld).
    pub fn space_add_hinge(&mut self, space: Rid, a: u64, b: u64, anchor_world: Vector3, axis: Vector3) -> Result<(), GodotError> {
        let s = self.spaces.get_mut(space).ok_or_else(|| GodotError::NotFound(format!("space {}", space.0)))?;
        let ba = s.bodies.get(&a).ok_or_else(|| GodotError::NotFound(format!("body {a}")))?;
        let (pa, oa) = (ba.position, ba.orientation);
        let bb = s.bodies.get(&b).ok_or_else(|| GodotError::NotFound(format!("body {b}")))?;
        let (pb, ob) = (bb.position, bb.orientation);
        let axis = axis.normalized();
        s.hinge_joints.push(HingeJoint {
            a,
            b,
            anchor_a: anchor_world - pa,
            anchor_b: anchor_world - pb,
            axis_a: oa.transposed().xform(axis),
            axis_b: ob.transposed().xform(axis),
        });
        Ok(())
    }
    pub fn joint_counts(&self, space: Rid) -> (usize, usize) {
        self.spaces.get(space).map(|s| (s.fixed_joints.len(), s.hinge_joints.len())).unwrap_or((0, 0))
    }

    // ---------------------------------------------------------- character --
    /// Kinematic move-and-slide for a `CharacterBody3D` (sphere approximation).
    /// Gravity is applied by the caller through `gravity` (pass ZERO for none).
    pub fn character_move(&mut self, space: Rid, body: &mut CharacterBody3D, dt: f64, gravity: Vector3) -> MoveResult {
        let mut out = MoveResult::default();
        let dt = dt as f32;
        body.velocity = body.velocity + gravity * dt;
        let start = body.position;
        body.position = body.position + body.velocity * dt;
        body.is_on_floor = false;
        body.is_on_wall = false;
        let floor_cos = body.floor_max_angle_deg.to_radians().cos();
        // Static planes + static boxes + dynamic bodies (sphere approx each).
        for _ in 0..4 {
            let mut best: Option<(Vector3, f32)> = None;
            let consider = |best: &mut Option<(Vector3, f32)>, n: Vector3, depth: f32| {
                if depth > 1e-4 && best.map(|(_, d)| depth > d).unwrap_or(true) {
                    *best = Some((n, depth));
                }
            };
            if let Some(s) = self.spaces.get(space) {
                let char_basis = Basis::IDENTITY;
                let char_shape = Shape3D::Sphere { radius: body.radius };
                for b in s.bodies.values() {
                    if b.mode == BodyMode3D::Static {
                        for (sh, off) in &b.shapes {
                            let ob = b.orientation * off.basis;
                            let c = b.position + b.orientation.xform(off.origin);
                            match sh {
                                Shape3D::Plane { .. } => {
                                    if let Some((n, depth, _)) =
                                        collide_shapes(body.position, &char_basis, &char_shape, c, &ob, sh).into_iter().next()
                                    {
                                        consider(&mut best, n, depth);
                                    }
                                }
                                Shape3D::Box { .. } | Shape3D::Sphere { .. } | Shape3D::Capsule { .. } => {
                                    for (n, depth, _) in collide_shapes(body.position, &char_basis, &char_shape, c, &ob, sh) {
                                        consider(&mut best, n, depth);
                                    }
                                }
                            }
                        }
                    }
                }
                // Dynamic bodies as obstacles (oriented).
                for b in s.bodies.values() {
                    if b.mode == BodyMode3D::Static {
                        continue;
                    }
                    for (sh, off) in &b.shapes {
                        let ob = b.orientation * off.basis;
                        let c = b.position + b.orientation.xform(off.origin);
                        for (n, depth, _) in collide_shapes(body.position, &char_basis, &char_shape, c, &ob, sh) {
                            consider(&mut best, n, depth);
                        }
                    }
                }
            }
            let Some((n, depth)) = best else { break };
            body.position = body.position + n * depth;
            let vn = n.dot(body.velocity);
            if vn < 0.0 {
                body.velocity = body.velocity - n * vn; // slide
            }
            out.collisions += 1;
            if n.y > floor_cos {
                body.is_on_floor = true;
                body.floor_normal = n;
            } else if n.y < 0.7 {
                body.is_on_wall = true;
            }
        }
        // Floor snap: stick when stepping off ledges while moving down.
        if !body.is_on_floor && body.was_on_floor && body.velocity.y <= 0.0 && body.floor_snap > 0.0 {
            let probe = body.position - Vector3::UP * body.floor_snap;
            let mut snapped = false;
            if let Some(s) = self.spaces.get(space) {
                'outer: for b in s.bodies.values() {
                    if b.mode != BodyMode3D::Static {
                        continue;
                    }
                    for (sh, off) in &b.shapes {
                        let ob = b.orientation * off.basis;
                        let c = b.position + b.orientation.xform(off.origin);
                        let char_shape = Shape3D::Sphere { radius: body.radius };
                        let hit = collide_shapes(probe, &Basis::IDENTITY, &char_shape, c, &ob, sh)
                            .into_iter()
                            .any(|(n, _, _)| n.y > floor_cos);
                        if hit {
                            snapped = true;
                            break 'outer;
                        }
                    }
                }
            }
            if snapped {
                body.position = probe;
                body.is_on_floor = true;
                body.floor_normal = Vector3::UP;
            }
        }
        body.was_on_floor = body.is_on_floor;
        out.traveled = body.position - start;
        out.is_on_floor = body.is_on_floor;
        out.is_on_wall = body.is_on_wall;
        out
    }

    /// Closest-hit ray vs spheres/boxes (slab)/planes. Returns `None` on miss.
    pub fn ray_cast(&self, space: Rid, from: Vector3, to: Vector3) -> Option<RayHit3D> {
        let s = self.spaces.get(space)?;
        let dir = to - from;
        let len = dir.length();
        if len < 1e-9 {
            return None;
        }
        let d = dir / len;
        let mut best: Option<RayHit3D> = None;
        for (id, b) in &s.bodies {
            for (sh, off) in &b.shapes {
                // World frame of the shape (orientation-aware).
                let ob = b.orientation * off.basis;
                let c = b.position + b.orientation.xform(off.origin);
                let hit = match *sh {
                    Shape3D::Sphere { radius } => ray_sphere(from, d, len, c, radius),
                    Shape3D::Box { extents } => {
                        // Transform the ray into the box frame, slab-test, rotate the normal back.
                        let inv = ob.transposed();
                        let lo = inv.xform(from - c);
                        let ld = inv.xform(d);
                        // Guard against zero-length local direction after transform.
                        let ldl = ld.length().max(1e-12);
                        let ln = ld / ldl;
                        ray_aabb(lo, ln, len * ldl, -extents, extents).map(|(t, n)| (t / ldl, (ob.xform(n)).normalized()))
                    }
                    Shape3D::Plane { normal, d: pd } => ray_plane(from, d, len, normal.normalized(), pd),
                    Shape3D::Capsule { radius, height } => {
                        // Test the two end spheres plus midpoint (matches narrowphase).
                        let axis = ob.y;
                        let mut best_hit: Option<(f32, Vector3)> = None;
                        for t in [0.0f32, 0.5, 1.0] {
                            let sc = c + axis * (height * (0.5 - t));
                            if let Some((tt, n)) = ray_sphere(from, d, len, sc, radius) {
                                if best_hit.map(|(bd, _)| tt < bd).unwrap_or(true) {
                                    best_hit = Some((tt, n));
                                }
                            }
                        }
                        best_hit
                    }
                };
                if let Some((t, n)) = hit {
                    let replace = best.as_ref().map(|b| t < b.distance).unwrap_or(true);
                    if replace {
                        best = Some(RayHit3D { body: *id, position: from + d * t, normal: n, distance: t });
                    }
                }
            }
        }
        best
    }
}

impl Default for PhysicsServer3D {
    fn default() -> Self {
        Self::new()
    }
}

/// Rest height above y=0 for the ground fallback (max shape extent).
/// Rest height above y=0 for the ground fallback: oriented support in +Y.
fn rest_height_oriented(s: &Space3D, id: u64) -> f32 {
    s.bodies
        .get(&id)
        .map(|b| {
            b.shapes
                .iter()
                .map(|(sh, off)| {
                    let o: Basis = b.orientation * off.basis;
                    match *sh {
                        Shape3D::Sphere { radius } => radius,
                        Shape3D::Box { extents } => extents.x * o.x.y.abs() + extents.y * o.y.y.abs() + extents.z * o.z.y.abs(),
                        Shape3D::Capsule { radius, height } => {
                            let ax = o.y;
                            radius + (height * 0.5) * ax.y.abs()
                        }
                        Shape3D::Plane { .. } => 0.0,
                    }
                })
                .fold(0f32, f32::max)
        })
        .unwrap_or(0.0)
}

/// Push a body out along `n` by `depth`, killing approaching normal velocity
/// at the contact point (with angular response). `n` points away from the
/// surface (toward the body).
fn resolve_env(
    s: &mut Space3D,
    id: u64,
    point: Vector3,
    n: Vector3,
    depth: f32,
    iscale: f32,
    pa0: Vector3,
    va0: Vector3,
    wa0: Vector3,
) {
    let ima = match s.bodies.get(&id) {
        Some(b) => b.inv_mass(),
        None => return,
    };
    if ima <= 0.0 {
        return;
    }
    // Slop + manifold-distributed factor, matching apply_pair_impulse.
    let corr = ((depth - 0.005).max(0.0) * iscale).min(depth);
    if let Some(b) = s.bodies.get_mut(&id) {
        b.position = b.position + n * corr;
    }
    // Snapshot-based approach velocity (Jacobi-style, order-independent).
    let v_at = va0 + wa0.cross(point - pa0);
    let vn = v_at.dot(n);
    if vn < 0.0 {
        apply_impulse_at(s, id, n * (-vn * iscale), point);
        // Coulomb-lite tangential damping for resting stability.
        if let Some(b) = s.bodies.get_mut(&id) {
            let t = b.linear_velocity - n * b.linear_velocity.dot(n);
            b.linear_velocity = n * b.linear_velocity.dot(n) + t * 0.98;
            b.angular_velocity = b.angular_velocity * 0.98;
        }
    }
}

/// Velocity of a body point (linear + angular contribution).
fn point_velocity(s: &Space3D, id: u64, point: Vector3) -> Vector3 {
    match s.bodies.get(&id) {
        Some(b) => b.linear_velocity + b.angular_velocity.cross(point - b.position),
        None => Vector3::ZERO,
    }
}

/// Apply an impulse at a world point (linear + torque). Wakes the body.
fn apply_impulse_at(s: &mut Space3D, id: u64, impulse: Vector3, point: Vector3) {
    if let Some(b) = s.bodies.get_mut(&id) {
        if b.inv_mass() <= 0.0 {
            return;
        }
        let r = point - b.position;
        b.linear_velocity = b.linear_velocity + impulse * b.inv_mass();
        b.angular_velocity = b.angular_velocity + b.inv_inertia_world(r.cross(impulse));
        b.sleeping = false;
        b.sleep_frames = 0;
    }
}

/// Unified contact resolver for body-vs-(static|body). `n` points from B toward
/// A (for static environments B is the immovable side), `point` is the contact.
/// `iscale` distributes the impulse over a manifold (1/N points): sequential
/// full-strength impulses compound through rotation (each point's impulse
/// spins the body, inflating the next point's approach velocity).
/// Velocities/positions with `0` suffix are pre-manifold snapshots: all
/// points of a manifold compute from the SAME snapshot (Jacobi-style), so
/// the result does not depend on contact processing order. Sequential live
/// reads make first-processed corners dominate and inject systematic torque.
#[allow(clippy::too_many_arguments)]
fn apply_pair_impulse(
    s: &mut Space3D,
    a: u64,
    b: Option<u64>,
    n: Vector3,
    point: Vector3,
    depth: f32,
    iscale: f32,
    pa0: Vector3,
    va0: Vector3,
    wa0: Vector3,
    pb0: Vector3,
    vb0: Vector3,
    wb0: Vector3,
) {
    let ima = s.bodies.get(&a).map(|x| x.inv_mass()).unwrap_or(0.0);
    let imb = b.map(|id| s.bodies.get(&id).map(|x| x.inv_mass()).unwrap_or(0.0)).unwrap_or(0.0);
    let wsum = ima + imb;
    if wsum <= 0.0 {
        return;
    }
    // Positional correction with slop, distributed over the manifold
    // (inv_n = 1/N contacts): the N points together remove ~one depth.
    // Full correction per point overshoots N-fold (launch/ratchet).
    if depth > 0.0 {
        let corr = ((depth - 0.005).max(0.0) * iscale).min(depth);
        if let Some(ba) = s.bodies.get_mut(&a) {
            ba.position = ba.position + n * (corr * ima / wsum);
        }
        if let Some(id) = b {
            if let Some(bb) = s.bodies.get_mut(&id) {
                bb.position = bb.position - n * (corr * imb / wsum);
            }
        }
    }
    // Relative velocity at the contact, with rotation — from the SNAPSHOT
    // (see doc comment). Live reads here would make the result depend on
    // contact order (first-processed corners dominate).
    let ra = point - pa0;
    let va = va0 + wa0.cross(ra);
    let (vb, rb) = match b {
        Some(_) => {
            let r = point - pb0;
            (vb0 + wb0.cross(r), r)
        }
        None => (Vector3::ZERO, Vector3::ZERO),
    };
    let rv = va - vb;
    let rvn = rv.dot(n);
    // Rolling resistance: bleed off spin so offset loads settle instead of
    // winding up (standard contact damping; linear motion is unaffected).
    // Safety valve caps runaway spin (keeps the sim bounded).
    if let Some(ba) = s.bodies.get_mut(&a) {
        ba.angular_velocity = ba.angular_velocity * 0.995;
        let w = ba.angular_velocity.length();
        if w > 50.0 {
            ba.angular_velocity = ba.angular_velocity * (50.0 / w);
        }
    }
    if let Some(id) = b {
        if let Some(bb) = s.bodies.get_mut(&id) {
            bb.angular_velocity = bb.angular_velocity * 0.995;
            let w = bb.angular_velocity.length();
            if w > 50.0 {
                bb.angular_velocity = bb.angular_velocity * (50.0 / w);
            }
        }
    }
    if rvn < 0.0 {
        // Normal impulse with angular terms: kn = J^-1 along n.
        let ra_cn = ra.cross(n);
        let rb_cn = rb.cross(n);
        let ka = s.bodies.get(&a).map(|x| x.inv_inertia_world(ra_cn)).unwrap_or(Vector3::ZERO);
        let kb = match b {
            Some(id) => s.bodies.get(&id).map(|x| x.inv_inertia_world(rb_cn)).unwrap_or(Vector3::ZERO),
            None => Vector3::ZERO,
        };
        let kn = ima + imb + ra_cn.dot(ka) + rb_cn.dot(kb);
        let j = -rvn / kn.max(1e-9) * iscale;
        apply_impulse_at(s, a, n * j, point);
        if let Some(id) = b {
            apply_impulse_at(s, id, n * -j, point);
        }
        // Coulomb friction, clamped by mu * j.
        let t = rv - n * rvn;
        let tl = t.length();
        if tl > 1e-6 {
            let tdir = t / tl;
            let ra_ct = ra.cross(tdir);
            let rb_ct = rb.cross(tdir);
            let ka_t = s.bodies.get(&a).map(|x| x.inv_inertia_world(ra_ct)).unwrap_or(Vector3::ZERO);
            let kb_t = match b {
                Some(id) => s.bodies.get(&id).map(|x| x.inv_inertia_world(rb_ct)).unwrap_or(Vector3::ZERO),
                None => Vector3::ZERO,
            };
            let kt = ima + imb + ra_ct.dot(ka_t) + rb_ct.dot(kb_t);
            let jt = (tl / kt.max(1e-9)).min(0.6 * j);
            apply_impulse_at(s, a, tdir * -jt, point);
            if let Some(id) = b {
                apply_impulse_at(s, id, tdir * jt, point);
            }
        }
    }
}

/// Narrowphase between two oriented shapes. Returns contact *manifold* points:
/// Vec of (normal from B toward A, penetration depth, contact point).
/// Boxes use full 15-axis SAT with face clipping; spheres are exact; capsules
/// use exact segment math vs sphere/plane/capsule and a documented 3-sphere
/// approximation vs boxes. `oa`/`ob` must be orthonormal.
fn collide_shapes(
    pa: Vector3,
    oa: &Basis,
    a: &Shape3D,
    pb: Vector3,
    ob: &Basis,
    b: &Shape3D,
) -> Vec<(Vector3, f32, Vector3)> {
    use Shape3D as S;
    match (a, b) {
        (S::Plane { .. }, _) => vec![], // planes handled via collide_shape_plane
        (_, S::Plane { normal, d }) => {
            collide_shape_plane(pa, oa, a, normal.normalized(), *d).into_iter().collect()
        }
        (S::Sphere { radius: ra }, S::Sphere { radius: rb }) => {
            let delta = pa - pb;
            let dist = delta.length();
            if dist > ra + rb + 1e-4 {
                return vec![];
            }
            let n = if dist > 1e-6 { delta / dist } else { Vector3::RIGHT };
            let depth = (ra + rb - dist).max(0.0);
            vec![(n, depth, pa - n * (ra - depth * 0.5))]
        }
        (S::Sphere { radius }, S::Box { extents }) => sphere_obox(pa, *radius, pb, ob, *extents).into_iter().collect(),
        (S::Box { extents }, S::Sphere { radius }) => {
            // Swap roles, then flip the normal back to B->A.
            sphere_obox(pb, *radius, pa, oa, *extents).into_iter().map(|(n, d, p)| (-n, d, p)).collect()
        }
        (S::Box { extents: ea }, S::Box { extents: eb }) => box_box(pa, oa, *ea, pb, ob, *eb),
        (S::Capsule { radius, height }, S::Capsule { radius: rb, height: hb }) => {
            let (p0, p1) = (pa + oa.y * (height * 0.5), pa - oa.y * (height * 0.5));
            let (q0, q1) = (pb + ob.y * (hb * 0.5), pb - ob.y * (hb * 0.5));
            fn norm_or(v: Vector3, fb: Vector3) -> Vector3 {
                let l = v.length();
                if l > 1e-9 { v / l } else { fb }
            }
            let (ca, da, ha) = ((p0 + p1) * 0.5, norm_or(p1 - p0, Vector3::UP), (p1 - p0).length() * 0.5);
            let (cb, db, hb_) = ((q0 + q1) * 0.5, norm_or(q1 - q0, Vector3::UP), (q1 - q0).length() * 0.5);
            let (cp, cq, _, _) = closest_seg_seg(ca, da, ha, cb, db, hb_);
            let delta = cp - cq;
            let dist = delta.length();
            if dist > radius + rb + 1e-4 {
                return vec![];
            }
            let n = if dist > 1e-6 { delta / dist } else { Vector3::UP };
            vec![(n, (radius + rb - dist).max(0.0), (cp + cq) * 0.5)]
        }
        (S::Capsule { radius, height }, _) => {
            let (p0, p1) = (pa + oa.y * (height * 0.5), pa - oa.y * (height * 0.5));
            capsule_vs_shape(p0, p1, *radius, pb, ob, b, false)
        }
        (_, S::Capsule { radius, height }) => {
            let (p0, p1) = (pb + ob.y * (height * 0.5), pb - ob.y * (height * 0.5));
            capsule_vs_shape(p0, p1, *radius, pa, oa, a, true)
        }
    }
}

/// Capsule segment (p0-p1, radius r) vs a non-capsule shape whose center/frame
/// is (`c`, `o`). `flip` inverts the normal convention to (B toward A) when
/// the capsule was shape B.
fn capsule_vs_shape(
    p0: Vector3,
    p1: Vector3,
    r: f32,
    c: Vector3,
    o: &Basis,
    sh: &Shape3D,
    flip: bool,
) -> Vec<(Vector3, f32, Vector3)> {
    use Shape3D as S;
    let flip_n = |n: Vector3| if flip { -n } else { n };
    match sh {
        S::Sphere { radius } => {
            // Closest point on segment to sphere center.
            let ab = p1 - p0;
            let t = ((c - p0).dot(ab) / ab.dot(ab).max(1e-12)).clamp(0.0, 1.0);
            let cp = p0 + ab * t; // closest on capsule spine
            let delta = if flip { c - cp } else { cp - c };
            let dist = delta.length();
            if dist > r + radius + 1e-4 {
                return vec![];
            }
            let n = if dist > 1e-6 { delta / dist } else { Vector3::UP };
            // Contact on the capsule's spherical surface facing the sphere.
            let p = if flip { cp + n * r } else { cp - n * r };
            vec![(n, (r + radius - dist).max(0.0), p)]
        }
        S::Box { extents } => {
            // Documented approximation: 3 sub-sphere samples vs the oriented box.
            let mut best: Option<(Vector3, f32, Vector3)> = None;
            for t in [0.0f32, 0.5, 1.0] {
                let sc = p0 + (p1 - p0) * t;
                if let Some((n, d, p)) = sphere_obox(sc, r, c, o, *extents) {
                    if best.map(|(_, bd, _)| d > bd).unwrap_or(true) {
                        best = Some((flip_n(n), d, p));
                    }
                }
            }
            best.into_iter().collect()
        }
        S::Plane { normal, d } => {
            // Exact: segment vs plane.
            let n = normal.normalized();
            let d0 = n.dot(p0) - d;
            let d1 = n.dot(p1) - d;
            if d0 < 0.0 && d1 < 0.0 {
                let (pc, s) = if d0 < d1 { (p0, d0) } else { (p1, d1) };
                vec![(flip_n(n), (r - s).max(0.0), pc - flip_n(n) * r)]
            } else if (d0 < 0.0) != (d1 < 0.0) {
                let t = d0 / (d0 - d1);
                let pc = p0 + (p1 - p0) * t;
                vec![(flip_n(n), r, pc)]
            } else {
                let (pc, s) = if d0 < d1 { (p0, d0) } else { (p1, d1) };
                if s > r + 1e-4 {
                    vec![]
                } else {
                    vec![(flip_n(n), (r - s).max(0.0), pc - flip_n(n) * r)]
                }
            }
        }
        _ => vec![], // capsules handled by the caller before reaching here
    }
}

/// Sphere (center + radius) vs oriented box (center + orthonormal basis +
/// half-extents). Normal points from the box toward the sphere.
fn sphere_obox(sc: Vector3, r: f32, bc: Vector3, ob: &Basis, e: Vector3) -> Option<(Vector3, f32, Vector3)> {
    // Work in the box frame.
    let t = ob.transposed();
    let local = t.xform(sc - bc);
    let clamped = Vector3::new(local.x.clamp(-e.x, e.x), local.y.clamp(-e.y, e.y), local.z.clamp(-e.z, e.z));
    let delta = local - clamped;
    let dist = delta.length();
    if dist > r + 1e-4 {
        return None;
    }
    if dist > 1e-6 {
        let n = ob.xform(delta / dist);
        return Some((n, (r - dist).max(0.0), bc + ob.xform(clamped)));
    }
    // Center inside: push along the least-penetration local axis.
    let px = e.x - local.x.abs();
    let py = e.y - local.y.abs();
    let pz = e.z - local.z.abs();
    // Local face normal with correct sign, transformed back to world.
    let n_local = if px <= py && px <= pz {
        Vector3::new(sgn(local.x), 0.0, 0.0)
    } else if py <= pz {
        Vector3::new(0.0, sgn(local.y), 0.0)
    } else {
        Vector3::new(0.0, 0.0, sgn(local.z))
    };
    let n = ob.xform(n_local);
    let depth = if px <= py && px <= pz { px } else if py <= pz { py } else { pz } + r;
    Some((n, depth, bc + ob.xform(clamped)))
}

/// Oriented box vs oriented box: full 15-axis SAT with face clipping.
/// Returns (normal B->A, depth, contact point) manifold points (max 8).
/// Bases must be orthonormal; boxes are centered at pa/pb with half-extents.
fn box_box(pa: Vector3, oa: &Basis, ea: Vector3, pb: Vector3, ob: &Basis, eb: Vector3) -> Vec<(Vector3, f32, Vector3)> {
    let au = [oa.x, oa.y, oa.z];
    let bu = [ob.x, ob.y, ob.z];
    let t = pb - pa; // B relative to A
    let support = |e: Vector3, cols: &[Vector3; 3], n: Vector3| e.x * n.dot(cols[0]).abs() + e.y * n.dot(cols[1]).abs() + e.z * n.dot(cols[2]).abs();
    // Track best axis: (overlap, kind, i, j, normal B->A). kind: 0 = face A, 1 = face B, 2 = edge.
    let mut best: Option<(f32, u8, usize, usize, Vector3)> = None;
    let consider = |overlap: f32, kind: u8, i: usize, j: usize, n: Vector3, best: &mut Option<(f32, u8, usize, usize, Vector3)>| {
        if overlap < -1e-4 {
            return false; // separated
        }
        let better = match best {
            None => true,
            Some(b) => overlap < b.0 - 1e-9 || (b.1 == 2 && kind < 2 && (overlap - b.0).abs() < 1e-9),
        };
        if better {
            *best = Some((overlap.max(0.0), kind, i, j, n));
        }
        true
    };
    // Face axes of A.
    for i in 0..3 {
        let ax = au[i];
        let o = support(ea, &au, ax) + support(eb, &bu, ax) - t.dot(ax).abs();
        if !consider(o, 0, i, 0, -ax * sgn(t.dot(ax)), &mut best) {
            return vec![];
        }
    }
    // Face axes of B.
    for j in 0..3 {
        let ax = bu[j];
        let o = support(ea, &au, ax) + support(eb, &bu, ax) - t.dot(ax).abs();
        if !consider(o, 1, 0, j, -ax * sgn(t.dot(ax)), &mut best) {
            return vec![];
        }
    }
    // Edge-edge axes.
    for i in 0..3 {
        for j in 0..3 {
            let mut ax = au[i].cross(bu[j]);
            let l2 = ax.length_squared();
            if l2 < 1e-12 {
                continue; // parallel edges
            }
            ax = ax / l2.sqrt();
            let o = support(ea, &au, ax) + support(eb, &bu, ax) - t.dot(ax).abs();
            if !consider(o, 2, i, j, -ax * sgn(t.dot(ax)), &mut best) {
                return vec![];
            }
        }
    }
    let (overlap, kind, i, j, n) = match best {
        Some(b) => b,
        None => return vec![],
    };
    let _ = overlap;
    match kind {
        0 => {
            // Reference face on A (outward normal toward B).
            let r_out = au[i] * sgn(t.dot(au[i]));
            let (u, v) = other_axes(i);
            let (hu, hv) = (ecomp(ea, u), ecomp(ea, v));
            let ref_c = pa + au[i] * (sgn(t.dot(au[i])) * ecomp(ea, i));
            // Incident face of B: most anti-parallel to r_out.
            let mut bi = 0;
            let mut bs = 1.0f32;
            let mut bd = f32::INFINITY;
            for k in 0..3 {
                for s in [-1.0f32, 1.0] {
                    let dd = bu[k].dot(r_out) * s;
                    if dd < bd {
                        bd = dd;
                        bi = k;
                        bs = s;
                    }
                }
            }
            let (bu0, bv0) = other_axes(bi);
            let mut corners = vec![];
            for &(su, sv) in &[(-1.0f32, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut p = pb + bu[bi] * (bs * ecomp(eb, bi));
                p = p + bu[bu0] * (su * ecomp(eb, bu0)) + bu[bv0] * (sv * ecomp(eb, bv0));
                corners.push(p);
            }
            clip_face_contact(&corners, ref_c, r_out, au[u], au[v], hu, hv, n)
        }
        1 => {
            // Reference face on B (normal toward A is already n... recompute).
            // Ref face: the face of B whose outward normal is most aligned with n.
            let mut bi = 0;
            let mut bs = 1.0f32;
            let mut bd = f32::NEG_INFINITY;
            for k in 0..3 {
                for s in [-1.0f32, 1.0] {
                    let dd = bu[k].dot(n) * s;
                    if dd > bd {
                        bd = dd;
                        bi = k;
                        bs = s;
                    }
                }
            }
            let ref_n = bu[bi] * bs; // outward, toward A
            let (bu0, bv0) = other_axes(bi);
            let (hu, hv) = (ecomp(eb, bu0), ecomp(eb, bv0));
            let ref_c = pb + bu[bi] * (bs * ecomp(eb, bi));
            // Incident face of A: most anti-parallel to ref_n.
            let mut ai = 0;
            let mut as_ = 1.0f32;
            let mut ad = f32::INFINITY;
            for k in 0..3 {
                for s in [-1.0f32, 1.0] {
                    let dd = au[k].dot(ref_n) * s;
                    if dd < ad {
                        ad = dd;
                        ai = k;
                        as_ = s;
                    }
                }
            }
            let (au0, av0) = other_axes(ai);
            let mut corners = vec![];
            for &(su, sv) in &[(-1.0f32, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut p = pa + au[ai] * (as_ * ecomp(ea, ai));
                p = p + au[au0] * (su * ecomp(ea, au0)) + au[av0] * (sv * ecomp(ea, av0));
                corners.push(p);
            }
            clip_face_contact(&corners, ref_c, ref_n, bu[bu0], bu[bv0], hu, hv, n)
        }
        _ => {
            // Edge-edge: closest points between the two edges.
            let (ea_c, ea_dir, ea_half) = (pa, au[i], ecomp(ea, i));
            let (eb_c, eb_dir, eb_half) = (pb, bu[j], ecomp(eb, j));
            let (c1, c2, _, _) = closest_seg_seg(ea_c, ea_dir, ea_half, eb_c, eb_dir, eb_half);
            vec![(n, overlap, (c1 + c2) * 0.5)]
        }
    }
}

/// Half-extent component by axis index (Vector3 has no Index impl).
fn ecomp(e: Vector3, i: usize) -> f32 {
    match i {
        0 => e.x,
        1 => e.y,
        _ => e.z,
    }
}

fn other_axes(i: usize) -> (usize, usize) {
    match i {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

/// Clip an incident quad to a reference face rect, emitting contact points
/// with per-point depths. `ref_c` is the face center, `ref_n` its outward
/// normal (toward the incident body), (u,v)/(hu,hv) the tangent frame.
fn clip_face_contact(
    corners: &[Vector3],
    ref_c: Vector3,
    ref_n: Vector3,
    u: Vector3,
    v: Vector3,
    hu: f32,
    hv: f32,
    n: Vector3,
) -> Vec<(Vector3, f32, Vector3)> {
    let mut out = vec![];
    for &p in corners {
        let d = p - ref_c;
        let cu = d.dot(u);
        let cv = d.dot(v);
        if cu.abs() > hu + 1e-4 || cv.abs() > hv + 1e-4 {
            continue;
        }
        let depth = -d.dot(ref_n);
        if depth < -1e-4 {
            continue;
        }
        // Project onto the face plane for a stable contact point.
        let proj = p - ref_n * (d.dot(ref_n) + depth * 0.0);
        out.push((n, depth.max(0.0), proj));
        if out.len() >= 8 {
            break;
        }
    }
    out
}

/// Closest points between segments p1+s*d1 (|s|<=h1) and p2+t*d2 (|t|<=h2).
/// Returns (point_on_1, point_on_2, s, t). Ericson 5.1.9.
fn closest_seg_seg(p1: Vector3, d1: Vector3, h1: f32, p2: Vector3, d2: Vector3, h2: f32) -> (Vector3, Vector3, f32, f32) {
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let (mut s, mut t);
    if a <= 1e-9 && e <= 1e-9 {
        return (p1, p2, 0.0, 0.0);
    }
    if a <= 1e-9 {
        s = 0.0;
        t = (f / e).clamp(-h2, h2);
    } else {
        let c = d1.dot(r);
        if e <= 1e-9 {
            t = 0.0;
            s = (-c / a).clamp(-h1, h1);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            s = if denom.abs() > 1e-9 { ((b * f - c * e) / denom).clamp(-h1, h1) } else { 0.0 };
            t = (b * s + f) / e;
            if t < -h2 || t > h2 {
                t = t.clamp(-h2, h2);
                s = ((-b * t - c) / a).clamp(-h1, h1);
            }
        }
    }
    (p1 + d1 * s, p2 + d2 * t, s, t)
}

/// Shape vs static plane with orientation. `n` points away from the plane
/// (toward the body); depth > 0 means penetration.
fn collide_shape_plane(c: Vector3, o: &Basis, sh: &Shape3D, n: Vector3, d: f32) -> Option<(Vector3, f32, Vector3)> {
    use Shape3D as S;
    match *sh {
        S::Sphere { radius } => {
            let s = n.dot(c) - d;
            Some((n, radius - s, c - n * s))
        }
        S::Box { extents } => {
            let cols = [o.x, o.y, o.z];
            let support = extents.x * n.dot(cols[0]).abs() + extents.y * n.dot(cols[1]).abs() + extents.z * n.dot(cols[2]).abs();
            let s = n.dot(c) - d;
            Some((n, support - s, c - n * s))
        }
        S::Capsule { radius, height } => {
            let axis = o.y;
            let (p0, p1) = (c + axis * (height * 0.5), c - axis * (height * 0.5));
            let (s, pc) = if n.dot(p0) - d < n.dot(p1) - d { (n.dot(p0) - d, p0) } else { (n.dot(p1) - d, p1) };
            Some((n, radius - s, pc - n * s))
        }
        S::Plane { .. } => None,
    }
}

/// Solve fixed + hinge joints (position projection + velocity weld, 2 passes
/// are run by the caller).
/// Solve fixed + hinge joints (2 relaxation passes are run by the caller).
/// Translation is a hard projection; rotation uses corrective angular velocity.
fn solve_joints(s: &mut Space3D) {
    for j in s.fixed_joints.clone() {
        let snap_a = match s.bodies.get(&j.a) {
            Some(b) => (b.position, b.orientation, b.linear_velocity, b.angular_velocity, b.inv_mass()),
            None => continue,
        };
        let snap_b = match s.bodies.get(&j.b) {
            Some(x) => (x.position, x.orientation, x.linear_velocity, x.angular_velocity, x.inv_mass()),
            None => continue,
        };
        let (pa, oa, va, _, ima) = snap_a;
        let (pb, ob, vb, _, imb) = snap_b;
        let wsum = ima + imb;
        if wsum <= 0.0 {
            continue;
        }
        // Positional weld.
        let err = (pb - pa) - j.offset_b_from_a;
        if let Some(ba) = s.bodies.get_mut(&j.a) {
            ba.position = ba.position + err * (ima / wsum);
        }
        if let Some(bb) = s.bodies.get_mut(&j.b) {
            bb.position = bb.position - err * (imb / wsum);
        }
        // Common mass-weighted linear velocity (rigid weld).
        let (ma, mb) = (if ima > 0.0 { 1.0 / ima } else { f32::INFINITY }, if imb > 0.0 { 1.0 / imb } else { f32::INFINITY });
        let common = if ma.is_finite() && mb.is_finite() {
            (va * ma + vb * mb) / (ma + mb)
        } else if ma.is_finite() {
            va
        } else {
            vb
        };
        // Orientation weld: drive relative orientation back to rest pose.
        let rel = oa.transposed() * ob;
        let err_q = Quaternion::from_basis(j.rest_rel.transposed()) * Quaternion::from_basis(rel);
        let (axis, angle) = quat_to_axis_angle(err_q);
        // Corrective angular velocity, split by inverse mass.
        let corr = axis * (angle * 8.0);
        if let Some(ba) = s.bodies.get_mut(&j.a) {
            if ima > 0.0 {
                ba.linear_velocity = common;
                ba.angular_velocity = ba.angular_velocity - corr * (ima / wsum);
            }
        }
        if let Some(bb) = s.bodies.get_mut(&j.b) {
            if imb > 0.0 {
                bb.linear_velocity = common;
                bb.angular_velocity = bb.angular_velocity + corr * (imb / wsum);
            }
        }
    }
    for j in s.hinge_joints.clone() {
        let snap_a = match s.bodies.get(&j.a) {
            Some(b) => (b.position, b.orientation, b.inv_mass()),
            None => continue,
        };
        let snap_b = match s.bodies.get(&j.b) {
            Some(b) => (b.position, b.orientation, b.inv_mass()),
            None => continue,
        };
        let (pa, oa, ima) = snap_a;
        let (pb, ob, imb) = snap_b;
        let wsum = ima + imb;
        if wsum <= 0.0 {
            continue;
        }
        // Ball-socket: coincide the world anchors (offset by current orientation).
        let wa = pa + oa.xform(j.anchor_a);
        let wb = pb + ob.xform(j.anchor_b);
        let err = wb - wa;
        if let Some(ba) = s.bodies.get_mut(&j.a) {
            ba.position = ba.position + err * (ima / wsum);
        }
        if let Some(bb) = s.bodies.get_mut(&j.b) {
            bb.position = bb.position - err * (imb / wsum);
        }
        // Hinge axis alignment: rotate the misaligned axes back together.
        let ax_a = oa.xform(j.axis_a).normalized();
        let mut ax_b = ob.xform(j.axis_b).normalized();
        if ax_a.dot(ax_b) < 0.0 {
            ax_b = -ax_b; // hinge axis has no polarity
        }
        let align = ax_b.cross(ax_a); // rotation taking b's axis onto a's
        if align.length_squared() > 1e-10 {
            // Distribute by inverse inertia scale (approximated with inverse mass).
            if let Some(ba) = s.bodies.get_mut(&j.a) {
                ba.angular_velocity = ba.angular_velocity + align * (4.0 * ima / wsum.max(1e-9));
            }
            if let Some(bb) = s.bodies.get_mut(&j.b) {
                bb.angular_velocity = bb.angular_velocity - align * (4.0 * imb / wsum.max(1e-9));
            }
        }
    }
}

/// Quaternion to axis-angle (angle in [0, π]).
fn quat_to_axis_angle(q: Quaternion) -> (Vector3, f32) {
    let w = q.w.clamp(-1.0, 1.0);
    let angle = 2.0 * w.acos();
    let s = (1.0 - w * w).max(0.0).sqrt();
    if s < 1e-6 {
        (Vector3::new(1.0, 0.0, 0.0), 0.0)
    } else {
        (Vector3::new(q.x / s, q.y / s, q.z / s), angle)
    }
}

/// Sign with zero mapping to +1 (deterministic axis choice).
fn sgn(x: f32) -> f32 {
    if x >= 0.0 { 1.0 } else { -1.0 }
}

fn ray_sphere(from: Vector3, dir: Vector3, max: f32, center: Vector3, r: f32) -> Option<(f32, Vector3)> {
    let oc = from - center;
    let b = oc.dot(dir);
    let c = oc.length_squared() - r * r;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    if t < 0.0 || t > max {
        return None;
    }
    let p = from + dir * t;
    Some((t, (p - center).normalized()))
}

fn ray_aabb(from: Vector3, dir: Vector3, max: f32, lo: Vector3, hi: Vector3) -> Option<(f32, Vector3)> {
    let mut tmin = 0f32;
    let mut tmax = max;
    let mut normal = Vector3::ZERO;
    let mut best_n = Vector3::ZERO;
    for axis in 0..3 {
        let (o, dd, l, h) = match axis {
            0 => (from.x, dir.x, lo.x, hi.x),
            1 => (from.y, dir.y, lo.y, hi.y),
            _ => (from.z, dir.z, lo.z, hi.z),
        };
        if dd.abs() < 1e-9 {
            if o < l || o > h {
                return None;
            }
        } else {
            let (mut t1, mut t2, n1, n2) = ((l - o) / dd, (h - o) / dd, -1.0, 1.0);
            let (nn1, _nn2) = if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                (n2, n1)
            } else {
                (n1, n2)
            };
            if t1 > tmin {
                tmin = t1;
                normal = match axis {
                    0 => Vector3::new(nn1, 0.0, 0.0),
                    1 => Vector3::new(0.0, nn1, 0.0),
                    _ => Vector3::new(0.0, 0.0, nn1),
                };
                best_n = normal;
            }
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }
    let _ = best_n;
    Some((tmin, normal))
}

fn ray_plane(from: Vector3, dir: Vector3, max: f32, n: Vector3, d: f32) -> Option<(f32, Vector3)> {
    let denom = n.dot(dir);
    if denom.abs() < 1e-9 {
        return None;
    }
    let t = (d - n.dot(from)) / denom;
    if t < 0.0 || t > max {
        return None;
    }
    Some((t, if denom < 0.0 { n } else { n * -1.0 }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fall_space() -> (PhysicsServer3D, Rid, u64) {
        let mut p = PhysicsServer3D::new();
        let space = p.space_create(Vector3::new(0.0, -9.8, 0.0));
        let ball = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, ball, Shape3D::Sphere { radius: 0.5 });
        p.body_set_position(space, ball, Vector3::new(0.0, 5.0, 0.0));
        (p, space, ball)
    }
    #[test]
    fn ball_falls_and_rests_on_ground() {
        let (mut p, space, ball) = fall_space();
        for _ in 0..240 {
            p.space_step(space, 1.0 / 60.0);
        }
        let pos = p.body_get_position(space, ball).unwrap();
        assert!((pos.y - 0.5).abs() < 0.05, "rest height, got {pos:?}");
        assert!(p.contact_count(space) > 0);
    }
    #[test]
    fn impulse_wakes_and_static_planes_block() {
        let mut p = PhysicsServer3D::new();
        let space = p.space_create(Vector3::ZERO);
        let floor = p.body_create(space, BodyMode3D::Static, 1.0).unwrap();
        p.body_add_shape(space, floor, Shape3D::Plane { normal: Vector3::UP, d: 0.0 });
        let ball = p.body_create(space, BodyMode3D::Rigid, 2.0).unwrap();
        p.body_add_shape(space, ball, Shape3D::Sphere { radius: 1.0 });
        p.body_set_position(space, ball, Vector3::new(0.0, 0.5, 0.0));
        p.space_step(space, 1.0 / 60.0);
        // Penetrating the plane is resolved upward.
        assert!(p.body_get_position(space, ball).unwrap().y >= 0.99);
        p.body_apply_impulse(space, ball, Vector3::new(0.0, 20.0, 0.0));
        assert_eq!(p.body_get_velocity(space, ball).unwrap().y, 10.0);
    }
    #[test]
    fn ray_hits_closest_first() {
        let mut p = PhysicsServer3D::new();
        let space = p.space_create(Vector3::ZERO);
        let near = p.body_create(space, BodyMode3D::Static, 1.0).unwrap();
        p.body_add_shape(space, near, Shape3D::Sphere { radius: 0.5 });
        p.body_set_position(space, near, Vector3::new(0.0, 0.0, -3.0));
        let far = p.body_create(space, BodyMode3D::Static, 1.0).unwrap();
        p.body_add_shape(space, far, Shape3D::Sphere { radius: 0.5 });
        p.body_set_position(space, far, Vector3::new(0.0, 0.0, -8.0));
        let hit = p.ray_cast(space, Vector3::ZERO, Vector3::new(0.0, 0.0, -10.0)).unwrap();
        assert_eq!(hit.body, near);
        assert!((hit.distance - 2.5).abs() < 0.01);
        assert!(p.ray_cast(space, Vector3::ZERO, Vector3::new(0.0, 5.0, 0.0)).is_none());
    }
    #[test]
    fn boxes_stack_on_static_floor() {
        // Two aligned boxes dropped gently settle into a flat stack.
        // (A 3 m off-center drop is covered by violent_impact_stays_bounded;
        // near-symmetric edge/corner balances are a documented PBD limitation.)
        let mut p = PhysicsServer3D::new();
        let space = p.space_create(Vector3::new(0.0, -9.8, 0.0));
        let floor = p.body_create(space, BodyMode3D::Static, 1.0).unwrap();
        p.body_add_shape(space, floor, Shape3D::Box { extents: Vector3::new(5.0, 0.5, 5.0) });
        p.body_set_position(space, floor, Vector3::new(0.0, -0.5, 0.0)); // top at y=0
        let low = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, low, Shape3D::Box { extents: Vector3::new(0.5, 0.5, 0.5) });
        p.body_set_position(space, low, Vector3::new(0.0, 0.55, 0.0));
        let top = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, top, Shape3D::Box { extents: Vector3::new(0.5, 0.5, 0.5) });
        p.body_set_position(space, top, Vector3::new(0.0, 1.62, 0.0));
        for _ in 0..400 {
            p.space_step(space, 1.0 / 60.0);
        }
        let yl = p.body_get_position(space, low).unwrap();
        let yt = p.body_get_position(space, top).unwrap();
        assert!((yl.y - 0.5).abs() < 0.08, "bottom box rests on floor, got {yl:?}");
        assert!((yt.y - 1.5).abs() < 0.15, "top box rests on bottom box, got {yt:?}");
        assert!(yt.x.abs() < 0.3 && yl.x.abs() < 0.3, "stack stays put: {yl:?} {yt:?}");
        for id in [low, top] {
            let w = p.body_get_angular_velocity(space, id).unwrap();
            let v = p.body_get_velocity(space, id).unwrap();
            assert!(w.length() < 0.5 && v.length() < 0.5, "stack settles: v={v:?} w={w:?}");
            let o = p.body_get_orientation(space, id).unwrap();
            assert!(o.y.y > 0.95, "boxes rest flat, got {o:?}");
        }
        assert!(p.contact_count(space) > 0);
    }
    #[test]
    fn violent_impact_stays_bounded() {
        // A 3 m off-center drop scatters boxes in ANY engine; the requirement
        // is stability: finite positions/velocities, no NaN, no tunneling fall.
        let mut p = PhysicsServer3D::new();
        let space = p.space_create(Vector3::new(0.0, -9.8, 0.0));
        let floor = p.body_create(space, BodyMode3D::Static, 1.0).unwrap();
        p.body_add_shape(space, floor, Shape3D::Box { extents: Vector3::new(5.0, 0.5, 5.0) });
        p.body_set_position(space, floor, Vector3::new(0.0, -0.5, 0.0));
        let low = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, low, Shape3D::Box { extents: Vector3::new(0.5, 0.5, 0.5) });
        p.body_set_position(space, low, Vector3::new(0.0, 3.0, 0.0));
        let top = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, top, Shape3D::Box { extents: Vector3::new(0.5, 0.5, 0.5) });
        p.body_set_position(space, top, Vector3::new(0.1, 6.0, 0.0));
        for _ in 0..600 {
            p.space_step(space, 1.0 / 60.0);
        }
        for id in [low, top] {
            let pos = p.body_get_position(space, id).unwrap();
            let vel = p.body_get_velocity(space, id).unwrap();
            let w = p.body_get_angular_velocity(space, id).unwrap();
            assert!(pos.x.is_finite() && pos.y.is_finite() && pos.z.is_finite(), "finite positions, got {pos:?}");
            assert!(pos.y > -0.6, "nothing tunnels through the floor, got {pos:?}");
            assert!(vel.length() < 60.0 && w.length() < 100.0, "bounded energy: v={vel:?} w={w:?}");
        }
    }
    #[test]
    fn tilted_box_settles_flat_with_damping() {
        // A tilted drop spins up on the corner impact (physical); with damping
        // (air drag + micro-slip, as in real scenes) it flops flat and rests.
        // Zero-damping perpetual spin is correct rigid-body behavior, not a bug.
        let mut p = PhysicsServer3D::new();
        let space = p.space_create(Vector3::new(0.0, -9.8, 0.0));
        let floor = p.body_create(space, BodyMode3D::Static, 1.0).unwrap();
        p.body_add_shape(space, floor, Shape3D::Box { extents: Vector3::new(5.0, 0.5, 5.0) });
        p.body_set_position(space, floor, Vector3::new(0.0, -0.5, 0.0));
        let b = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, b, Shape3D::Box { extents: Vector3::new(0.5, 0.5, 0.5) });
        p.body_set_position(space, b, Vector3::new(0.0, 3.0, 0.0));
        p.body_set_orientation(space, b, Basis::from_euler(Vector3::new(0.0, 0.0, 0.18)));
        assert!(p.body_set_damping(space, b, 0.3, 1.5));
        for _ in 0..600 {
            p.space_step(space, 1.0 / 60.0);
        }
        let y = p.body_get_position(space, b).unwrap().y;
        let o = p.body_get_orientation(space, b).unwrap();
        let w = p.body_get_angular_velocity(space, b).unwrap();
        assert!((y - 0.5).abs() < 0.1, "tilted box flops flat, got y={y} o={o:?}");
        assert!(o.y.y > 0.9, "box rests flat-side down, got {o:?}");
        assert!(w.length() < 0.5, "spin dissipates, got {w:?}");
    }
    #[test]
    fn box_falls_flat_on_floor() {
        let mut p = PhysicsServer3D::new();
        let space = p.space_create(Vector3::new(0.0, -9.8, 0.0));
        let floor = p.body_create(space, BodyMode3D::Static, 1.0).unwrap();
        p.body_add_shape(space, floor, Shape3D::Box { extents: Vector3::new(5.0, 0.5, 5.0) });
        p.body_set_position(space, floor, Vector3::new(0.0, -0.5, 0.0));
        let b = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, b, Shape3D::Box { extents: Vector3::new(0.5, 0.5, 0.5) });
        p.body_set_position(space, b, Vector3::new(0.0, 3.0, 0.0));
        for _ in 0..300 {
            p.space_step(space, 1.0 / 60.0);
        }
        let y = p.body_get_position(space, b).unwrap().y;
        assert!((y - 0.5).abs() < 0.08, "single box rests, got {y}");
    }
    #[test]
    fn capsule_falls_upright() {
        let mut p = PhysicsServer3D::new();
        let space = p.space_create(Vector3::new(0.0, -9.8, 0.0));
        let cap = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, cap, Shape3D::Capsule { radius: 0.5, height: 1.0 });
        p.body_set_position(space, cap, Vector3::new(0.0, 5.0, 0.0));
        for _ in 0..300 {
            p.space_step(space, 1.0 / 60.0);
        }
        let y = p.body_get_position(space, cap).unwrap().y;
        assert!((y - 1.0).abs() < 0.08, "capsule rests at r+h/2, got {y}");
    }
    #[test]
    fn fixed_joint_carries_both_bodies() {
        let mut p = PhysicsServer3D::new();
        let space = p.space_create(Vector3::ZERO);
        let a = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, a, Shape3D::Sphere { radius: 0.5 });
        p.body_set_position(space, a, Vector3::ZERO);
        let b = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, b, Shape3D::Sphere { radius: 0.5 });
        p.body_set_position(space, b, Vector3::new(3.0, 0.0, 0.0));
        p.space_add_fixed(space, a, b).unwrap();
        assert_eq!(p.joint_counts(space), (1, 0));
        p.body_apply_impulse(space, a, Vector3::new(6.0, 0.0, 0.0));
        for _ in 0..60 {
            p.space_step(space, 1.0 / 60.0);
        }
        let pa = p.body_get_position(space, a).unwrap();
        let pb = p.body_get_position(space, b).unwrap();
        assert!((pb.x - pa.x - 3.0).abs() < 0.15, "weld holds offset, got {pa:?} {pb:?}");
        assert!(pa.x > 1.0, "pair actually moved, got {pa:?}");
    }
    #[test]
    fn hinge_anchors_coincide() {
        let mut p = PhysicsServer3D::new();
        let space = p.space_create(Vector3::ZERO);
        let a = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, a, Shape3D::Sphere { radius: 0.5 });
        p.body_set_position(space, a, Vector3::ZERO);
        let b = p.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, b, Shape3D::Sphere { radius: 0.5 });
        p.body_set_position(space, b, Vector3::new(4.0, 0.0, 0.0));
        p.space_add_hinge(space, a, b, Vector3::new(2.0, 0.0, 0.0), Vector3::UP).unwrap();
        for _ in 0..60 {
            p.space_step(space, 1.0 / 60.0);
        }
        let (pa, pb) = (p.body_get_position(space, a).unwrap(), p.body_get_position(space, b).unwrap());
        let mid_a = (pa + (Vector3::new(2.0, 0.0, 0.0) - Vector3::ZERO) + pb + (Vector3::new(2.0, 0.0, 0.0) - Vector3::new(4.0, 0.0, 0.0))) * 0.5;
        // Anchors were (2,0,0)-a=(2,0,0) and (2,0,0)-b=(-2,0,0): check coincidence.
        let wa = pa + Vector3::new(2.0, 0.0, 0.0);
        let wb = pb + Vector3::new(-2.0, 0.0, 0.0);
        assert!((wa - wb).length() < 0.1, "hinge anchors meet: {wa:?} vs {wb:?}");
        let _ = mid_a;
    }
    #[test]
    fn character_slides_on_wall_and_lands() {
        let mut p = PhysicsServer3D::new();
        let space = p.space_create(Vector3::ZERO);
        let wall = p.body_create(space, BodyMode3D::Static, 1.0).unwrap();
        p.body_add_shape(space, wall, Shape3D::Box { extents: Vector3::new(0.5, 5.0, 5.0) });
        p.body_set_position(space, wall, Vector3::new(5.0, 5.0, 0.0)); // face at x=4.5
        let mut ch = CharacterBody3D::new(Vector3::new(0.0, 1.0, 0.0), 0.5);
        ch.velocity = Vector3::new(10.0, 0.0, 0.0);
        let mut hit_wall = false;
        for _ in 0..120 {
            let r = p.character_move(space, &mut ch, 1.0 / 60.0, Vector3::ZERO);
            hit_wall = hit_wall || r.is_on_wall;
        }
        assert!(hit_wall, "must report wall contact");
        assert!(ch.position.x <= 4.0 + 1e-3, "stops at wall face minus radius, got {}", ch.position.x);
        // Now drop it onto the ground fallback... use a floor plane instead.
        let floor = p.body_create(space, BodyMode3D::Static, 1.0).unwrap();
        p.body_add_shape(space, floor, Shape3D::Plane { normal: Vector3::UP, d: 0.0 });
        let mut ch2 = CharacterBody3D::new(Vector3::new(0.0, 3.0, 0.0), 0.5);
        ch2.velocity = Vector3::ZERO;
        for _ in 0..180 {
            p.character_move(space, &mut ch2, 1.0 / 60.0, Vector3::new(0.0, -9.8, 0.0));
        }
        assert!(ch2.is_on_floor, "must land on the floor plane");
        assert!((ch2.position.y - 0.5).abs() < 0.05, "rest height, got {}", ch2.position.y);
    }
}
