//! Where every `mod` of a crate is visible, what its items hand out, and what its `use`
//! declarations re-export.
use super::super::HANDOUT_EXEMPTIONS;
use super::{
    data_scope, exempted, import_leaves,
    imports::Imports,
    items::{self, Hand, Reach},
    resolve,
};
use crate::test_only;
use std::collections::{BTreeMap, HashMap, HashSet};
use syn::{Item, Visibility};

/// A `use` that is not private, as written inside its module.
struct Reexport {
    path: Vec<String>,
    alias: String,
    glob: bool,
}

/// What a crate's modules declare: where each `mod` is visible, the handles its items hand
/// out and the `use` declarations that are not private.
#[derive(Default)]
pub(in super::super) struct ModuleScopes {
    scopes: HashMap<Vec<String>, Vec<String>>,
    public: HashSet<Vec<String>>,
    hands: BTreeMap<(Vec<String>, String), Vec<Hand>>,
    reexports: HashMap<Vec<String>, Vec<Reexport>>,
}

/// Where each `mod` declared in the files (given by repository-relative path and module) is
/// visible, and what the files' items hand out.
pub(in super::super) fn module_scopes<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [String], &'a syn::File)>,
) -> ModuleScopes {
    let mut scopes = ModuleScopes::default();
    for (path, module, ast) in files {
        scopes.declare(&ast.items, module);
        scopes.reexport(&ast.items, module);
        let imports = Imports::of(ast);
        let exempt = |owner: &str, item: &str| {
            HANDOUT_EXEMPTIONS
                .iter()
                .any(|entry| entry.0 == path && exempted(entry, owner, item))
        };
        for hand in items::hands(ast, module, &imports, &exempt) {
            let key = (hand.module.clone(), hand.key.clone());
            scopes.hands.entry(key).or_default().push(hand);
        }
    }
    scopes
}

impl ModuleScopes {
    fn declare(&mut self, items: &[Item], module: &[String]) {
        for item in items {
            let Item::Mod(declaration) = item else {
                continue;
            };
            if test_only(&declaration.attrs) {
                continue;
            }
            let mut inner = module.to_vec();
            inner.push(declaration.ident.to_string());
            self.scopes
                .insert(inner.clone(), resolve(&declaration.vis, module));
            if matches!(declaration.vis, Visibility::Public(_)) {
                self.public.insert(inner.clone());
            }
            if let Some((_, items)) = &declaration.content {
                self.declare(items, &inner);
            }
        }
    }

    fn reexport(&mut self, items: &[Item], module: &[String]) {
        for item in items {
            match item {
                Item::Use(declaration)
                    if !matches!(declaration.vis, Visibility::Inherited)
                        && !test_only(&declaration.attrs) =>
                {
                    for leaf in import_leaves(&declaration.tree) {
                        self.reexports
                            .entry(module.to_vec())
                            .or_default()
                            .push(Reexport {
                                path: leaf.path,
                                alias: leaf.alias,
                                glob: leaf.glob,
                            });
                    }
                }
                Item::Mod(declaration) if !test_only(&declaration.attrs) => {
                    if let Some((_, inner)) = &declaration.content {
                        let mut path = module.to_vec();
                        path.push(declaration.ident.to_string());
                        self.reexport(inner, &path);
                    }
                }
                _ => {}
            }
        }
    }

    /// The deepest of the scope and the scopes the modules around `module` are declared in.
    pub(super) fn narrowed(&self, mut scope: Vec<String>, module: &[String]) -> Vec<String> {
        for end in 1..=module.len() {
            let declared = module.get(..end).and_then(|prefix| self.scopes.get(prefix));
            if let Some(declared) = declared {
                if declared.len() > scope.len() {
                    scope = declared.clone();
                }
            }
        }
        scope
    }

    /// Whether every module around `module` is declared `pub`.
    fn exposed(&self, module: &[String]) -> bool {
        (1..=module.len()).all(|end| {
            module
                .get(..end)
                .is_some_and(|prefix| self.public.contains(prefix))
        })
    }

