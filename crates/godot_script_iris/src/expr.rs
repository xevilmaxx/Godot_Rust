//! Iris expression engine v1 — lexer + Pratt parser + evaluator.
//! Covers the `core/variant/variant_op.cpp` operator surface used by scripts:
//! arithmetic, comparisons, `and/or/not`, member access (`position.x`),
//! indexing (`a[0]`), calls (`len(x)`, `vec3(1,2,3)`, `range(3)`), parentheses.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Vec2(f64, f64),
    Vec3(f64, f64, f64),
    ColorV(f64, f64, f64, f64),
    Arr(Vec<Value>),
}

impl Value {
    pub fn to_display(&self) -> String {
        match self {
            Value::Nil => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => {
                if f.fract() == 0.0 && f.is_finite() {
                    format!("{f:.1}")
                } else {
                    format!("{f}")
                }
            }
            Value::Str(s) => s.clone(),
            Value::Vec2(x, y) => format!("Vector2({x}, {y})"),
            Value::Vec3(x, y, z) => format!("Vector3({x}, {y}, {z})"),
            Value::ColorV(r, g, b, a) => format!("Color({r}, {g}, {b}, {a})"),
            Value::Arr(items) => {
                format!("[{}]", items.iter().map(|v| v.to_display()).collect::<Vec<_>>().join(", "))
            }
        }
    }
    pub fn truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Arr(a) => !a.is_empty(),
            _ => true,
        }
    }
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "str",
            Value::Vec2(_, _) => "vec2",
            Value::Vec3(_, _, _) => "vec3",
            Value::ColorV(_, _, _, _) => "color",
            Value::Arr(_) => "array",
        }
    }
    fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Bool(b) => Some(*b as i32 as f64),
            _ => None,
        }
    }
    fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }
}

// ------------------------------------------------------------------ lexer --
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64, bool),
    Str(String),
    Ident(String),
    True,
    False,
    Null,
    And,
    Or,
    Not,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    StarStar,
    EqEq,
    BangEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Eof,
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let mut toks = vec![];
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '#' {
            break; // trailing comment
        }
        if c == '"' {
            let mut s = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    s.push(match chars[i] {
                        'n' => '\n',
                        't' => '\t',
                        c => c,
                    });
                } else {
                    s.push(chars[i]);
                }
                i += 1;
            }
            i += 1; // closing quote
            toks.push(Tok::Str(s));
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
            let start = i;
            let mut is_float = false;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == 'e' || chars[i] == 'E' || ((chars[i] == '+' || chars[i] == '-') && i > start && (chars[i-1] == 'e' || chars[i-1] == 'E'))) {
                if chars[i] == '.' || chars[i] == 'e' || chars[i] == 'E' {
                    is_float = true;
                }
                i += 1;
            }
            let num: f64 = chars[start..i].iter().collect::<String>().parse().map_err(|_| format!("bad number"))?;
            toks.push(Tok::Num(num, is_float));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            toks.push(match word.as_str() {
                "true" => Tok::True,
                "false" => Tok::False,
                "null" => Tok::Null,
                "and" => Tok::And,
                "or" => Tok::Or,
                "not" => Tok::Not,
                _ => Tok::Ident(word),
            });
            continue;
        }
        let two = if i + 1 < chars.len() { Some((c, chars[i + 1])) } else { None };
        match two {
            Some(('*', '*')) => {
                toks.push(Tok::StarStar);
                i += 2;
                continue;
            }
            Some(('=', '=')) => {
                toks.push(Tok::EqEq);
                i += 2;
                continue;
            }
            Some(('!', '=')) => {
                toks.push(Tok::BangEq);
                i += 2;
                continue;
            }
            Some(('<', '=')) => {
                toks.push(Tok::LtEq);
                i += 2;
                continue;
            }
            Some(('>', '=')) => {
                toks.push(Tok::GtEq);
                i += 2;
                continue;
            }
            _ => {}
        }
        toks.push(match c {
            '+' => Tok::Plus,
            '-' => Tok::Minus,
            '*' => Tok::Star,
            '/' => Tok::Slash,
            '%' => Tok::Percent,
            '<' => Tok::Lt,
            '>' => Tok::Gt,
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            ',' => Tok::Comma,
            '.' => Tok::Dot,
            _ => return Err(format!("unexpected character `{c}`")),
        });
        i += 1;
    }
    toks.push(Tok::Eof);
    Ok(toks)
}

