//! NEAR Python — a tiny Python-subset runtime for NEAR OutLayer
//!
//! Reads a Python-like script from stdin, executes it with NEAR builtins.
//! Supports: variables, expressions, if/else, for, while, def, return,
//! try/except, list/dict literals, indexing, method calls, comparisons,
//! boolean/arithmetic operators, range(), near.view/call/block/storage,
//! json operations, http stubs, and more.

use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;

wit_bindgen::generate!({
    world: "rpc-host",
});

// ============================================================
// Value type
// ============================================================

#[derive(Clone, Debug)]
enum PyVal {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<PyVal>),
    Dict(HashMap<String, PyVal>),
}

impl PyVal {
    fn to_json(&self) -> Value {
        match self {
            PyVal::None => Value::Null,
            PyVal::Bool(b) => Value::Bool(*b),
            PyVal::Int(n) => Value::Number((*n).into()),
            PyVal::Float(f) => serde_json::Number::from_f64(*f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            PyVal::Str(s) => Value::String(s.clone()),
            PyVal::List(v) => Value::Array(v.iter().map(|x| x.to_json()).collect()),
            PyVal::Dict(m) => {
                Value::Object(m.iter().map(|(k, v)| (k.clone(), v.to_json())).collect())
            }
        }
    }

    fn from_json(v: &Value) -> PyVal {
        match v {
            Value::Null => PyVal::None,
            Value::Bool(b) => PyVal::Bool(*b),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    PyVal::Int(i)
                } else {
                    PyVal::Float(n.as_f64().unwrap_or(0.0))
                }
            }
            Value::String(s) => PyVal::Str(s.clone()),
            Value::Array(a) => PyVal::List(a.iter().map(PyVal::from_json).collect()),
            Value::Object(m) => PyVal::Dict(
                m.iter()
                    .map(|(k, v)| (k.clone(), PyVal::from_json(v)))
                    .collect(),
            ),
        }
    }

    fn to_str(&self) -> String {
        match self {
            PyVal::Str(s) => s.clone(),
            PyVal::Int(n) => n.to_string(),
            PyVal::Float(f) => f.to_string(),
            PyVal::Bool(b) => b.to_string(),
            PyVal::None => "None".to_string(),
            PyVal::List(l) => {
                let items: Vec<String> = l.iter().map(|v| v.display()).collect();
                format!("[{}]", items.join(", "))
            }
            PyVal::Dict(d) => {
                let items: Vec<String> = d.iter().map(|(k, v)| format!("{}: {}", k, v.display())).collect();
                format!("{{{}}}", items.join(", "))
            }
        }
    }

    fn display(&self) -> String {
        match self {
            PyVal::Str(s) => format!("'{}'", s),
            other => other.to_str(),
        }
    }

    fn is_truthy(&self) -> bool {
        match self {
            PyVal::Bool(b) => *b,
            PyVal::Int(n) => *n != 0,
            PyVal::Float(f) => *f != 0.0,
            PyVal::Str(s) => !s.is_empty(),
            PyVal::None => false,
            PyVal::List(l) => !l.is_empty(),
            PyVal::Dict(d) => !d.is_empty(),
        }
    }
}

// ============================================================
// Control flow for exec()
// ============================================================

#[derive(Clone, Debug)]
enum ControlFlow {
    Ok,
    Return(PyVal),
    Break,
    ContinueLoop,
}

// ============================================================
// Function definition
// ============================================================

#[derive(Clone, Debug)]
struct FuncDef {
    params: Vec<String>,
    body: Vec<Op>,
}

// ============================================================
// Expression types
// ============================================================

#[derive(Clone, Debug)]
enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or,
}

#[derive(Clone, Debug)]
enum UnaryOp {
    Not,
    Neg,
}

