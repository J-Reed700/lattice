//! What a file imports from sqlx, what it defines, and the handle types its types name.
use super::{import_leaves, HANDLES};
use crate::{item_attrs, test_only};
use std::collections::{HashMap, HashSet};
use syn::{
    visit::{self, Visit},
    Item, Signature, Type,
};

/// What the file imports from sqlx, what it imports from elsewhere, and what it defines.
#[derive(Default)]
pub(super) struct Imports {
    named: HashMap<String, Vec<String>>,
    foreign: HashSet<String>,
    local: HashSet<String>,
    aliases: HashMap<String, Type>,
}

impl Imports {
    pub fn of(ast: &syn::File) -> Self {
        let mut imports = Self::default();
        imports.visit_file(ast);
        imports
    }

    fn is_handle(&self, segments: &[String]) -> bool {
        let Some(first) = segments.first() else {
            return false;
        };
        if let Some(target) = self.named.get(first) {
            return target
                .iter()
                .chain(segments.iter().skip(1))
                .last()
                .is_some_and(|last| HANDLES.contains(&last.as_str()));
        }
        let Some(last) = segments.last() else {
            return false;
        };
        HANDLES.contains(&last.as_str())
            && (first == "sqlx"
                || (segments.len() == 1
                    && !self.foreign.contains(first)
                    && !self.local.contains(first)))
    }
}

impl<'ast> Visit<'ast> for Imports {
    fn visit_item(&mut self, item: &'ast Item) {
        if item_attrs(item).is_some_and(test_only) {
            return;
        }
        let defined = match item {
            Item::Struct(item) => Some(&item.ident),
            Item::Enum(item) => Some(&item.ident),
            Item::Union(item) => Some(&item.ident),
            Item::Type(item) => {
                self.aliases
                    .insert(item.ident.to_string(), (*item.ty).clone());
                Some(&item.ident)
            }
            Item::Trait(item) => Some(&item.ident),
            _ => None,
        };
        if let Some(ident) = defined {
            self.local.insert(ident.to_string());
        }
        visit::visit_item(self, item);
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        for leaf in import_leaves(&item.tree) {
            let first = leaf.path.first().map(String::as_str);
            if first == Some("sqlx") {
                if !leaf.glob {
                    self.named.insert(leaf.alias, leaf.path);
                }
                continue;
            }
            let passes_through = !leaf.glob
                && matches!(first, Some("super" | "self"))
                && leaf.path.last() == Some(&leaf.alias);
            if !passes_through {
                self.foreign.insert(leaf.alias);
            }
        }
    }
}

/// The first handle type named anywhere in a type, and its line.
struct Finder<'a> {
    imports: &'a Imports,
    seen: Vec<String>,
    found: Option<(String, usize)>,
}

impl Finder<'_> {
    fn through_alias(&self, name: &str) -> bool {
        if self.seen.iter().any(|seen| seen == name) {
            return false;
        }
        let Some(target) = self.imports.aliases.get(name) else {
            return false;
        };
        let mut inner = Finder {
            imports: self.imports,
            seen: self.seen.clone(),
            found: None,
        };
        inner.seen.push(name.to_string());
        inner.visit_type(target);
        inner.found.is_some()
    }
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if self.found.is_none() {
            let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
            let line = path
                .segments
                .first()
                .map_or(1, |segment| segment.ident.span().start().line);
            if self.imports.is_handle(&segments) {
                self.found = Some((segments.join("::"), line));
            } else if let [name] = segments.as_slice() {
                if self.through_alias(name) {
                    self.found = Some((name.clone(), line));
                }
            }
        }
        visit::visit_path(self, path);
    }
}

pub(super) fn find_handle(imports: &Imports, ty: &Type) -> Option<(String, usize)> {
    let mut finder = Finder {
        imports,
        seen: Vec::new(),
        found: None,
    };
    finder.visit_type(ty);
    finder.found
}

/// The first handle among the argument types of the closures a signature accepts: `Fn`,
/// `FnMut` and `FnOnce` bounds, and fn-pointer types.
struct Closures<'a> {
    imports: &'a Imports,
    found: Option<(String, usize)>,
}

impl Closures<'_> {
    fn inputs<'i>(&mut self, inputs: impl Iterator<Item = &'i Type>) {
        for ty in inputs {
            if self.found.is_none() {
                self.found = find_handle(self.imports, ty);
            }
        }
    }
}

impl<'ast> Visit<'ast> for Closures<'_> {
    fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
        if let syn::PathArguments::Parenthesized(arguments) = &segment.arguments {
            if matches!(
                segment.ident.to_string().as_str(),
                "Fn" | "FnMut" | "FnOnce"
            ) {
                self.inputs(arguments.inputs.iter());
            }
        }
        visit::visit_path_segment(self, segment);
    }
    fn visit_type_bare_fn(&mut self, ty: &'ast syn::TypeBareFn) {
        self.inputs(ty.inputs.iter().map(|argument| &argument.ty));
        visit::visit_type_bare_fn(self, ty);
    }
}

pub(super) fn closure_handle(imports: &Imports, signature: &Signature) -> Option<(String, usize)> {
    let mut finder = Closures {
        imports,
        found: None,
    };
    finder.visit_generics(&signature.generics);
    for input in &signature.inputs {
        finder.visit_fn_arg(input);
    }
    finder.found
}
