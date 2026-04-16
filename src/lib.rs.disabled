//! NEAR Python — a tiny Python-subset runtime for NEAR OutLayer
//!
//! Reads a Python-like script from stdin, executes it with NEAR builtins.
//! Supports: variable assignment, print, near.view(), near.block_height(),
//! near.storage.get/put, json operations, if/else, basic expressions.

use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;

wit_bindgen::generate!({
    world: "rpc-host",
});

/// Python value representation
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
            other => format!("{:?}", other),
        }
    }
}

/// Script operation (parsed from Python-subset)
#[derive(Clone, Debug)]
enum Op {
    /// variable = expression
    Assign(String, Expr),
    /// print(expr)
    Print(Expr),
    /// if expr: ops [else: ops]
    If(Expr, Vec<Op>, Vec<Op>),
    /// for var in expr: ops
    For(String, Expr, Vec<Op>),
}

#[derive(Clone, Debug)]
enum Expr {
    Lit(PyVal),
    Var(String),
    /// near.view(contract, method, args)
    NearView(Box<Expr>, Box<Expr>, Box<Expr>),
    /// near.call(signer_id, signer_key, receiver, method, args, deposit, gas)
    NearCall(Box<Expr>, Box<Expr>, Box<Expr>, Box<Expr>, Box<Expr>, Box<Expr>, Box<Expr>),
    /// near.block_height()
    NearBlockHeight,
    /// near.block(block_id)
    NearBlock(Box<Expr>),
    /// near.storage.get(key)
    StorageGet(Box<Expr>),
    /// near.storage.put(key, value)
    StoragePut(Box<Expr>, Box<Expr>),
    /// json.dumps(expr)
    JsonDumps(Box<Expr>),
    /// json.loads(expr)
    JsonLoads(Box<Expr>),
    /// len(expr)
    Len(Box<Expr>),
    /// expr.key or expr[key]
    GetAttr(Box<Expr>, String),
    /// f-string or string concat
    Concat(Vec<Expr>),
}

