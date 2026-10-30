//! godot_script_iris — **Iris**: AI-native scripting language replacing GDScript/C#.
//!
//! Why a new language instead of porting `modules/gdscript/` + `modules/mono/`?
//! GDScript's dynamic resolver and C#'s MSBuild/SDK dependency are both hostile
//! to in-editor AI agents (non-deterministic completions, huge toolchains).
//! Iris is specified so an LLM can generate it reliably:
//! - expression-oriented, indentation-insensitive, semicolons optional
//! - static types = Godot Variant types (`int`, `float`, `str`, `vec3`, …)
//! - only 9 statements; full grammar fits on one page (see docs/IRIS_SPEC.md)
//! - every program exposes JSON schema for its props/methods (agent-discoverable)
//! - deterministic interpreter; `iris check` outputs machine-readable diagnostics
//!
//! Example:
//! ```iris
//! node MySprite : Node2D {
//!   prop speed: float = 200.0
//!   fn _process(dt: float) {
//!     position.x += speed * dt
//!   }
//! }
//! ```

pub mod ast;
pub mod check;
pub mod doc;
pub mod expr;
pub mod gdtranspile;
pub mod interp;
pub mod parse;

pub use ast::{IrisFile, IrisStmt};
pub use check::check;
pub use doc::{doc_json, IrisDoc};
pub use expr::{eval as eval_expr, Value};
pub use interp::{variant_to_value, value_to_variant, Interpreter};
pub use parse::parse;
