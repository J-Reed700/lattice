//! Rule 6(a): a statement that writes runs on a write unit.
use super::{
    sql::{parse_args, Chain, Locals, Reader},
    words, WRITES,
};
use crate::{
    data_access::{
        rule3::{binding, unit_params, Kind, STATEMENTS},
        Violation,
    },
    item_attrs,
    ratchet::{impl_item_attrs, trait_item_attrs},
    test_only,
};
use quote::ToTokens;
use std::collections::{HashMap, HashSet};
use syn::{
    visit::{self, Visit},
    Expr, FnArg, ImplItem, Item, TraitItem,
};

/// The constants and statics of the file by name, with the expressions they hold.
#[derive(Default)]
struct Consts(HashMap<String, Vec<Expr>>);

impl<'ast> Visit<'ast> for Consts {
    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        self.0
            .entry(item.ident.to_string())
            .or_default()
            .push((*item.expr).clone());
        visit::visit_item_const(self, item);
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        self.0
            .entry(item.ident.to_string())
            .or_default()
            .push((*item.expr).clone());
        visit::visit_item_static(self, item);
    }
    fn visit_impl_item_const(&mut self, item: &'ast syn::ImplItemConst) {
        self.0
            .entry(item.ident.to_string())
            .or_default()
            .push(item.expr.clone());
        visit::visit_impl_item_const(self, item);
    }
}

/// A function with a body: the names of its parameters, those that are write units, and the body.
type Function<'ast> = (HashSet<String>, HashSet<String>, &'ast syn::Block);

#[derive(Default)]
struct Functions<'ast> {
    found: Vec<Function<'ast>>,
}

impl<'ast> Functions<'ast> {
    fn add(&mut self, signature: &syn::Signature, block: &'ast syn::Block) {
        let params = signature
            .inputs
            .iter()
            .filter_map(|input| match input {
                FnArg::Typed(typed) => binding(&typed.pat),
                FnArg::Receiver(_) => None,
            })
            .collect();
        let writes = unit_params(signature)
            .into_iter()
            .filter(|(_, kind, _)| *kind == Kind::Write)
            .map(|(name, _, _)| name)
            .collect();
        self.found.push((params, writes, block));
    }
}

impl<'ast> Visit<'ast> for Functions<'ast> {
    fn visit_item(&mut self, item: &'ast Item) {
        if !item_attrs(item).is_some_and(test_only) {
            visit::visit_item(self, item);
        }
    }
    fn visit_impl_item(&mut self, item: &'ast ImplItem) {
        if !impl_item_attrs(item).is_some_and(test_only) {
            visit::visit_impl_item(self, item);
        }
    }
    fn visit_trait_item(&mut self, item: &'ast TraitItem) {
        if !trait_item_attrs(item).is_some_and(test_only) {
            visit::visit_trait_item(self, item);
        }
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.add(&item.sig, &item.block);
        visit::visit_item_fn(self, item);
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.add(&item.sig, &item.block);
        visit::visit_impl_item_fn(self, item);
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if let Some(block) = &item.default {
            self.add(&item.sig, block);
        }
        visit::visit_trait_item_fn(self, item);
    }
}

/// Walks one function body, flagging each statement whose SQL writes and does not run on a
/// write unit. SQL it cannot read is left to the read unit, which refuses to commit a change.
struct Statements<'a> {
    path: &'a str,
    reader: &'a Reader<'a>,
    found: Vec<Violation>,
}

impl Statements<'_> {
    fn statement(&mut self, call: &syn::ExprMethodCall) {
        let Chain::Sql(Some(sql)) = self.reader.chain(&call.receiver, &mut Vec::new()) else {
            return;
        };
        if !words::writes(&sql) {
            return;
        }
        let executor = call
            .args
            .first()
            .map(|arg| arg.to_token_stream().to_string())
            .unwrap_or_default();
        let on_unit = executor
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|word| self.reader.locals.writes.contains(word));
        if on_unit {
            return;
        }
        self.found.push(Violation {
            rule: 6,
            path: self.path.to_string(),
            line: call.method.span().start().line,
            message: WRITES.to_string(),
        });
    }
}

impl<'ast> Visit<'ast> for Statements<'_> {
    fn visit_item(&mut self, _: &'ast Item) {}
    fn visit_stmt(&mut self, stmt: &'ast syn::Stmt) {
        if let syn::Stmt::Local(local) = stmt {
            if test_only(&local.attrs) {
                return;
            }
        }
        visit::visit_stmt(self, stmt);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if STATEMENTS.contains(&call.method.to_string().as_str()) {
            self.statement(call);
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if let Some(args) = parse_args(mac.tokens.clone()) {
            for arg in &args {
                self.visit_expr(arg);
            }
        } else if let Ok(stmts) =
            syn::parse::Parser::parse2(syn::Block::parse_within, mac.tokens.clone())
        {
            for stmt in &stmts {
                self.visit_stmt(stmt);
            }
        }
    }
}

pub(super) fn violations(path: &str, ast: &syn::File) -> Vec<Violation> {
    let mut consts = Consts::default();
    consts.visit_file(ast);
    let mut functions = Functions::default();
    functions.visit_file(ast);
    let mut found = Vec::new();
    for (params, writes, block) in functions.found {
        let locals = Locals::of(params, writes, block);
        let reader = Reader {
            locals: &locals,
            consts: &consts.0,
        };
        let mut walk = Statements {
            path,
            reader: &reader,
            found: Vec::new(),
        };
        walk.visit_block(block);
        found.extend(walk.found);
    }
    found
}