// ------------------------------------------------------------------ parser -
#[derive(Debug, Clone)]
enum Expr {
    Lit(Value),
    Var(String),
    Unary { op: UnaryOp, expr: Box<Expr> },
    Binary { op: BinOp, left: Box<Expr>, right: Box<Expr> },
    Member { obj: Box<Expr>, attr: String },
    Index { obj: Box<Expr>, idx: Box<Expr> },
    Call { name: String, args: Vec<Expr> },
}

#[derive(Debug, Clone, Copy)]
enum UnaryOp {
    Neg,
    Not,
}
#[derive(Debug, Clone, Copy, PartialEq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }
    fn next(&mut self) -> Tok {
        let t = self.peek().clone();
        self.pos += 1;
        t
    }
    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_or()
    }
    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while *self.peek() == Tok::Or {
            self.next();
            let right = self.parse_and()?;
            left = Expr::Binary { op: BinOp::Or, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }
    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_cmp()?;
        while *self.peek() == Tok::And {
            self.next();
            let right = self.parse_cmp()?;
            left = Expr::Binary { op: BinOp::And, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }
    fn parse_cmp(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_add()?;
        loop {
            let op = match self.peek() {
                Tok::EqEq => BinOp::Eq,
                Tok::BangEq => BinOp::Ne,
                Tok::Lt => BinOp::Lt,
                Tok::LtEq => BinOp::Le,
                Tok::Gt => BinOp::Gt,
                Tok::GtEq => BinOp::Ge,
                _ => break,
            };
            self.next();
            let right = self.parse_add()?;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }
    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            self.next();
            let right = self.parse_mul()?;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }
    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_pow()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Mod,
                _ => break,
            };
            self.next();
            let right = self.parse_pow()?;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }
    fn parse_pow(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        if *self.peek() == Tok::StarStar {
            self.next();
            let right = self.parse_unary()?;
            left = Expr::Binary { op: BinOp::Pow, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }
    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Tok::Minus => {
                self.next();
                Ok(Expr::Unary { op: UnaryOp::Neg, expr: Box::new(self.parse_unary()?) })
            }
            Tok::Not => {
                self.next();
                Ok(Expr::Unary { op: UnaryOp::Not, expr: Box::new(self.parse_unary()?) })
            }
            _ => self.parse_postfix(),
        }
    }
    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_primary()?;
        loop {
            match self.peek() {
                Tok::Dot => {
                    self.next();
                    match self.next() {
                        Tok::Ident(attr) => e = Expr::Member { obj: Box::new(e), attr },
                        t => return Err(format!("expected attribute after `.`, got {t:?}")),
                    }
                }
                Tok::LBracket => {
                    self.next();
                    let idx = self.parse_expr()?;
                    match self.next() {
                        Tok::RBracket => e = Expr::Index { obj: Box::new(e), idx: Box::new(idx) },
                        t => return Err(format!("expected `]`, got {t:?}")),
                    }
                }
                _ => break,
            }
        }
        Ok(e)
    }
    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Tok::Num(n, is_float) => {
                if !is_float && n.fract() == 0.0 && n.abs() < 9e15 {
                    Ok(Expr::Lit(Value::Int(n as i64)))
                } else {
                    Ok(Expr::Lit(Value::Float(n)))
                }
            }
            Tok::Str(s) => Ok(Expr::Lit(Value::Str(s))),
            Tok::True => Ok(Expr::Lit(Value::Bool(true))),
            Tok::False => Ok(Expr::Lit(Value::Bool(false))),
            Tok::Null => Ok(Expr::Lit(Value::Nil)),
            Tok::Ident(name) => {
                if *self.peek() == Tok::LParen {
                    self.next();
                    let mut args = vec![];
                    if *self.peek() != Tok::RParen {
                        loop {
                            args.push(self.parse_expr()?);
                            match self.next() {
                                Tok::Comma => continue,
                                Tok::RParen => break,
                                t => return Err(format!("expected `,` or `)`, got {t:?}")),
                            }
                        }
                    } else {
                        self.next();
                    }
                    Ok(Expr::Call { name, args })
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Tok::LParen => {
                let e = self.parse_expr()?;
                match self.next() {
                    Tok::RParen => Ok(e),
                    t => Err(format!("expected `)`, got {t:?}")),
                }
            }
            Tok::LBracket => {
                let mut items = vec![];
                if *self.peek() != Tok::RBracket {
                    loop {
                        items.push(self.parse_expr()?);
                        match self.next() {
                            Tok::Comma => continue,
                            Tok::RBracket => break,
                            t => return Err(format!("expected `,` or `]`, got {t:?}")),
                        }
                    }
                } else {
                    self.next();
                }
                Ok(Expr::Lit(Value::Arr(items.into_iter().map(|e| eval_const(&e)).collect::<Result<Vec<_>, _>>()?)))
            }
            t => Err(format!("unexpected token {t:?}")),
        }
    }
}

