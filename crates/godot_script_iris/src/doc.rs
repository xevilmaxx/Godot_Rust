//! `iris doc` — machine-readable API surface for agents (mirrors the ClassDB
//! JSON export). Emits the node schema: class, props with types+defaults,
//! functions with args, and the Iris grammar reference.

use crate::ast::IrisFile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrisDoc {
    pub node_class: String,
    pub base_class: String,
    pub props: Vec<IrisPropDoc>,
    pub fns: Vec<IrisFnDoc>,
    pub grammar: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrisPropDoc {
    pub name: String,
    pub ty: String,
    pub default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrisFnDoc {
    pub name: String,
    pub args: Vec<(String, String)>,
    pub lifecycle: bool,
}

pub fn doc_json(file: &IrisFile) -> IrisDoc {
    IrisDoc {
        node_class: file.node_class.clone(),
        base_class: file.base_class.clone(),
        props: file
            .props
            .iter()
            .map(|p| IrisPropDoc { name: p.name.clone(), ty: p.ty.clone(), default: p.default.clone() })
            .collect(),
        fns: file
            .fns
            .iter()
            .map(|f| IrisFnDoc {
                name: f.name.clone(),
                args: f.args.clone(),
                lifecycle: matches!(f.name.as_str(), "_ready" | "_process" | "_physics_process" | "_init"),
            })
            .collect(),
        grammar: "node N : Base { prop x: float = 0.0  fn _process(dt: float) { let y = x + 1  print y } } // stmts: let/assign/print/emit/if/while/for/return/expr; exprs: +-*/% ** == != < <= > >= and or not, member (a.b), index (a[0]), calls (len/str/int/float/abs/min/max/clamp/lerp/vec2/vec3/color/range)",
    }
}