/// Very simple line-by-line parser for a Python subset
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
            let line = lines[i].trim();
            if line.is_empty() || line.starts_with('#') {
                i += 1;
                continue;
            }
            let indent = Self::indent_level(lines[i]);
            if indent < base_indent {
                break;
            }
            if line.starts_with("if ") && line.ends_with(':') {
                // if expr: ...
                let cond_str = line.strip_prefix("if ").unwrap().strip_suffix(':').unwrap();
                let cond = Self::parse_expr(cond_str);
                // Find else or next at same indent
                let block_start = i + 1;
                let mut block_end = block_start;
                let mut else_start = None;
                while block_end < end {
                    let l = lines[block_end].trim();
                    if l.is_empty() { block_end += 1; continue; }
                    let ind = Self::indent_level(lines[block_end]);
                    if ind < base_indent + 1 { break; }
                    if ind == base_indent + 1 && l.starts_with("else") {
                        else_start = Some(block_end + 1);
                        break;
                    }
                    block_end += 1;
                }
                let if_body = Self::parse_block(lines, block_start, block_end, base_indent + 1);
                let else_body = if let Some(es) = else_start {
                    let mut ee = es;
                    while ee < end {
                        let l = lines[ee].trim();
                        if l.is_empty() { ee += 1; continue; }
                        if Self::indent_level(lines[ee]) < base_indent + 1 { break; }
                        ee += 1;
                    }
                    Self::parse_block(lines, es, ee, base_indent + 1)
                } else {
                    vec![]
                };
                ops.push(Op::If(cond, if_body, else_body));
                // Skip past the entire if/else block
                if let Some(es) = else_start {
                    let mut ee = es;
                    while ee < end {
                        let l = lines[ee].trim();
                        if l.is_empty() { ee += 1; continue; }
                        if Self::indent_level(lines[ee]) < base_indent + 1 { break; }
                        ee += 1;
                    }
                    i = ee;
                } else {
                    i = block_end;
                }
                continue;
            }
            if line.starts_with("for ") && line.ends_with(':') {
                // for var in expr: ...
                let for_str = line.strip_prefix("for ").unwrap().strip_suffix(':').unwrap();
                let parts: Vec<&str> = for_str.splitn(2, " in ").collect();
                let var = parts[0].trim().to_string();
                let iter_expr = Self::parse_expr(parts[1].trim());
                let block_start = i + 1;
                let mut block_end = block_start;
                while block_end < end {
                    let l = lines[block_end].trim();
                    if l.is_empty() { block_end += 1; continue; }
                    if Self::indent_level(lines[block_end]) < base_indent + 1 { break; }
                    block_end += 1;
                }
                let body = Self::parse_block(lines, block_start, block_end, base_indent + 1);
                ops.push(Op::For(var, iter_expr, body));
                i = block_end;
                continue;
            }
            if line.starts_with("print(") && line.ends_with(')') {
                let inner = &line[6..line.len()-1];
                ops.push(Op::Print(Self::parse_expr(inner)));
                i += 1;
                continue;
            }
            if let Some(eq) = line.find(" = ") {
                let var = line[..eq].trim().to_string();
                let expr_str = line[eq+3..].trim();
                ops.push(Op::Assign(var, Self::parse_expr(expr_str)));
                i += 1;
                continue;
            }
            i += 1;
        }
        ops
    }

    fn parse_expr(s: &str) -> Expr {
        let s = s.trim();

        // near.block_height()
        if s == "near.block_height()" {
            return Expr::NearBlockHeight;
        }

        // near.view(a, b, c)
        if s.starts_with("near.view(") && s.ends_with(')') {
            let inner = &s[10..s.len()-1];
            let args = split_args(inner);
            if args.len() >= 3 {
                return Expr::NearView(
                    Box::new(Self::parse_expr(args[0])),
                    Box::new(Self::parse_expr(args[1])),
                    Box::new(Self::parse_expr(args[2])),
                );
            }
        }

        // near.call(signer_id, signer_key, receiver, method, args, deposit, gas)
        if s.starts_with("near.call(") && s.ends_with(')') {
            let inner = &s[10..s.len()-1];
            let args = split_args(inner);
            if args.len() >= 7 {
                return Expr::NearCall(
                    Box::new(Self::parse_expr(args[0])),
                    Box::new(Self::parse_expr(args[1])),
                    Box::new(Self::parse_expr(args[2])),
                    Box::new(Self::parse_expr(args[3])),
                    Box::new(Self::parse_expr(args[4])),
                    Box::new(Self::parse_expr(args[5])),
                    Box::new(Self::parse_expr(args[6])),
                );
            }
        }

        // near.block(block_id)
        if s.starts_with("near.block(") && s.ends_with(')') {
            let inner = &s[11..s.len()-1];
            return Expr::NearBlock(Box::new(Self::parse_expr(inner)));
        }

        // near.storage.get(key)
        if s.starts_with("near.storage.get(") && s.ends_with(')') {
            let inner = &s[18..s.len()-1];
            return Expr::StorageGet(Box::new(Self::parse_expr(inner)));
        }

        // near.storage.put(key, value)
        if s.starts_with("near.storage.put(") && s.ends_with(')') {
            let inner = &s[18..s.len()-1];
            let args = split_args(inner);
            if args.len() >= 2 {
                return Expr::StoragePut(
                    Box::new(Self::parse_expr(args[0])),
                    Box::new(Self::parse_expr(args[1])),
                );
            }
        }

        // json.dumps(expr)
        if s.starts_with("json.dumps(") && s.ends_with(')') {
            let inner = &s[11..s.len()-1];
            return Expr::JsonDumps(Box::new(Self::parse_expr(inner)));
        }

        // json.loads(expr)
        if s.starts_with("json.loads(") && s.ends_with(')') {
            let inner = &s[11..s.len()-1];
            return Expr::JsonLoads(Box::new(Self::parse_expr(inner)));
        }

        // len(expr)
        if s.starts_with("len(") && s.ends_with(')') {
            let inner = &s[4..s.len()-1];
            return Expr::Len(Box::new(Self::parse_expr(inner)));
        }

        // f-string: f"..." — simple version, just variable references
        if s.starts_with("f\"") || s.starts_with("f'") {
            let quote = &s[1..2];
            let q = quote.chars().next().unwrap();
            if s.ends_with(q) {
                let content = &s[2..s.len()-1];
                return parse_fstring(content);
            }
        }

        // Variable reference with attribute access: expr.key
        if s.contains('.') && !s.starts_with('"') && !s.starts_with('\'') && !s.starts_with('{') {
            // Check if it's a known function call first
            let parts: Vec<&str> = s.splitn(2, '.').collect();
            if parts.len() == 2 && !parts[1].contains('(') {
                // Simple attribute access: var.key
                return Expr::GetAttr(
                    Box::new(Self::parse_expr(parts[0])),
                    parts[1].to_string(),
                );
            }
        }

        // Variable reference
        if !s.starts_with('"') && !s.starts_with('\'') && !s.starts_with('{')
            && !s.starts_with('[') && !s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
        {
            return Expr::Var(s.to_string());
        }

        // JSON object literal
        if s.starts_with('{') {
            if let Ok(v) = serde_json::from_str::<Value>(s) {
                return Expr::Lit(PyVal::from_json(&v));
            }
        }

        // String literal
        if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
            let inner = &s[1..s.len()-1];
            return Expr::Lit(PyVal::Str(inner.to_string()));
        }

        // Number
        if let Ok(n) = s.parse::<i64>() {
            return Expr::Lit(PyVal::Int(n));
        }
        if let Ok(f) = s.parse::<f64>() {
            return Expr::Lit(PyVal::Float(f));
        }

        // Empty dict literal {}
        if s == "{}" {
            return Expr::Lit(PyVal::Dict(HashMap::new()));
        }

        Expr::Lit(PyVal::Str(s.to_string()))
    }
}