/// Array literals must be constant-foldable at parse time if they contain only
/// literals; otherwise keep them dynamic (handled by wrapping in a call-like node).
/// Here we simply reject non-literal arrays at parse and re-parse dynamically.
fn eval_const(e: &Expr) -> Result<Value, String> {
    match e {
        Expr::Lit(v) => Ok(v.clone()),
        _ => Err("array literal must be constant in this position (use a variable instead)".into()),
    }
}

// ------------------------------------------------------------------ eval ---
pub fn eval(src: &str, vars: &BTreeMap<String, Value>) -> Result<Value, String> {
    let toks = lex(src)?;
    let mut p = Parser { toks, pos: 0 };
    let e = p.parse_expr()?;
    if *p.peek() != Tok::Eof {
        return Err(format!("trailing tokens after expression"));
    }
    eval_expr(&e, vars)
}

fn eval_expr(e: &Expr, vars: &BTreeMap<String, Value>) -> Result<Value, String> {
    match e {
        Expr::Lit(Value::Arr(_)) => {
            // Dynamic arrays were folded at parse; re-evaluate by re-parsing is
            // unnecessary — literals evaluate to themselves.
            Ok(eval_lit(e, vars)?)
        }
        Expr::Lit(v) => Ok(v.clone()),
        Expr::Var(name) => vars.get(name).cloned().ok_or_else(|| format!("unknown variable `{name}`")),
        Expr::Unary { op, expr } => {
            let v = eval_expr(expr, vars)?;
            match op {
                UnaryOp::Neg => match v {
                    Value::Int(i) => Ok(Value::Int(-i)),
                    Value::Float(f) => Ok(Value::Float(-f)),
                    _ => Err(format!("unary - on {}", v.type_name())),
                },
                UnaryOp::Not => Ok(Value::Bool(!v.truthy())),
            }
        }
        Expr::Binary { op, left, right } => eval_bin(*op, &eval_expr(left, vars)?, &eval_expr(right, vars)?),
        Expr::Member { obj, attr } => {
            let v = eval_expr(obj, vars)?;
            member(&v, attr).ok_or_else(|| format!("no attribute `{attr}` on {}", v.type_name()))
        }
        Expr::Index { obj, idx } => {
            let (o, i) = (eval_expr(obj, vars)?, eval_expr(idx, vars)?);
            index(&o, &i).ok_or_else(|| format!("cannot index {} with {}", o.type_name(), i.type_name()))
        }
        Expr::Call { name, args } => {
            let vals: Result<Vec<Value>, String> = args.iter().map(|a| eval_expr(a, vars)).collect();
            call(name, &vals?)
        }
    }
}

