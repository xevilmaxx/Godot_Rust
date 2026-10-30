//! Scene ↔ servers sync helpers — the glue `scene/main/*` uses to push node
//! transforms into physics bodies / render instances and pull results back.
//! Godot does this in `SceneTree` flush steps; here they are explicit so tests
//! and agents observe each direction.

use godot_core::{Rid, Transform3D};

use crate::{PhysicsServer3D, RenderingServer};

/// Push a `Node3D` local transform into a render instance.
pub fn sync_instance_to_node_transform(
    rendering: &mut RenderingServer,
    instance: Rid,
    transform: Transform3D,
) -> bool {
    rendering.instance_set_transform(instance, transform)
}

/// Pull a physics body position into a `Node3D`-style position (caller owns the node).
pub fn body_position_to_node(
    physics: &PhysicsServer3D,
    space: Rid,
    body: u64,
) -> Option<godot_core::Vector3> {
    physics.body_get_position(space, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics3d::{BodyMode3D, Shape3D};
    use godot_core::Vector3;

    #[test]
    fn physics_drives_render_instance() {
        let mut physics = PhysicsServer3D::new();
        let space = physics.space_create(Vector3::new(0.0, -9.8, 0.0));
        let ball = physics.body_create(space, BodyMode3D::Rigid, 1.0).unwrap();
        physics.body_add_shape(space, ball, Shape3D::Sphere { radius: 0.5 });
        physics.body_set_position(space, ball, Vector3::new(0.0, 5.0, 0.0));

        let mut rendering = RenderingServer::default();
        let mesh = rendering.mesh_create_box(Vector3::new(0.5, 0.5, 0.5));
        let mat = rendering.material_create(godot_core::Color::RED);
        let inst = rendering.instance_create(Some(mesh), Some(mat));

        for _ in 0..120 {
            physics.space_step(space, 1.0 / 60.0);
        }
        let pos = body_position_to_node(&physics, space, ball).unwrap();
        assert!((pos.y - 0.5).abs() < 0.1);
        assert!(sync_instance_to_node_transform(&mut rendering, inst, Transform3D::from_translation(pos)));
    }
}
