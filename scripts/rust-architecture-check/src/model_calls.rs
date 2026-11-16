//! The old ways to ask a model: the `model_contract::legacy` module, written
//! `legacy::…` at every use, and `model_contract`'s `answer` function. Uses
//! are counted per file (`old-model-call`) so the baseline only shrinks;
//! importing an item out of `legacy`, or renaming the module, would hide uses
//! from that count and is held at zero (`legacy-item-import`).
use crate::{
    item_attrs,
    ratchet::{
        impl_item_attrs, test_only_file, trait_item_attrs, Finding, Key, SourceFile,
        LEGACY_ITEM_IMPORT, OLD_MODEL_CALL,
    },
    test_only,
};
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use std::collections::HashSet;
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
    ImplItem, Item, TraitItem, UseTree,
};

const CONTRACT: &str = "model_contract";
const LEGACY: &str = "legacy";
const ANSWER: &str = "answer";
/// The legacy module's own file, in the core crate's source root.
const LEGACY_FILE: &str = "application/services/model_contract/legacy.rs";
const TEST_SUPPORT_MODULE: &str = "test_support";

enum Leaf {
    Name,
    Rename(String),
    Glob,
}

/// One name a `use` brings in: the full path, how it is bound and where.
struct Import {
    segments: Vec<String>,
    leaf: Leaf,
    line: usize,
}

/// A path the file writes, as a path and/or as the callee of a call.
struct Site {
    segments: Vec<String>,
    line: usize,
    path: bool,
    call: bool,
}

#[derive(Default)]
struct Scan {
    imports: Vec<Import>,
    sites: Vec<Site>,
}

impl Scan {
    fn use_tree(&mut self, tree: &UseTree, mut prefix: Vec<String>) {
        match tree {
            UseTree::Path(p) => {
                prefix.push(p.ident.to_string());
                self.use_tree(&p.tree, prefix);
            }
            UseTree::Group(g) => {
                for item in &g.items {
                    self.use_tree(item, prefix.clone());
                }
            }
            UseTree::Name(n) => {
                prefix.push(n.ident.to_string());
                self.imports.push(Import {
                    segments: prefix,
                    leaf: Leaf::Name,
                    line: n.ident.span().start().line,
                });
            }
            UseTree::Rename(r) => {
                prefix.push(r.ident.to_string());
                self.imports.push(Import {
                    segments: prefix,
                    leaf: Leaf::Rename(r.rename.to_string()),
                    line: r.ident.span().start().line,
                });
            }
            UseTree::Glob(g) => self.imports.push(Import {
                segments: prefix,
                leaf: Leaf::Glob,
                line: g.star_token.span().start().line,
            }),
        }
    }
}

impl<'ast> Visit<'ast> for Scan {
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
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if item.ident != TEST_SUPPORT_MODULE {
            visit::visit_item_mod(self, item);
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
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.use_tree(&item.tree, Vec::new());
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.sites.push(Site {
            segments: path.segments.iter().map(|s| s.ident.to_string()).collect(),
            line: path.span().start().line,
            path: true,
            call: false,
        });
        visit::visit_path(self, path);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(callee) = &*call.func {
            if callee.qself.is_none() {
                self.sites.push(Site {
                    segments: callee
                        .path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect(),
                    line: callee.path.span().start().line,
                    path: false,
                    call: true,
                });
            }
        }
        visit::visit_expr_call(self, call);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        visit::visit_macro(self, mac);
        macro_sites(mac.tokens.clone(), &mut self.sites);
    }
}

