//! Reading the SQL text of a statement: literals, `concat!`, `format!`, constants and the
//! locals of the function built from them.
use super::super::{is_sql_function, is_sql_macro, rule3::binding};
use proc_macro2::TokenStream;
use quote::ToTokens;
use std::collections::{HashMap, HashSet};
use syn::{
    punctuated::Punctuated,
    visit::{self, Visit},
    BinOp, Expr, ExprPath, Item, Lit, Token,
};

/// The arguments of a macro that reads like a call.
pub(super) fn parse_args(tokens: TokenStream) -> Option<Vec<Expr>> {
    let parse = Punctuated::<Expr, Token![,]>::parse_terminated;
    syn::parse::Parser::parse2(parse, tokens)
        .ok()
        .map(|args| args.into_iter().collect())
}

/// A value a local receives: an expression, or the arguments of a `write!` into it.
pub(super) enum Piece {
    Expr(Expr),
    Format(Vec<Expr>),
}

/// The name a bare path, possibly behind `&` or a dereference, spells.
pub(super) fn ident_of(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(path) if path.qself.is_none() => path.path.get_ident().map(ToString::to_string),
        Expr::Reference(inner) => ident_of(&inner.expr),
        Expr::Paren(inner) => ident_of(&inner.expr),
        Expr::Group(inner) => ident_of(&inner.expr),
        Expr::Unary(inner) => ident_of(&inner.expr),
        _ => None,
    }
}

/// What a function binds: every value each local receives, the arguments of its `.push(..)`
/// calls, its parameters and the locals that hold a write unit.
#[derive(Default)]
pub(super) struct Locals {
    texts: HashMap<String, Vec<Piece>>,
    pushes: HashMap<String, Vec<Expr>>,
    params: HashSet<String>,
    pub(super) writes: HashSet<String>,
}

impl Locals {
    pub(super) fn of(params: HashSet<String>, writes: HashSet<String>, block: &syn::Block) -> Self {
        let mut locals = Self {
            params,
            writes,
            ..Self::default()
        };
        locals.visit_block(block);
        locals.grow();
        locals
    }

    fn add(&mut self, name: String, piece: Piece) {
        self.texts.entry(name).or_default().push(piece);
    }

    /// A local that only reborrows a write unit is one.
    fn grow(&mut self) {
        loop {
            let more: Vec<String> = self
                .texts
                .iter()
                .filter(|(name, pieces)| {
                    !self.writes.contains(*name)
                        && pieces.iter().any(|piece| {
                            matches!(piece, Piece::Expr(expr)
                                if ident_of(expr).is_some_and(|alias| self.writes.contains(&alias)))
                        })
                })
                .map(|(name, _)| name.clone())
                .collect();
            if more.is_empty() {
                return;
            }
            self.writes.extend(more);
        }
    }
}

