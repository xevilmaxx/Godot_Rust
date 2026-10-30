//! GDScript → Iris transpiler (subset, honest scope).
//!
//! Supported: `extends`/`class_name`, member `var`/`@export var`/`const`,
//! `func` (typed args, defaults stripped), `if/elif/else`, `while`,
//! `for x in range()/array`, `return`, `print()`, `emit_signal()`, `pass`,
//! assignments (`= += -= *= /= %=`), expression statements, `match` (literal
//! patterns desugared to if/elif), ternary `a if c else b` in let/assign/return,
//! `PI/TAU/INF` constants. See `docs/GDSCRIPT_COVERAGE.md` for the full matrix.
//!
//! Known gaps (documented, not silent): `await`/coroutines, lambdas, `signal`
//! declarations (use `emit`), `super`, static typing enforcement, `$Node`
//! references (become path strings), `match` binding patterns, `%` formatting,
//! typed arrays/dictionaries enforcement.

use crate::ast::*;
use godot_core::GodotError;

pub fn transpile(src: &str) -> Result<IrisFile, GodotError> {
    let mut lines: Vec<(usize, usize, String)> = vec![];
    for (i, l) in src.lines().enumerate() {
        let code = strip_comment(l);
        if code.trim().is_empty() {
            continue;
        }
        let indent = indent_of_strict(&code).ok_or_else(|| {
            GodotError::Parse(format!("line {}: indentation must be tabs or groups of 4 spaces", i + 1))
        })?;
        lines.push((i + 1, indent, code.trim().to_string()));
    }
    let mut file = IrisFile {
        node_class: "MyNode".into(),
        base_class: "Node".into(),
        props: vec![],
        fns: vec![],
    };
    let mut pos = 0;
    // Top level: extends/class_name/var/func (indent 0).
    while pos < lines.len() {
        let (ln, ind, line) = &lines[pos];
        let (ln, ind, line) = (*ln, *ind, line.clone());
        if ind != 0 {
            return Err(GodotError::Parse(format!("line {ln}: top-level statement must not be indented")));
        }
        if let Some(rest) = line.strip_prefix("extends ") {
            file.base_class = rest.trim().to_string();
            pos += 1;
        } else if let Some(rest) = line.strip_prefix("class_name ") {
            file.node_class = rest.trim().split_whitespace().next().unwrap_or("MyNode").to_string();
            pos += 1;
        } else if line.starts_with("var ") || line.starts_with("@export var ") || line.starts_with("const ") {
            let decl = line.strip_prefix("@export ").unwrap_or(&line);
            let decl = decl.strip_prefix("var ").or_else(|| decl.strip_prefix("const ")).unwrap_or(decl);
            let (name, ty, default) = parse_var_decl(&decl, ln)?;
            file.props.push(Prop { name, ty, default });
            pos += 1;
        } else if line.starts_with("signal ") {
            // Iris has no signal declarations; emit_signal() maps to Emit.
            pos += 1;
        } else if line.starts_with("func ") {
            let (func, next) = parse_func(&lines, pos)?;
            pos = next;
            file.fns.push(func);
        } else {
            return Err(GodotError::Parse(format!("line {ln}: unsupported top-level `{line}` (see GDSCRIPT_COVERAGE.md)")));
        }
    }
    Ok(file)
}

/// Emit an IrisFile back to Iris source (for the Script editor import path).
pub fn emit_iris(file: &IrisFile) -> String {
    let mut out = String::new();
    out.push_str(&format!("node {} : {} {{\n", file.node_class, file.base_class));
    for p in &file.props {
        out.push_str(&format!("prop {}: {} = {}\n", p.name, p.ty, p.default));
    }
    for f in &file.fns {
        let args: Vec<String> = f.args.iter().map(|(n, t)| format!("{n}: {t}")).collect();
        out.push_str(&format!("fn {}({}) {{\n", f.name, args.join(", ")));
        emit_block(&mut out, &f.body, 0);
        out.push_str("}\n");
    }
    out.push_str("}\n");
    out
}

