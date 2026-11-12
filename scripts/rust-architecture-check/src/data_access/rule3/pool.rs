//! Whether a call's argument is the pool, decided by the types and bindings in the file
//! first and by the spelling of the name last.
use super::Arg;
use quote::ToTokens;
use std::collections::{HashMap, HashSet};
use syn::{
    visit::{self, Visit},
    Expr, Type,
};

const POOL_TYPES: [&str; 4] = ["Database", "SqlitePool", "PgPool", "Pool"];
const POOL_CALLS: [&str; 3] = ["database", "db_pool", "sqlx_pool"];
const ALIAS_CALLS: [&str; 4] = ["clone", "as_ref", "as_deref", "to_owned"];

struct Named {
    found: bool,
}

impl<'ast> Visit<'ast> for Named {
    fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
        if POOL_TYPES.contains(&segment.ident.to_string().as_str()) {
            self.found = true;
        }
        visit::visit_path_segment(self, segment);
    }
}

/// Whether a type is, or holds behind `&`, `Arc` or `Option`, a pool.
pub(super) fn pool_type(ty: &Type) -> bool {
    let mut named = Named { found: false };
    named.visit_type(ty);
    named.found
}

/// For each struct declared in the file, the names of its fields that hold a pool.
#[derive(Default)]
struct Structs {
    fields: HashMap<String, HashSet<String>>,
}

impl<'ast> Visit<'ast> for Structs {
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        let pools = item
            .fields
            .iter()
            .filter(|field| pool_type(&field.ty))
            .filter_map(|field| field.ident.as_ref().map(ToString::to_string))
            .collect();
        self.fields.insert(item.ident.to_string(), pools);
        visit::visit_item_struct(self, item);
    }
}

pub(super) fn pool_fields(ast: &syn::File) -> HashMap<String, HashSet<String>> {
    let mut structs = Structs::default();
    structs.visit_file(ast);
    structs.fields
}

fn mentions(text: &str, needle: &str) -> bool {
    text.match_indices(needle).any(|(at, _)| {
        !text
            .get(at + needle.len()..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|next| next.is_alphanumeric() || next == '_')
    })
}

/// Whether the argument is the pool: it names a binding known to be one, reads a pool field
/// of `self`, unwraps a pool, checks out a connection or calls the container's accessor.
pub(super) fn pool_text(arg: &Arg, names: &HashSet<String>, fields: &HashSet<String>) -> bool {
    arg.words.iter().any(|word| names.contains(word))
        || fields
            .iter()
            .any(|field| mentions(&arg.squashed, &format!("self.{field}")))
        || arg.squashed.contains("acquire()")
        || POOL_CALLS
            .iter()
            .any(|call| arg.squashed.contains(&format!("{call}()")))
}

/// The expression a `let` initializer only renames, copies or clones, as an argument.
pub(super) fn alias(expr: &Expr) -> Option<Arg> {
    match expr {
        Expr::Reference(inner) => alias(&inner.expr),
        Expr::Paren(inner) => alias(&inner.expr),
        Expr::Path(_) | Expr::Field(_) => Some(Arg::new(&expr.to_token_stream().to_string())),
        Expr::MethodCall(call) if call.args.is_empty() => {
            let method = call.method.to_string();
            if ALIAS_CALLS.contains(&method.as_str()) {
                alias(&call.receiver)
            } else if POOL_CALLS.contains(&method.as_str()) {
                Some(Arg::new(&expr.to_token_stream().to_string()))
            } else {
                None
            }
        }
        Expr::Call(call) if call.args.len() == 1 => {
            let clones = matches!(&*call.func, Expr::Path(path)
                if path.path.segments.last().is_some_and(|last| last.ident == "clone"));
            call.args.first().filter(|_| clones).and_then(alias)
        }
        _ => None,
    }
}
