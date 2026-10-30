//! `iris check` diagnostics — machine-readable so agents can self-repair.

use crate::ast::IrisFile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub level: String,
    pub message: String,
    pub hint: String,
}

const KNOWN_TYPES: &[&str] = &[
    "nil", "bool", "int", "float", "str", "vec2", "vec3", "vec4", "color", "rect2", "nodepath", "array",
    "dict",
];

pub fn check(file: &IrisFile) -> Vec<Diagnostic> {
    let mut out = vec![];
    if file.node_class.is_empty() {
        out.push(Diagnostic {
            level: "error".into(),
            message: "missing node name".into(),
            hint: "start file with `node Name : Base {`".into(),
        });
    }
    for p in &file.props {
        if !KNOWN_TYPES.contains(&p.ty.as_str()) {
            out.push(Diagnostic {
                level: "warning".into(),
                message: format!("unknown type `{}` on prop `{}`", p.ty, p.name),
                hint: format!("use one of {:?}", KNOWN_TYPES),
            });
        }
    }
    if file.fns.is_empty() {
        out.push(Diagnostic {
            level: "info".into(),
            message: "no functions; node will be static".into(),
            hint: "add `fn _process(dt: float) { ... }` for per-frame logic".into(),
        });
    }
    out
}