fn emit_block(out: &mut String, stmts: &[IrisStmt], depth: usize) {
    let pad = "  ".repeat(depth + 1);
    for s in stmts {
        match s {
            IrisStmt::Let { name, ty, expr } => {
                if let Some(t) = ty {
                    out.push_str(&format!("{pad}let {name}: {t} = {expr}\n"));
                } else {
                    out.push_str(&format!("{pad}let {name} = {expr}\n"));
                }
            }
            IrisStmt::Assign { target, op, expr } => out.push_str(&format!("{pad}{target} {op} {expr}\n")),
            IrisStmt::Print(e) => out.push_str(&format!("{pad}print {e}\n")),
            IrisStmt::Emit(e) => out.push_str(&format!("{pad}emit {e}\n")),
            IrisStmt::Return(e) => {
                if let Some(x) = e {
                    out.push_str(&format!("{pad}return {x}\n"));
                } else {
                    out.push_str(&format!("{pad}return\n"));
                }
            }
            IrisStmt::Expr(e) => out.push_str(&format!("{pad}{e}\n")),
            IrisStmt::If { cond, then_b, else_b } => {
                out.push_str(&format!("{pad}if {cond} {{\n"));
                emit_block(out, then_b, depth + 1);
                if else_b.is_empty() {
                    out.push_str(&format!("{pad}}}\n"));
                } else if else_b.len() == 1 {
                    if let IrisStmt::If { cond: c2, then_b: t2, else_b: e2 } = &else_b[0] {
                        out.push_str(&format!("{pad}}} else if {c2} {{\n"));
                        emit_block(out, t2, depth + 1);
                        if e2.is_empty() {
                            out.push_str(&format!("{pad}}}\n"));
                        } else {
                            out.push_str(&format!("{pad}}} else {{\n"));
                            emit_block(out, e2, depth + 1);
                            out.push_str(&format!("{pad}}}\n"));
                        }
                    } else {
                        out.push_str(&format!("{pad}}} else {{\n"));
                        emit_block(out, else_b, depth + 1);
                        out.push_str(&format!("{pad}}}\n"));
                    }
                } else {
                    out.push_str(&format!("{pad}}} else {{\n"));
                    emit_block(out, else_b, depth + 1);
                    out.push_str(&format!("{pad}}}\n"));
                }
            }
            IrisStmt::While { cond, body } => {
                out.push_str(&format!("{pad}while {cond} {{\n"));
                emit_block(out, body, depth + 1);
                out.push_str(&format!("{pad}}}\n"));
            }
            IrisStmt::For { var, iter, body } => {
                out.push_str(&format!("{pad}for {var} in {iter} {{\n"));
                emit_block(out, body, depth + 1);
                out.push_str(&format!("{pad}}}\n"));
            }
        }
    }
}

/// Tabs count 1 level each; spaces must come in groups of 4. Mixed → None.
fn indent_of_strict(line: &str) -> Option<usize> {
    let mut tabs = 0;
    let mut spaces = 0;
    for c in line.chars() {
        if c == '\t' {
            tabs += 1;
        } else if c == ' ' {
            spaces += 1;
        } else {
            break;
        }
    }
    if tabs > 0 && spaces > 0 {
        return None;
    }
    if spaces % 4 != 0 {
        return None;
    }
    Some(tabs + spaces / 4)
}

fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    let mut prev = '\0';
    for (i, c) in line.char_indices() {
        if c == '"' && prev != '\\' {
            in_str = !in_str;
        } else if c == '#' && !in_str {
            return line[..i].trim_end();
        }
        prev = c;
    }
    line
}

fn map_type(t: &str) -> String {
    match t.trim() {
        "int" => "int",
        "float" => "float",
        "String" => "str",
        "bool" => "bool",
        "Vector2" => "vec2",
        "Vector3" => "vec3",
        "Vector4" => "vec4",
        "Color" => "color",
        "Rect2" => "rect2",
        "NodePath" => "nodepath",
        "Array" => "array",
        "Dictionary" => "dict",
        "" => "variant",
        other => other,
    }
    .to_string()
}

