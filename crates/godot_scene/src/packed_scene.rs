//! Port of `scene/resources/packed_scene.h` — scene save/load + instantiate.
//! Godot's text format is `.tscn`; v1 uses JSON with the same node data so
//! agents and `git diff` both stay readable. Binary (`.scn`) in module 10.

use godot_core::{GodotError, ObjectId};
use serde::{Deserialize, Serialize};

use crate::SceneTree;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackedScene {
    pub nodes: Vec<StoredNodeSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredNodeSnapshot {
    pub name: String,
    pub class: String,
    pub parent: Option<String>,
    pub properties: std::collections::BTreeMap<String, godot_core::Variant>,
    pub groups: Vec<String>,
}

impl PackedScene {
    /// Capture a subtree (`None` = whole tree under root, excluding root).
    pub fn pack(tree: &SceneTree, from: Option<ObjectId>) -> Result<Self, GodotError> {
        let start = from.unwrap_or_else(|| tree.root_id());
        let mut out = vec![];
        Self::pack_sub(tree, start, None, &mut out)?;
        Ok(Self { nodes: out })
    }

    fn pack_sub(
        tree: &SceneTree,
        id: ObjectId,
        parent_name: Option<String>,
        out: &mut Vec<StoredNodeSnapshot>,
    ) -> Result<(), GodotError> {
        let n = tree.get_stored(id)?;
        let mut props = std::collections::BTreeMap::new();
        // Persist class-relevant props (position/rotation/scale/visible/…).
        for key in ["position", "rotation", "scale", "visible", "z_index", "modulate", "top_level"] {
            if let Some(v) = n.get_prop(key) {
                props.insert(key.to_owned(), v);
            }
        }
        for (k, v) in n.data().properties.clone() {
            props.insert(k, v);
        }
        out.push(StoredNodeSnapshot {
            name: n.name().to_owned(),
            class: n.class().to_owned(),
            parent: parent_name,
            properties: props,
            groups: n.data().groups.iter().cloned().collect(),
        });
        let me = n.name().to_owned();
        for child in n.data().children.clone() {
            Self::pack_sub(tree, child, Some(me.clone()), out)?;
        }
        Ok(())
    }

    /// Instantiate under `parent` (fresh ids, groups restored).
    pub fn instantiate(&self, tree: &mut SceneTree, parent: Option<ObjectId>) -> Result<ObjectId, GodotError> {
        use crate::{Node, Node2D, Node3D};
        if self.nodes.is_empty() {
            return Err(GodotError::InvalidArgument("empty scene".into()));
        }
        // Map snapshot index -> new id; parent links by name stack.
        let mut id_by_index: Vec<ObjectId> = vec![];
        let mut first = None;
        for (i, snap) in self.nodes.iter().enumerate() {
            let pid = match (&snap.parent, i) {
                (None, 0) => parent,
                (None, _) => first,
                (Some(pname), _) => {
                    // Find latest snapshot with this name below `first`.
                    let mut found = first;
                    for (j, prev) in self.nodes[..i].iter().enumerate() {
                        if &prev.name == pname {
                            found = Some(id_by_index[j]);
                        }
                    }
                    found.or(parent)
                }
            };
            let mut id = match snap.class.as_str() {
                "Node2D" => tree.add_2d(Node2D::new(&snap.name), pid)?,
                "Node3D" => tree.add_3d(Node3D::new(&snap.name), pid)?,
                _ => tree.add_base(Node::new(&snap.name), pid)?,
            };
            for (k, v) in &snap.properties {
                let _ = tree.set_prop(id, k, v.clone());
            }
            for g in &snap.groups {
                let _ = tree.add_to_group(id, g);
            }
            if first.is_none() {
                first = Some(id);
            }
            let _ = &mut id;
            id_by_index.push(id);
        }
        first.ok_or_else(|| GodotError::InvalidArgument("empty scene".into()))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }
    pub fn from_json(s: &str) -> Result<Self, GodotError> {
        serde_json::from_str(s).map_err(|e| GodotError::Parse(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Node, Node2D};
    #[test]
    fn pack_instantiate_roundtrip() {
        let mut t = SceneTree::new();
        let a = t.add_base(Node::new("A"), None).unwrap();
        let mut b = Node2D::new("B");
        b.set_position(godot_core::Vector2::new(3.0, 4.0));
        let b_id = t.add_2d(b, Some(a)).unwrap();
        t.add_to_group(b_id, "team").unwrap();
        let packed = PackedScene::pack(&t, Some(a)).unwrap();
        let json = packed.to_json();
        let back = PackedScene::from_json(&json).unwrap();
        let mut t2 = SceneTree::new();
        let root2 = back.instantiate(&mut t2, None).unwrap();
        let kids = t2.get_children(root2).unwrap();
        assert_eq!(kids.len(), 1);
        let pos = t2.get_prop(kids[0], "position").unwrap();
        assert_eq!(pos, godot_core::Variant::Vector2(godot_core::Vector2::new(3.0, 4.0)));
        assert_eq!(t2.get_nodes_in_group("team").len(), 1);
    }
}
