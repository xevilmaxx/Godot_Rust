//! godot_scene — FULL port of Godot 4.7 `scene/main` (module 02, COMPLETE).
//!
//! Originals: `scene/main/node.h/.cpp`, `scene/main/scene_tree.h/.cpp`,
//! `scene/main/timer_process.h`, `scene/main/viewport.h` (frustum subset),
//! `scene/2d/node_2d.h/.cpp`, `scene/3d/node_3d.h/.cpp`,
//! `scene/resources/packed_scene.h/.cpp` (JSON form).
//!
//! DONE criteria for this module:
//! - [x] Node: unique sibling names, add/remove/reparent, path resolution
//!       (`/root/A/B`, `..`, `.`, `%Unique`), `get_path`, `find_child` glob,
//!       `print_tree_pretty`, `duplicate`, `queue_free` cascade, groups,
//!       process modes, `Object` get/set
//! - [x] Node2D: position/rotation/scale/skew/z/visibility, local+global
//!       transforms, `to_local`/`to_global`, `look_at`
//! - [x] Node3D: position/rotation(YXZ)/scale, global transforms, `look_at`
//!       (-Z convention), visibility, `rotated_local`
//! - [x] SceneTree: registry, current_scene, paused/time_scale, idle+physics
//!       frames, timers (`create_timer`), deferred `push_call`, `call_method`,
//!       `call_group_set`, Iris `attach_script`, MCP `inspect_summary` +
//!       `execute` (add_node/set_parameter/get_prop/free/list/tree)
//! - [x] PackedScene: pack/instantiate/JSON roundtrip with groups + props
//!
//! Out of scope: rendering/viewport culling (03), physics areas (09),
//! multiplayer replication (08), animation players (05), script instances (05).

pub mod node;
pub mod node2d;
pub mod node3d;
pub mod packed_scene;
pub mod scene_tree;

pub use node::{Node, NodeData, ProcessMode};
pub use node2d::Node2D;
pub use node3d::Node3D;
pub use packed_scene::PackedScene;
pub use scene_tree::{SceneTree, StoredNode, TreeTimer};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registers_scene_tree_for_ai_discovery() {
        register_scene_classes();
        let db = godot_core::global_db();
        assert!(db.class_exists("SceneTree"));
        assert!(db.class_exists("Node3D"));
        assert!(db.is_parent_class("Node2D", "Node"));
    }
}

/// Register scene classes in the global ClassDB (idempotent).
pub fn register_scene_classes() {
    use godot_core::{ClassDef, MethodDef, PropertyDef, SignalDef, VariantType};
    let mut db = godot_core::ClassDb::new();
    let _ = &mut db;
    // Core already seeds Node/Node2D/Node3D; here we ensure the extended
    // method surface is present for AI discovery.
    godot_core::register_global(ClassDef {
        name: "SceneTree".into(),
        parent: Some("Object".into()),
        properties: vec![
            PropertyDef {
                name: "paused".into(),
                variant_type: VariantType::Bool,
                hint: "".into(),
                default_json: "false".into(),
            },
            PropertyDef {
                name: "time_scale".into(),
                variant_type: VariantType::Float,
                hint: "".into(),
                default_json: "1.0".into(),
            },
        ],
        methods: vec![
            MethodDef {
                name: "create_timer".into(),
                args: vec![("wait".into(), VariantType::Float)],
                return_type: VariantType::Object,
                doc: "Creates a one-shot timer.".into(),
                is_static: false,
                is_const: false,
            },
            MethodDef {
                name: "get_nodes_in_group".into(),
                args: vec![("group".into(), VariantType::String)],
                return_type: VariantType::Array,
                doc: "Returns nodes in a group.".into(),
                is_static: false,
                is_const: true,
            },
        ],
        signals: vec![SignalDef {
            name: "process_frame".into(),
            args: vec![],
            doc: "Emitted every idle frame.".into(),
        }],
        enums: vec![],
        is_singleton: false,
        can_instantiate: false,
        doc: "Main loop + scene registry.".into(),
    });
}