fn convert_expr(expr: &str) -> String {
    // $Node/Path -> "Node/Path" (documented limitation: path strings).
    let mut out = String::new();
    let mut chars = expr.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            let mut path = String::new();
            while let Some(&d) = chars.peek() {
                if d.is_alphanumeric() || d == '_' || d == '/' || d == '.' {
                    path.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push('"');
            out.push_str(&path);
            out.push('"');
        } else {
            out.push(c);
        }
    }
    // Known constants.
    let out = out.replace("TAU", "6.2831853").replace("PI", "3.14159265").replace("INF", "1e30");
    out
}

fn parse_var_decl(decl: &str, ln: usize) -> Result<(String, String, String), GodotError> {
    // `name [: Type] [= default]`
    let (name_ty, default) = match decl.split_once('=') {
        Some((a, b)) => (a, b.trim().to_string()),
        None => (decl, "null".to_string()),
    };
    // Guard against `==` sneaking in (not valid in a declaration).
    if default.starts_with('=') || name_ty.trim_end().ends_with(['=', '<', '>', '!']) {
        return Err(GodotError::Parse(format!("line {ln}: bad variable declaration")));
    }
    let mut parts = name_ty.split(':');
    let name = parts.next().unwrap_or("").trim().to_string();
    let ty = map_type(parts.next().unwrap_or("variant"));
    if name.is_empty() {
        return Err(GodotError::Parse(format!("line {ln}: var needs a name")));
    }
    Ok((name, ty, convert_expr(&default)))
}

fn parse_func(lines: &[(usize, usize, String)], pos: usize) -> Result<(Func, usize), GodotError> {
    let (ln, ind, line) = &lines[pos];
    let (ln, ind) = (*ln, *ind);
    let rest = line.strip_prefix("func ").unwrap_or(line).trim().trim_end_matches(':').trim();
    let name_end = rest.find('(').ok_or_else(|| GodotError::Parse(format!("line {ln}: func needs `(args)`")))?;
    let name = rest[..name_end].trim().to_string();
    let arg_end = rest.find(')').ok_or_else(|| GodotError::Parse(format!("line {ln}: func needs `)`")))?;
    let args: Vec<(String, String)> = rest[name_end + 1..arg_end]
        .split(',')
        .filter_map(|a| {
            let a = a.trim();
            if a.is_empty() {
                return None;
            }
            let a = if a == "_" || a.starts_with("_ ") { "_arg" } else { a.trim_start_matches('_') };
            if a.is_empty() {
                return None;
            }
            let a = a.split('=').next().unwrap_or("").trim(); // strip defaults
            let mut parts = a.split(':');
            Some((parts.next().unwrap_or("").trim().to_string(), map_type(parts.next().unwrap_or("variant"))))
        })
        .collect();
    let mut body = vec![];
    let mut i = pos + 1;
    while i < lines.len() && lines[i].1 > ind {
        let (stmts, next) = parse_stmt(lines, i, ind + 1)?;
        body.extend(stmts);
        i = next;
    }
    Ok((Func { name, args, body }, i))
}

