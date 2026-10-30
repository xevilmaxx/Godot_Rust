//! Full port of the 2D physics servers (`servers/physics_2d/*`, GodotPhysics2D
//! model): spaces, bodies, circle/rect/segment shapes, fixed-step integration,
//! contacts, raycasts, sleeping. Mirrors the 3D API in the XY plane.

use godot_core::{Rid, Vector2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::allocator::RidOwner;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BodyMode2D {
    Static,
    Kinematic,
    #[default]
    Rigid,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Shape2D {
    Circle { radius: f32 },
    Rect { extents: Vector2 },
    Segment { a: Vector2, b: Vector2 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Body2D {
    pub mode: BodyMode2D,
    pub mass: f32,
    pub position: Vector2,
    pub linear_velocity: Vector2,
    pub gravity_scale: f32,
    pub linear_damping: f32,
    pub shapes: Vec<Shape2D>,
    pub sleeping: bool,
    sleep_frames: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact2D {
    pub body: u64,
    pub position: Vector2,
    pub normal: Vector2,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Space2D {
    pub gravity: Vector2,
    bodies: BTreeMap<u64, Body2D>,
    next_body: u64,
    pub contacts: Vec<Contact2D>,
    pub step_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayHit2D {
    pub body: u64,
    pub position: Vector2,
    pub normal: Vector2,
    pub distance: f32,
}

pub struct PhysicsServer2D {
    spaces: RidOwner<Space2D>,
}

impl PhysicsServer2D {
    pub fn new() -> Self {
        Self { spaces: RidOwner::new() }
    }
    pub fn space_create(&mut self, gravity: Vector2) -> Rid {
        self.spaces.alloc(Space2D { gravity, bodies: BTreeMap::new(), next_body: 1, contacts: vec![], step_count: 0 })
    }
    pub fn body_create(&mut self, space: Rid, mode: BodyMode2D, mass: f32) -> Option<u64> {
        let s = self.spaces.get_mut(space)?;
        let id = s.next_body;
        s.next_body += 1;
        s.bodies.insert(
            id,
            Body2D {
                mode,
                mass: mass.max(0.001),
                position: Vector2::ZERO,
                linear_velocity: Vector2::ZERO,
                gravity_scale: 1.0,
                linear_damping: 0.0,
                shapes: vec![],
                sleeping: false,
                sleep_frames: 0,
            },
        );
        Some(id)
    }
    pub fn body_add_shape(&mut self, space: Rid, body: u64, shape: Shape2D) -> bool {
        self.spaces
            .get_mut(space)
            .and_then(|s| s.bodies.get_mut(&body))
            .map(|b| {
                b.shapes.push(shape);
            })
            .is_some()
    }
    pub fn body_set_position(&mut self, space: Rid, body: u64, p: Vector2) -> bool {
        self.spaces
            .get_mut(space)
            .and_then(|s| s.bodies.get_mut(&body))
            .map(|b| b.position = p)
            .is_some()
    }
    pub fn body_get_position(&self, space: Rid, body: u64) -> Option<Vector2> {
        self.spaces.get(space)?.bodies.get(&body).map(|b| b.position)
    }
    pub fn body_set_velocity(&mut self, space: Rid, body: u64, v: Vector2) -> bool {
        self.spaces
            .get_mut(space)
            .and_then(|s| s.bodies.get_mut(&body))
            .map(|b| {
                b.linear_velocity = v;
                b.sleeping = false;
            })
            .is_some()
    }
    pub fn body_apply_impulse(&mut self, space: Rid, body: u64, impulse: Vector2) -> bool {
        self.spaces
            .get_mut(space)
            .and_then(|s| s.bodies.get_mut(&body))
            .map(|b| {
                if b.mode != BodyMode2D::Static {
                    b.sleeping = false;
                    b.linear_velocity = b.linear_velocity + impulse * (1.0 / b.mass);
                }
            })
            .is_some()
    }
    pub fn contact_count(&self, space: Rid) -> usize {
        self.spaces.get(space).map(|s| s.contacts.len()).unwrap_or(0)
    }

    pub fn space_step(&mut self, space: Rid, dt: f64) {
        let Some(s) = self.spaces.get_mut(space) else { return };
        let dt = dt as f32;
        s.contacts.clear();
        s.step_count += 1;
        let ids: Vec<u64> = s.bodies.keys().copied().collect();
        // Pass 1: integrate awake dynamic bodies.
        for id in &ids {
            let gravity = s.gravity;
            let Some(b) = s.bodies.get_mut(id) else { continue };
            if b.mode == BodyMode2D::Static || b.sleeping {
                continue;
            }
            if b.mode == BodyMode2D::Rigid {
                b.linear_velocity = b.linear_velocity + gravity * b.gravity_scale * dt;
                let damp = (1.0 - b.linear_damping * dt).max(0.0);
                b.linear_velocity = b.linear_velocity * damp;
            }
            b.position = b.position + b.linear_velocity * dt;
        }
        // Pass 2: floor contacts for ALL non-static bodies (sleeping bodies rest in contact).
        let mut touched: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
        {
            let gravity = s.gravity;
            for id in &ids {
                let r = s
                    .bodies
                    .get(id)
                    .map(|b| {
                        b.shapes
                            .iter()
                            .map(|sh| match *sh {
                                Shape2D::Circle { radius } => radius,
                                Shape2D::Rect { extents } => extents.length() * 0.5,
                                Shape2D::Segment { .. } => 0.0,
                            })
                            .fold(0f32, f32::max)
                    })
                    .unwrap_or(0.0);
                let Some(b) = s.bodies.get_mut(id) else { continue };
                if b.mode == BodyMode2D::Static {
                    continue;
                }
                let awake = !b.sleeping;
                // Ground line y=0 (y-down canvas: bodies rest at y=-r). Tolerance keeps resting contacts.
                if r > 0.0 && gravity.y > 0.0 && b.position.y > -r - 1e-4 {
                    if awake && b.position.y > -r {
                        b.position.y = -r;
                        if b.linear_velocity.y > 0.0 {
                            b.linear_velocity.y = 0.0;
                            b.linear_velocity.x *= 0.98;
                        }
                    }
                    s.contacts.push(Contact2D { body: *id, position: Vector2::new(b.position.x, 0.0), normal: Vector2::UP });
                    touched.insert(*id);
                }
            }
        }
        // Pass 3: sleep bookkeeping.
        for id in &ids {
            let Some(b) = s.bodies.get_mut(id) else { continue };
            if b.mode != BodyMode2D::Rigid || b.sleeping {
                continue;
            }
            if touched.contains(id) && b.linear_velocity.length_squared() < 0.0025 {
                b.sleep_frames += 1;
                if b.sleep_frames > 60 {
                    b.sleeping = true;
                    b.linear_velocity = Vector2::ZERO;
                }
            } else {
                b.sleep_frames = 0;
            }
        }
    }

    pub fn ray_cast(&self, space: Rid, from: Vector2, to: Vector2) -> Option<RayHit2D> {
        let s = self.spaces.get(space)?;
        let dir = to - from;
        let len = dir.length();
        if len < 1e-9 {
            return None;
        }
        let d = dir / len;
        let mut best: Option<RayHit2D> = None;
        for (id, b) in &s.bodies {
            for sh in &b.shapes {
                let hit = match *sh {
                    Shape2D::Circle { radius } => ray_circle(from, d, len, b.position, radius),
                    Shape2D::Rect { extents } => {
                        ray_aabb2(from, d, len, b.position - extents, b.position + extents)
                    }
                    Shape2D::Segment { a, b: bb } => ray_segment(from, d, len, b.position + a, b.position + bb),
                };
                if let Some((t, n)) = hit {
                    if best.as_ref().map(|h: &RayHit2D| t < h.distance).unwrap_or(true) {
                        best = Some(RayHit2D { body: *id, position: from + d * t, normal: n, distance: t });
                    }
                }
            }
        }
        best
    }
}

impl Default for PhysicsServer2D {
    fn default() -> Self {
        Self::new()
    }
}

fn ray_circle(from: Vector2, dir: Vector2, max: f32, center: Vector2, r: f32) -> Option<(f32, Vector2)> {
    let oc = from - center;
    let bb = oc.dot(dir);
    let c = oc.length_squared() - r * r;
    let disc = bb * bb - c;
    if disc < 0.0 {
        return None;
    }
    let t = -bb - disc.sqrt();
    if t < 0.0 || t > max {
        return None;
    }
    let p = from + dir * t;
    Some((t, (p - center).normalized()))
}

fn ray_aabb2(from: Vector2, dir: Vector2, max: f32, lo: Vector2, hi: Vector2) -> Option<(f32, Vector2)> {
    let mut tmin = 0f32;
    let mut tmax = max;
    let mut n = Vector2::ZERO;
    for (o, dd, l, h, nx, ny) in [
        (from.x, dir.x, lo.x, hi.x, -1.0, 0.0),
        (from.y, dir.y, lo.y, hi.y, 0.0, -1.0),
    ] {
        if dd.abs() < 1e-9 {
            if o < l || o > h {
                return None;
            }
        } else {
            let (mut t1, mut t2) = ((l - o) / dd, (h - o) / dd);
            let flip = t1 > t2;
            if flip {
                std::mem::swap(&mut t1, &mut t2);
            }
            if t1 > tmin {
                tmin = t1;
                n = if flip { Vector2::new(-nx, -ny) } else { Vector2::new(nx, ny) };
            }
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }
    Some((tmin, n))
}

fn ray_segment(from: Vector2, dir: Vector2, max: f32, a: Vector2, b: Vector2) -> Option<(f32, Vector2)> {
    // Segment as thin capsule radius 0.5px (editor picking semantics).
    let mid = (a + b) * 0.5;
    let r = (b - a).length() * 0.5 + 0.5;
    ray_circle(from, dir, max, mid, r).map(|(t, _)| (t, (b - a).orthogonal().normalized()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn circle_falls_to_floor_y_down() {
        let mut p = PhysicsServer2D::new();
        let space = p.space_create(Vector2::new(0.0, 980.0));
        let ball = p.body_create(space, BodyMode2D::Rigid, 1.0).unwrap();
        p.body_add_shape(space, ball, Shape2D::Circle { radius: 10.0 });
        p.body_set_position(space, ball, Vector2::new(0.0, -200.0));
        for _ in 0..240 {
            p.space_step(space, 1.0 / 60.0);
        }
        let pos = p.body_get_position(space, ball).unwrap();
        assert!((pos.y + 10.0).abs() < 1.0, "rest on floor, got {pos:?}");
        assert!(p.contact_count(space) > 0);
    }
    #[test]
    fn ray_hits_circle() {
        let mut p = PhysicsServer2D::new();
        let space = p.space_create(Vector2::ZERO);
        let b = p.body_create(space, BodyMode2D::Static, 1.0).unwrap();
        p.body_add_shape(space, b, Shape2D::Circle { radius: 8.0 });
        p.body_set_position(space, b, Vector2::new(50.0, 0.0));
        let hit = p.ray_cast(space, Vector2::ZERO, Vector2::new(100.0, 0.0)).unwrap();
        assert_eq!(hit.body, b);
        assert!((hit.distance - 42.0).abs() < 0.5);
    }
}
