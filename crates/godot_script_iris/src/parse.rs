//! Iris parser v1 — brace-structured with nested `if/while/for`, compound
//! assignment (`= += -= *= /= %=`), `else` chains, `emit`. Grammar in
//! docs/IRIS_SPEC.md. Errors carry line numbers for `iris check`.

use crate::ast::*;
use godot_core::GodotError;

pub fn parse(src: &str) -> Result<IrisFile, GodotError> {
    let lines: Vec<(usize, String)> = src
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, strip_comment(l).trim().to_owned()))
        .filter(|(_, l)| !l.is_empty())
        .collect();
    let mut pos = 0;
    let mut file = IrisFile {
        node_class: "MyNode".into(),
        base_class: "Node".into(),
        props: vec![],
        fns: vec![],
    };
    // Optional `node Name : Base {` header opens the file body; its closing
    // `}` terminates parsing (the brace is structural, like GDScript indent).
    let mut in_node_block = false;
    if pos < lines.len() {
        if let Some(rest) = lines[pos].1.strip_prefix("node ") {
            let (_ln, line) = &lines[pos];
            let rest = rest.trim().trim_end_matches('{').trim();
            let mut parts = rest.split(':');
            file.node_class = parts.next().unwrap_or("MyNode").trim().to_string();
            file.base_class = parts.next().unwrap_or("Node").trim().to_string();
            in_node_block = line.trim_end().ends_with('{');
            pos += 1;
        }
    }
    while pos < lines.len() {
        let (ln, line) = &lines[pos];
        if line == "}" {
            if in_node_block {
                pos += 1; // node close: must be the last line
                if pos < lines.len() {
                    let (eln, eline) = &lines[pos];
                    return Err(GodotError::Parse(format!("line {eln}: expected end of file after node `}}`, got `{eline}`")));
                }
                break;
            }
            return Err(GodotError::Parse(format!("line {ln}: unmatched `}}`")));
        }
        if let Some(rest) = line.strip_prefix("prop ") {
            let (name_ty, default) = rest
                .split_once('=')
                .ok_or_else(|| GodotError::Parse(format!("line {ln}: prop needs `= default`")))?;
            let mut nt = name_ty.split(':');
            file.props.push(Prop {
                name: nt.next().unwrap_or("").trim().to_string(),
                ty: nt.next().unwrap_or("variant").trim().to_string(),
                default: default.trim().to_string(),
            });
            pos += 1;
        } else if let Some(rest) = line.strip_prefix("fn ") {
            let rest = rest.trim().trim_end_matches('{').trim();
            let name = rest.split('(').next().unwrap_or("main").trim().to_string();
            let args = parse_arg_list(rest);
            expect_open_brace(line, *ln)?;
            pos += 1;
            let (body, next) = parse_block(&lines, pos)?;
            pos = next;
            file.fns.push(Func { name, args, body });
        } else if line == "}" {
            return Err(GodotError::Parse(format!("line {ln}: unmatched `}}`")));
        } else {
            return Err(GodotError::Parse(format!("line {ln}: expected `node`, `prop` or `fn`, got `{line}`")));
        }
    }
    Ok(file)
}

fn strip_comment(line: &str) -> &str {
    // `#` starts a comment unless inside a string literal.
    let mut in_str = false;
    for (i, c) in line.char_indices() {
        if c == '"' {
            in_str = !in_str;
        } else if c == '#' && !in_str {
            return line[..i].trim_end();
        }
    }
    line
}

fn parse_arg_list(rest: &str) -> Vec<(String, String)> {
    let (Some(l), Some(r)) = (rest.find('('), rest.find(')')) else { return vec![] };
    if r <= l {
        return vec![];
    }
    rest[l + 1..r]
        .split(',')
        .filter_map(|a| {
            let a = a.trim();
            if a.is_empty() {
                return None;
            }
            let mut parts = a.split(':');
            Some((parts.next().unwrap_or("").trim().to_string(), parts.next().unwrap_or("variant").trim().to_string()))
        })
        .collect()
}

fn expect_open_brace(line: &str, ln: usize) -> Result<(), GodotError> {
    if line.trim_end().ends_with('{') {
        Ok(())
    } else {
        Err(GodotError::Parse(format!("line {ln}: expected `{{` at end of block opener")))
    }
}