/// Parse one logical statement at `pos` (which must be at `want` indent);
/// nested blocks consume deeper-indented lines. Returns (stmts, next_pos).
fn parse_stmt(lines: &[(usize, usize, String)], pos: usize, want: usize) -> Result<(Vec<IrisStmt>, usize), GodotError> {
    let (ln, ind, line) = &lines[pos];
    let (ln, ind, line) = (*ln, *ind, line.clone());
    if ind != want {
        return Err(GodotError::Parse(format!("line {ln}: unexpected indent (want {want}, got {ind})")));
    }
    // if / while / for / match open blocks.
    if let Some(rest) = line.strip_prefix("if ") {
        let cond = convert_expr(rest.trim().trim_end_matches(':').trim());
        let (then_b, mut i) = parse_block(lines, pos + 1, want + 1)?;
        // Collect elif/else branches, then fold into nested Ifs (last first).
        let mut branches: Vec<(Option<String>, Vec<IrisStmt>)> = vec![];
        loop {
            if i >= lines.len() || lines[i].1 != want {
                break;
            }
            if let Some(r) = lines[i].2.strip_prefix("elif ") {
                let c = convert_expr(r.trim().trim_end_matches(':').trim());
                let (tb, nx) = parse_block(lines, i + 1, want + 1)?;
                branches.push((Some(c), tb));
                i = nx;
            } else if lines[i].2 == "else:" || lines[i].2 == "else" {
                let (eb, nx) = parse_block(lines, i + 1, want + 1)?;
                branches.push((None, eb));
                i = nx;
                break; // else is always last
            } else {
                break;
            }
        }
        let mut else_b: Vec<IrisStmt> = vec![];
        for (cond_opt, body) in branches.into_iter().rev() {
            match cond_opt {
                Some(c) => else_b = vec![IrisStmt::If { cond: c, then_b: body, else_b }],
                None => else_b = body,
            }
        }
        return Ok((vec![IrisStmt::If { cond, then_b, else_b }], i));
    }
    if let Some(rest) = line.strip_prefix("while ") {
        let cond = convert_expr(rest.trim().trim_end_matches(':').trim());
        let (body, next) = parse_block(lines, pos + 1, want + 1)?;
        return Ok((vec![IrisStmt::While { cond, body }], next));
    }
    if let Some(rest) = line.strip_prefix("for ") {
        let rest = rest.trim().trim_end_matches(':').trim();
        let (var, iter) = rest.split_once(" in ").ok_or_else(|| GodotError::Parse(format!("line {ln}: for needs `for x in <iter>`")))?;
        let (body, next) = parse_block(lines, pos + 1, want + 1)?;
        return Ok((vec![IrisStmt::For { var: var.trim().to_string(), iter: convert_expr(iter.trim()), body }], next));
    }
    if line.starts_with("match ") {
        return parse_match(lines, pos, want);
    }
    // Single-line statements (also handle ternary in let/assign/return/print).
    if line == "pass" {
        return Ok((vec![], pos + 1));
    }
    if let Some(rest) = line.strip_prefix("return") {
        let rest = rest.trim();
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        if rest.is_empty() {
            return Ok((vec![IrisStmt::Return(None)], pos + 1));
        }
        if let Some((v, c, alt)) = split_ternary(rest) {
            let then_b = vec![IrisStmt::Return(Some(v))];
            let else_b = vec![IrisStmt::Return(Some(alt))];
            return Ok((vec![IrisStmt::If { cond: c, then_b, else_b }], pos + 1));
        }
        return Ok((vec![IrisStmt::Return(Some(convert_expr(rest)))], pos + 1));
    }
    if let Some(rest) = line.strip_prefix("print(") {
        let inner = rest.trim_end_matches(')').trim();
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        if let Some((v, c, alt)) = split_ternary(inner) {
            return Ok((
                vec![IrisStmt::If {
                    cond: c,
                    then_b: vec![IrisStmt::Print(v)],
                    else_b: vec![IrisStmt::Print(alt)],
                }],
                pos + 1,
            ));
        }
        return Ok((vec![IrisStmt::Print(convert_expr(inner))], pos + 1));
    }
    if let Some(rest) = line.strip_prefix("print ") {
        return Ok((vec![IrisStmt::Print(convert_expr(rest.trim()))], pos + 1));
    }
    if let Some(rest) = line.strip_prefix("emit_signal(") {
        let inner = rest.trim_end_matches(')').trim();
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        let name = inner.split(',').next().unwrap_or("").trim().trim_matches('"');
        return Ok((vec![IrisStmt::Emit(name.to_string())], pos + 1));
    }
    if let Some(rest) = line.strip_prefix("let ") {
        return parse_let_assign(rest, ln, pos, true);
    }
    if line.starts_with("var ") {
        // Local var inside a function -> Iris let.
        let rest = line["var ".len()..].trim();
        return parse_let_assign(rest, ln, pos, true);
    }
    // Assignment (GDScript uses the same operators as Iris).
    for op in ["+=", "-=", "*=", "/=", "%=", "="] {
        if let Some((target, expr)) = split_assign_op(&line, op) {
            if op == "=" && target.trim_end().ends_with(['=', '<', '>', '!']) {
                continue;
            }
            if let Some((v, c, alt)) = split_ternary(&expr) {
                return Ok((
                    vec![IrisStmt::If {
                        cond: c,
                        then_b: vec![IrisStmt::Assign { target: target.clone(), op: op.to_string(), expr: v }],
                        else_b: vec![IrisStmt::Assign { target, op: op.to_string(), expr: alt }],
                    }],
                    pos + 1,
                ));
            }
            return Ok((vec![IrisStmt::Assign { target, op: op.to_string(), expr: convert_expr(&expr) }], pos + 1));
        }
    }
    // Explicitly unsupported (documented in GDSCRIPT_COVERAGE.md).
    for kw in ["await ", "await\t", "yield ", "yield\t", "super(", "super ."] {
        if line.starts_with(kw) || line.contains(" lambda ") || line.contains("\tlambda ") {
            return Err(GodotError::Parse(format!("line {ln}: `{}` needs coroutines/closures — unsupported (see GDSCRIPT_COVERAGE.md)", kw.trim())));
        }
    }
    if line.contains(" is ") {
        return Err(GodotError::Parse(format!("line {ln}: `is` type-test unsupported (see GDSCRIPT_COVERAGE.md)")));
    }
    // Expression statement (e.g. method call).
    Ok((vec![IrisStmt::Expr(convert_expr(&line))], pos + 1))
}

