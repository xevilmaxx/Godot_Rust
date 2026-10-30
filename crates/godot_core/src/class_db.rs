//! Full port of `core/object/class_db.h/.cpp` — the runtime class registry.
//!
//! Godot's ClassDB powers the editor Create dialog, docs, serialization and
//! scripting bindings. Same role here + JSON export so AI agents (ACP/MCP) can
//! discover the API without reading C++ headers.
//!
//! v1 completeness: register/inheritance query, singletons, property/method/
//! signal/enum metadata, `can_instantiate`, JSON dump, doc strings.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{OnceLock, RwLock};

use crate::VariantType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyDef {
    pub name: String,
    pub variant_type: VariantType,
    pub hint: String,
    pub default_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodDef {
    pub name: String,
    pub args: Vec<(String, VariantType)>,
    pub return_type: VariantType,
    pub doc: String,
    #[serde(default)]
    pub is_static: bool,
    #[serde(default)]
    pub is_const: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalDef {
    pub name: String,
    pub args: Vec<(String, VariantType)>,
    pub doc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumDef {
    pub name: String,
    pub values: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassDef {
    pub name: String,
    pub parent: Option<String>,
    pub properties: Vec<PropertyDef>,
    pub methods: Vec<MethodDef>,
    pub signals: Vec<SignalDef>,
    pub enums: Vec<EnumDef>,
    pub is_singleton: bool,
    #[serde(default = "default_true")]
    pub can_instantiate: bool,
    #[serde(default)]
    pub doc: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Default)]
pub struct ClassDb {
    classes: BTreeMap<String, ClassDef>,
    singletons: BTreeMap<String, String>,
}

impl ClassDb {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, def: ClassDef) {
        if def.is_singleton {
            self.singletons.insert(def.name.clone(), def.name.clone());
        }
        self.classes.insert(def.name.clone(), def);
    }
    pub fn get(&self, name: &str) -> Option<&ClassDef> {
        self.classes.get(name)
    }
    pub fn class_exists(&self, name: &str) -> bool {
        self.classes.contains_key(name)
    }
    pub fn class_names(&self) -> Vec<String> {
        self.classes.keys().cloned().collect()
    }
    /// `ClassDB::is_parent_class(child, parent)` — walks the parent chain.
    pub fn is_parent_class(&self, child: &str, parent: &str) -> bool {
        if child == parent {
            return true;
        }
        let mut cur = self.classes.get(child).and_then(|c| c.parent.as_deref());
        while let Some(name) = cur {
            if name == parent {
                return true;
            }
            cur = self.classes.get(name).and_then(|c| c.parent.as_deref());
        }
        false
    }
    pub fn get_parent(&self, child: &str) -> Option<&str> {
        self.classes.get(child)?.parent.as_deref()
    }
    pub fn get_inheriters(&self, parent: &str) -> Vec<String> {
        self.classes
            .keys()
            .filter(|k| *k != parent && self.is_parent_class(k, parent))
            .cloned()
            .collect()
    }
    pub fn can_instantiate(&self, name: &str) -> bool {
        self.classes.get(name).map(|c| c.can_instantiate).unwrap_or(false)
    }
    pub fn is_singleton(&self, name: &str) -> bool {
        self.classes.get(name).map(|c| c.is_singleton).unwrap_or(false)
    }
    pub fn get_property(&self, class: &str, prop: &str) -> Option<&PropertyDef> {
        // Walk up the chain like `Object::get` fallback.
        let mut cur = Some(class);
        while let Some(name) = cur {
            if let Some(def) = self.classes.get(name) {
                if let Some(p) = def.properties.iter().find(|p| p.name == prop) {
                    return Some(p);
                }
                cur = def.parent.as_deref();
            } else {
                break;
            }
        }
        None
    }
    pub fn get_method(&self, class: &str, method: &str) -> Option<&MethodDef> {
        let mut cur = Some(class);
        while let Some(name) = cur {
            if let Some(def) = self.classes.get(name) {
                if let Some(m) = def.methods.iter().find(|m| m.name == method) {
                    return Some(m);
                }
                cur = def.parent.as_deref();
            } else {
                break;
            }
        }
        None
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.classes).unwrap_or_else(|_| "{}".into())
    }
    /// Compact AI prompt: `Class(property: type, …) — method(arg) -> ret; …`.
    pub fn to_ai_summary(&self) -> String {
        let mut out = String::new();
        for (name, def) in &self.classes {
            out.push_str(&format!("{} ", name));
            if !def.properties.is_empty() {
                let props: Vec<String> =
                    def.properties.iter().map(|p| format!("{}: {}", p.name, p.variant_type.name())).collect();
                out.push_str(&format!("({}) ", props.join(", ")));
            }
            let methods: Vec<String> = def
                .methods
                .iter()
                .map(|m| {
                    let args: Vec<String> =
                        m.args.iter().map(|(n, t)| format!("{n}: {}", t.name())).collect();
                    format!("{}({}) -> {}", m.name, args.join(", "), m.return_type.name())
                })
                .collect();
            if !methods.is_empty() {
                out.push_str(&format!("— {}", methods.join("; ")));
            }
            out.push('\n');
        }
        out
    }
    pub fn len(&self) -> usize {
        self.classes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }
}

static GLOBAL: OnceLock<RwLock<ClassDb>> = OnceLock::new();

fn global() -> &'static RwLock<ClassDb> {
    GLOBAL.get_or_init(|| RwLock::new(builtin_classes()))
}