fn eval_lit(e: &Expr, _vars: &BTreeMap<String, Value>) -> Result<Value, String> {
    match e {
        Expr::Lit(v) => Ok(v.clone()),
        _ => Err("not a literal".into()),
    }
}

fn num_bin(op: BinOp, a: f64, b: f64, ai: Option<i64>, bi: Option<i64>) -> Result<Value, String> {
    match op {
        BinOp::Add => Ok(int_or_float(ai, bi, a + b)),
        BinOp::Sub => Ok(int_or_float(ai, bi, a - b)),
        BinOp::Mul => Ok(int_or_float(ai, bi, a * b)),
        BinOp::Div => {
            if ai.is_some() && bi.is_some() {
                if bi == Some(0) {
                    return Err("integer division by zero".into());
                }
                Ok(Value::Int(ai.unwrap() / bi.unwrap()))
            } else {
                Ok(Value::Float(a / b))
            }
        }
        BinOp::Mod => {
            if let (Some(x), Some(y)) = (ai, bi) {
                if y == 0 {
                    return Err("modulo by zero".into());
                }
                Ok(Value::Int(x % y))
            } else {
                Ok(Value::Float(a % b))
            }
        }
        BinOp::Pow => Ok(Value::Float(a.powf(b))),
        _ => unreachable!(),
    }
}

fn int_or_float(ai: Option<i64>, bi: Option<i64>, f: f64) -> Value {
    match (ai, bi) {
        (Some(_), Some(_)) => Value::Int(f as i64),
        _ => Value::Float(f),
    }
}

fn eval_bin(op: BinOp, a: &Value, b: &Value) -> Result<Value, String> {
    use Value as V;
    match op {
        BinOp::And => Ok(V::Bool(a.truthy() && b.truthy())),
        BinOp::Or => Ok(V::Bool(a.truthy() || b.truthy())),
        BinOp::Eq => Ok(V::Bool(values_equal(a, b))),
        BinOp::Ne => Ok(V::Bool(!values_equal(a, b))),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let ord = compare(a, b).ok_or_else(|| format!("cannot compare {} and {}", a.type_name(), b.type_name()))?;
            Ok(V::Bool(match op {
                BinOp::Lt => ord < 0,
                BinOp::Le => ord <= 0,
                BinOp::Gt => ord > 0,
                _ => ord >= 0,
            }))
        }
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow => {
            match (a, b) {
                (V::Str(x), V::Str(y)) if op == BinOp::Add => Ok(V::Str(format!("{x}{y}"))),
                (V::Str(x), o) if op == BinOp::Add => Ok(V::Str(format!("{x}{}", o.to_display()))),
                (V::Arr(x), V::Arr(y)) if op == BinOp::Add => {
                    Ok(V::Arr(x.iter().chain(y.iter()).cloned().collect()))
                }
                (V::Str(s), V::Int(n)) if op == BinOp::Mul => Ok(V::Str(s.repeat((*n).max(0) as usize))),
                (V::Vec2(x1, y1), V::Vec2(x2, y2)) => match op {
                    BinOp::Add => Ok(V::Vec2(x1 + x2, y1 + y2)),
                    BinOp::Sub => Ok(V::Vec2(x1 - x2, y1 - y2)),
                    BinOp::Mul => Ok(V::Vec2(x1 * x2, y1 * y2)),
                    _ => Err(format!("unsupported vec2 operator")),
                },
                (V::Vec3(x1, y1, z1), V::Vec3(x2, y2, z2)) => match op {
                    BinOp::Add => Ok(V::Vec3(x1 + x2, y1 + y2, z1 + z2)),
                    BinOp::Sub => Ok(V::Vec3(x1 - x2, y1 - y2, z1 - z2)),
                    BinOp::Mul => Ok(V::Vec3(x1 * x2, y1 * y2, z1 * z2)),
                    _ => Err(format!("unsupported vec3 operator")),
                },
                _ => {
                    let (af, bf) = (a.as_f64(), b.as_f64());
                    match (af, bf) {
                        (Some(x), Some(y)) => num_bin(op, x, y, a.as_i64(), b.as_i64()),
                        _ => Err(format!("operator on {} and {}", a.type_name(), b.type_name())),
                    }
                }
            }
        }
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Float(y)) => (*x as f64) == *y,
        (Value::Float(x), Value::Int(y)) => *x == (*y as f64),
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Nil, Value::Nil) => true,
        _ => a.to_display() == b.to_display() && a.type_name() == b.type_name(),
    }
}