/// Parse statements until the matching `}` (consuming it). Returns (stmts, next_pos).
fn parse_block(lines: &[(usize, String)], mut pos: usize) -> Result<(Vec<IrisStmt>, usize), GodotError> {
    let mut out = vec![];
    while pos < lines.len() {
        let (ln, line) = &lines[pos];
        if line == "}" {
            return Ok((out, pos + 1));
        }
        if let Some(rest) = line.strip_prefix("if ") {
            let cond = rest.trim().trim_end_matches('{').trim().to_string();
            expect_open_brace(line, *ln)?;
            pos += 1;
            let (then_b, next) = parse_block(lines, pos)?;
            pos = next;
            // Optional `else {` / `else if ... {` right after.
            let mut else_b = vec![];
            if pos < lines.len() && lines[pos].1.starts_with("else") {
                let eline = lines[pos].1.clone();
                let eln = lines[pos].0;
                if eline.starts_with("else if ") {
                    // Desugar: else { if ... }.
                    let cond2 = eline["else if ".len()..].trim().trim_end_matches('{').trim().to_string();
                    expect_open_brace(&eline, eln)?;
                    let (inner_then, next2) = parse_block(lines, pos + 1)?;
                    let mut inner_else = vec![];
                    if next2 < lines.len() && lines[next2].1.starts_with("else") {
                        let (eb, n3) = parse_else_block(lines, next2)?;
                        inner_else = eb;
                        pos = n3;
                    } else {
                        pos = next2;
                    }
                    else_b = vec![IrisStmt::If { cond: cond2, then_b: inner_then, else_b: inner_else }];
                } else {
                    let (eb, n2) = parse_else_block(lines, pos)?;
                    else_b = eb;
                    pos = n2;
                }
            }
            out.push(IrisStmt::If { cond, then_b, else_b });
            continue;
        }
        if let Some(rest) = line.strip_prefix("while ") {
            let cond = rest.trim().trim_end_matches('{').trim().to_string();
            expect_open_brace(line, *ln)?;
            let (body, next) = parse_block(lines, pos + 1)?;
            out.push(IrisStmt::While { cond, body });
            pos = next;
            continue;
        }
        if let Some(rest) = line.strip_prefix("for ") {
            // `for i in <expr> {`
            let rest = rest.trim().trim_end_matches('{').trim();
            let (var, iter) = rest
                .split_once(" in ")
                .ok_or_else(|| GodotError::Parse(format!("line {ln}: for needs `for var in <expr> {{`")))?;
            expect_open_brace(line, *ln)?;
            let (body, next) = parse_block(lines, pos + 1)?;
            out.push(IrisStmt::For { var: var.trim().to_string(), iter: iter.trim().to_string(), body });
            pos = next;
            continue;
        }
        if line.starts_with("let ") {
            let rest = line["let ".len()..].trim();
            // `let name [: type] = expr`
            let (name_ty, expr) = rest
                .split_once('=')
                .ok_or_else(|| GodotError::Parse(format!("line {ln}: let needs `=`")))?;
            let mut nt = name_ty.split(':');
            let name = nt.next().unwrap_or("").trim().to_string();
            let ty = nt.next().map(|t| t.trim().to_string());
            if name.is_empty() {
                return Err(GodotError::Parse(format!("line {ln}: let needs a name")));
            }
            out.push(IrisStmt::Let { name, ty, expr: expr.trim().to_string() });
            pos += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("print ") {
            out.push(IrisStmt::Print(rest.trim().to_string()));
            pos += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("emit ") {
            out.push(IrisStmt::Emit(rest.trim().to_string()));
            pos += 1;
            continue;
        }
        if line == "return" || line.starts_with("return ") {
            let expr = line["return".len()..].trim();
            out.push(IrisStmt::Return(if expr.is_empty() { None } else { Some(expr.to_string()) }));
            pos += 1;
            continue;
        }
        if let Some((target, op, expr)) = split_assignment(line) {
            out.push(IrisStmt::Assign { target, op, expr });
            pos += 1;
            continue;
        }
        // Bare expression statement (e.g. a call like `queue_free()`).
        out.push(IrisStmt::Expr(line.clone()));
        pos += 1;
    }
    Err(GodotError::Parse("unexpected end of file: missing `}`".into()))
}

fn parse_else_block(lines: &[(usize, String)], pos: usize) -> Result<(Vec<IrisStmt>, usize), GodotError> {
    let (ln, line) = &lines[pos];
    let rest = line.strip_prefix("else").unwrap_or("").trim();
    let rest = rest.trim_end_matches('{').trim();
    if !rest.is_empty() {
        return Err(GodotError::Parse(format!("line {ln}: expected `else {{`, got `else {rest}`")));
    }
    expect_open_brace(line, *ln)?;
    parse_block(lines, pos + 1)
}

/// Split `target <op> expr` for `= += -= *= /= %=` (not `==`/`<=`/`>=`/`!=`).
fn split_assignment(line: &str) -> Option<(String, String, String)> {
    for op in ["+=", "-=", "*=", "/=", "%=", "="] {
        let mut search = 0;
        while let Some(rel) = line[search..].find(op) {
            let i = search + rel;
            let before = line[..i].chars().last();
            let after = line[i + op.len()..].chars().next();
            let ok = match op {
                "=" => {
                    // Exclude ==, <=, >=, !=.
                    !matches!(before, Some('=') | Some('<') | Some('>') | Some('!'))
                        && after != Some('=')
                }
                _ => after != Some('='),
            };
            if ok {
                let target = line[..i].trim();
                let expr = line[i + op.len()..].trim();
                if !target.is_empty() && !expr.is_empty() && is_lvalue(target) {
                    return Some((target.to_string(), op.to_string(), expr.to_string()));
                }
            }
            search = i + op.len();
        }
    }
    None
}

fn is_lvalue(target: &str) -> bool {
    // identifier with optional `.attr` / `[expr]` suffixes, no spaces/operators.
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
                    // Balanced bracket (no nesting check beyond depth).
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
