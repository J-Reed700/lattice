//! Rule 5(c): `sqlx_pool()` is called only inside data modules.
use super::super::{is_group, is_punct, walk_tokens, Violation};
use crate::{
    item_attrs,
    ratchet::{impl_item_attrs, trait_item_attrs},
    test_only,
};
use proc_macro2::{Delimiter, TokenTree};
use syn::{
    visit::{self, Visit},
    ImplItem, Item, TraitItem,
};

struct Calls<'a> {
    path: &'a str,
    found: Vec<Violation>,
}

impl Calls<'_> {
    fn add(&mut self, line: usize) {
        self.found.push(Violation {
            rule: 5,
            path: self.path.to_string(),
            line,
            message: "calls sqlx_pool() outside a data module; pass the Database to a \
                      data-module function"
                .to_string(),
        });
    }
}

impl<'ast> Visit<'ast> for Calls<'_> {
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
    fn visit_stmt(&mut self, stmt: &'ast syn::Stmt) {
        if let syn::Stmt::Local(local) = stmt {
            if test_only(&local.attrs) {
                return;
            }
        }
        visit::visit_stmt(self, stmt);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "sqlx_pool" {
            self.add(call.method.span().start().line);
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(function) = &*call.func {
            if let Some(last) = function.path.segments.last() {
                if last.ident == "sqlx_pool" {
                    self.add(last.ident.span().start().line);
                }
            }
        }
        visit::visit_expr_call(self, call);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        visit::visit_macro(self, mac);
        let mut lines = Vec::new();
        walk_tokens(mac.tokens.clone(), &mut |tokens, index| {
            let Some(TokenTree::Ident(ident)) = tokens.get(index) else {
                return;
            };
            if ident != "sqlx_pool" || !is_group(tokens.get(index + 1), Delimiter::Parenthesis) {
                return;
            }
            let before = index.checked_sub(1).and_then(|i| tokens.get(i));
            if is_punct(before, '.') || is_punct(before, ':') {
                lines.push(ident.span().start().line);
            }
        });
        for line in lines {
            self.add(line);
        }
    }
}

/// Rule 5(c): the calls of `sqlx_pool()` in a file that is not a data module.
pub(in super::super) fn calls(path: &str, ast: &syn::File) -> Vec<Violation> {
    let mut scan = Calls {
        path,
        found: Vec::new(),
    };
    scan.visit_file(ast);
    scan.found
}