impl<'ast> Visit<'ast> for Locals {
    fn visit_item(&mut self, _: &'ast Item) {}
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let (Some(name), Some(init)) = (binding(&local.pat), &local.init) {
            let text: String = init
                .expr
                .to_token_stream()
                .to_string()
                .split_whitespace()
                .collect();
            if text.contains("begin_write(") {
                self.writes.insert(name.clone());
            }
            self.add(name, Piece::Expr((*init.expr).clone()));
        }
        visit::visit_local(self, local);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if let (Some(name), Some(arg)) = (ident_of(&call.receiver), call.args.first()) {
            match call.method.to_string().as_str() {
                "push_str" => self.add(name, Piece::Expr(arg.clone())),
                "push" => self.pushes.entry(name).or_default().push(arg.clone()),
                _ => {}
            }
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_expr_assign(&mut self, assign: &'ast syn::ExprAssign) {
        if let Some(name) = ident_of(&assign.left) {
            self.add(name, Piece::Expr((*assign.right).clone()));
        }
        visit::visit_expr_assign(self, assign);
    }
    fn visit_expr_binary(&mut self, binary: &'ast syn::ExprBinary) {
        if let (BinOp::AddAssign(_), Some(name)) = (&binary.op, ident_of(&binary.left)) {
            self.add(name, Piece::Expr((*binary.right).clone()));
        }
        visit::visit_expr_binary(self, binary);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let writing = mac
            .path
            .segments
            .last()
            .is_some_and(|last| last.ident == "write" || last.ident == "writeln");
        if writing {
            if let Some(mut args) = parse_args(mac.tokens.clone()) {
                if !args.is_empty() {
                    let target = args.remove(0);
                    if let Some(name) = ident_of(&target) {
                        self.add(name, Piece::Format(args));
                    }
                }
            }
        }
        visit::visit_macro(self, mac);
    }
}

/// What a statement's call chain starts at.
pub(super) enum Chain {
    /// Not an SQL function, macro or query builder.
    Other,
    /// SQL, with its text when the checker can read it.
    Sql(Option<String>),
}

/// Reads SQL text inside one function.
pub(super) struct Reader<'a> {
    pub locals: &'a Locals,
    pub consts: &'a HashMap<String, Vec<Expr>>,
}

fn joined(texts: Vec<String>) -> String {
    texts.join(" ")
}

impl Reader<'_> {
    pub(super) fn text(&self, expr: &Expr, seen: &mut Vec<String>) -> Option<String> {
        match expr {
            Expr::Lit(lit) => match &lit.lit {
                Lit::Str(text) => Some(text.value()),
                _ => None,
            },
            Expr::Paren(inner) => self.text(&inner.expr, seen),
            Expr::Group(inner) => self.text(&inner.expr, seen),
            Expr::Reference(inner) => self.text(&inner.expr, seen),
            Expr::Macro(mac) => self.macro_text(&mac.mac, seen),
            Expr::Path(path) => {
                let name = path.path.segments.last()?.ident.to_string();
                let single = path.qself.is_none() && path.path.segments.len() == 1;
                self.name_text(&name, single, seen)
            }
            Expr::MethodCall(call) if call.args.is_empty() => {
                match call.method.to_string().as_str() {
                    "to_string" | "to_owned" | "clone" | "as_str" | "into" | "as_ref" => {
                        self.text(&call.receiver, seen)
                    }
                    _ => None,
                }
            }
            Expr::Call(call) => {
                let Expr::Path(function) = &*call.func else {
                    return None;
                };
                let names: Vec<String> = function
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect();
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                match names.as_slice() {
                    [.., "String", "from"] => self.text(call.args.first()?, seen),
                    [.., "String", "new"] => Some(String::new()),
                    _ => None,
                }
            }
            Expr::Binary(binary) if matches!(binary.op, BinOp::Add(_)) => {
                let left = self.text(&binary.left, seen)?;
                let right = self.text(&binary.right, seen)?;
                Some(joined(vec![left, right]))
            }
            Expr::If(branch) => {
                let then = self.block_text(&branch.then_branch, seen)?;
                let (_, other) = branch.else_branch.as_ref()?;
                Some(joined(vec![then, self.text(other, seen)?]))
            }
            Expr::Block(block) => self.block_text(&block.block, seen),
            Expr::Match(matched) => {
                let arms: Option<Vec<String>> = matched
                    .arms
                    .iter()
                    .map(|arm| self.text(&arm.body, seen))
                    .collect();
                Some(joined(arms?))
            }
            _ => None,
        }
    }

    fn block_text(&self, block: &syn::Block, seen: &mut Vec<String>) -> Option<String> {
        match block.stmts.last() {
            Some(syn::Stmt::Expr(tail, None)) => self.text(tail, seen),
            _ => None,
        }
    }

    /// A local of the function when `single`, else a constant or static of the file.
    fn name_text(&self, name: &str, single: bool, seen: &mut Vec<String>) -> Option<String> {
        if single {
            if let Some(pieces) = self.locals.texts.get(name) {
                if seen.iter().any(|open| open == name) {
                    return (!self.locals.params.contains(name)).then(String::new);
                }
                seen.push(name.to_string());
                let texts: Option<Vec<String>> = pieces
                    .iter()
                    .map(|piece| match piece {
                        Piece::Expr(expr) => self.text(expr, seen),
                        Piece::Format(args) => self.format_text(args, seen),
                    })
                    .collect();
                seen.pop();
                return texts.map(joined);
            }
        }
        let items = self.consts.get(name)?;
        let texts: Option<Vec<String>> = items.iter().map(|item| self.text(item, seen)).collect();
        texts.map(joined)
    }

    fn macro_text(&self, mac: &syn::Macro, seen: &mut Vec<String>) -> Option<String> {
        let name = mac.path.segments.last()?.ident.to_string();
        let args = parse_args(mac.tokens.clone())?;
        match name.as_str() {
            "concat" => {
                let parts: Option<Vec<String>> =
                    args.iter().map(|arg| self.text(arg, seen)).collect();
                Some(parts?.concat())
            }
            "format" => self.format_text(&args, seen),
            _ => None,
        }
    }

    /// The format string with each `{..}` replaced by the text of its argument when that is
    /// readable, else by `x`.
    fn format_text(&self, args: &[Expr], seen: &mut Vec<String>) -> Option<String> {
        let Expr::Lit(syn::ExprLit {
            lit: Lit::Str(template),
            ..
        }) = args.first()?
        else {
            return None;
        };
        let mut positional: Vec<&Expr> = Vec::new();
        let mut named: HashMap<String, &Expr> = HashMap::new();
        for arg in args.iter().skip(1) {
            match arg {
                Expr::Assign(assign) => match ident_of(&assign.left) {
                    Some(name) => {
                        named.insert(name, &assign.right);
                    }
                    None => positional.push(arg),
                },
                _ => positional.push(arg),
            }
        }
        let chars: Vec<char> = template.value().chars().collect();
        let mut out = String::new();
        let mut at = 0;
        let mut next = 0;
        while let Some(&c) = chars.get(at) {
            let doubled = chars.get(at + 1) == Some(&c);
            if (c == '{' || c == '}') && doubled {
                out.push(c);
                at += 2;
            } else if c == '{' {
                let end = chars
                    .iter()
                    .skip(at)
                    .position(|&c| c == '}')
                    .map_or(chars.len(), |found| at + found);
                let inner: String = chars.get(at + 1..end).unwrap_or_default().iter().collect();
                at = end + 1;
                let key = inner.split(':').next().unwrap_or_default().trim();
                let argument = if key.is_empty() {
                    next += 1;
                    positional.get(next - 1).copied()
                } else if let Ok(index) = key.parse::<usize>() {
                    positional.get(index).copied()
                } else {
                    named.get(key).copied()
                };
                let value = match argument {
                    Some(argument) => self.text(argument, seen),
                    None => self.name_text(key, true, seen),
                };
                out.push_str(&value.unwrap_or_else(|| "x".to_string()));
            } else {
                out.push(c);
                at += 1;
            }
        }
        Some(out)
    }

