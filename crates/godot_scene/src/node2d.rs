//! Port of `scene/2d/node_2d.h` + `scene/gui` visibility subset (`CanvasItem`).
//! Transform convention: `transform = T(position) * R(rotation) * S(scale)`,
//! skew applied as shear — matches `Node2D::get_transform`. Global transform
//! walks `/root` ancestry (relative parents only; canvas-layer/top-level in 03).

use godot_core::{Object, ObjectId, Transform2D, Variant, Vector2};
use serde::{Deserialize, Serialize};

use crate::Node;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node2D {
    pub base: Node,
    pub position: Vector2,
    pub rotation: f32,
    pub scale: Vector2,
    pub skew: f32,
    pub z_index: i32,
    pub show_behind_parent: bool,
    pub top_level: bool,
    pub visible: bool,
    pub modulate: godot_core::Color,
}

impl Node2D {
    pub fn new(name: impl Into<String>) -> Self {
        let mut base = Node::new(name);
        base.data.class = "Node2D".into();
        Self {
            base,
            position: Vector2::ZERO,
            rotation: 0.0,
            scale: Vector2::ONE,
            skew: 0.0,
            z_index: 0,
            show_behind_parent: false,
            top_level: false,
            visible: true,
            modulate: godot_core::Color::WHITE,
        }
    }
    pub fn position(&self) -> Vector2 {
        self.position
    }
    pub fn set_position(&mut self, p: Vector2) {
        self.position = p;
    }
    /// Local transform (matches `Node2D::get_transform` without skew shear detail
    /// loss — skew is applied as an x-shear like Godot).
    pub fn transform(&self) -> Transform2D {
        let (sr, cr) = self.rotation.sin_cos();
        let (ss, cs) = (self.skew.tan(), 1.0f32);
        let x = Vector2::new(cr * self.scale.x, sr * self.scale.x);
        let y = Vector2::new((-sr + ss * cr) * self.scale.y, (cr + ss * sr) * self.scale.y * cs / cs);
        Transform2D { x, y, origin: self.position }
    }
    pub fn global_transform(&self, ancestors: &[Transform2D]) -> Transform2D {
        // ancestors[0] = root … ancestors[last] = parent.
        let mut g = self.transform();
        if !self.top_level {
            for t in ancestors.iter().rev() {
                g = *t * g;
            }
        }
        g
    }
    pub fn global_position(&self, ancestors: &[Transform2D]) -> Vector2 {
        self.global_transform(ancestors).origin
    }
    /// `to_local(global)` / `to_global(local)` — Godot `CanvasItem` API.
    pub fn to_local(&self, global: Vector2, ancestors: &[Transform2D]) -> Vector2 {
        self.global_transform(ancestors).affine_inverse().xform(global)
    }
    pub fn to_global(&self, local: Vector2, ancestors: &[Transform2D]) -> Vector2 {
        self.global_transform(ancestors).xform(local)
    }
    /// Rotate to face a point in global space (2D `look_at`).
    pub fn look_at(&mut self, target_global: Vector2, ancestors: &[Transform2D]) {
        let here = self.global_position(ancestors);
        self.rotation = (target_global - here).angle();
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
}

impl Object for Node2D {
    fn object_id(&self) -> ObjectId {
        self.base.data.id
    }
    fn class_name(&self) -> &'static str {
        "Node2D"
    }
    fn get(&self, property: &str) -> Option<Variant> {
        match property {
            "position" => Some(Variant::Vector2(self.position)),
            "rotation" => Some(Variant::Float(self.rotation as f64)),
            "scale" => Some(Variant::Vector2(self.scale)),
            "skew" => Some(Variant::Float(self.skew as f64)),
            "z_index" => Some(Variant::Int(self.z_index as i64)),
            "visible" => Some(Variant::Bool(self.visible)),
            "modulate" => Some(Variant::Color(self.modulate)),
            "name" => self.base.get(property),
            _ => self.base.data.properties.get(property).cloned(),
        }
    }
    fn set(&mut self, property: &str, value: Variant) -> bool {
        match property {
            "position" => {
                if let Variant::Vector2(v) = value {
                    self.position = v;
                    return true;
                }
                false
            }
            "rotation" => {
                if let Variant::Float(f) = value {
                    self.rotation = f as f32;
                    return true;
                }
                if let Variant::Int(i) = value {
                    self.rotation = i as f32;
                    return true;
                }
                false
            }
            "scale" => {
                if let Variant::Vector2(v) = value {
                    self.scale = v;
                    return true;
                }
                false
            }
            "z_index" => {
                if let Variant::Int(i) = value {
                    self.z_index = i as i32;
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
    fn node2d_global_matches_godot() {
        let mut child = Node2D::new("Child");
        child.set_position(Vector2::new(10.0, 0.0));
        let parent_t = Transform2D::from_rotation_position(std::f32::consts::FRAC_PI_2, Vector2::ZERO);
        // Child at (10,0) under a 90°-rotated parent lands at (0,10).
        let g = child.global_position(&[parent_t]);
        assert!((g.x - 0.0).abs() < 1e-4 && (g.y - 10.0).abs() < 1e-4);
        // Roundtrip through to_local/to_global.
        let back = child.to_local(child.to_global(Vector2::new(1.0, 2.0), &[parent_t]), &[parent_t]);
        assert!((back.x - 1.0).abs() < 1e-4 && (back.y - 2.0).abs() < 1e-4);
    }
}