fn parse_let_assign(rest: &str, ln: usize, pos: usize, _is_let: bool) -> Result<(Vec<IrisStmt>, usize), GodotError> {
    let (name_ty, expr) = rest
        .split_once('=')
        .ok_or_else(|| GodotError::Parse(format!("line {ln}: let needs `=`")))?;
    if expr.trim_start().starts_with('=') {
        return Err(GodotError::Parse(format!("line {ln}: bad assignment")));
    }
    let mut parts = name_ty.split(':');
    let name = parts.next().unwrap_or("").trim().to_string();
    let ty = parts.next().map(|t| map_type(t));
    if name.is_empty() {
        return Err(GodotError::Parse(format!("line {ln}: let needs a name")));
    }
    if let Some((v, c, alt)) = split_ternary(expr.trim()) {
        return Ok((
            vec![IrisStmt::If {
                cond: c,
                then_b: vec![IrisStmt::Let { name: name.clone(), ty: ty.clone(), expr: v }],
                else_b: vec![IrisStmt::Let { name, ty, expr: alt }],
            }],
            pos + 1,
        ));
    }
    Ok((vec![IrisStmt::Let { name, ty, expr: convert_expr(expr.trim()) }], pos + 1))
}

fn split_assign_op(line: &str, op: &str) -> Option<(String, String)> {
    let mut search = 0;
    while let Some(rel) = line[search..].find(op) {
        let i = search + rel;
        let before = line[..i].trim_end();
        let after = line[i + op.len()..].trim();
        let ok = match op {
            "=" => {
                let b = line[..i].chars().last();
                let a = line[i + op.len()..].chars().next();
                !matches!(b, Some('=') | Some('<') | Some('>') | Some('!')) && a != Some('=')
            }
            _ => line[i + op.len()..].chars().next() != Some('='),
        };
        if ok && !before.is_empty() && !after.is_empty() && is_lvalue(before) {
            return Some((before.to_string(), after.to_string()));
        }
        search = i + op.len();
    }
    None
}

