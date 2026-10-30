//! Iris AST. Minimal by design: 9 statement forms, typed props, fns.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrisFile {
    pub node_class: String,
    pub base_class: String,
    pub props: Vec<Prop>,
    pub fns: Vec<Func>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prop {
    pub name: String,
    pub ty: String,
    pub default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Func {
    pub name: String,
    pub args: Vec<(String, String)>,
    pub body: Vec<IrisStmt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IrisStmt {
    Let { name: String, ty: Option<String>, expr: String },
    Assign { target: String, op: String, expr: String },
    Expr(String),
    If { cond: String, then_b: Vec<IrisStmt>, else_b: Vec<IrisStmt> },
    While { cond: String, body: Vec<IrisStmt> },
    For { var: String, iter: String, body: Vec<IrisStmt> },
    Return(Option<String>),
    Print(String),
    Emit(String),
}
