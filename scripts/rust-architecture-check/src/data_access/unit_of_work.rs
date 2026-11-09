//! Rule 3(e): while a function holds a write transaction, every `.await` passes
//! the transaction or is a method call on it. A write transaction holds the
//! database's only write lock, so any other await makes every other writer wait.
use super::{
    rule3::{binding, unit_params, Kind},
    Exception, Exempt, Violation,
};
use crate::{
    item_attrs,
    ratchet::{impl_item_attrs, trait_item_attrs},
    test_only,
};
use quote::ToTokens;
use syn::{
    visit::{self, Visit},
    Expr, ImplItem, Item, TraitItem,
};

/// A function with a body: its name, the `&mut WriteTx` parameters, and the body.
type Function<'ast> = (String, Vec<String>, &'ast syn::Block);

#[derive(Default)]
struct Functions<'ast> {
    found: Vec<Function<'ast>>,
}

impl<'ast> Functions<'ast> {
    fn add(&mut self, signature: &syn::Signature, block: &'ast syn::Block) {
        let writes = unit_params(signature)
            .into_iter()
            .filter(|(_, kind)| *kind == Kind::Write)
            .map(|(name, _)| name)
            .collect();
        self.found
            .push((signature.ident.to_string(), writes, block));
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

/// Whether the expression is the result of `begin_write()`, through `.await`, `?` and adapters.
fn begins(expr: &Expr) -> bool {
    match expr {
        Expr::Await(awaited) => begins(&awaited.base),
        Expr::Try(attempt) => begins(&attempt.expr),
        Expr::Paren(inner) => begins(&inner.expr),
        Expr::MethodCall(call) => call.method == "begin_write" || begins(&call.receiver),
        Expr::Call(call) => matches!(
            &*call.func,
            Expr::Path(function)
                if function.path.segments.last().is_some_and(|last| last.ident == "begin_write")
        ),
        _ => false,
    }
}

/// What an awaited expression is called in the message.
fn label(base: &Expr) -> String {
    match base {
        Expr::MethodCall(call) => format!("{}()", call.method),
        Expr::Call(call) => match &*call.func {
            Expr::Path(function) => {
                let names: Vec<String> = function
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect();
                format!("{}()", names.join("::"))
            }
            _ => "a future".to_string(),
        },
        Expr::Try(attempt) => label(&attempt.expr),
        Expr::Paren(inner) => label(&inner.expr),
        _ => "a future".to_string(),
    }
}

/// Walks one function body, tracking the write transactions open at each point.
struct Under<'a> {
    path: &'a str,
    name: &'a str,
    /// Parameters typed `&mut WriteTx`: open for the whole body.
    held: Vec<String>,
    /// Bindings of `begin_write()` results not yet committed or rolled back.
    open: Vec<String>,
    found: Vec<Violation>,
}

impl Under<'_> {
    fn mentions(&self, tokens: &impl ToTokens) -> bool {
        let text = tokens.to_token_stream().to_string();
        text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|word| self.held.iter().chain(&self.open).any(|name| name == word))
    }

    /// Whether any argument of a call in the chain, or the chain's receiver, is the transaction.
    fn passes(&self, expr: &Expr) -> bool {
        match expr {
            Expr::MethodCall(call) => {
                call.args.iter().any(|arg| self.mentions(arg)) || self.passes(&call.receiver)
            }
            Expr::Call(call) => call.args.iter().any(|arg| self.mentions(arg)),
            Expr::Await(awaited) => self.passes(&awaited.base),
            Expr::Try(attempt) => self.passes(&attempt.expr),
            Expr::Paren(inner) => self.passes(&inner.expr),
            Expr::Field(field) => self.passes(&field.base),
            Expr::Reference(reference) => self.passes(&reference.expr),
            Expr::Unary(unary) => self.passes(&unary.expr),
            Expr::Path(_) | Expr::Macro(_) => self.mentions(expr),
            _ => false,
        }
    }

    /// Leaves a scope: bindings begun inside it end, and a commit inside a
    /// scope that returns closes the transaction for that scope only.
    fn leave(&mut self, before: Vec<String>, returns: bool) {
        let open = std::mem::take(&mut self.open);
        self.open = before
            .into_iter()
            .filter(|name| returns || open.contains(name))
            .collect();
    }
}

impl<'ast> Visit<'ast> for Under<'_> {
    fn visit_item(&mut self, _: &'ast Item) {}
    fn visit_block(&mut self, block: &'ast syn::Block) {
        let before = self.open.clone();
        visit::visit_block(self, block);
        let returns = block
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, syn::Stmt::Expr(Expr::Return(_), _)));
        self.leave(before, returns);
    }
    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        let before = self.open.clone();
        visit::visit_arm(self, arm);
        self.leave(before, matches!(&*arm.body, Expr::Return(_)));
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if test_only(&local.attrs) {
            return;
        }
        visit::visit_local(self, local);
        if let (Some(init), Some(name)) = (&local.init, binding(&local.pat)) {
            if begins(&init.expr) {
                self.open.push(name);
            }
        }
    }
    fn visit_expr_await(&mut self, expr: &'ast syn::ExprAwait) {
        let held = !self.held.is_empty() || !self.open.is_empty();
        if held && !matches!(&*expr.base, Expr::Async(_)) && !self.passes(&expr.base) {
            self.found.push(Violation {
                rule: 3,
                path: self.path.to_string(),
                line: expr.await_token.span.start().line,
                message: format!(
                    "{} awaits {} while its write transaction is open; pass the transaction \
                     to it, or move the await before begin_write() or after commit()",
                    self.name,
                    label(&expr.base)
                ),
            });
        }
        visit::visit_expr_await(self, expr);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        visit::visit_expr_method_call(self, call);
        if call.method == "commit" || call.method == "rollback" {
            let receiver = call.receiver.to_token_stream().to_string();
            let words: Vec<&str> = receiver
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .collect();
            self.open.retain(|name| !words.contains(&name.as_str()));
        }
    }
}

pub fn violations(path: &str, ast: &syn::File, exceptions: &[Exception]) -> Vec<Violation> {
    let mut functions = Functions::default();
    functions.visit_file(ast);
    let mut found = Vec::new();
    for (name, held, block) in functions.found {
        if exceptions.iter().any(|(file, function, exempt, _)| {
            *file == path && *function == name && *exempt == Exempt::Awaits
        }) {
            continue;
        }
        let mut under = Under {
            path,
            name: &name,
            held,
            open: Vec::new(),
            found: Vec::new(),
        };
        under.visit_block(block);
        found.extend(under.found);
    }
    found
}
