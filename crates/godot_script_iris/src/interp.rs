//! Tree-walking Iris interpreter v1 — full expressions, member assignment,
//! bounded loops, builtins, `Variant` bridge for scene sync.

use crate::ast::IrisStmt;
use std::collections::BTreeMap;

pub use crate::expr::{eval, Value};

pub struct Interpreter {
    pub vars: BTreeMap<String, Value>,
    pub output: Vec<String>,
    /// Debugger trace: `enter <fn>` / `exit <fn>` markers in run order.
    pub trace: Vec<String>,
    returned: Option<Value>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self { vars: BTreeMap::new(), output: vec![], trace: vec![], returned: None }
    }
    pub fn return_value(&self) -> Option<&Value> {
        self.returned.as_ref()
    }
    /// Run a function body with debugger trace markers (used by the Debugger tab).
    pub fn run_traced(&mut self, name: &str, stmts: &[IrisStmt]) {
        self.trace.push(format!("enter {name}"));
        self.run(stmts);
        self.trace.push(format!("exit {name}"));
    }

    fn eval_or_nil(&self, expr: &str) -> Value {
        match eval(expr, &self.vars) {
            Ok(v) => v,
            Err(e) => {
                // Unknown variables read as their name (v0 compat for bare words
                // like `print title` where title is a prop preset at runtime).
                if e.starts_with("unknown variable") {
                    Value::Str(expr.trim().trim_matches('"').to_owned())
                } else {
                    Value::Str(format!("error: {e}"))
                }
            }
        }
    }

    /// Assign to `target` (`name`, `a.b`, `a[0]`) with `op` (`=`, `+=`, …).
    fn assign(&mut self, target: &str, op: &str, value: Value) {
        if let Some((base, attr)) = split_member(target) {
            let cur = self.vars.get(base).cloned().unwrap_or(Value::Nil);
            let cur_member = member_get(&cur, attr).unwrap_or(Value::Nil);
            let base_op = if op == "=" { "=" } else { op.trim_end_matches('=') };
            let next = apply_op(base_op, &cur_member, &value);
            let updated = member_set(&cur, attr, next);
            self.vars.insert(base.to_owned(), updated);
            return;
        }
        if let Some((base, idx_src)) = split_index(target) {
            let idx = self.eval_or_nil(idx_src);
            let cur = self.vars.get(base).cloned().unwrap_or(Value::Arr(vec![]));
            if let Value::Arr(mut items) = cur {
                if let Value::Int(i) = idx {
                    let n = items.len().max(1) as i64;
                    let k = i.rem_euclid(n) as usize;
                    if k < items.len() {
                        items[k] = apply_op(op, &items[k].clone(), &value);
                        self.vars.insert(base.to_owned(), Value::Arr(items));
                    }
                }
            }
            return;
        }
        let next = if op == "=" {
            value
        } else {
            let cur = self.vars.get(target).cloned().unwrap_or(Value::Nil);
            let base_op = op.trim_end_matches('=');
            apply_op(base_op, &cur, &value)
        };
        self.vars.insert(target.to_owned(), next);
    }

    pub fn run(&mut self, stmts: &[IrisStmt]) {
        for s in stmts {
            if self.returned.is_some() {
                break;
            }
            match s {
                IrisStmt::Let { name, expr, .. } => {
                    let v = self.eval_or_nil(expr);
                    self.vars.insert(name.clone(), v);
                }
                IrisStmt::Assign { target, op, expr } => {
                    let v = self.eval_or_nil(expr);
                    self.assign(target, op, v);
                }
                IrisStmt::Print(expr) => {
                    let v = self.eval_or_nil(expr);
                    self.output.push(v.to_display());
                }
                IrisStmt::Expr(e) => {
                    let _ = eval(e, &self.vars);
                }
                IrisStmt::If { cond, then_b, else_b } => {
                    if self.eval_or_nil(cond).truthy() {
                        self.run(then_b);
                    } else {
                        self.run(else_b);
                    }
                }
                IrisStmt::While { cond, body } => {
                    let mut guard = 0;
                    while self.eval_or_nil(cond).truthy() && guard < 10_000 {
                        self.run(body);
                        guard += 1;
                        if self.returned.is_some() {
                            break;
                        }
                    }
                }
                IrisStmt::For { var, iter, body } => {
                    let vals = match self.eval_or_nil(iter) {
                        Value::Arr(items) => items,
                        Value::Int(n) => (0..n.max(0).min(100_000)).map(Value::Int).collect(),
                        _ => vec![],
                    };
                    for v in vals.into_iter().take(100_000) {
                        self.vars.insert(var.clone(), v);
                        self.run(body);
                        if self.returned.is_some() {
                            break;
                        }
                    }
                }
                IrisStmt::Return(expr) => {
                    self.returned = Some(expr.as_ref().map(|e| self.eval_or_nil(e)).unwrap_or(Value::Nil));
                }
                IrisStmt::Emit(payload) => {
                    self.output.push(format!("emit {payload}"));
                }
            }
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

fn split_member(target: &str) -> Option<(&str, &str)> {
    // Last `.attr` where attr is a valid identifier (not a float like `1.5`).
    let dot = target.rfind('.')?;
    let (base, attr) = (&target[..dot], &target[dot + 1..]);
    if base.is_empty() || attr.is_empty() {
        return None;
    }
    if !attr.chars().all(|c| c.is_alphanumeric() || c == '_') || attr.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        return None;
    }
    Some((base.trim(), attr.trim()))
}

fn split_index(target: &str) -> Option<(&str, &str)> {
    if !target.ends_with(']') {
        return None;
    }
    let open = target.rfind('[')?;
    Some((target[..open].trim(), target[open + 1..target.len() - 1].trim()))
}

fn member_get(v: &Value, attr: &str) -> Option<Value> {
    match v {
        Value::Vec2(x, y) => match attr {
            "x" => Some(Value::Float(*x)),
            "y" => Some(Value::Float(*y)),
            _ => None,
        },
        Value::Vec3(x, y, z) => match attr {
            "x" => Some(Value::Float(*x)),
            "y" => Some(Value::Float(*y)),
            "z" => Some(Value::Float(*z)),
            _ => None,
        },
        Value::ColorV(r, g, b, a) => match attr {
            "r" => Some(Value::Float(*r)),
            "g" => Some(Value::Float(*g)),
            "b" => Some(Value::Float(*b)),
            "a" => Some(Value::Float(*a)),
            _ => None,
        },
        _ => None,
    }
}

fn member_set(v: &Value, attr: &str, next: Value) -> Value {
    let f = |x: &Value| match x {
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
        _ => 0.0,
    };
    match v {
        Value::Vec2(x, y) => {
            let (mut a, mut b) = (*x, *y);
            match attr {
                "x" => a = f(&next),
                "y" => b = f(&next),
                _ => {}
            }
            Value::Vec2(a, b)
        }
        Value::Vec3(x, y, z) => {
            let (mut a, mut b, mut c) = (*x, *y, *z);
            match attr {
                "x" => a = f(&next),
                "y" => b = f(&next),
                "z" => c = f(&next),
                _ => {}
            }
            Value::Vec3(a, b, c)
        }
        Value::ColorV(r, g, b, a) => {
            let (mut rr, mut gg, mut bb, mut aa) = (*r, *g, *b, *a);
            match attr {
                "r" => rr = f(&next),
                "g" => gg = f(&next),
                "b" => bb = f(&next),
                "a" => aa = f(&next),
                _ => {}
            }
            Value::ColorV(rr, gg, bb, aa)
        }
        _ => v.clone(),
    }
}

fn apply_op(op: &str, a: &Value, b: &Value) -> Value {
    if op == "=" {
        return b.clone();
    }
    let num = |x: &Value| match x {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        Value::Bool(v) => Some(*v as i32 as f64),
        _ => None,
    };
    match (a, b) {
        (Value::Str(s), o) if op == "+" => Value::Str(format!("{s}{}", o.to_display())),
        (Value::Arr(x), Value::Arr(y)) if op == "+" => {
            Value::Arr(x.iter().chain(y.iter()).cloned().collect())
        }
        (Value::Vec2(x1, y1), Value::Vec2(x2, y2)) => match op {
            "+" => Value::Vec2(x1 + x2, y1 + y2),
            "-" => Value::Vec2(x1 - x2, y1 - y2),
            "*" => Value::Vec2(x1 * x2, y1 * y2),
            _ => Value::Nil,
        },
        (Value::Vec3(x1, y1, z1), Value::Vec3(x2, y2, z2)) => match op {
            "+" => Value::Vec3(x1 + x2, y1 + y2, z1 + z2),
            "-" => Value::Vec3(x1 - x2, y1 - y2, z1 - z2),
            "*" => Value::Vec3(x1 * x2, y1 * y2, z1 * z2),
            _ => Value::Nil,
        },
        _ => {
            // Scalar/vector promotion: vec <op> scalar applies per component.
            match (a, op, b) {
                (Value::Vec2(x1, y1), "+", _) | (Value::Vec2(x1, y1), "-", _)
                | (Value::Vec2(x1, y1), "*", _) | (Value::Vec2(x1, y1), "/", _) => {
                    let s = num(b).unwrap_or(0.0);
                    match op {
                        "+" => Value::Vec2(x1 + s, y1 + s),
                        "-" => Value::Vec2(x1 - s, y1 - s),
                        "*" => Value::Vec2(x1 * s, y1 * s),
                        "/" => Value::Vec2(x1 / s, y1 / s),
                        _ => Value::Nil,
                    }
                }
                (Value::Vec3(x1, y1, z1), "+", _)
                | (Value::Vec3(x1, y1, z1), "-", _)
                | (Value::Vec3(x1, y1, z1), "*", _)
                | (Value::Vec3(x1, y1, z1), "/", _) => {
                    let s = num(b).unwrap_or(0.0);
                    match op {
                        "+" => Value::Vec3(x1 + s, y1 + s, z1 + s),
                        "-" => Value::Vec3(x1 - s, y1 - s, z1 - s),
                        "*" => Value::Vec3(x1 * s, y1 * s, z1 * s),
                        "/" => Value::Vec3(x1 / s, y1 / s, z1 / s),
                        _ => Value::Nil,
                    }
                }
                _ => match (num(a), num(b)) {
                    (Some(x), Some(y)) => match op {
                        "+" => num_result(a, b, x + y),
                        "-" => num_result(a, b, x - y),
                        "*" => num_result(a, b, x * y),
                        "/" => {
                            if matches!((a, b), (Value::Int(_), Value::Int(_))) {
                                if y == 0.0 {
                                    Value::Nil
                                } else {
                                    Value::Int((x / y) as i64)
                                }
                            } else {
                                Value::Float(x / y)
                            }
                        }
                        "%" => {
                            if matches!((a, b), (Value::Int(_), Value::Int(_))) {
                                if y == 0.0 {
                                    Value::Nil
                                } else {
                                    Value::Int((x as i64) % (y as i64))
                                }
                            } else {
                                Value::Float(x % y)
                            }
                        }
                        "**" => Value::Float(x.powf(y)),
                        _ => Value::Nil,
                    },
                    _ => Value::Nil,
                },
            }
        }
    }
}

fn num_result(a: &Value, b: &Value, f: f64) -> Value {
    match (a, b) {
        (Value::Int(_), Value::Int(_)) => Value::Int(f as i64),
        _ => Value::Float(f),
    }
}

// ------------------------------------------------------- Variant bridge ----
pub fn value_to_variant(v: &Value) -> godot_core::Variant {
    use godot_core::Variant;
    match v {
        Value::Nil => Variant::Nil,
        Value::Bool(b) => Variant::Bool(*b),
        Value::Int(i) => Variant::Int(*i),
        Value::Float(f) => Variant::Float(*f),
        Value::Str(s) => Variant::String(s.clone()),
        Value::Vec2(x, y) => Variant::Vector2(godot_core::Vector2::new(*x as f32, *y as f32)),
        Value::Vec3(x, y, z) => Variant::Vector3(godot_core::Vector3::new(*x as f32, *y as f32, *z as f32)),
        Value::ColorV(r, g, b, a) => Variant::Color(godot_core::Color::rgba(*r as f32, *g as f32, *b as f32, *a as f32)),
        Value::Arr(items) => {
            let mut arr = godot_core::GodotArray::new();
            for item in items {
                let _ = arr.push(value_to_variant(item));
            }
            Variant::Array(arr)
        }
    }
}

pub fn variant_to_value(v: &godot_core::Variant) -> Value {
    use godot_core::Variant as V;
    match v {
        V::Nil => Value::Nil,
        V::Bool(b) => Value::Bool(*b),
        V::Int(i) => Value::Int(*i),
        V::Float(f) => Value::Float(*f),
        V::String(s) | V::StringName(s) => Value::Str(s.clone()),
        V::Vector2(p) => Value::Vec2(p.x as f64, p.y as f64),
        V::Vector2i(p) => Value::Vec2(p.x as f64, p.y as f64),
        V::Vector3(p) => Value::Vec3(p.x as f64, p.y as f64, p.z as f64),
        V::Vector3i(p) => Value::Vec3(p.x as f64, p.y as f64, p.z as f64),
        V::Color(c) => Value::ColorV(c.r as f64, c.g as f64, c.b as f64, c.a as f64),
        V::Array(a) => Value::Arr(a.iter().map(variant_to_value).collect()),
        other => Value::Str(other.to_string_lossy()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    fn run_src(src: &str) -> Interpreter {
        let f = parse(src).expect("parse");
        let mut interp = Interpreter::new();
        for func in &f.fns {
            interp.run(&func.body);
        }
        interp
    }
    #[test]
    fn member_assign_moves_like_gdscript() {
        let mut interp = Interpreter::new();
        interp.vars.insert("position".into(), Value::Vec3(0.0, 0.0, 0.0));
        interp.vars.insert("speed".into(), Value::Float(200.0));
        interp.vars.insert("dt".into(), Value::Float(0.5));
        interp.run(&[crate::ast::IrisStmt::Assign {
            target: "position.x".into(),
            op: "+=".into(),
            expr: "speed * dt".into(),
        }]);
        assert_eq!(interp.vars.get("position"), Some(&Value::Vec3(100.0, 0.0, 0.0)));
    }
    #[test]
    fn loops_and_conditionals() {
        let out = run_src("node N : Node {\nfn main() {\nlet total = 0\nfor i in range(4) {\nlet total = total + i\n}\nprint total\nlet n = 0\nwhile n < 3 {\nlet n = n + 1\n}\nprint n\nif total == 6 {\nprint ok\n}\n}\n");
        assert!(out.output.contains(&"6".to_string()));
        assert!(out.output.contains(&"3".to_string()));
        assert!(out.output.contains(&"ok".to_string()));
    }
}