#[derive(Clone, Debug)]
enum Expr {
    Lit(PyVal),
    Var(String),
    BinOp(Box<Expr>, BinOp, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    // NEAR builtins
    NearView(Box<Expr>, Box<Expr>, Box<Expr>),
    NearCall(Vec<Expr>), // 7 or 8 args
    NearBlock(Box<Expr>),
    NearBlockHeight,
    NearViewAccount(Box<Expr>),
    // Storage (in-memory)
    StorageGet(Box<Expr>),
    StoragePut(Box<Expr>, Box<Expr>),
    // JSON
    JsonDumps(Box<Expr>),
    JsonLoads(Box<Expr>),
    // Builtins
    Len(Box<Expr>),
    Range(Vec<Expr>),       // 1, 2, or 3 args
    Int_(Box<Expr>),        // int() conversion
    Str_(Box<Expr>),        // str() conversion
    TypeOf(Box<Expr>),      // type() - returns type name
    // Access
    GetAttr(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    // Method call: obj.method(args)
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    // User function call
    FuncCall(String, Vec<Expr>),
    // f-string
    Concat(Vec<Expr>),
    // HTTP stubs
    HttpGet(Box<Expr>),
    HttpPost(Box<Expr>, Box<Expr>),
    // String concatenation via +
    // List literal
    ListLiteral(Vec<Expr>),
    DictLiteralExpr(Vec<Expr>, Vec<Expr>), // keys, values
}

// ============================================================
// Statement types
// ============================================================

#[derive(Clone, Debug)]
enum Op {
    Assign(String, Expr),
    Print(Expr),
    If(Expr, Vec<Op>, Vec<Op>),
    For(String, Expr, Vec<Op>),
    While(Expr, Vec<Op>),
    Def(String, Vec<String>, Vec<Op>),
    Return(Option<Expr>),
    TryExcept(Vec<Op>, Vec<Op>),
    Break,
    Continue,
    ExprStmt(Expr),  // expression as statement (for method calls etc.)
}

// ============================================================
// Helper: split respecting strings and brackets
// ============================================================

/// Find the position of `needle` in `s` at depth 0 (outside strings and brackets).
/// Searches from left (start=true) or right (start=false).
fn find_at_depth0(s: &str, needles: &[&str], from_right: bool) -> Option<(usize, usize)> {
    let chars: Vec<char> = s.chars().collect();
    let byte_pos: Vec<usize> = {
        let mut v = vec![0usize; chars.len()];
        let mut bi = 0;
        for i in 0..chars.len() {
            v[i] = bi;
            bi += chars[i].len_utf8();
        }
        v
    };

    let range: Vec<usize> = if from_right {
        (0..chars.len()).rev().collect()
    } else {
        (0..chars.len()).collect()
    };

    let mut depth = 0i32;
    let mut in_string = false;
    let mut string_char = ' ';

    for &i in &range {
        let c = chars[i];
        if in_string {
            if c == string_char && (i == 0 || chars[i - 1] != '\\') {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                in_string = true;
                string_char = c;
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {
                if depth == 0 {
                    for needle in needles {
                        // For word operators (and, or), check word boundaries
                        let needle_chars: Vec<char> = needle.chars().collect();
                        if needle_chars.iter().all(|c| c.is_alphabetic()) {
                            let end = i + needle_chars.len();
                            if end > chars.len() { continue; }
                            let before_ok = i == 0 || !chars[i - 1].is_alphanumeric() && chars[i - 1] != '_';
                            let after_ok = end >= chars.len() || !chars[end].is_alphanumeric() && chars[end] != '_';
                            if before_ok && after_ok {
                                // Check match
                                let mut matches = true;
                                for (j, &nc) in needle_chars.iter().enumerate() {
                                    if chars[i + j] != nc { matches = false; break; }
                                }
                                if matches {
                                    return Some((byte_pos[i], needle.len()));
                                }
                            }
                        } else {
                            // Symbol operators
                            if s[byte_pos[i]..].starts_with(needle) {
                                return Some((byte_pos[i], needle.len()));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Split args at comma, respecting depth
fn split_args(s: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let mut in_string = false;
    let mut string_char = ' ';

    for (i, c) in s.char_indices() {
        if in_string {
            if c == string_char && (i == 0 || s.as_bytes()[i - 1] != b'\\') {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' | '\'' => { in_string = true; string_char = c; }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(s[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    if start < s.len() {
        args.push(s[start..].trim());
    }
    args
}

/// Find the matching `[` for a `]` at the end of s
fn find_matching_bracket(s: &str) -> Option<usize> {
    if !s.ends_with(']') { return None; }
    let chars: Vec<char> = s.chars().collect();
    let byte_pos: Vec<usize> = {
        let mut v = vec![0usize; chars.len()];
        let mut bi = 0;
        for i in 0..chars.len() {
            v[i] = bi;
            bi += chars[i].len_utf8();
        }
        v
    };
    let mut depth = 0i32;
    let mut in_string = false;
    let mut string_char = ' ';
    for i in (0..chars.len()).rev() {
        let c = chars[i];
        if in_string {
            if c == string_char { in_string = false; }
            continue;
        }
        match c {
            '"' | '\'' => { in_string = true; string_char = c; }
            ']' => depth += 1,
            '[' => {
                depth -= 1;
                if depth == 0 {
                    return Some(byte_pos[i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse contents of a list literal: "1, 2, 3" -> vec of exprs
fn parse_list_contents(s: &str) -> Vec<Expr> {
    if s.trim().is_empty() { return vec![]; }
    split_args(s).iter().map(|a| Parser::parse_expr(a)).collect()
}

/// Parse contents of a dict literal: '"a": 1, "b": 2' -> vec of (key, val) expr pairs
fn parse_dict_contents(s: &str) -> Vec<(Expr, Expr)> {
    if s.trim().is_empty() { return vec![]; }
    let args = split_args(s);
    let mut pairs = Vec::new();
    for arg in &args {
        // Find colon at depth 0
        if let Some(colon_pos) = find_colon(arg) {
            let key = &arg[..colon_pos];
            let val = &arg[colon_pos + 1..];
            pairs.push((Parser::parse_expr(key), Parser::parse_expr(val)));
        }
    }
    pairs
}

fn find_colon(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut string_char = ' ';
    for (i, c) in s.char_indices() {
        if in_string {
            if c == string_char { in_string = false; }
            continue;
        }
        match c {
            '"' | '\'' => { in_string = true; string_char = c; }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

// ============================================================
// f-string parser
// ============================================================

fn parse_fstring(content: &str) -> Expr {
    let mut parts = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    let mut literal = String::new();
    while i < chars.len() {
        if chars[i] == '{' && i + 1 < chars.len() {
            if !literal.is_empty() {
                parts.push(Expr::Lit(PyVal::Str(literal.clone())));
                literal.clear();
            }
            let mut end = i + 1;
            while end < chars.len() && chars[end] != '}' {
                end += 1;
            }
            let var_name: String = chars[i + 1..end].iter().collect();
            parts.push(Parser::parse_expr(var_name.trim()));
            i = end + 1;
        } else {
            literal.push(chars[i]);
            i += 1;
        }
    }
    if !literal.is_empty() {
        parts.push(Expr::Lit(PyVal::Str(literal)));
    }
    if parts.len() == 1 {
        parts.pop().unwrap()
    } else {
        Expr::Concat(parts)
    }
}

// ============================================================
// Parser
// ============================================================

struct Parser;

impl Parser {
    fn parse(source: &str) -> Vec<Op> {
        let lines: Vec<&str> = source.lines().collect();
        Self::parse_block(&lines, 0, lines.len(), 0)
    }

    fn indent_level(line: &str) -> usize {
        line.chars().take_while(|c| *c == ' ').count() / 4
    }

    fn parse_block(lines: &[&str], start: usize, end: usize, base_indent: usize) -> Vec<Op> {
        let mut ops = Vec::new();
        let mut i = start;
        while i < end {
            let line = lines[i];
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }
            let indent = Self::indent_level(line);
            if indent < base_indent {
                break;
            }

            // def name(params):
            if trimmed.starts_with("def ") && trimmed.ends_with(':') {
                let def_str = trimmed.strip_prefix("def ").unwrap().strip_suffix(':').unwrap();
                let (name, params) = if let Some(popen) = def_str.find('(') {
                    let n = def_str[..popen].trim().to_string();
                    let p: Vec<String> = if def_str.ends_with(')') {
                        let inner = &def_str[popen + 1..def_str.len() - 1];
                        if inner.trim().is_empty() {
                            vec![]
                        } else {
                            inner.split(',').map(|s| s.trim().to_string()).collect()
                        }
                    } else {
                        vec![]
                    };
                    (n, p)
                } else {
                    (def_str.trim().to_string(), vec![])
                };
                let block_start = i + 1;
                let block_end = Self::find_block_end(lines, block_start, end, base_indent + 1);
                let body = Self::parse_block(lines, block_start, block_end, base_indent + 1);
                ops.push(Op::Def(name, params, body));
                i = block_end;
                continue;
            }

            // while condition:
            if trimmed.starts_with("while ") && trimmed.ends_with(':') {
                let cond_str = trimmed[6..trimmed.len() - 1].trim();
                let cond = Self::parse_expr(cond_str);
                let block_start = i + 1;
                let block_end = Self::find_block_end(lines, block_start, end, base_indent + 1);
                let body = Self::parse_block(lines, block_start, block_end, base_indent + 1);
                ops.push(Op::While(cond, body));
                i = block_end;
                continue;
            }

            // if expr:
            if trimmed.starts_with("if ") && trimmed.ends_with(':') {
                let cond_str = trimmed[3..trimmed.len() - 1].trim();
                let cond = Self::parse_expr(cond_str);
                let block_start = i + 1;

                let (if_body, else_body, next_i) =
                    Self::parse_if_block(lines, block_start, end, base_indent);
                ops.push(Op::If(cond, if_body, else_body));
                i = next_i;
                continue;
            }

            // elif expr: (treated as else: if expr:)
            // handled inside parse_if_block

            // for var in expr:
            if trimmed.starts_with("for ") && trimmed.ends_with(':') {
                let for_str = trimmed[4..trimmed.len() - 1].trim();
                let parts: Vec<&str> = for_str.splitn(2, " in ").collect();
                if parts.len() == 2 {
                    let var = parts[0].trim().to_string();
                    let iter_expr = Self::parse_expr(parts[1].trim());
                    let block_start = i + 1;
                    let block_end = Self::find_block_end(lines, block_start, end, base_indent + 1);
                    let body = Self::parse_block(lines, block_start, block_end, base_indent + 1);
                    ops.push(Op::For(var, iter_expr, body));
                    i = block_end;
                    continue;
                }
            }

            // try:
            if trimmed == "try:" {
                let block_start = i + 1;
                let mut except_start = None;
                let mut block_end = block_start;
                while block_end < end {
                    let l = lines[block_end].trim();
                    if l.is_empty() { block_end += 1; continue; }
                    let ind = Self::indent_level(lines[block_end]);
                    if ind < base_indent + 1 { break; }
                    if ind == base_indent && l.starts_with("except") {
                        except_start = Some(block_end);
                        break;
                    }
                    block_end += 1;
                }
                let try_body = Self::parse_block(lines, block_start, block_end, base_indent + 1);
                let except_body = if let Some(es) = except_start {
                    let eblock_start = es + 1;
                    let eblock_end = Self::find_block_end(lines, eblock_start, end, base_indent + 1);
                    let body = Self::parse_block(lines, eblock_start, eblock_end, base_indent + 1);
                    i = eblock_end;
                    body
                } else {
                    i = block_end;
                    vec![]
                };
                ops.push(Op::TryExcept(try_body, except_body));
                continue;
            }

            // return
            if trimmed.starts_with("return") {
                let val = if trimmed.len() > 6 && trimmed.as_bytes()[6] == b' ' {
                    Some(Self::parse_expr(&trimmed[7..]))
                } else if trimmed == "return" {
                    None
                } else {
                    None
                };
                ops.push(Op::Return(val));
                i += 1;
                continue;
            }

            // break / continue
            if trimmed == "break" {
                ops.push(Op::Break);
                i += 1;
                continue;
            }
            if trimmed == "continue" {
                ops.push(Op::Continue);
                i += 1;
                continue;
            }

            // print(expr)
            if trimmed.starts_with("print(") && trimmed.ends_with(')') {
                let inner = &trimmed[6..trimmed.len() - 1];
                ops.push(Op::Print(Self::parse_expr(inner)));
                i += 1;
                continue;
            }

            // variable assignment: var = expr
            // Check for = that's not ==, !=, <=, >=
            if let Some(eq_pos) = Self::find_assignment(trimmed) {
                let var = trimmed[..eq_pos].trim().to_string();
                // Handle compound assignment: +=, -=
                let rest = &trimmed[eq_pos + 1..];
                if rest.starts_with('=') {
                    // ==
                    // Not an assignment, fall through
                } else {
                    let expr_str = rest.trim();
                    if !var.contains('(') && !var.contains('[') && !var.contains('.') {
                        ops.push(Op::Assign(var, Self::parse_expr(expr_str)));
                        i += 1;
                        continue;
                    }
                }
            }

            // Handle augmented assignment: var += expr, var -= expr
            if let Some(pos) = Self::find_augmented_assign(trimmed) {
                let var = trimmed[..pos].trim().to_string();
                let op_char = trimmed.as_bytes()[pos];
                let expr_str = trimmed[pos + 2..].trim();
                if !var.contains('(') && !var.contains('[') && !var.contains('.') {
                    let op = match op_char {
                        b'+' => BinOp::Add,
                        b'-' => BinOp::Sub,
                        b'*' => BinOp::Mul,
                        b'/' => BinOp::Div,
                        _ => BinOp::Add,
                    };
                    let aug_expr = Expr::BinOp(
                        Box::new(Expr::Var(var.clone())),
                        op,
                        Box::new(Self::parse_expr(expr_str)),
                    );
                    ops.push(Op::Assign(var, aug_expr));
                    i += 1;
                    continue;
                }
            }

            // Expression statement (e.g., list.append(x), near.storage.put(...))
            ops.push(Op::ExprStmt(Self::parse_expr(trimmed)));
            i += 1;
        }
        ops
    }

    fn find_assignment(s: &str) -> Option<usize> {
        let bytes = s.as_bytes();
        let mut in_string = false;
        let mut string_char = b' ';
        let mut depth = 0i32;
        for i in 0..bytes.len() {
            let c = bytes[i];
            if in_string {
                if c == string_char && (i == 0 || bytes[i - 1] != b'\\') {
                    in_string = false;
                }
                continue;
            }
            match c {
                b'"' | b'\'' => { in_string = true; string_char = c; }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b'=' if depth == 0 => {
                    // Make sure it's not ==, !=, <=, >=
                    if i + 1 < bytes.len() && bytes[i + 1] == b'=' { continue; }
                    if i > 0 && (bytes[i - 1] == b'!' || bytes[i - 1] == b'<' || bytes[i - 1] == b'>') { continue; }
                    // Make sure it's not +=, -=, etc.
                    if i > 0 && (bytes[i - 1] == b'+' || bytes[i - 1] == b'-' || bytes[i - 1] == b'*' || bytes[i - 1] == b'/') {
                        continue;
                    }
                    return Some(i);
                }
                _ => {}
            }
        }
        None
    }

    fn find_augmented_assign(s: &str) -> Option<usize> {
        let bytes = s.as_bytes();
        let mut in_string = false;
        let mut string_char = b' ';
        let mut depth = 0i32;
        for i in 0..bytes.len().saturating_sub(1) {
            let c = bytes[i];
            if in_string {
                if c == string_char && (i == 0 || bytes[i - 1] != b'\\') { in_string = false; }
                continue;
            }
            match c {
                b'"' | b'\'' => { in_string = true; string_char = c; }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                _ => {
                    if depth == 0 && (c == b'+' || c == b'-' || c == b'*' || c == b'/') {
                        if bytes[i + 1] == b'=' {
                            return Some(i);
                        }
                    }
                }
            }
        }
        None
    }

    fn find_block_end(lines: &[&str], start: usize, end: usize, target_indent: usize) -> usize {
        let mut i = start;
        while i < end {
            let l = lines[i].trim();
            if l.is_empty() { i += 1; continue; }
            if Self::indent_level(lines[i]) < target_indent { break; }
            i += 1;
        }
        i
    }

    fn parse_if_block(
        lines: &[&str], block_start: usize, end: usize, base_indent: usize,
    ) -> (Vec<Op>, Vec<Op>, usize) {
        // Parse the if body
        let mut block_end = block_start;
        let _else_line: Option<usize> = None;
        while block_end < end {
            let l = lines[block_end].trim();
            if l.is_empty() { block_end += 1; continue; }
            let ind = Self::indent_level(lines[block_end]);
            if ind < base_indent + 1 { break; }
            block_end += 1;
        }
        let if_body = Self::parse_block(lines, block_start, block_end, base_indent + 1);

        // Check for elif / else at base_indent
        let mut else_body = Vec::new();
        let mut next_i = block_end;

        if block_end < end {
            let l = lines[block_end].trim();
            let ind = Self::indent_level(lines[block_end]);
            if ind == base_indent {
                if l.starts_with("elif ") && l.ends_with(':') {
                    // elif -> treat as else: if ...
                    let elif_cond_str = l[5..l.len() - 1].trim();
                    let elif_cond = Self::parse_expr(elif_cond_str);
                    let (elif_body, elif_else, elif_next) =
                        Self::parse_if_block(lines, block_end + 1, end, base_indent);
                    let mut elif_ops = vec![Op::If(elif_cond, elif_body, elif_else)];
                    else_body.append(&mut elif_ops);
                    next_i = elif_next;
                } else if l.starts_with("else") && l.ends_with(':') {
                    let eblock_start = block_end + 1;
                    let eblock_end = Self::find_block_end(lines, eblock_start, end, base_indent + 1);
                    else_body = Self::parse_block(lines, eblock_start, eblock_end, base_indent + 1);
                    next_i = eblock_end;
                }
            }
        }

        (if_body, else_body, next_i)
    }

    // ============================================================
    // Expression parser with precedence
    // ============================================================

    fn parse_expr(s: &str) -> Expr {
        Self::parse_or(s.trim())
    }

    fn parse_or(s: &str) -> Expr {
        if let Some((pos, len)) = find_at_depth0(s, &[" or "], true) {
            let left = &s[..pos];
            let right = &s[pos + len..];
            return Expr::BinOp(
                Box::new(Self::parse_or(left)),
                BinOp::Or,
                Box::new(Self::parse_and(right)),
            );
        }
        Self::parse_and(s)
    }

    fn parse_and(s: &str) -> Expr {
        if let Some((pos, len)) = find_at_depth0(s, &[" and "], true) {
            let left = &s[..pos];
            let right = &s[pos + len..];
            return Expr::BinOp(
                Box::new(Self::parse_and(left)),
                BinOp::And,
                Box::new(Self::parse_not(right)),
            );
        }
        Self::parse_not(s)
    }

    fn parse_not(s: &str) -> Expr {
        let s = s.trim();
        if s.starts_with("not ") {
            return Expr::UnaryOp(UnaryOp::Not, Box::new(Self::parse_not(&s[4..])));
        }
        Self::parse_comparison(s)
    }

    fn parse_comparison(s: &str) -> Expr {
        // Search for comparison operators at depth 0
        let ops = ["==", "!=", ">=", "<=", ">", "<"];
        for op in &ops {
            if let Some((pos, len)) = find_at_depth0(s, &[*op], false) {
                // Make sure we're not inside an assignment (= without ==)
                let left = s[..pos].trim();
                let right = s[pos + len..].trim();
                if left.is_empty() || right.is_empty() { continue; }
                let binop = match *op {
                    "==" => BinOp::Eq,
                    "!=" => BinOp::Ne,
                    ">=" => BinOp::Ge,
                    "<=" => BinOp::Le,
                    ">" => BinOp::Gt,
                    "<" => BinOp::Lt,
                    _ => continue,
                };
                return Expr::BinOp(
                    Box::new(Self::parse_addition(left)),
                    binop,
                    Box::new(Self::parse_addition(right)),
                );
            }
        }
        Self::parse_addition(s)
    }

    fn parse_addition(s: &str) -> Expr {
        // Find last + or - at depth 0 (for left-associativity)
        if let Some((pos, len)) = find_at_depth0(s, &["+", "-"], true) {
            let left = s[..pos].trim();
            let right = s[pos + len..].trim();
            if left.is_empty() {
                // Unary minus/plus, fall through
                return Self::parse_multiplication(s);
            }
            let op_char = &s[pos..pos + len];
            let binop = match op_char {
                "+" => BinOp::Add,
                "-" => BinOp::Sub,
                _ => unreachable!(),
            };
            return Expr::BinOp(
                Box::new(Self::parse_addition(left)),
                binop,
                Box::new(Self::parse_multiplication(right)),
            );
        }
        Self::parse_multiplication(s)
    }

    fn parse_multiplication(s: &str) -> Expr {
        if let Some((pos, len)) = find_at_depth0(s, &["*", "/", "%"], true) {
            let left = s[..pos].trim();
            let right = s[pos + len..].trim();
            if left.is_empty() { return Self::parse_unary(s); }
            let op_char = &s[pos..pos + len];
            let binop = match op_char {
                "*" => BinOp::Mul,
                "/" => BinOp::Div,
                "%" => BinOp::Mod,
                _ => unreachable!(),
            };
            return Expr::BinOp(
                Box::new(Self::parse_multiplication(left)),
                binop,
                Box::new(Self::parse_unary(right)),
            );
        }
        Self::parse_unary(s)
    }

    fn parse_unary(s: &str) -> Expr {
        let s = s.trim();
        if s.starts_with('-') && s.len() > 1 {
            // Check if it's unary minus (not a subtraction)
            return Expr::UnaryOp(UnaryOp::Neg, Box::new(Self::parse_unary(&s[1..])));
        }
        Self::parse_postfix(s)
    }

    fn parse_postfix(s: &str) -> Expr {
        let s = s.trim();

        // Check for index access at end: expr[key]
        if s.ends_with(']') {
            if let Some(pos) = find_matching_bracket(s) {
                if pos > 0 {
                    let base = &s[..pos];
                    let key = &s[pos + 1..s.len() - 1];
                    return Expr::Index(
                        Box::new(Self::parse_postfix(base)),
                        Box::new(Self::parse_expr(key)),
                    );
                }
            }
        }

        // Check for method call: expr.method(args)
        // Look for pattern: ... . name ( ... ) at the end
        if s.ends_with(')') {
            // Find the matching (
            if let Some(paren_pos) = find_matching_paren(s) {
                if paren_pos > 0 {
                    let before_paren = &s[..paren_pos];
                    // Check if before_paren ends with .identifier
                    if let Some(dot_pos) = before_paren.rfind('.') {
                        let obj_str = &before_paren[..dot_pos];
                        let method = &before_paren[dot_pos + 1..];
                        // Make sure method is a valid identifier
                        if method.chars().all(|c| c.is_alphanumeric() || c == '_') && !method.is_empty() {
                            let args_str = &s[paren_pos + 1..s.len() - 1];

                            // Check for known module.function patterns first
                            let full_name = format!("{}.{}", obj_str, method);
                            let is_known = [
                                "near.view", "near.call", "near.block",
                                "near.block_height",
                                "near.view_account", "near.view_access_key",
                                "near.storage.get", "near.storage.put",
                                "json.dumps", "json.loads",
                                "http.get", "http.post",
                            ].contains(&full_name.as_str());

                            if is_known {
                                return Self::parse_function_call(&full_name, args_str);
                            }

                            let args = if args_str.trim().is_empty() {
                                vec![]
                            } else {
                                split_args(args_str).iter().map(|a| Self::parse_expr(a)).collect()
                            };

                            // Check for known object methods
                            let method_s = method.to_string();
                            return Expr::MethodCall {
                                object: Box::new(Self::parse_postfix(obj_str)),
                                method: method_s,
                                args,
                            };
                        }
                    }

                    // It could be a function call: name(args)
                    let func_name = before_paren.trim();
                    if !func_name.is_empty() && func_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                        return Self::parse_function_call(func_name, &s[paren_pos + 1..s.len() - 1]);
                    }
                }
            }
        }

        // Check for attribute access: expr.attr (no parens)
        if let Some(dot_pos) = s.rfind('.') {
            let obj_str = &s[..dot_pos];
            let attr = &s[dot_pos + 1..];
            if attr.chars().all(|c| c.is_alphanumeric() || c == '_') && !attr.is_empty() {
                // Don't split if obj_str starts with a quote (it's a float like "3.14")
                if !obj_str.starts_with('"') && !obj_str.starts_with('\'') {
                    // Don't split numbers like 3.14
                    if obj_str.parse::<f64>().is_err() {
                        return Expr::GetAttr(
                            Box::new(Self::parse_postfix(obj_str)),
                            attr.to_string(),
                        );
                    }
                }
            }
        }

        Self::parse_atom(s)
    }

    fn parse_function_call(name: &str, args_str: &str) -> Expr {
        let args = if args_str.trim().is_empty() {
            vec![]
        } else {
            split_args(args_str).iter().map(|a| Self::parse_expr(a)).collect()
        };

        match name {
            "near.view" if args.len() >= 3 => Expr::NearView(
                Box::new(args[0].clone()),
                Box::new(args[1].clone()),
                Box::new(args[2].clone()),
            ),
            "near.call" => Expr::NearCall(args),
            "near.block" if args.len() >= 1 => Expr::NearBlock(Box::new(args[0].clone())),
            "near.block_height" => Expr::NearBlockHeight,
            "near.view_account" if args.len() >= 1 => Expr::NearViewAccount(Box::new(args[0].clone())),
            "near.storage.get" if args.len() >= 1 => Expr::StorageGet(Box::new(args[0].clone())),
            "near.storage.put" if args.len() >= 2 => Expr::StoragePut(
                Box::new(args[0].clone()),
                Box::new(args[1].clone()),
            ),
            "json.dumps" if args.len() >= 1 => Expr::JsonDumps(Box::new(args[0].clone())),
            "json.loads" if args.len() >= 1 => Expr::JsonLoads(Box::new(args[0].clone())),
            "len" if args.len() >= 1 => Expr::Len(Box::new(args[0].clone())),
            "range" => Expr::Range(args),
            "int" if args.len() >= 1 => Expr::Int_(Box::new(args[0].clone())),
            "str" if args.len() >= 1 => Expr::Str_(Box::new(args[0].clone())),
            "type" if args.len() >= 1 => Expr::TypeOf(Box::new(args[0].clone())),
            "http.get" if args.len() >= 1 => Expr::HttpGet(Box::new(args[0].clone())),
            "http.post" if args.len() >= 2 => Expr::HttpPost(
                Box::new(args[0].clone()),
                Box::new(args[1].clone()),
            ),
            _ => Expr::FuncCall(name.to_string(), args),
        }
    }

    fn parse_atom(s: &str) -> Expr {
        let s = s.trim();

        // near.block_height() - special no-arg builtin
        if s == "near.block_height()" {
            return Expr::NearBlockHeight;
        }

        // Boolean literals
        if s == "True" { return Expr::Lit(PyVal::Bool(true)); }
        if s == "False" { return Expr::Lit(PyVal::Bool(false)); }
        if s == "None" { return Expr::Lit(PyVal::None); }

        // List literal: [1, 2, 3]
        if s.starts_with('[') && s.ends_with(']') {
            let inner = &s[1..s.len() - 1];
            return Expr::ListLiteral(parse_list_contents(inner));
        }

        // Dict/JSON object literal: {"key": "val"} or {"key": variable}
        if s.starts_with('{') && s.ends_with('}') {
            if s == "{}" {
                return Expr::Lit(PyVal::Dict(HashMap::new()));
            }
            // First try pure JSON parse (fast path)
            if let Ok(v) = serde_json::from_str::<Value>(s) {
                return Expr::Lit(PyVal::from_json(&v));
            }
            // Fallback: parse as dict with expression values (supports variables)
            let inner = &s[1..s.len()-1];
            let pairs = parse_dict_contents(inner);
            if !pairs.is_empty() {
                // Evaluate each pair at runtime
                let keys: Vec<Expr> = pairs.iter().map(|(k, _)| k.clone()).collect();
                let vals: Vec<Expr> = pairs.iter().map(|(_, v)| v.clone()).collect();
                // Use a special approach: create a dict by evaluating pairs
                // We'll use a Concat-like approach but for dicts
                // For now, store as DictLiteral expr
                return Expr::DictLiteralExpr(keys, vals);
            }
        }

        // f-string
        if s.starts_with("f\"") || s.starts_with("f'") {
            let q = &s[1..2];
            if s.ends_with(q.chars().next().unwrap()) {
                let content = &s[2..s.len() - 1];
                return parse_fstring(content);
            }
        }

        // String literal
        if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
            || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
        {
            let inner = &s[1..s.len() - 1];
            return Expr::Lit(PyVal::Str(inner.to_string()));
        }

        // Number
        if let Ok(n) = s.parse::<i64>() {
            return Expr::Lit(PyVal::Int(n));
        }
        if let Ok(f) = s.parse::<f64>() {
            return Expr::Lit(PyVal::Float(f));
        }

        // Variable name (identifiers)
        if s.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
            && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            return Expr::Var(s.to_string());
        }

        // Fallback
        Expr::Lit(PyVal::Str(s.to_string()))
    }
}

/// Find matching ( for ) at end of string
fn find_matching_paren(s: &str) -> Option<usize> {
    if !s.ends_with(')') { return None; }
    let chars: Vec<char> = s.chars().collect();
    let byte_pos: Vec<usize> = {
        let mut v = vec![0usize; chars.len()];
        let mut bi = 0;
        for i in 0..chars.len() {
            v[i] = bi;
            bi += chars[i].len_utf8();
        }
        v
    };
    let mut depth = 0i32;
    let mut in_string = false;
    let mut string_char = ' ';
    for i in (0..chars.len()).rev() {
        let c = chars[i];
        if in_string {
            if c == string_char { in_string = false; }
            continue;
        }
        match c {
            '"' | '\'' => { in_string = true; string_char = c; }
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(byte_pos[i]);
                }
            }
            _ => {}
        }
    }
    None
}

// ============================================================
// Execution environment
// ============================================================

struct Env {
    vars: HashMap<String, PyVal>,
    funcs: HashMap<String, FuncDef>,
    storage: HashMap<String, String>,
    output: String,
}

impl Env {
    fn new() -> Self {
        Env {
            vars: HashMap::new(),
            funcs: HashMap::new(),
            storage: HashMap::new(),
            output: String::new(),
        }
    }

    fn eval(&mut self, expr: &Expr) -> PyVal {
        match expr {
            Expr::Lit(v) => v.clone(),
            Expr::Var(name) => self.vars.get(name).cloned().unwrap_or(PyVal::None),

            Expr::BinOp(left, op, right) => {
                // Short-circuit for And/Or
                match op {
                    BinOp::And => {
                        let lv = self.eval(left);
                        if !lv.is_truthy() { return PyVal::Bool(false); }
                        let rv = self.eval(right);
                        return PyVal::Bool(rv.is_truthy());
                    }
                    BinOp::Or => {
                        let lv = self.eval(left);
                        if lv.is_truthy() { return PyVal::Bool(true); }
                        let rv = self.eval(right);
                        return PyVal::Bool(rv.is_truthy());
                    }
                    _ => {}
                }
                let lv = self.eval(left);
                let rv = self.eval(right);
                match op {
                    BinOp::Add => match (&lv, &rv) {
                        (PyVal::Int(a), PyVal::Int(b)) => PyVal::Int(a + b),
                        (PyVal::Float(a), PyVal::Float(b)) => PyVal::Float(a + b),
                        (PyVal::Int(a), PyVal::Float(b)) => PyVal::Float(*a as f64 + b),
                        (PyVal::Float(a), PyVal::Int(b)) => PyVal::Float(a + *b as f64),
                        (PyVal::Str(a), PyVal::Str(b)) => PyVal::Str(format!("{}{}", a, b)),
                        (PyVal::List(a), PyVal::List(b)) => PyVal::List([&a[..], &b[..]].concat()),
                        _ => PyVal::None,
                    },
                    BinOp::Sub => match (&lv, &rv) {
                        (PyVal::Int(a), PyVal::Int(b)) => PyVal::Int(a - b),
                        (PyVal::Float(a), PyVal::Float(b)) => PyVal::Float(a - b),
                        (PyVal::Int(a), PyVal::Float(b)) => PyVal::Float(*a as f64 - b),
                        (PyVal::Float(a), PyVal::Int(b)) => PyVal::Float(a - *b as f64),
                        _ => PyVal::None,
                    },
                    BinOp::Mul => match (&lv, &rv) {
                        (PyVal::Int(a), PyVal::Int(b)) => PyVal::Int(a * b),
                        (PyVal::Float(a), PyVal::Float(b)) => PyVal::Float(a * b),
                        (PyVal::Int(a), PyVal::Float(b)) => PyVal::Float(*a as f64 * b),
                        (PyVal::Float(a), PyVal::Int(b)) => PyVal::Float(a * *b as f64),
                        (PyVal::Str(a), PyVal::Int(b)) => PyVal::Str(a.repeat(*b as usize)),
                        (PyVal::Int(a), PyVal::Str(b)) => PyVal::Str(b.repeat(*a as usize)),
                        _ => PyVal::None,
                    },
                    BinOp::Div => match (&lv, &rv) {
                        (PyVal::Int(a), PyVal::Int(b)) if *b != 0 => PyVal::Int(a / b),
                        (PyVal::Float(a), PyVal::Float(b)) if *b != 0.0 => PyVal::Float(a / b),
                        (PyVal::Int(a), PyVal::Float(b)) if *b != 0.0 => PyVal::Float(*a as f64 / b),
                        (PyVal::Float(a), PyVal::Int(b)) if *b != 0 => PyVal::Float(a / *b as f64),
                        _ => PyVal::None,
                    },
                    BinOp::Mod => match (&lv, &rv) {
                        (PyVal::Int(a), PyVal::Int(b)) if *b != 0 => PyVal::Int(a % b),
                        _ => PyVal::None,
                    },
                    BinOp::Eq => PyVal::Bool(Self::values_equal(&lv, &rv)),
                    BinOp::Ne => PyVal::Bool(!Self::values_equal(&lv, &rv)),
                    BinOp::Lt => Self::compare_values(&lv, &rv).map(|o| PyVal::Bool(o == std::cmp::Ordering::Less)).unwrap_or(PyVal::Bool(false)),
                    BinOp::Gt => Self::compare_values(&lv, &rv).map(|o| PyVal::Bool(o == std::cmp::Ordering::Greater)).unwrap_or(PyVal::Bool(false)),
                    BinOp::Le => Self::compare_values(&lv, &rv).map(|o| PyVal::Bool(o != std::cmp::Ordering::Greater)).unwrap_or(PyVal::Bool(false)),
                    BinOp::Ge => Self::compare_values(&lv, &rv).map(|o| PyVal::Bool(o != std::cmp::Ordering::Less)).unwrap_or(PyVal::Bool(false)),
                    _ => PyVal::None,
                }
            }

            Expr::UnaryOp(op, expr) => {
                let v = self.eval(expr);
                match op {
                    UnaryOp::Not => PyVal::Bool(!v.is_truthy()),
                    UnaryOp::Neg => match v {
                        PyVal::Int(n) => PyVal::Int(-n),
                        PyVal::Float(f) => PyVal::Float(-f),
                        _ => PyVal::None,
                    },
                }
            }

            Expr::NearView(contract, method, args) => {
                let c = self.eval(contract).to_str();
                let m = self.eval(method).to_str();
                let a = self.eval(args).to_str();
                let args_json = if a.starts_with('{') {
                    a.clone()
                } else if let Some(v) = self.vars.get(&a) {
                    v.to_json().to_string()
                } else {
                    a
                };
                let (result, err) = near::rpc::api::view(&c, &m, &args_json, "final");
                if !err.is_empty() {
                    eprintln!("near.view error: {}", err);
                    PyVal::None
                } else {
                    match serde_json::from_str::<Value>(&result) {
                        Ok(v) => PyVal::from_json(&v),
                        Err(_) => PyVal::Str(result),
                    }
                }
            }

            Expr::NearCall(args) => {
                let eval_args: Vec<String> = args.iter().map(|a| self.eval(a).to_str()).collect();
                let (result, err) = if eval_args.len() >= 8 {
                    near::rpc::api::call(
                        &eval_args[0], &eval_args[1], &eval_args[2], &eval_args[3],
                        &eval_args[4], &eval_args[5], &eval_args[6], &eval_args[7],
                    )
                } else if eval_args.len() >= 7 {
                    near::rpc::api::call(
                        &eval_args[0], &eval_args[1], &eval_args[2], &eval_args[3],
                        &eval_args[4], &eval_args[5], &eval_args[6], "FINAL",
                    )
                } else {
                    eprintln!("near.call needs at least 7 args");
                    return PyVal::Str("error: not enough args".to_string());
                };
                if !err.is_empty() {
                    eprintln!("near.call error: {}", err);
                    PyVal::Str(err)
                } else {
                    PyVal::Str(result)
                }
            }

            Expr::NearBlockHeight => {
                let (result, err) = near::rpc::api::block("final");
                if !err.is_empty() {
                    eprintln!("near.block error: {}", err);
                    PyVal::Int(0)
                } else {
                    match serde_json::from_str::<Value>(&result) {
                        Ok(v) => {
                            let height = v.get("result")
                                .and_then(|r| r.get("header"))
                                .and_then(|h| h.get("height"))
                                .and_then(|h| h.as_u64())
                                .unwrap_or(0) as i64;
                            PyVal::Int(height)
                        }
                        Err(_) => PyVal::Str(result),
                    }
                }
            }

            Expr::NearBlock(block_id) => {
                let bid = self.eval(block_id).to_str();
                let (result, err) = near::rpc::api::block(&bid);
                if !err.is_empty() {
                    PyVal::Str(format!("error: {}", err))
                } else {
                    match serde_json::from_str::<Value>(&result) {
                        Ok(v) => PyVal::from_json(&v),
                        Err(_) => PyVal::Str(result),
                    }
                }
            }

            Expr::NearViewAccount(account) => {
                let aid = self.eval(account).to_str();
                let (result, err) = near::rpc::api::view_account(&aid, "final");
                if !err.is_empty() {
                    eprintln!("near.view_account error: {}", err);
                    PyVal::None
                } else {
                    match serde_json::from_str::<Value>(&result) {
                        Ok(v) => PyVal::from_json(&v),
                        Err(_) => PyVal::Str(result),
                    }
                }
            }

            Expr::StorageGet(key) => {
                let k = self.eval(key).to_str();
                match self.storage.get(&k) {
                    Some(v) => {
                        match serde_json::from_str::<Value>(v) {
                            Ok(json_v) => PyVal::from_json(&json_v),
                            Err(_) => PyVal::Str(v.clone()),
                        }
                    }
                    None => PyVal::None,
                }
            }

            Expr::StoragePut(key, value) => {
                let k = self.eval(key).to_str();
                let v = self.eval(value);
                let stored = match &v {
                    PyVal::Str(s) => s.clone(),
                    _ => v.to_json().to_string(),
                };
                self.storage.insert(k, stored);
                PyVal::None
            }

            Expr::JsonDumps(expr) => {
                let v = self.eval(expr);
                PyVal::Str(v.to_json().to_string())
            }

            Expr::JsonLoads(expr) => {
                let s = self.eval(expr).to_str();
                match serde_json::from_str::<Value>(&s) {
                    Ok(v) => PyVal::from_json(&v),
                    Err(e) => {
                        eprintln!("json.loads error: {}", e);
                        PyVal::None
                    }
                }
            }

            Expr::Len(expr) => {
                let v = self.eval(expr);
                match v {
                    PyVal::List(l) => PyVal::Int(l.len() as i64),
                    PyVal::Dict(d) => PyVal::Int(d.len() as i64),
                    PyVal::Str(s) => PyVal::Int(s.len() as i64),
                    _ => PyVal::Int(0),
                }
            }

            Expr::Range(args) => {
                let nums: Vec<i64> = args.iter().map(|a| match self.eval(a) {
                    PyVal::Int(n) => n,
                    PyVal::Float(f) => f as i64,
                    _ => 0,
                }).collect();
                let items = match nums.len() {
                    1 => (0..nums[0]).map(PyVal::Int).collect(),
                    2 => (nums[0]..nums[1]).map(PyVal::Int).collect(),
                    3 => {
                        let step = nums[2];
                        let mut v = Vec::new();
                        let mut i = nums[0];
                        if step > 0 {
                            while i < nums[1] { v.push(PyVal::Int(i)); i += step; }
                        } else if step < 0 {
                            while i > nums[1] { v.push(PyVal::Int(i)); i += step; }
                        }
                        v
                    }
                    _ => vec![],
                };
                PyVal::List(items)
            }

            Expr::Int_(expr) => {
                let v = self.eval(expr);
                match v {
                    PyVal::Int(n) => PyVal::Int(n),
                    PyVal::Float(f) => PyVal::Int(f as i64),
                    PyVal::Str(s) => s.parse::<i64>().map(PyVal::Int).unwrap_or(PyVal::Int(0)),
                    PyVal::Bool(b) => PyVal::Int(if b { 1 } else { 0 }),
                    _ => PyVal::Int(0),
                }
            }

            Expr::Str_(expr) => {
                PyVal::Str(self.eval(expr).to_str())
            }

            Expr::TypeOf(expr) => {
                let v = self.eval(expr);
                let type_name = match v {
                    PyVal::None => "NoneType",
                    PyVal::Bool(_) => "bool",
                    PyVal::Int(_) => "int",
                    PyVal::Float(_) => "float",
                    PyVal::Str(_) => "str",
                    PyVal::List(_) => "list",
                    PyVal::Dict(_) => "dict",
                };
                PyVal::Str(type_name.to_string())
            }

            Expr::GetAttr(obj, key) => {
                let v = self.eval(obj);
                match &v {
                    PyVal::Dict(m) => m.get(key).cloned().unwrap_or(PyVal::None),
                    PyVal::List(l) => {
                        match key.as_str() {
                            "length" | "len" | "size" => PyVal::Int(l.len() as i64),
                            _ => PyVal::None,
                        }
                    }
                    PyVal::Str(_) => {
                        match key.as_str() {
                            "length" | "len" | "size" => PyVal::Int(v.to_str().len() as i64),
                            _ => PyVal::None,
                        }
                    }
                    _ => PyVal::None,
                }
            }

            Expr::Index(obj, key) => {
                let v = self.eval(obj);
                let k = self.eval(key);
                match (&v, &k) {
                    (PyVal::Dict(m), PyVal::Str(s)) => m.get(s).cloned().unwrap_or(PyVal::None),
                    (PyVal::List(l), PyVal::Int(i)) => {
                        let idx = if *i < 0 { (l.len() as i64 + *i) as usize } else { *i as usize };
                        l.get(idx).cloned().unwrap_or(PyVal::None)
                    }
                    (PyVal::List(l), PyVal::Str(s)) => {
                        // Try to parse string as int
                        if let Ok(i) = s.parse::<i64>() {
                            let idx = if i < 0 { (l.len() as i64 + i) as usize } else { i as usize };
                            l.get(idx).cloned().unwrap_or(PyVal::None)
                        } else {
                            PyVal::None
                        }
                    }
                    _ => PyVal::None,
                }
            }

            Expr::MethodCall { object, method, args } => {
                let obj_val = self.eval(object);
                let method_s = method.as_str();

                match (&obj_val, method_s) {
                    // str methods
                    (PyVal::Str(s), "split") => {
                        let sep = args.first().map(|a| self.eval(a).to_str()).unwrap_or_else(|| " ".to_string());
                        let maxsplit = args.get(1).and_then(|a| match self.eval(a) { PyVal::Int(n) => Some(n as usize), _ => None });
                        let parts: Vec<PyVal> = if let Some(max) = maxsplit {
                            s.splitn(max + 1, sep.as_str()).map(|p| PyVal::Str(p.to_string())).collect()
                        } else {
                            s.split(sep.as_str()).map(|p| PyVal::Str(p.to_string())).collect()
                        };
                        PyVal::List(parts)
                    }
                    (PyVal::Str(s), "replace") => {
                        let old = args.first().map(|a| self.eval(a).to_str()).unwrap_or_default();
                        let new = args.get(1).map(|a| self.eval(a).to_str()).unwrap_or_default();
                        PyVal::Str(s.replace(&old, &new))
                    }
                    (PyVal::Str(s), "strip") => PyVal::Str(s.trim().to_string()),
                    (PyVal::Str(s), "lower") => PyVal::Str(s.to_lowercase()),
                    (PyVal::Str(s), "upper") => PyVal::Str(s.to_uppercase()),
                    (PyVal::Str(s), "startswith") => {
                        let prefix = args.first().map(|a| self.eval(a).to_str()).unwrap_or_default();
                        PyVal::Bool(s.starts_with(&prefix))
                    }
                    (PyVal::Str(s), "endswith") => {
                        let suffix = args.first().map(|a| self.eval(a).to_str()).unwrap_or_default();
                        PyVal::Bool(s.ends_with(&suffix))
                    }
                    (PyVal::Str(s), "find") => {
                        let needle = args.first().map(|a| self.eval(a).to_str()).unwrap_or_default();
                        PyVal::Int(s.find(&needle).map(|i| i as i64).unwrap_or(-1))
                    }
                    (PyVal::Str(s), "join") => {
                        let list = args.first().map(|a| self.eval(a)).unwrap_or(PyVal::None);
                        match list {
                            PyVal::List(l) => {
                                let parts: Vec<String> = l.iter().map(|v| v.to_str()).collect();
                                PyVal::Str(parts.join(s))
                            }
                            _ => PyVal::None
                        }
                    }
                    (PyVal::Str(s), "count") => {
                        let needle = args.first().map(|a| self.eval(a).to_str()).unwrap_or_default();
                        PyVal::Int(s.matches(&needle).count() as i64)
                    }

                    // list methods
                    (PyVal::List(l), "append") => {
                        let val = args.first().map(|a| self.eval(a)).unwrap_or(PyVal::None);
                        let mut new_list = l.clone();
                        new_list.push(val);
                        // Try to update the variable if object is a Var
                        if let Expr::Var(name) = object.as_ref() {
                            self.vars.insert(name.clone(), PyVal::List(new_list));
                        }
                        PyVal::None
                    }
                    (PyVal::List(l), "extend") => {
                        let other = args.first().map(|a| self.eval(a)).unwrap_or(PyVal::None);
                        let mut new_list = l.clone();
                        if let PyVal::List(other_list) = other {
                            new_list.extend(other_list);
                        }
                        if let Expr::Var(name) = object.as_ref() {
                            self.vars.insert(name.clone(), PyVal::List(new_list));
                        }
                        PyVal::None
                    }
                    (PyVal::List(l), "insert") => {
                        let idx = args.first().map(|a| match self.eval(a) { PyVal::Int(n) => n as usize, _ => 0 }).unwrap_or(0);
                        let val = args.get(1).map(|a| self.eval(a)).unwrap_or(PyVal::None);
                        let mut new_list = l.clone();
                        if idx <= new_list.len() {
                            new_list.insert(idx, val);
                        }
                        if let Expr::Var(name) = object.as_ref() {
                            self.vars.insert(name.clone(), PyVal::List(new_list));
                        }
                        PyVal::None
                    }
                    (PyVal::List(l), "pop") => {
                        let idx = args.first().map(|a| match self.eval(a) { PyVal::Int(n) => (if n < 0 { l.len() as i64 + n } else { n }) as usize, _ => l.len().saturating_sub(1) }).unwrap_or(l.len().saturating_sub(1));
                        let mut new_list = l.clone();
                        let val = if idx < new_list.len() { new_list.remove(idx) } else { PyVal::None };
                        if let Expr::Var(name) = object.as_ref() {
                            self.vars.insert(name.clone(), PyVal::List(new_list));
                        }
                        val
                    }
                    (PyVal::List(l), "reverse") => {
                        let mut new_list = l.clone();
                        new_list.reverse();
                        if let Expr::Var(name) = object.as_ref() {
                            self.vars.insert(name.clone(), PyVal::List(new_list));
                        }
                        PyVal::None
                    }
                    (PyVal::List(l), "sort") => {
                        let mut new_list = l.clone();
                        new_list.sort_by(|a, b| Self::compare_values(a, b).unwrap_or(std::cmp::Ordering::Equal));
                        if let Expr::Var(name) = object.as_ref() {
                            self.vars.insert(name.clone(), PyVal::List(new_list));
                        }
                        PyVal::None
                    }
                    (PyVal::List(l), "index") => {
                        let val = args.first().map(|a| self.eval(a)).unwrap_or(PyVal::None);
                        let idx = l.iter().position(|v| Self::values_equal(v, &val));
                        PyVal::Int(idx.map(|i| i as i64).unwrap_or(-1))
                    }

                    // dict methods
                    (PyVal::Dict(d), "keys") => {
                        PyVal::List(d.keys().map(|k| PyVal::Str(k.clone())).collect())
                    }
                    (PyVal::Dict(d), "values") => {
                        PyVal::List(d.values().cloned().collect())
                    }
                    (PyVal::Dict(d), "items") => {
                        PyVal::List(d.iter().map(|(k, v)| PyVal::List(vec![PyVal::Str(k.clone()), v.clone()])).collect())
                    }
                    (PyVal::Dict(d), "get") => {
                        let key = args.first().map(|a| self.eval(a).to_str()).unwrap_or_default();
                        let default = args.get(1).map(|a| self.eval(a)).unwrap_or(PyVal::None);
                        d.get(&key).cloned().unwrap_or(default)
                    }
                    (PyVal::Dict(d), "pop") => {
                        let key = args.first().map(|a| self.eval(a).to_str()).unwrap_or_default();
                        // Can't remove from immutable ref, would need mut
                        d.get(&key).cloned().unwrap_or(PyVal::None)
                    }

                    _ => {
                        eprintln!("Unknown method: {}.{}()", "value", method_s);
                        PyVal::None
                    }
                }
            }

            Expr::FuncCall(name, args) => {
                let eval_args: Vec<PyVal> = args.iter().map(|a| self.eval(a)).collect();
                if let Some(func) = self.funcs.get(name).cloned() {
                    let mut saved_vars: Vec<(String, Option<PyVal>)> = Vec::new();
                    for (i, param) in func.params.iter().enumerate() {
                        saved_vars.push((param.clone(), self.vars.get(param).cloned()));
                        if i < eval_args.len() {
                            self.vars.insert(param.clone(), eval_args[i].clone());
                        }
                    }
                    let result = self.exec_with_flow(&func.body);
                    // Restore vars
                    for (param, old_val) in saved_vars {
                        match old_val {
                            Some(v) => { self.vars.insert(param, v); }
                            None => { self.vars.remove(&param); }
                        }
                    }
                    match result {
                        ControlFlow::Return(v) => v,
                        _ => PyVal::None,
                    }
                } else {
                    eprintln!("Unknown function: {}", name);
                    PyVal::None
                }
            }

            Expr::Concat(parts) => {
                let s: String = parts.iter().map(|p| self.eval(p).to_str()).collect();
                PyVal::Str(s)
            }

            Expr::HttpGet(url) => {
                let _url = self.eval(url).to_str();
                eprintln!("http.get: stub (no HTTP host support)");
                PyVal::Str("{}".to_string())
            }

            Expr::HttpPost(url, body) => {
                let _url = self.eval(url).to_str();
                let _body = self.eval(body).to_str();
                eprintln!("http.post: stub (no HTTP host support)");
                PyVal::Str("{}".to_string())
            }

            Expr::ListLiteral(items) => {
                PyVal::List(items.iter().map(|e| self.eval(e)).collect())
            }
            Expr::DictLiteralExpr(keys, vals) => {
                let mut map = HashMap::new();
                for (k, v) in keys.iter().zip(vals.iter()) {
                    let key = self.eval(k).to_str();
                    let val = self.eval(v);
                    map.insert(key, val);
                }
                PyVal::Dict(map)
            }
        }
    }

    fn values_equal(a: &PyVal, b: &PyVal) -> bool {
        match (a, b) {
            (PyVal::None, PyVal::None) => true,
            (PyVal::Bool(a), PyVal::Bool(b)) => a == b,
            (PyVal::Int(a), PyVal::Int(b)) => a == b,
            (PyVal::Float(a), PyVal::Float(b)) => a == b,
            (PyVal::Int(a), PyVal::Float(b)) => (*a as f64) == *b,
            (PyVal::Float(a), PyVal::Int(b)) => *a == (*b as f64),
            (PyVal::Str(a), PyVal::Str(b)) => a == b,
            (PyVal::Bool(a), PyVal::Int(b)) => (*a as i64) == *b,
            (PyVal::Int(a), PyVal::Bool(b)) => *a == (*b as i64),
            _ => false,
        }
    }

    fn compare_values(a: &PyVal, b: &PyVal) -> Option<std::cmp::Ordering> {
        match (a, b) {
            (PyVal::Int(a), PyVal::Int(b)) => Some(a.cmp(b)),
            (PyVal::Float(a), PyVal::Float(b)) => a.partial_cmp(b),
            (PyVal::Int(a), PyVal::Float(b)) => (*a as f64).partial_cmp(b),
            (PyVal::Float(a), PyVal::Int(b)) => a.partial_cmp(&(*b as f64)),
            (PyVal::Str(a), PyVal::Str(b)) => Some(a.cmp(b)),
            (PyVal::Bool(a), PyVal::Bool(b)) => Some(a.cmp(b)),
            _ => None,
        }
    }

    fn exec(&mut self, ops: &[Op]) {
        self.exec_with_flow(ops);
    }

    fn exec_with_flow(&mut self, ops: &[Op]) -> ControlFlow {
        for op in ops {
            match op {
                Op::Assign(var, expr) => {
                    let val = self.eval(expr);
                    self.vars.insert(var.clone(), val);
                }
                Op::Print(expr) => {
                    let val = self.eval(expr);
                    let s = val.to_str();
                    self.output.push_str(&s);
                    self.output.push('\n');
                }
                Op::If(cond, if_body, else_body) => {
                    let v = self.eval(cond);
                    if v.is_truthy() {
                        let r = self.exec_with_flow(if_body);
                        if !matches!(r, ControlFlow::Ok) { return r; }
                    } else {
                        let r = self.exec_with_flow(else_body);
                        if !matches!(r, ControlFlow::Ok) { return r; }
                    }
                }
                Op::For(var, iter_expr, body) => {
                    let v = self.eval(iter_expr);
                    let items = match &v {
                        PyVal::List(l) => l.clone(),
                        PyVal::Dict(d) => d.keys().map(|k| PyVal::Str(k.clone())).collect(),
                        _ => vec![],
                    };
                    for item in items {
                        self.vars.insert(var.clone(), item);
                        match self.exec_with_flow(body) {
                            ControlFlow::Break => break,
                            ControlFlow::ContinueLoop => continue,
                            ControlFlow::Return(v) => return ControlFlow::Return(v),
                            ControlFlow::Ok => {}
                        }
                    }
                }
                Op::While(cond, body) => {
                    loop {
                        let v = self.eval(cond);
                        if !v.is_truthy() { break; }
                        match self.exec_with_flow(body) {
                            ControlFlow::Break => break,
                            ControlFlow::ContinueLoop => continue,
                            ControlFlow::Return(v) => return ControlFlow::Return(v),
                            ControlFlow::Ok => {}
                        }
                    }
                }
                Op::Def(name, params, body) => {
                    self.funcs.insert(name.clone(), FuncDef {
                        params: params.clone(),
                        body: body.clone(),
                    });
                }
                Op::Return(expr) => {
                    let val = expr.as_ref().map(|e| self.eval(e)).unwrap_or(PyVal::None);
                    return ControlFlow::Return(val);
                }
                Op::Break => return ControlFlow::Break,
                Op::Continue => return ControlFlow::ContinueLoop,
                Op::TryExcept(try_body, except_body) => {
                    match self.exec_with_flow(try_body) {
                        ControlFlow::Ok => {}
                        ControlFlow::Return(v) => return ControlFlow::Return(v),
                        _ => {
                            // On any error, run except body
                            let r = self.exec_with_flow(except_body);
                            if !matches!(r, ControlFlow::Ok) { return r; }
                        }
                    }
                }
                Op::ExprStmt(expr) => {
                    self.eval(expr);
                }
            }
        }
        ControlFlow::Ok
    }
}

// ============================================================
// Main
// ============================================================

fn main() {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);

    let script = if input.trim().is_empty() {
        "print(\"No script provided.\")".to_string()
    } else if input.trim().starts_with('{') {
        serde_json::from_str::<serde_json::Value>(&input)
            .ok()
            .and_then(|v| v.get("script").cloned())
            .and_then(|s| s.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "print(\"Invalid JSON input.\")".to_string())
    } else {
        input
    };

    let ops = Parser::parse(&script);
    let mut env = Env::new();
    env.exec(&ops);

    use std::io::Write;
    print!("{}", env.output);
    std::io::stdout().flush().ok();
}
