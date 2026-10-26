//! Port of CSharp/BooleanExpressionParser.cs and PreprocessorSymbols.cs
//!
//! Finds the sets of preprocessor symbols needed so that every branch of every `#if` is active
//! in at least one set. The formatter is run once per set so code in all branches is formatted.

use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    And,
    Or,
    Not,
    Var(String),
    LParen,
    RParen,
    Eq,
    NotEq,
    True,
    False,
}

fn tokenize(input: &str) -> Vec<Tok> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut x = 0;
    while x < chars.len() {
        let c = chars[x];
        if c.is_whitespace() {
            x += 1;
        } else if c.is_alphanumeric() || c == '_' {
            let start = x;
            while x < chars.len() && (chars[x].is_alphanumeric() || chars[x] == '_') {
                x += 1;
            }
            let value: String = chars[start..x].iter().collect();
            tokens.push(match value.as_str() {
                "true" => Tok::True,
                "false" => Tok::False,
                _ => Tok::Var(value),
            });
        } else {
            match c {
                '(' => tokens.push(Tok::LParen),
                ')' => tokens.push(Tok::RParen),
                '&' => {
                    x += 1;
                    tokens.push(Tok::And)
                }
                '|' => {
                    x += 1;
                    tokens.push(Tok::Or)
                }
                '=' => {
                    x += 1;
                    tokens.push(Tok::Eq)
                }
                '!' => {
                    if chars.get(x + 1) == Some(&'=') {
                        x += 1;
                        tokens.push(Tok::NotEq)
                    } else {
                        tokens.push(Tok::Not)
                    }
                }
                _ => {}
            }
            x += 1;
        }
    }
    tokens
}

/// The C# version uses a shunting yard without precedence, operators are left associative
/// and all have the same precedence. Kept the same so both agree.
fn shunting_yard(tokens: Vec<Tok>) -> Vec<Tok> {
    let mut output = Vec::new();
    let mut ops: Vec<Tok> = Vec::new();
    for t in tokens {
        match t {
            Tok::True | Tok::False | Tok::Var(_) => output.push(t),
            Tok::And | Tok::Or | Tok::Eq | Tok::NotEq => {
                while matches!(ops.last(), Some(o) if *o != Tok::LParen) {
                    output.push(ops.pop().unwrap());
                }
                ops.push(t);
            }
            Tok::Not | Tok::LParen => ops.push(t),
            Tok::RParen => {
                while matches!(ops.last(), Some(o) if *o != Tok::LParen) {
                    output.push(ops.pop().unwrap());
                }
                ops.pop();
            }
        }
    }
    while let Some(o) = ops.pop() {
        output.push(o);
    }
    output
}

#[derive(Clone, Debug)]
pub struct BoolExpr {
    pub params: Vec<String>,
    postfix: Option<Vec<Tok>>, // None means "all params are true"
}

impl BoolExpr {
    pub fn parse(input: &str) -> BoolExpr {
        let postfix = shunting_yard(tokenize(input));
        let mut params: Vec<String> = Vec::new();
        for t in &postfix {
            if let Tok::Var(v) = t {
                if !params.contains(v) {
                    params.push(v.clone());
                }
            }
        }
        BoolExpr {
            params,
            postfix: Some(postfix),
        }
    }

    fn all_true(params: Vec<String>) -> BoolExpr {
        BoolExpr {
            params,
            postfix: None,
        }
    }

