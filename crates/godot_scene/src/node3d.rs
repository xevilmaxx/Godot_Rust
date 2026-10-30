//! Port of `scene/3d/node_3d.h/.cpp` — 3D transforms + visibility.
//! `transform = T(position) * R(rotation euler YXZ) * S(scale)` — matches
//! `Node3D::get_transform`. Global transform walks ancestry unless `top_level`.

use godot_core::{Basis, Object, ObjectId, Quaternion, Transform3D, Variant, Vector3};
use serde::{Deserialize, Serialize};

use crate::Node;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node3D {
    pub base: Node,
    pub position: Vector3,
    /// Euler radians, YXZ order (Godot default `Node3D.rotation`).
    pub rotation: Vector3,
    pub scale: Vector3,
    pub top_level: bool,
    pub visible: bool,
}

impl Node3D {
    pub fn new(name: impl Into<String>) -> Self {
        let mut base = Node::new(name);
        base.data.class = "Node3D".into();
        Self {
            base,
            position: Vector3::ZERO,
            rotation: Vector3::ZERO,
            scale: Vector3::ONE,
            top_level: false,
            visible: true,
        }
    }
    pub fn position(&self) -> Vector3 {
        self.position
    }
    pub fn set_position(&mut self, p: Vector3) {
        self.position = p;
    }
    pub fn basis(&self) -> Basis {
        Basis::from_euler(self.rotation).scaled(self.scale)
    }
    pub fn transform(&self) -> Transform3D {
        Transform3D::new(self.basis(), self.position)
    }
    pub fn global_transform(&self, ancestors: &[Transform3D]) -> Transform3D {
        let mut g = self.transform();
        if !self.top_level {
            for t in ancestors.iter().rev() {
                g = *t * g;
            }
        }
        g
    }
    pub fn global_position(&self, ancestors: &[Transform3D]) -> Vector3 {
        self.global_transform(ancestors).origin
    }
    pub fn to_local(&self, global: Vector3, ancestors: &[Transform3D]) -> Vector3 {
        self.global_transform(ancestors).affine_inverse().xform(global)
    }
    pub fn to_global(&self, local: Vector3, ancestors: &[Transform3D]) -> Vector3 {
        self.global_transform(ancestors).xform(local)
    }
    /// `Node3D::look_at(target)` — -Z faces the target (Godot convention).
    pub fn look_at(&mut self, target_global: Vector3, ancestors: &[Transform3D], use_model_front: bool) {
        let here = self.global_position(ancestors);
        let dir = target_global - here;
        if dir.length_squared() < 1e-8 {
            return;
        }
        let up = Vector3::UP;
        // Build a looking-at basis: z = -dir.normalized (or +dir for model front).
        let z = if use_model_front { dir.normalized() } else { (here - target_global).normalized() };
        let x = up.cross(z).normalized();
        if x.length_squared() < 1e-8 {
            return; // looking straight up/down — keep rotation.
        }
        let y = z.cross(x).normalized();
        let basis = Basis { x, y, z };
        // Strip parent rotation + scale to get local euler.
        let parent_inv = {
            let mut g = Transform3D::IDENTITY;
            if !self.top_level {
                for t in ancestors.iter().rev() {
                    g = *t * g;
                }
            }
            g.basis.inverse()
        };
        let local = parent_inv * basis;
        // Remove our own scale before extracting euler.
        let no_scale = Basis {
            x: local.x / self.scale.x.max(1e-8),
            y: local.y / self.scale.y.max(1e-8),
            z: local.z / self.scale.z.max(1e-8),
        };
        self.rotation = no_scale.to_euler();
        let _ = basis;
    }
    pub fn show(&mut self) {
        self.visible = true;
    }
    pub fn hide(&mut self) {
        self.visible = false;
    }
    pub fn is_visible_in_tree(&self, ancestors_visible: bool) -> bool {
        self.visible && ancestors_visible
    }
    pub fn rotated_local(&mut self, axis: Vector3, angle: f32) {
        let q = Quaternion::from_axis_angle(axis.normalized(), angle);
        let cur = Quaternion::from_euler(self.rotation);
        self.rotation = (q * cur).to_euler();
    }
}

impl Object for Node3D {
    fn object_id(&self) -> ObjectId {
        self.base.data.id
    }
    fn class_name(&self) -> &'static str {
        "Node3D"
    }
    fn get(&self, property: &str) -> Option<Variant> {
        match property {
            "position" => Some(Variant::Vector3(self.position)),
            "rotation" => Some(Variant::Vector3(self.rotation)),
            "scale" => Some(Variant::Vector3(self.scale)),
            "visible" => Some(Variant::Bool(self.visible)),
            "top_level" => Some(Variant::Bool(self.top_level)),
            "name" => self.base.get(property),
            _ => self.base.data.properties.get(property).cloned(),
        }
    }
    fn set(&mut self, property: &str, value: Variant) -> bool {
        match property {
            "position" => {
                if let Variant::Vector3(v) = value {
                    self.position = v;
                    return true;
                }
                false
            }
            "rotation" => {
                if let Variant::Vector3(v) = value {
                    self.rotation = v;
                    return true;
                }
                false
            }
            "scale" => {
                if let Variant::Vector3(v) = value {
                    self.scale = v;
                    return true;
                }
                false
            }
            "visible" => {
                if let Variant::Bool(b) = value {
                    self.visible = b;
                    return true;
                }
                false
            }
            "name" => self.base.set(property, value),
            _ => {
                self.base.data.properties.insert(property.to_owned(), value);
                true
            }
        }
    }
    fn method_names(&self) -> Vec<String> {
        let mut m = self.base.method_names();
        m.extend(["to_local".into(), "to_global".into(), "look_at".into(), "show".into(), "hide".into()]);
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn node3d_look_at_faces_target() {
        let mut n = Node3D::new("N");
        n.look_at(Vector3::new(0.0, 0.0, -5.0), &[], false);
        // Facing -Z from origin with identity rotation stays ~identity.
        assert!(n.rotation.length() < 1e-4);
        n.look_at(Vector3::new(5.0, 0.0, 0.0), &[], false);
        // -Z toward +X means yaw of -90°.
        assert!((n.rotation.y + std::f32::consts::FRAC_PI_2).abs() < 1e-3);
    }
    #[test]
    fn node3d_global_roundtrip() {
        let mut c = Node3D::new("C");
        c.set_position(Vector3::new(1.0, 2.0, 3.0));
        let parent = Transform3D::from_translation(Vector3::new(10.0, 0.0, 0.0));
        let g = c.global_position(&[parent]);
        assert!((g.x - 11.0).abs() < 1e-4);
        let back = c.to_local(c.to_global(Vector3::new(1.0, 1.0, 1.0), &[parent]), &[parent]);
        assert!((back.x - 1.0).abs() < 1e-4);
    }
}