fn compare(a: &Value, b: &Value) -> Option<i8> {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => Some(if x < y { -1 } else if x > y { 1 } else { 0 }),
        _ => match (a, b) {
            (Value::Str(x), Value::Str(y)) => Some(if x < y { -1 } else if x > y { 1 } else { 0 }),
            _ => None,
        },
    }
}

fn member(v: &Value, attr: &str) -> Option<Value> {
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
        Value::Str(s) => match attr {
            "length" => Some(Value::Int(s.chars().count() as i64)),
            _ => None,
        },
        Value::Arr(items) => match attr {
            "length" | "size" => Some(Value::Int(items.len() as i64)),
            _ => None,
        },
        _ => None,
    }
}

fn index(obj: &Value, idx: &Value) -> Option<Value> {
    match (obj, idx) {
        (Value::Arr(items), Value::Int(i)) => {
            items.get(i.rem_euclid(items.len().max(1) as i64) as usize).cloned()
        }
        (Value::Str(s), Value::Int(i)) => {
            let chars: Vec<char> = s.chars().collect();
            chars.get(i.rem_euclid(chars.len().max(1) as i64) as usize).map(|c| Value::Int(*c as i64))
        }
        _ => None,
    }
}

fn call(name: &str, args: &[Value]) -> Result<Value, String> {
    let bad = |n: usize| format!("{name} expects {n} argument(s)");
    match name {
        "len" => match args {
            [Value::Arr(a)] => Ok(Value::Int(a.len() as i64)),
            [Value::Str(s)] => Ok(Value::Int(s.chars().count() as i64)),
            [_] => Err("len() needs an array or string".into()),
            _ => Err(bad(1)),
        },
        "str" => match args {
            [v] => Ok(Value::Str(v.to_display())),
            _ => Err(bad(1)),
        },
        "int" => match args {
            [Value::Int(i)] => Ok(Value::Int(*i)),
            [Value::Float(f)] => Ok(Value::Int(*f as i64)),
            [Value::Bool(b)] => Ok(Value::Int(*b as i64)),
            [Value::Str(s)] => s.trim().parse::<i64>().map(Value::Int).map_err(|_| "int() parse failed".into()),
            _ => Err(bad(1)),
        },
        "float" => match args {
            [Value::Float(f)] => Ok(Value::Float(*f)),
            [Value::Int(i)] => Ok(Value::Float(*i as f64)),
            [Value::Bool(b)] => Ok(Value::Float(*b as i32 as f64)),
            [Value::Str(s)] => s.trim().parse::<f64>().map(Value::Float).map_err(|_| "float() parse failed".into()),
            _ => Err(bad(1)),
        },
        "abs" => match args {
            [Value::Int(i)] => Ok(Value::Int(i.abs())),
            [Value::Float(f)] => Ok(Value::Float(f.abs())),
            _ => Err("abs() needs a number".into()),
        },
        "min" => match args {
            [a, b] => compare(a, b).map(|o| if o <= 0 { a.clone() } else { b.clone() }).ok_or_else(|| "min() needs comparable values".into()),
            _ => Err(bad(2)),
        },
        "max" => match args {
            [a, b] => compare(a, b).map(|o| if o >= 0 { a.clone() } else { b.clone() }).ok_or_else(|| "max() needs comparable values".into()),
            _ => Err(bad(2)),
        },
        "clamp" => match args {
            [v, lo, hi] => {
                let (a, l, h) = (v.as_f64(), lo.as_f64(), hi.as_f64());
                match (a, l, h) {
                    (Some(x), Some(aa), Some(bb)) => Ok(Value::Float(x.clamp(aa, bb))),
                    _ => Err("clamp() needs numbers".into()),
                }
            }
            _ => Err(bad(3)),
        },
        "lerp" => match args {
            [a, b, w] => {
                let (x, y, t) = (a.as_f64(), b.as_f64(), w.as_f64());
                match (x, y, t) {
                    (Some(x), Some(y), Some(t)) => Ok(Value::Float(x + (y - x) * t)),
                    _ => Err("lerp() needs numbers".into()),
                }
            }
            _ => Err(bad(3)),
        },
        "vec2" => match args {
            [x, y] => match (x.as_f64(), y.as_f64()) {
                (Some(a), Some(b)) => Ok(Value::Vec2(a, b)),
                _ => Err("vec2() needs numbers".into()),
            },
            _ => Err(bad(2)),
        },
        "vec3" => match args {
            [x, y, z] => match (x.as_f64(), y.as_f64(), z.as_f64()) {
                (Some(a), Some(b), Some(c)) => Ok(Value::Vec3(a, b, c)),
                _ => Err("vec3() needs numbers".into()),
            },
            _ => Err(bad(3)),
        },
        "color" => match args {
            [r, g, b] => match (r.as_f64(), g.as_f64(), b.as_f64()) {
                (Some(a), Some(bb), Some(c)) => Ok(Value::ColorV(a, bb, c, 1.0)),
                _ => Err("color() needs numbers".into()),
            },
            [r, g, b, a] => match (r.as_f64(), g.as_f64(), b.as_f64(), a.as_f64()) {
                (Some(x), Some(y), Some(z), Some(w)) => Ok(Value::ColorV(x, y, z, w)),
                _ => Err("color() needs numbers".into()),
            },
            _ => Err("color() needs 3 or 4 numbers".into()),
        },
        "range" => match args {
            [Value::Int(n)] => {
                if *n < 0 || *n > 100000 {
                    return Err("range() out of bounds (0..100000)".into());
                }
                Ok(Value::Arr((0..*n).map(Value::Int).collect()))
            }
            _ => Err("range(n) needs an int".into()),
        },
        _ => Err(format!("unknown function `{name}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ev(src: &str) -> Value {
        eval(src, &BTreeMap::new()).expect(src)
    }
    #[test]
    fn arithmetic_matches_gdscript() {
        assert_eq!(ev("2 + 3 * 4"), Value::Int(14));
        assert_eq!(ev("(2 + 3) * 4"), Value::Int(20));
        assert_eq!(ev("7 / 2"), Value::Int(3));
        assert_eq!(ev("7.0 / 2"), Value::Float(3.5));
        assert_eq!(ev("2 ** 3"), Value::Float(8.0));
        assert_eq!(ev("10 % 3"), Value::Int(1));
        assert_eq!(ev("-5 + 8"), Value::Int(3));
    }
    #[test]
    fn logic_and_strings() {
        assert_eq!(ev("1 < 2 and 3 > 2"), Value::Bool(true));
        assert_eq!(ev("not false or false"), Value::Bool(true));
        assert_eq!(ev("\"a\" + \"b\""), Value::Str("ab".into()));
        assert_eq!(ev("\"ab\" + 1"), Value::Str("ab1".into()));
        assert_eq!(ev("len(\"hello\")"), Value::Int(5));
    }
    #[test]
    fn vectors_members_calls() {
        assert_eq!(ev("vec2(1, 2).x + 10"), Value::Float(11.0));
        assert_eq!(ev("clamp(15, 0, 10)"), Value::Float(10.0));
        assert_eq!(ev("range(3)"), Value::Arr(vec![Value::Int(0), Value::Int(1), Value::Int(2)]));
    }
}