    /// The SQL a call chain starts at, with the literals pushed onto a query builder in it.
    pub(super) fn chain(&self, expr: &Expr, seen: &mut Vec<String>) -> Chain {
        match expr {
            Expr::MethodCall(call) => {
                let mut inner = self.chain(&call.receiver, seen);
                if call.method == "push" {
                    if let (Chain::Sql(Some(text)), Some(arg)) = (&mut inner, call.args.first()) {
                        if let Some(added) = self.text(arg, seen) {
                            text.push(' ');
                            text.push_str(&added);
                        }
                    }
                }
                inner
            }
            Expr::Await(inner) => self.chain(&inner.base, seen),
            Expr::Try(inner) => self.chain(&inner.expr, seen),
            Expr::Paren(inner) => self.chain(&inner.expr, seen),
            Expr::Group(inner) => self.chain(&inner.expr, seen),
            Expr::Reference(inner) => self.chain(&inner.expr, seen),
            Expr::Call(call) => self.call_chain(call, seen),
            Expr::Macro(mac) => self.macro_chain(&mac.mac, seen),
            Expr::Path(path) => self.local_chain(path, seen),
            _ => Chain::Other,
        }
    }

    fn call_chain(&self, call: &syn::ExprCall, seen: &mut Vec<String>) -> Chain {
        let Expr::Path(function) = &*call.func else {
            return Chain::Other;
        };
        let segments = &function.path.segments;
        let Some(last) = segments.last() else {
            return Chain::Other;
        };
        let name = last.ident.to_string();
        let builder = segments
            .iter()
            .any(|segment| segment.ident == "QueryBuilder")
            && (name == "new" || name == "with_arguments");
        if !builder && !is_sql_function(&name) {
            return Chain::Other;
        }
        if name.starts_with("query_file") {
            return Chain::Sql(None);
        }
        Chain::Sql(call.args.first().and_then(|arg| self.text(arg, seen)))
    }

    fn macro_chain(&self, mac: &syn::Macro, seen: &mut Vec<String>) -> Chain {
        let Some(name) = mac.path.segments.last().map(|last| last.ident.to_string()) else {
            return Chain::Other;
        };
        if !is_sql_macro(&name) {
            return Chain::Other;
        }
        if name.starts_with("query_file") {
            return Chain::Sql(None);
        }
        let skip = usize::from(name.starts_with("query_as"));
        let text = match parse_args(mac.tokens.clone()) {
            Some(args) => args.get(skip).and_then(|arg| self.text(arg, seen)),
            None => mac.tokens.clone().into_iter().find_map(|token| {
                syn::parse2::<syn::LitStr>(TokenStream::from(token))
                    .ok()
                    .map(|literal| literal.value())
            }),
        };
        Chain::Sql(text)
    }

    /// A local bound to an SQL chain or a query builder, with what is pushed onto it later.
    fn local_chain(&self, path: &ExprPath, seen: &mut Vec<String>) -> Chain {
        let name = match (&path.qself, path.path.get_ident()) {
            (None, Some(ident)) => ident.to_string(),
            _ => return Chain::Other,
        };
        let Some(pieces) = self.locals.texts.get(&name) else {
            return Chain::Other;
        };
        if seen.iter().any(|open| *open == name) {
            return Chain::Other;
        }
        seen.push(name.clone());
        let chains: Vec<Option<String>> = pieces
            .iter()
            .filter_map(|piece| match piece {
                Piece::Expr(expr) => match self.chain(expr, seen) {
                    Chain::Sql(text) => Some(text),
                    Chain::Other => None,
                },
                Piece::Format(_) => None,
            })
            .collect();
        let pushed: Vec<String> = self
            .locals
            .pushes
            .get(&name)
            .map(|args| args.iter().filter_map(|arg| self.text(arg, seen)).collect())
            .unwrap_or_default();
        seen.pop();
        if chains.is_empty() {
            return Chain::Other;
        }
        let texts: Option<Vec<String>> = chains.into_iter().collect();
        Chain::Sql(texts.map(|mut texts| {
            texts.extend(pushed);
            joined(texts)
        }))
    }
}