    /// Whether something visible in `scope` reaches code outside data modules. In the store
    /// crate, which is all data, that is anything `pub` all the way up from the crate root.
    pub(super) fn reaches(
        &self,
        store: bool,
        scope: &[String],
        public: bool,
        module: &[String],
    ) -> bool {
        if store {
            public && scope.is_empty() && self.exposed(module)
        } else {
            !data_scope(scope)
        }
    }

    pub(super) fn reaches_hand(&self, store: bool, hand: &Hand) -> bool {
        match &hand.reach {
            Reach::Everywhere => true,
            Reach::Vis { scope, public } => {
                let scope = self.narrowed(scope.clone(), &hand.module);
                self.reaches(store, &scope, *public, &hand.module)
            }
        }
    }

    /// Whether a re-export in `module`, visible in `site`, reaches outside data modules
    /// with the handle. The modules around the item do not narrow it: the re-export is
    /// another path to it.
    pub(super) fn reexport_reaches(
        &self,
        store: bool,
        site: (&[String], bool),
        module: &[String],
        hand: &Hand,
    ) -> bool {
        let (site, public) = site;
        match &hand.reach {
            Reach::Everywhere => self.reaches(store, site, public, module),
            Reach::Vis { scope, public: own } => {
                let deeper = if scope.len() > site.len() {
                    scope.as_slice()
                } else {
                    site
                };
                self.reaches(store, deeper, public && *own, module)
            }
        }
    }

    fn is_module(&self, path: &[String]) -> bool {
        path.is_empty() || self.scopes.contains_key(path)
    }

    /// The module and the item a `use` path names when written in `module`; for a glob
    /// the whole path is a module and there is no item.
    pub(super) fn target(
        &self,
        module: &[String],
        path: &[String],
        glob: bool,
    ) -> Option<(Vec<String>, Option<String>)> {
        let mut base = module.to_vec();
        let mut rest = path;
        match path.first().map(String::as_str)? {
            "crate" => {
                base.clear();
                rest = path.get(1..)?;
            }
            "self" => rest = path.get(1..)?,
            "super" => {
                while rest.first().is_some_and(|segment| segment == "super") {
                    base.pop()?;
                    rest = rest.get(1..)?;
                }
            }
            first => {
                let mut child = base.clone();
                child.push(first.to_string());
                if !self.scopes.contains_key(&child) {
                    return None;
                }
            }
        }
        let (modules, item) = if glob {
            (rest, None)
        } else {
            let (last, init) = rest.split_last()?;
            (init, Some(last.clone()))
        };
        for segment in modules {
            base.push(segment.clone());
            if !self.is_module(&base) {
                return None;
            }
        }
        Some((base, item))
    }

    /// The handles reachable through `name` in `module`, or through every name when there is
    /// none, following the re-exports that are not private.
    pub(super) fn reexported(
        &self,
        module: &[String],
        name: Option<&str>,
        seen: &mut HashSet<(Vec<String>, Option<String>)>,
    ) -> Vec<&Hand> {
        if !seen.insert((module.to_vec(), name.map(str::to_string))) {
            return Vec::new();
        }
        let mut found: Vec<&Hand> = Vec::new();
        match name {
            Some(name) => {
                let key = (module.to_vec(), name.to_string());
                found.extend(self.hands.get(&key).into_iter().flatten());
            }
            None => {
                for ((owner, _), hands) in &self.hands {
                    if owner.as_slice() == module {
                        found.extend(hands.iter());
                    }
                }
            }
        }
        for reexport in self.reexports.get(module).into_iter().flatten() {
            if name.is_some_and(|name| !reexport.glob && reexport.alias != name) {
                continue;
            }
            let Some((target, item)) = self.target(module, &reexport.path, reexport.glob) else {
                continue;
            };
            let next = if reexport.glob {
                name.map(str::to_string)
            } else {
                item
            };
            found.extend(self.reexported(&target, next.as_deref(), seen));
        }
        found
    }
}