/// Paths in unparsed macro input, each with whether a `(...)` follows it.
/// A name after `.` or `fn` is a method or a definition, not a path.
fn macro_sites(tokens: TokenStream, sites: &mut Vec<Site>) {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let separator = |at: usize| {
        matches!(trees.get(at), Some(TokenTree::Punct(p)) if p.as_char() == ':')
            && matches!(trees.get(at + 1), Some(TokenTree::Punct(p)) if p.as_char() == ':')
    };
    let mut at = 0;
    while at < trees.len() {
        match &trees[at] {
            TokenTree::Group(group) => {
                macro_sites(group.stream(), sites);
                at += 1;
            }
            TokenTree::Ident(first) => {
                let line = first.span().start().line;
                let declared = at > 0
                    && match &trees[at - 1] {
                        TokenTree::Punct(p) => p.as_char() == '.',
                        TokenTree::Ident(keyword) => keyword == "fn",
                        _ => false,
                    };
                let mut segments = vec![first.to_string()];
                at += 1;
                while separator(at) {
                    let Some(TokenTree::Ident(next)) = trees.get(at + 2) else {
                        break;
                    };
                    segments.push(next.to_string());
                    at += 3;
                }
                let call = matches!(
                    trees.get(at),
                    Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis
                );
                if !declared {
                    sites.push(Site {
                        segments,
                        line,
                        path: true,
                        call,
                    });
                }
            }
            _ => at += 1,
        }
    }
}

/// The index just after `model_contract::legacy` in a path, if it has it.
fn after_legacy(segments: &[String]) -> Option<usize> {
    segments
        .windows(2)
        .position(|pair| pair[0] == CONTRACT && pair[1] == LEGACY)
        .map(|at| at + 2)
}

/// Findings for one production file: old model-call symbols counted, and
/// imports that would hide them.
pub fn findings(file: &SourceFile) -> Vec<Finding> {
    if test_only_file(&file.relative) || (file.prefix.is_empty() && file.relative == LEGACY_FILE) {
        return Vec::new();
    }
    let mut scan = Scan::default();
    scan.visit_file(&file.ast);
    let shown = format!("{}{}", file.prefix, file.relative);
    let at = |line: usize| format!("{shown}:{line}");
    let mut found = Vec::new();
    let mut module = false;
    let mut answers = HashSet::new();
    let mut contract_glob = false;
    for import in &scan.imports {
        let segments = &import.segments;
        if let Some(rest) = after_legacy(segments) {
            let tail = &segments[rest..];
            let module_only = match import.leaf {
                Leaf::Name => tail.is_empty() || tail == ["self"],
                Leaf::Rename(_) | Leaf::Glob => false,
            };
            if module_only {
                module = true;
            } else {
                let glob = if matches!(import.leaf, Leaf::Glob) {
                    "::*"
                } else {
                    ""
                };
                let alias = match &import.leaf {
                    Leaf::Rename(name) => format!(" as {name}"),
                    _ => String::new(),
                };
                found.push(Finding {
                    key: Key::new(LEGACY_ITEM_IMPORT, &shown, LEGACY),
                    location: format!("{} {}{glob}{alias}", at(import.line), segments.join("::")),
                });
            }
        } else if segments.last().is_some_and(|last| last == CONTRACT) {
            contract_glob |= matches!(import.leaf, Leaf::Glob);
        } else if segments.len() >= 2 && segments[segments.len() - 2] == CONTRACT {
            let name = &segments[segments.len() - 1];
            match &import.leaf {
                Leaf::Name if name == ANSWER => {
                    answers.insert(ANSWER.to_string());
                }
                Leaf::Rename(alias) if name == ANSWER => {
                    answers.insert(alias.clone());
                }
                _ => {}
            }
        }
    }
    let is_answer = |segments: &[String]| match segments {
        [name] => answers.contains(name) || (contract_glob && name == ANSWER),
        [.., contract, name] => contract == CONTRACT && name == ANSWER,
        [] => false,
    };
    for site in &scan.sites {
        let segments = &site.segments;
        if site.path
            && ((module && segments.first().is_some_and(|first| first == LEGACY))
                || after_legacy(segments).is_some())
        {
            found.push(Finding {
                key: Key::new(OLD_MODEL_CALL, &shown, LEGACY),
                location: at(site.line),
            });
        }
        if site.call && is_answer(segments) {
            found.push(Finding {
                key: Key::new(OLD_MODEL_CALL, &shown, ANSWER),
                location: at(site.line),
            });
        }
    }
    found
}

#[cfg(test)]
mod tests;