    fn eval(&self, values: &HashMap<String, bool>) -> bool {
        let Some(postfix) = &self.postfix else {
            return values.values().all(|v| *v);
        };
        let mut stack: Vec<bool> = Vec::new();
        for t in postfix {
            match t {
                Tok::Var(v) => stack.push(*values.get(v).unwrap_or(&false)),
                Tok::True => stack.push(true),
                Tok::False => stack.push(false),
                Tok::Not => {
                    let a = stack.pop().unwrap_or(false);
                    stack.push(!a)
                }
                Tok::And | Tok::Or | Tok::Eq | Tok::NotEq => {
                    let r = stack.pop().unwrap_or(false);
                    let l = stack.pop().unwrap_or(false);
                    stack.push(match t {
                        Tok::And => l && r,
                        Tok::Or => l || r,
                        Tok::Eq => l == r,
                        _ => l != r,
                    });
                }
                _ => {}
            }
        }
        stack.pop().unwrap_or(false)
    }
}

/// all true/false assignments, in the same order as the C# implementation
fn combinations(names: &[String]) -> Vec<Vec<(String, bool)>> {
    if names.is_empty() {
        return vec![vec![]];
    }
    let mut result = Vec::new();
    for sub in combinations(&names[1..]) {
        for value in [false, true] {
            let mut c = sub.clone();
            c.push((names[0].clone(), value));
            result.push(c);
        }
    }
    result
}

fn to_map(c: &[(String, bool)]) -> HashMap<String, bool> {
    c.iter().cloned().collect()
}

fn get_symbols(expr: &BoolExpr) -> Vec<String> {
    for c in combinations(&expr.params) {
        if expr.eval(&to_map(&c)) {
            let mut symbols: Vec<String> =
                c.into_iter().filter(|(_, v)| *v).map(|(k, _)| k).collect();
            symbols.sort();
            return symbols;
        }
    }
    vec![]
}

#[derive(Debug, Clone)]
pub enum Event {
    If(String),
    Elif(String),
    Else,
    EndIf,
}

#[derive(Default)]
struct Context {
    exprs: Vec<BoolExpr>,
}

fn strip_comment(expression: &str) -> &str {
    match expression.find('/') {
        Some(i) if i > 0 => &expression[..i],
        _ => expression,
    }
}

pub fn get_sets(events: &[Event]) -> Vec<Vec<String>> {
    let mut sets: Vec<Vec<String>> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    // the bottom two contexts mirror the C# root and its parent
    let mut contexts: Vec<Context> = vec![Context::default(), Context::default()];

    for event in events {
        match event {
            Event::If(cond) => {
                contexts.push(Context::default());
                contexts
                    .last_mut()
                    .unwrap()
                    .exprs
                    .push(BoolExpr::parse(strip_comment(cond)));
            }
            Event::Elif(cond) => {
                contexts
                    .last_mut()
                    .unwrap()
                    .exprs
                    .push(BoolExpr::parse(strip_comment(cond)));
            }
            Event::Else => {
                let ctx = contexts.last_mut().unwrap();
                let mut all: Vec<String> = Vec::new();
                for e in &ctx.exprs {
                    for p in &e.params {
                        if !all.contains(p) {
                            all.push(p.clone());
                        }
                    }
                }
                let found = combinations(&all).into_iter().find(|c| {
                    let m = to_map(c);
                    ctx.exprs.iter().all(|e| !e.eval(&m))
                });
                if let Some(c) = found {
                    let params = c.into_iter().filter(|(_, v)| *v).map(|(k, _)| k).collect();
                    ctx.exprs.push(BoolExpr::all_true(params));
                }
            }
            Event::EndIf => {
                if contexts.len() <= 2 {
                    continue;
                }
                let parent_symbols = contexts[contexts.len() - 2]
                    .exprs
                    .last()
                    .map(get_symbols)
                    .unwrap_or_default();
                for expr in &contexts.last().unwrap().exprs {
                    let mut symbol_set = get_symbols(expr);
                    for p in &parent_symbols {
                        if !symbol_set.contains(p) {
                            symbol_set.push(p.clone());
                        }
                    }
                    if symbol_set.is_empty() {
                        continue;
                    }
                    if seen.insert(symbol_set.join(",")) {
                        sets.push(symbol_set);
                    }
                }
                contexts.pop();
            }
        }
    }
    sets
}
