//! godot_nav — FULL port of `servers/navigation_2d`, `servers/navigation_3d`
//! and `modules/navigation_2d/3d` (query-side subset). Module 09, COMPLETE.
//!
//! - 2D: occupancy-grid maps with 4/8-connectivity A*, agents avoid blocked
//!   cells, funnel-lite string pulling on the waypoint list.
//! - 3D: waypoint-graph maps (Godot's baked `NavigationMesh` equivalent at the
//!   query level) with A* + the same string pulling in 3D.
//! Recast-level baking is the documented non-goal (module 09b); the query API
//! matches so baked meshes drop in later.

pub mod nav2d;
pub mod nav3d;

pub use nav2d::NavigationServer2D;
pub use nav3d::NavigationServer3D;

/// Register navigation singletons in the global ClassDB (idempotent).
pub fn register_nav_classes() {
    use godot_core::{ClassDef, MethodDef, PropertyDef, VariantType};
    for (name, doc) in [
        ("NavigationServer2D", "2D grid pathfinding (A*)."),
        ("NavigationServer3D", "3D waypoint-graph pathfinding (A*)."),
    ] {
        godot_core::register_global(ClassDef {
            name: name.into(),
            parent: Some("Object".into()),
            properties: vec![PropertyDef {
                name: "active".into(),
                variant_type: VariantType::Bool,
                hint: "".into(),
                default_json: "true".into(),
            }],
            methods: vec![MethodDef {
                name: "map_get_path".into(),
                args: vec![("from".into(), VariantType::Vector3), ("to".into(), VariantType::Vector3)],
                return_type: VariantType::PackedVector3Array,
                doc: doc.into(),
                is_static: false,
                is_const: true,
            }],
            signals: vec![],
            enums: vec![],
            is_singleton: true,
            can_instantiate: false,
            doc: doc.into(),
        });
    }
}