fn is_lvalue(target: &str) -> bool {
    let mut chars = target.chars().peekable();
    let mut need_ident = true;
    while chars.peek().is_some() {
        if need_ident {
            let mut n = 0;
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' {
                    chars.next();
                    n += 1;
                } else {
                    break;
                }
            }
            if n == 0 {
                return false;
            }
            need_ident = false;
        } else {
            match chars.peek() {
                Some('.') => {
                    chars.next();
                    need_ident = true;
                }
                Some('[') => {
                    let mut depth = 0;
                    let mut any = false;
                    while let Some(c) = chars.next() {
                        any = true;
                        if c == '[' {
                            depth += 1;
                        } else if c == ']' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    if depth != 0 || !any {
                        return false;
                    }
                }
                Some(c) if c.is_whitespace() => return false,
                Some(_) => return false,
                None => break,
            }
        }
    }
    !need_ident
}

/// Split top-level ternary `a if c else b` (depth-aware). Returns (a, c, b).
fn split_ternary(expr: &str) -> Option<(String, String, String)> {
    let bytes: Vec<char> = expr.chars().collect();
    let mut depth = 0;
    let mut in_str = false;
    let mut if_at: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '"' && (i == 0 || bytes[i - 1] != '\\') {
            in_str = !in_str;
        } else if !in_str {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                _ => {}
            }
            if depth == 0 && expr[i..].starts_with(" if ") && if_at.is_none() {
                if_at = Some(i);
                i += 4;
                continue;
            }
            if depth == 0 && if_at.is_some() && expr[i..].starts_with(" else ") {
                let a = expr[..if_at.unwrap()].trim().to_string();
                let cnd = expr[if_at.unwrap() + 4..i].trim().to_string();
                let b = expr[i + 6..].trim().to_string();
                if !a.is_empty() && !cnd.is_empty() && !b.is_empty() {
                    // Guard: `x if` at statement start is an if-statement, not ternary.
                    return Some((convert_expr(&a), convert_expr(&cnd), convert_expr(&b)));
                }
                return None;
            }
        }
        i += 1;
    }
    None
}

fn parse_match(lines: &[(usize, usize, String)], pos: usize, want: usize) -> Result<(Vec<IrisStmt>, usize), GodotError> {
    let (ln, _, line) = &lines[pos];
    let (ln, line) = (*ln, line.clone());
    let subject = convert_expr(line.strip_prefix("match ").unwrap_or("").trim().trim_end_matches(':').trim());
    // Collect (patterns, body) arms at want+1.
    let mut arms: Vec<(Vec<String>, Vec<IrisStmt>)> = vec![];
    let mut i = pos + 1;
    while i < lines.len() && lines[i].1 == want + 1 {
        let (aln, _, aline) = &lines[i];
        let (aln, aline) = (*aln, aline.clone());
        let pat_line = aline.trim().trim_end_matches(':').trim();
        let (pats, body);
        if pat_line == "_" || pat_line == "else" {
            let (b, nx) = parse_block(lines, i + 1, want + 2)?;
            arms.push((vec!["_".to_string()], b));
            i = nx;
            continue;
        }
        // `1, 2:` or single literal/expr.
        let list: Vec<String> = pat_line.split(',').map(|p| p.trim().to_string()).collect();
        if list.iter().any(|p| p.is_empty() || p == "_" || p.contains(' ')) {
            return Err(GodotError::Parse(format!("line {aln}: only literal match patterns (or `_`) are supported")));
        }
        pats = list;
        let (b, nx) = parse_block(lines, i + 1, want + 2)?;
        body = b;
        arms.push((pats, body));
        i = nx;
        let _ = aln;
    }
    if arms.is_empty() {
        return Err(GodotError::Parse(format!("line {ln}: match needs at least one arm")));
    }
    // Desugar to if/elif chain on a temp.
    let tmp = format!("__match{}", ln);
    let mut out: Vec<IrisStmt> = vec![IrisStmt::Let { name: tmp.clone(), ty: None, expr: subject }];
    // Build from the last arm backwards.
    let mut else_b: Vec<IrisStmt> = vec![];
    for (pats, body) in arms.into_iter().rev() {
        if pats.len() == 1 && pats[0] == "_" {
            else_b = body;
            continue;
        }
        let cond = pats.into_iter().map(|p| format!("{tmp} == {p}")).collect::<Vec<_>>().join(" or ");
        else_b = vec![IrisStmt::If { cond, then_b: body, else_b: std::mem::take(&mut else_b) }];
    }
    out.extend(else_b);
    Ok((out, i))
}