fn split_args(s: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        args.push(s[start..].trim());
    }
    args
}

fn parse_fstring(content: &str) -> Expr {
    let mut parts = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = content.chars().collect();
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
            let var_name: String = chars[i+1..end].iter().collect();
            parts.push(Expr::Var(var_name.trim().to_string()));
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

/// Execution environment
struct Env {
    vars: HashMap<String, PyVal>,
    output: String,
}

impl Env {
    fn new() -> Self {
        Env {
            vars: HashMap::new(),
            output: String::new(),
        }
    }

    fn eval(&mut self, expr: &Expr) -> PyVal {
        match expr {
            Expr::Lit(v) => v.clone(),
            Expr::Var(name) => self.vars.get(name).cloned().unwrap_or(PyVal::None),
            Expr::NearView(contract, method, args) => {
                let c = self.eval(contract).to_str();
                let m = self.eval(method).to_str();
                let a = self.eval(args).to_str();
                // Convert args to JSON string if it's a dict
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
            Expr::NearCall(signer_id, signer_key, receiver, method, args, deposit, gas) => {
                let sid = self.eval(signer_id).to_str();
                let skey = self.eval(signer_key).to_str();
                let recv = self.eval(receiver).to_str();
                let meth = self.eval(method).to_str();
                let a = self.eval(args).to_str();
                let dep = self.eval(deposit).to_str();
                let g = self.eval(gas).to_str();
                let (result, err) = near::rpc::api::call(
                    &sid, &skey, &recv, &meth, &a, &dep, &g, "FINAL",
                );
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
            Expr::StorageGet(key) => {
                let _k = self.eval(key).to_str();
                eprintln!("storage.get: stub (no storage host)");
                PyVal::None
            }
            Expr::StoragePut(key, value) => {
                let _k = self.eval(key).to_str();
                let _v = self.eval(value);
                eprintln!("storage.put: stub (no storage host)");
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
            Expr::GetAttr(obj, key) => {
                let v = self.eval(obj);
                match &v {
                    PyVal::Dict(m) => m.get(key).cloned().unwrap_or(PyVal::None),
                    _ => PyVal::None,
                }
            }
            Expr::Concat(parts) => {
                let s: String = parts.iter().map(|p| {
                    let v = self.eval(p);
                    match v {
                        PyVal::Str(s) => s,
                        PyVal::Int(n) => n.to_string(),
                        PyVal::Float(f) => f.to_string(),
                        PyVal::Bool(b) => b.to_string(),
                        other => format!("{:?}", other),
                    }
                }).collect();
                PyVal::Str(s)
            }
        }
    }

    fn exec(&mut self, ops: &[Op]) {
        for op in ops {
            match op {
                Op::Assign(var, expr) => {
                    let val = self.eval(expr);
                    self.vars.insert(var.clone(), val);
                }
                Op::Print(expr) => {
                    let val = self.eval(expr);
                    let s = match &val {
                        PyVal::Str(s) => s.clone(),
                        other => other.to_json().to_string(),
                    };
                    self.output.push_str(&s);
                    self.output.push('\n');
                }
                Op::If(cond, if_body, else_body) => {
                    let v = self.eval(cond);
                    let is_truthy = match &v {
                        PyVal::Bool(b) => *b,
                        PyVal::Int(n) => *n != 0,
                        PyVal::Str(s) => !s.is_empty(),
                        PyVal::None => false,
                        _ => true,
                    };
                    if is_truthy {
                        self.exec(if_body);
                    } else {
                        self.exec(else_body);
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
                        self.exec(body);
                    }
                }
            }
        }
    }
}

#[export_name = "wasi:cli/run#run"]
pub unsafe extern "C" fn _run() -> i32 {
    // Read script from stdin
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);

    let script = if input.trim().is_empty() {
        // Fallback: try to read main.py from the current dir
        std::fs::read_to_string("main.py").unwrap_or_else(|_| {
            "print(\"No script provided. Send Python script via stdin.\")".to_string()
        })
    } else {
        input
    };

    let ops = Parser::parse(&script);
    let mut env = Env::new();
    env.exec(&ops);

    // Print accumulated output to stdout
    print!("{}", env.output);
    0
}