/// Global registry (mirrors `ClassDB` singleton).
pub fn global_db() -> std::sync::RwLockReadGuard<'static, ClassDb> {
    global().read().expect("classdb poisoned")
}

pub fn register_global(def: ClassDef) {
    global().write().expect("classdb poisoned").register(def);
}

/// Seed: every class ported in module 01. Engine modules extend this.
fn builtin_classes() -> ClassDb {
    let mut db = ClassDb::new();
    let object_methods = vec![
        MethodDef {
            name: "_init".into(),
            args: vec![],
            return_type: VariantType::Nil,
            doc: "Called when the object is created.".into(),
            is_static: false,
            is_const: false,
        },
        MethodDef {
            name: "get_class".into(),
            args: vec![],
            return_type: VariantType::String,
            doc: "Returns the runtime class name.".into(),
            is_static: false,
            is_const: true,
        },
        MethodDef {
            name: "is_class".into(),
            args: vec![("class".into(), VariantType::String)],
            return_type: VariantType::Bool,
            doc: "Returns true if this object is of the given class or a subclass.".into(),
            is_static: false,
            is_const: true,
        },
        MethodDef {
            name: "set".into(),
            args: vec![("property".into(), VariantType::String), ("value".into(), VariantType::Nil)],
            return_type: VariantType::Nil,
            doc: "Sets a property by name.".into(),
            is_static: false,
            is_const: false,
        },
        MethodDef {
            name: "get".into(),
            args: vec![("property".into(), VariantType::String)],
            return_type: VariantType::Nil,
            doc: "Gets a property by name.".into(),
            is_static: false,
            is_const: true,
        },
        MethodDef {
            name: "connect".into(),
            args: vec![("signal".into(), VariantType::Signal), ("callable".into(), VariantType::Callable)],
            return_type: VariantType::Int,
            doc: "Connects a signal to a callable. Returns OK.".into(),
            is_static: false,
            is_const: false,
        },
        MethodDef {
            name: "emit_signal".into(),
            args: vec![("signal".into(), VariantType::Signal)],
            return_type: VariantType::Nil,
            doc: "Emits a signal.".into(),
            is_static: false,
            is_const: false,
        },
        MethodDef {
            name: "add_to_group".into(),
            args: vec![("group".into(), VariantType::String)],
            return_type: VariantType::Nil,
            doc: "Adds this object to a group.".into(),
            is_static: false,
            is_const: false,
        },
    ];
    db.register(ClassDef {
        name: "Object".into(),
        parent: None,
        properties: vec![],
        methods: object_methods,
        signals: vec![
            SignalDef { name: "script_changed".into(), args: vec![], doc: "Emitted when the script changes.".into() },
        ],
        enums: vec![EnumDef {
            name: "ConnectFlags".into(),
            values: vec![
                ("CONNECT_DEFERRED".into(), 1),
                ("CONNECT_PERSIST".into(), 2),
                ("CONNECT_ONE_SHOT".into(), 4),
                ("CONNECT_REFERENCE_COUNTED".into(), 8),
            ],
        }],
        is_singleton: false,
        can_instantiate: true,
        doc: "Base class of all objects.".into(),
    });
    db.register(ClassDef {
        name: "RefCounted".into(),
        parent: Some("Object".into()),
        properties: vec![],
        methods: vec![
            MethodDef {
                name: "reference".into(),
                args: vec![],
                return_type: VariantType::Bool,
                doc: "Adds a reference.".into(),
                is_static: false,
                is_const: false,
            },
            MethodDef {
                name: "unreference".into(),
                args: vec![],
                return_type: VariantType::Bool,
                doc: "Removes a reference. Deletes when zero.".into(),
                is_static: false,
                is_const: false,
            },
            MethodDef {
                name: "get_reference_count".into(),
                args: vec![],
                return_type: VariantType::Int,
                doc: "Current reference count.".into(),
                is_static: false,
                is_const: true,
            },
        ],
        signals: vec![],
        enums: vec![],
        is_singleton: false,
        can_instantiate: true,
        doc: "Reference-counted object.".into(),
    });
    db.register(ClassDef {
        name: "Node".into(),
        parent: Some("Object".into()),
        properties: vec![
            PropertyDef {
                name: "name".into(),
                variant_type: VariantType::String,
                hint: "".into(),
                default_json: "\"Node\"".into(),
            },
            PropertyDef {
                name: "unique_name_in_owner".into(),
                variant_type: VariantType::Bool,
                hint: "".into(),
                default_json: "false".into(),
            },
        ],
        methods: vec![
            MethodDef {
                name: "add_child".into(),
                args: vec![("node".into(), VariantType::Object)],
                return_type: VariantType::Nil,
                doc: "Adds a child node.".into(),
                is_static: false,
                is_const: false,
            },
            MethodDef {
                name: "get_node".into(),
                args: vec![("path".into(), VariantType::NodePath)],
                return_type: VariantType::Object,
                doc: "Fetches a node by path.".into(),
                is_static: false,
                is_const: true,
            },
            MethodDef {
                name: "queue_free".into(),
                args: vec![],
                return_type: VariantType::Nil,
                doc: "Deletes the node at frame end.".into(),
                is_static: false,
                is_const: false,
            },
        ],
        signals: vec![
            SignalDef { name: "ready".into(), args: vec![], doc: "Emitted when the node is ready.".into() },
            SignalDef { name: "renamed".into(), args: vec![], doc: "Emitted when renamed.".into() },
            SignalDef {
                name: "child_entered_tree".into(),
                args: vec![("node".into(), VariantType::Object)],
                doc: "A child entered the tree.".into(),
            },
        ],
        enums: vec![],
        is_singleton: false,
        can_instantiate: true,
        doc: "Base scene-tree node.".into(),
    });
    db.register(ClassDef {
        name: "Node2D".into(),
        parent: Some("Node".into()),
        properties: vec![
            PropertyDef {
                name: "position".into(),
                variant_type: VariantType::Vector2,
                hint: "".into(),
                default_json: "[0,0]".into(),
            },
            PropertyDef {
                name: "rotation".into(),
                variant_type: VariantType::Float,
                hint: "".into(),
                default_json: "0.0".into(),
            },
            PropertyDef {
                name: "scale".into(),
                variant_type: VariantType::Vector2,
                hint: "".into(),
                default_json: "[1,1]".into(),
            },
        ],
        methods: vec![],
        signals: vec![],
        enums: vec![],
        is_singleton: false,
        can_instantiate: true,
        doc: "2D game object.".into(),
    });
    db.register(ClassDef {
        name: "Node3D".into(),
        parent: Some("Node".into()),
        properties: vec![
            PropertyDef {
                name: "position".into(),
                variant_type: VariantType::Vector3,
                hint: "".into(),
                default_json: "[0,0,0]".into(),
            },
            PropertyDef {
                name: "rotation".into(),
                variant_type: VariantType::Vector3,
                hint: "".into(),
                default_json: "[0,0,0]".into(),
            },
            PropertyDef {
                name: "scale".into(),
                variant_type: VariantType::Vector3,
                hint: "".into(),
                default_json: "[1,1,1]".into(),
            },
        ],
        methods: vec![MethodDef {
            name: "look_at".into(),
            args: vec![("target".into(), VariantType::Vector3)],
            return_type: VariantType::Nil,
            doc: "Rotates to face a target.".into(),
            is_static: false,
            is_const: false,
        }],
        signals: vec![],
        enums: vec![],
        is_singleton: false,
        can_instantiate: true,
        doc: "3D game object.".into(),
    });
    db.register(ClassDef {
        name: "Engine".into(),
        parent: None,
        properties: vec![PropertyDef {
            name: "time_scale".into(),
            variant_type: VariantType::Float,
            hint: "".into(),
            default_json: "1.0".into(),
        }],
        methods: vec![MethodDef {
            name: "get_version_info".into(),
            args: vec![],
            return_type: VariantType::Dictionary,
            doc: "Returns engine version dict.".into(),
            is_static: true,
            is_const: false,
        }],
        signals: vec![],
        enums: vec![],
        is_singleton: true,
        can_instantiate: false,
        doc: "Engine singleton.".into(),
    });
    db
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inheritance_queries_match_classdb() {
        let db = builtin_classes();
        assert!(db.is_parent_class("Node3D", "Object"));
        assert!(db.is_parent_class("Node3D", "Node3D"));
        assert!(!db.is_parent_class("Node2D", "Node3D"));
        assert_eq!(db.get_parent("Node2D"), Some("Node"));
        assert!(db.get_inheriters("Node").contains(&"Node3D".to_string()));
        assert!(db.get_property("Node3D", "position").is_some());
        assert!(db.get_property("Node3D", "name").is_some()); // inherited
        assert!(db.get_method("Node3D", "get_class").is_some()); // from Object
        assert!(db.can_instantiate("Node3D"));
        assert!(!db.can_instantiate("Engine"));
        assert!(db.is_singleton("Engine"));
    }
}