fn parse_block(lines: &[(usize, usize, String)], mut pos: usize, want: usize) -> Result<(Vec<IrisStmt>, usize), GodotError> {
    let mut out = vec![];
    let mut any = false;
    while pos < lines.len() && lines[pos].1 >= want {
        if lines[pos].1 > want {
            let (ln, _, _) = &lines[pos];
            return Err(GodotError::Parse(format!("line {ln}: unexpected indent")));
        }
        let (stmts, next) = parse_stmt(lines, pos, want)?;
        any = true;
        out.extend(stmts);
        pos = next;
    }
    if !any {
        let ln = lines.get(pos).map(|l| l.0).unwrap_or(0);
        return Err(GodotError::Parse(format!("line {ln}: expected an indented block")));
    }
    Ok((out, pos))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic_script_transpiles_and_runs() {
        let src = "extends Node2D\nclass_name Mover\n@export var speed: float = 200.0\n\nfunc _process(dt: float) -> void:\n\tposition.x += speed * dt\n\tif position.x > 500:\n\t\tposition.x = 0\n";
        let f = transpile(src).expect("transpile");
        assert_eq!(f.node_class, "Mover");
        assert_eq!(f.base_class, "Node2D");
        assert_eq!(f.props.len(), 1);
        assert_eq!(f.props[0].ty, "float");
        // Round-trips through the Iris parser and runs.
        let text = emit_iris(&f);
        let g = crate::parse(&text).expect("reparse");
        assert_eq!(g.fns.len(), 1);
        let mut interp = crate::Interpreter::new();
        interp.vars.insert("position".into(), crate::Value::Vec3(490.0, 0.0, 0.0));
        interp.vars.insert("speed".into(), crate::Value::Float(200.0));
        interp.vars.insert("dt".into(), crate::Value::Float(0.5));
        interp.run(&g.fns[0].body);
        // 490 + 100 = 590 > 500 -> reset to 0.
        assert_eq!(interp.vars.get("position"), Some(&crate::Value::Vec3(0.0, 0.0, 0.0)));
    }
    #[test]
    fn match_and_ternary_desugar() {
        let src = "extends Node\nfunc f(x: int) -> void:\n\tmatch x:\n\t\t1:\n\t\t\tprint one\n\t\t2, 3:\n\t\t\tprint few\n\t\t_:\n\t\t\tprint many\n";
        let f = transpile(src).expect("transpile");
        assert_eq!(f.fns.len(), 1);
        let text = emit_iris(&f);
        assert!(text.contains("if __match"));
        let src2 = "extends Node\nfunc f(a: int) -> void:\n\tlet b = 1 if a > 0 else 2\n\tprint b\n";
        let f2 = transpile(src2).expect("ternary");
        let text2 = emit_iris(&f2);
        assert!(text2.contains("if a > 0"));
    }
    #[test]
    fn for_range_and_while() {
        let src = "extends Node\nfunc f() -> void:\n\tfor i in range(3):\n\t\tprint i\n\twhile false:\n\t\tprint x\n";
        let f = transpile(src).expect("transpile");
        assert_eq!(f.fns[0].body.len(), 2);
    }
    #[test]
    fn rejects_await_and_bad_indent() {
        let src = "extends Node\nfunc f() -> void:\n\tawait ready\n";
        assert!(transpile(src).is_err());
        let src2 = "extends Node\n  func f() -> void:\n\tpass\n";
        assert!(transpile(src2).is_err());
    }
}
