//! Ratcheted rules. Today's violations live in `baseline.txt`; the check fails
//! when a count rises, a new (subject, target) pair appears, or a baselined
//! entry shrinks without the baseline being regenerated, so the list only
//! ever gets shorter.
//!
//! Feature cycles are derived from the feature edges rather than found in
//! source: every edge inside a strongly connected component is an entry,
//! counted by the component's size, so a new cycle or a cycle that pulls in
//! another feature fails exactly like a new reference does.
//!
//! `feature-edge` is held at zero: it has no baseline, so every reference
//! from one feature into another fails, `--write-baseline` included, unless
//! `feature-graph.txt` allows its edge. That file allows a few edges outright,
//! each with a reason: one feature built on a lower-level one (see
//! `feature_graph`). Their references leave `feature-edge` but still feed the
//! cycle check; the allowed edges must form no cycle, and one with no
//! references left is STALE. `feature-reexport` counts the `use` items through which a feature
//! hands out core's or another feature's items, since every caller that
//! imports through one becomes a feature edge of its own.
//!
//! `--list <rule> [subject]` prints every finding of a rule with its
//! location, whether or not the baseline or the allowlist covers it; a
//! normal run prints locations only for violations.
//!
//! Only production code counts: the walk follows `mod` declarations from the
//! crate roots and skips `#[cfg(test)]` modules and items, so test files are
//! never read. File length is the exception: it reads every `.rs` file below
//! the root and skips test-only files by name. The app and core source roots
//! are walked separately and their findings counted together.
//!
//! The composition root (`bootstrap`) builds the app, so it names every
//! feature and the `Container`. It must stay a leaf: only the crate entry
//! points (`main.rs`, `lib.rs`, `bin/*`) and its own files may name it.
use proc_macro2::{Spacing, TokenStream, TokenTree};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, ImplItem, Item, TraitItem, UseTree,
};

use crate::{feature_graph, item_attrs, test_only};

pub const REGENERATE: &str = "bash scripts/check-rust-layer-boundaries.sh --write-baseline";

pub const INFRASTRUCTURE_IMPORTS_FEATURE: &str = "infrastructure-imports-feature";
pub const FEATURE_EDGE: &str = "feature-edge";
pub const FEATURE_CYCLE: &str = "feature-cycle";
pub const FEATURE_REEXPORT: &str = "feature-reexport";
pub const CONTAINER_IMPORT: &str = "container-import";
pub const RAW_SPAWN: &str = "raw-spawn";
pub const GLOBAL_STATE: &str = "global-state";
pub const FILE_LENGTH: &str = "file-length";
pub const COMPOSITION_ROOT_IMPORT: &str = "composition-root-import";
const RULES: [&str; 9] = [
    INFRASTRUCTURE_IMPORTS_FEATURE,
    FEATURE_EDGE,
    FEATURE_CYCLE,
    FEATURE_REEXPORT,
    CONTAINER_IMPORT,
    COMPOSITION_ROOT_IMPORT,
    RAW_SPAWN,
    GLOBAL_STATE,
    FILE_LENGTH,
];
/// Rules held at zero. They never appear in the baseline, so any finding
/// fails, even while the baseline is being written.
const ZERO_RULES: [&str; 1] = [FEATURE_EDGE];

const RAW_SPAWNS: [&[&str]; 4] = [
    &["tokio", "spawn"],
    &["tokio", "task", "spawn"],
    &["tauri", "async_runtime", "spawn"],
    &["std", "thread", "spawn"],
];
const SPAWN_MARKER: &str = "raw-spawn:";
const SUPERVISOR: &str = "shared/runtime/";
const CONTAINER_TARGET: &str = "interfaces::di";
/// The app's composition root module: startup wiring and shutdown.
const COMPOSITION_ROOT: &str = "bootstrap";
/// The app's library crate, as its binaries name it.
const APP_LIBRARY: &str = "lattice";
/// The core crate. Its paths are another crate's, outside the app's layers.
const CORE_LIBRARY: &str = "lattice_core";

const STATE_MARKER: &str = "global-state:";
/// Macros that declare statics the parser cannot see into. `task_local!` is
/// not among them: its value is scoped to one task by whoever sets it.
const STATE_MACROS: [&str; 2] = ["thread_local", "lazy_static"];
/// Scalar types: a static of these (or a reference, array or tuple of them)
/// is a constant, since only `static mut` could change it.
const SCALARS: [&str; 17] = [
    "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
    "i128", "isize", "f32", "f64",
];
/// Wrappers that build their value from the initializer written beside them.
const LAZY_WRAPPERS: [&str; 2] = ["Lazy", "LazyLock"];
/// Types a lazily built static may hold and still be a derived constant: a
/// compiled pattern, a stemmer, or collections of them. Nothing here can
/// change behind a shared reference, so `Lazy<Regex>` needs no marker.
const IMMUTABLE_PAYLOADS: [&str; 10] = [
    "Regex", "RegexSet", "Stemmer", "Option", "Vec", "Box", "HashMap", "HashSet", "BTreeMap",
    "BTreeSet",
];

/// Lines a source file may have, counted like `wc -l`. Test-only files
/// (`tests.rs`, `*_tests.rs`, anything under a `tests/` directory) may be longer.
const MAX_LINES: usize = 800;

const HEADER: &str = "\
# Architecture ratchet baseline, read by scripts/rust-architecture-check.
#
# Each line is `<rule> <subject> <target> <count>`: production-code violations
# that existed when the file was written. CI fails when a count rises or a new
# line would appear, and also when a count falls or a line disappears, so this
# file only shrinks. After removing violations, regenerate it (and the Tauri
# command-orphan baseline) with
#
#     bash scripts/check-rust-layer-boundaries.sh --write-baseline
#
# and commit the smaller file. Do not add lines to get a change through: fix
# the dependency, or mark a deliberate detached task with a
# `// raw-spawn: <reason>` comment directly above the statement or its fn.
# State a static holds belongs in a value the composition root builds and
# passes on; only a derived constant or a hook the process owns by nature
# (a panic hook, an OS callback target) may stay global, marked with a
# `// global-state: <reason>` comment directly above the static.
# Keep a file at or under 800 lines: move inline tests to a sibling tests.rs,
# then split it by responsibility.
#
# feature-edge is held at zero and never appears here: a reference from one
# feature into another fails. Depend on a core port or move the code; only an
# edge that is one feature built on a lower-level one may instead be allowed in
# scripts/rust-architecture-check/feature-graph.txt, one
# `allow features::<a> features::<b>  # <reason>` line each. Its references
# leave feature-edge but still count toward feature-cycle. To see every site
# of a rule, baselined or not:
#
#     bash scripts/check-rust-layer-boundaries.sh --list <rule> [subject]
#
#   infrastructure-imports-feature <file> features::<b>   infrastructure naming crate::features
#   feature-edge <features::a> <features::b>              references from one feature into another,
#                                                         except edges in feature-graph.txt; held at
#                                                         zero, so never listed here
#   feature-cycle <features::a> <features::b> <n>         a feature-edge on a cycle; n features share
#                                                         its strongly connected component
#   feature-reexport <file> <lattice_core|features::b>    a `use` in a feature, visible outside it,
#                                                         that re-exports core or another feature;
#                                                         callers should import from the source
#   container-import <file> interfaces::di                the DI Container outside plugin, command,
#                                                         DI, desktop, bootstrap and interfaces files
#   composition-root-import <file> bootstrap              the composition root named outside itself
#                                                         and the entry points main.rs, lib.rs, bin/*
#   raw-spawn <file> <spawn fn>                           detached spawns outside shared/runtime
#   global-state <file> <static>                          statics that hold mutable or lazily
#                                                         built state, unless a derived constant
#   file-length <file> max-800-lines                      files over 800 lines, except tests.rs,
#                                                         *_tests.rs and files under tests/
";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub rule: String,
    pub subject: String,
    pub target: String,
}

impl Key {
    pub(crate) fn new(rule: &str, subject: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            rule: rule.to_string(),
            subject: subject.into(),
            target: target.into(),
        }
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {} -> {}", self.rule, self.subject, self.target)
    }
}

#[derive(Debug)]
pub struct Finding {
    pub key: Key,
    pub location: String,
}

pub type Counts = BTreeMap<Key, usize>;

pub struct SourceFile {
    /// Path below the source root, `/`-separated.
    pub relative: String,
    pub module: Vec<String>,
    /// False for the binary crates, whose `crate::` is not the library.
    pub library: bool,
    pub source: String,
    pub ast: syn::File,
}

type Error = Box<dyn std::error::Error>;

/// Production files reachable from the crate roots through `mod` declarations.
pub fn production_files(root: &Path) -> Result<(Vec<SourceFile>, HashSet<Vec<String>>), Error> {
    let mut crates = vec![(root.join("lib.rs"), true), (root.join("main.rs"), false)];
    let bin = root.join("bin");
    if bin.is_dir() {
        let mut entries = fs::read_dir(&bin)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort();
        for path in entries {
            if path.is_dir() && path.join("main.rs").is_file() {
                crates.push((path.join("main.rs"), false));
            } else if path.extension().is_some_and(|e| e == "rs") {
                crates.push((path, false));
            }
        }
    }
    let mut walk = Walk {
        root: root.canonicalize()?,
        seen: HashSet::new(),
        files: Vec::new(),
        library_modules: HashSet::from([Vec::new()]),
    };
    for (path, library) in crates {
        if path.is_file() {
            walk.load(&path, Vec::new(), library, true)?;
        }
    }
    Ok((walk.files, walk.library_modules))
}

struct Walk {
    root: PathBuf,
    seen: HashSet<PathBuf>,
    files: Vec<SourceFile>,
    library_modules: HashSet<Vec<String>>,
}

impl Walk {
    fn load(
        &mut self,
        path: &Path,
        module: Vec<String>,
        library: bool,
        mod_rs: bool,
    ) -> Result<(), Error> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !self.seen.insert(path.clone()) {
            return Ok(());
        }
        let source = fs::read_to_string(&path)?;
        let ast = syn::parse_file(&source).map_err(|e| format!("{}: {e}", path.display()))?;
        if test_only(&ast.attrs) {
            return Ok(());
        }
        let directory = path.parent().ok_or("source file has no parent")?;
        let child_directory = if mod_rs {
            directory.to_path_buf()
        } else {
            directory.join(path.file_stem().ok_or("source file has no name")?)
        };
        let mut children = Vec::new();
        declared_modules(
            &ast.items,
            &module,
            &child_directory,
            directory,
            &mut children,
        );
        if library {
            self.library_modules
                .extend(children.iter().map(|child| child.module.clone()));
        }
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| format!("{} is outside the source root", path.display()))?
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        self.files.push(SourceFile {
            relative,
            module: module.clone(),
            library,
            source,
            ast,
        });
        for child in children {
            if let Some((file, mod_rs)) = child.file {
                self.load(&file, child.module, library, mod_rs)?;
            }
        }
        Ok(())
    }
}

struct DeclaredModule {
    module: Vec<String>,
    /// The file and whether it resolves its own children like `mod.rs`.
    /// `None` for inline modules, whose items belong to the declaring file.
    file: Option<(PathBuf, bool)>,
}

fn declared_modules(
    items: &[Item],
    module: &[String],
    child_directory: &Path,
    attribute_directory: &Path,
    out: &mut Vec<DeclaredModule>,
) {
    for item in items {
        let Item::Mod(declaration) = item else {
            continue;
        };
        if test_only(&declaration.attrs) {
            continue;
        }
        let name = declaration.ident.to_string();
        let mut child = module.to_vec();
        child.push(name.clone());
        if let Some((_, inner)) = &declaration.content {
            let nested = child_directory.join(&name);
            declared_modules(inner, &child, &nested, &nested, out);
            out.push(DeclaredModule {
                module: child,
                file: None,
            });
            continue;
        }
        let file = match path_attribute(&declaration.attrs) {
            // A file named by #[path] resolves its own children like mod.rs.
            Some(explicit) => (attribute_directory.join(explicit), true),
            None => {
                let flat = child_directory.join(format!("{name}.rs"));
                if flat.exists() {
                    (flat, false)
                } else {
                    (child_directory.join(&name).join("mod.rs"), true)
                }
            }
        };
        out.push(DeclaredModule {
            module: child,
            file: Some(file),
        });
    }
}

fn path_attribute(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attribute| {
        if !attribute.path().is_ident("path") {
            return None;
        }
        let syn::Meta::NameValue(value) = &attribute.meta else {
            return None;
        };
        let syn::Expr::Lit(value) = &value.value else {
            return None;
        };
        let syn::Lit::Str(value) = &value.lit else {
            return None;
        };
        Some(value.value())
    })
}

/// Findings below every source root. A root named twice, as the app root is
/// while it still holds core, counts once.
pub fn findings(roots: &[&Path]) -> Result<Vec<Finding>, Error> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for root in roots {
        let canonical = root
            .canonicalize()
            .map_err(|e| format!("{}: {e}", root.display()))?;
        if !seen.insert(canonical) {
            continue;
        }
        let (files, modules) = production_files(root)?;
        found.extend(files.iter().flat_map(|file| file_findings(file, &modules)));
        found.extend(length_findings(root)?);
    }
    Ok(found)
}

/// Every `.rs` file below the root that is too long. Unlike the other rules
/// this reads files from disk rather than following `mod` declarations, so a
/// file no module reaches still counts.
pub fn length_findings(root: &Path) -> Result<Vec<Finding>, Error> {
    let mut found = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        let mut entries = fs::read_dir(&directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort();
        for path in entries {
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let relative = path
                    .strip_prefix(root)?
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                if !test_only_file(&relative) {
                    found.extend(file_length(&relative, &fs::read_to_string(&path)?));
                }
            }
        }
    }
    Ok(found)
}

/// Test-only by name: `tests.rs`, `*_tests.rs`, or a file under `tests/`.
fn test_only_file(relative: &str) -> bool {
    let mut parts = relative.rsplit('/');
    let name = parts.next().unwrap_or_default();
    name == "tests.rs" || name.ends_with("_tests.rs") || parts.any(|part| part == "tests")
}

/// One finding when a non-test file has more than `MAX_LINES` lines.
pub fn file_length(relative: &str, source: &str) -> Option<Finding> {
    let lines = source.matches('\n').count();
    (lines > MAX_LINES && !test_only_file(relative)).then(|| Finding {
        key: Key::new(FILE_LENGTH, relative, format!("max-{MAX_LINES}-lines")),
        location: format!("{relative} ({lines} lines)"),
    })
}

/// A path the file names, resolved to the library crate where possible.
struct Reference {
    path: Vec<String>,
    glob: bool,
    line: usize,
}

pub fn file_findings(file: &SourceFile, modules: &HashSet<Vec<String>>) -> Vec<Finding> {
    let mut aliases = UseAliases::default();
    aliases.visit_file(&file.ast);
    let lines: Vec<&str> = file.source.lines().collect();
    let mut collector = Collector {
        module: file.module.clone(),
        library: file.library,
        modules,
        aliases: &aliases.0,
        lines: &lines,
        anchors: Vec::new(),
        references: Vec::new(),
        reexporting: false,
        reexports: Vec::new(),
        spawns: Vec::new(),
        statics: Vec::new(),
    };
    collector.visit_file(&file.ast);

    let relative = &file.relative;
    let at = |line: usize| format!("{relative}:{line}");
    let mut found = Vec::new();
    for reference in &collector.references {
        let path = &reference.path;
        if file.library && path.first().is_some_and(|s| s == "features") {
            let target = path.get(1).map_or_else(
                || "features".to_string(),
                |feature| format!("features::{feature}"),
            );
            // The site and the path it names, so a list shows what to move.
            let named = format!(
                "{} {}{}",
                at(reference.line),
                path.join("::"),
                if reference.glob { "::*" } else { "" }
            );
            if file.module.first().is_some_and(|s| s == "infrastructure") {
                found.push(Finding {
                    key: Key::new(INFRASTRUCTURE_IMPORTS_FEATURE, relative, target.clone()),
                    location: named.clone(),
                });
            }
            if let (Some(owner), Some(other)) = (feature_of(&file.module), path.get(1)) {
                if owner != other {
                    found.push(Finding {
                        key: Key::new(FEATURE_EDGE, format!("features::{owner}"), target),
                        location: named,
                    });
                }
            }
        }
        if names_container(path, reference.glob) && !may_import_container(relative) {
            found.push(Finding {
                key: Key::new(CONTAINER_IMPORT, relative, CONTAINER_TARGET),
                location: at(reference.line),
            });
        }
        if names_composition_root(path, reference.glob) && !may_import_composition_root(relative) {
            found.push(Finding {
                key: Key::new(COMPOSITION_ROOT_IMPORT, relative, COMPOSITION_ROOT),
                location: at(reference.line),
            });
        }
    }
    for (target, line, named) in collector.reexports {
        found.push(Finding {
            key: Key::new(FEATURE_REEXPORT, relative, target),
            location: format!("{} {named}", at(line)),
        });
    }
    if !relative.starts_with(SUPERVISOR) {
        for (target, line) in collector.spawns {
            found.push(Finding {
                key: Key::new(RAW_SPAWN, relative, target),
                location: at(line),
            });
        }
    }
    for (name, line) in collector.statics {
        found.push(Finding {
            key: Key::new(GLOBAL_STATE, relative, name),
            location: at(line),
        });
    }
    found
}

/// Whether a static of this type is a constant: a scalar, or a reference,
/// array, slice or tuple of constants.
fn plain_data(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Reference(_) => true,
        syn::Type::Array(array) => plain_data(&array.elem),
        syn::Type::Slice(slice) => plain_data(&slice.elem),
        syn::Type::Tuple(tuple) => tuple.elems.iter().all(plain_data),
        syn::Type::Paren(inner) => plain_data(&inner.elem),
        syn::Type::Group(inner) => plain_data(&inner.elem),
        syn::Type::Path(path) => {
            path.qself.is_none()
                && path.path.segments.len() == 1
                && path.path.segments.iter().all(|segment| {
                    segment.arguments.is_empty()
                        && SCALARS.contains(&segment.ident.to_string().as_str())
                })
        }
        _ => false,
    }
}

/// Whether a static of this type is a derived constant: a `Lazy` or
/// `LazyLock` whose initializer sits beside it and whose value cannot change
/// once built.
fn derived_constant(ty: &syn::Type) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };
    let Some(wrapper) = path.path.segments.last() else {
        return false;
    };
    LAZY_WRAPPERS.contains(&wrapper.ident.to_string().as_str())
        && type_arguments(&wrapper.arguments).is_some_and(|arguments| {
            !arguments.is_empty() && arguments.into_iter().all(immutable_payload)
        })
}

/// The type arguments of a path segment; `None` when it takes anything else.
fn type_arguments(arguments: &syn::PathArguments) -> Option<Vec<&syn::Type>> {
    match arguments {
        syn::PathArguments::None => Some(Vec::new()),
        syn::PathArguments::AngleBracketed(bracketed) => bracketed
            .args
            .iter()
            .filter(|argument| !matches!(argument, syn::GenericArgument::Lifetime(_)))
            .map(|argument| match argument {
                syn::GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
            .collect(),
        syn::PathArguments::Parenthesized(_) => None,
    }
}

fn immutable_payload(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Reference(reference) => immutable_payload(&reference.elem),
        syn::Type::Array(array) => immutable_payload(&array.elem),
        syn::Type::Slice(slice) => immutable_payload(&slice.elem),
        syn::Type::Tuple(tuple) => tuple.elems.iter().all(immutable_payload),
        syn::Type::Paren(inner) => immutable_payload(&inner.elem),
        syn::Type::Group(inner) => immutable_payload(&inner.elem),
        syn::Type::Path(path) if path.qself.is_none() => {
            path.path.segments.last().is_some_and(|segment| {
                let name = segment.ident.to_string();
                (IMMUTABLE_PAYLOADS.contains(&name.as_str()) || SCALARS.contains(&name.as_str()))
                    && type_arguments(&segment.arguments)
                        .is_some_and(|arguments| arguments.into_iter().all(immutable_payload))
            })
        }
        _ => false,
    }
}

fn feature_of(module: &[String]) -> Option<&String> {
    match module {
        [features, owner, ..] if features == "features" => Some(owner),
        _ => None,
    }
}

fn names_container(path: &[String], glob: bool) -> bool {
    let starts = |prefix: &[&str]| {
        path.len() >= prefix.len() && path.iter().zip(prefix).all(|(a, b)| a == b)
    };
    starts(&["interfaces", "di"])
        || starts(&["interfaces", "Container"])
        || starts(&["Container"])
        || (glob && (path.is_empty() || path == ["interfaces"]))
}

/// Adapters may hold the Container; business code receives what it needs.
pub fn may_import_container(relative: &str) -> bool {
    let parts: Vec<&str> = relative.split('/').collect();
    let Some((file, directories)) = parts.split_last() else {
        return false;
    };
    let stem = file.trim_end_matches(".rs");
    stem.starts_with("plugin")
        || stem.starts_with("commands")
        || matches!(*file, "di.rs" | "desktop.rs" | "main.rs" | "lib.rs")
        || directories
            .iter()
            .any(|d| d.starts_with("plugin") || d.starts_with("commands") || *d == "di")
        || matches!(
            directories.first(),
            Some(&"interfaces" | &"desktop_e2e" | &COMPOSITION_ROOT)
        )
}

fn names_composition_root(path: &[String], glob: bool) -> bool {
    path.first().is_some_and(|s| s == COMPOSITION_ROOT) || (glob && path.is_empty())
}

/// The composition root is a leaf: the entry points start it, and nothing
/// else may depend on the code that wires everything together.
pub fn may_import_composition_root(relative: &str) -> bool {
    let first = relative.split('/').next().unwrap_or_default();
    matches!(relative, "main.rs" | "lib.rs")
        || first == "bin"
        || first.trim_end_matches(".rs") == COMPOSITION_ROOT
}

/// Local names that `use` declarations bind, mapped to the path they import.
#[derive(Default)]
struct UseAliases(HashMap<String, Vec<String>>);

impl UseAliases {
    fn tree(&mut self, tree: &UseTree, mut prefix: Vec<String>) {
        match tree {
            UseTree::Path(p) => {
                prefix.push(p.ident.to_string());
                self.tree(&p.tree, prefix);
            }
            UseTree::Group(g) => {
                for item in &g.items {
                    self.tree(item, prefix.clone());
                }
            }
            UseTree::Name(n) if n.ident == "self" => {
                if let Some(last) = prefix.last() {
                    self.0.insert(last.clone(), prefix);
                }
            }
            UseTree::Name(n) => {
                prefix.push(n.ident.to_string());
                self.0.insert(n.ident.to_string(), prefix);
            }
            UseTree::Rename(r) => {
                if r.ident != "self" {
                    prefix.push(r.ident.to_string());
                }
                self.0.insert(r.rename.to_string(), prefix);
            }
            UseTree::Glob(_) => {}
        }
    }
}

impl<'ast> Visit<'ast> for UseAliases {
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
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.tree(&item.tree, Vec::new());
    }
}

fn impl_item_attrs(item: &ImplItem) -> Option<&[Attribute]> {
    match item {
        ImplItem::Const(i) => Some(&i.attrs),
        ImplItem::Fn(i) => Some(&i.attrs),
        ImplItem::Type(i) => Some(&i.attrs),
        ImplItem::Macro(i) => Some(&i.attrs),
        _ => None,
    }
}

fn trait_item_attrs(item: &TraitItem) -> Option<&[Attribute]> {
    match item {
        TraitItem::Const(i) => Some(&i.attrs),
        TraitItem::Fn(i) => Some(&i.attrs),
        TraitItem::Type(i) => Some(&i.attrs),
        TraitItem::Macro(i) => Some(&i.attrs),
        _ => None,
    }
}

struct Collector<'a> {
    module: Vec<String>,
    library: bool,
    modules: &'a HashSet<Vec<String>>,
    aliases: &'a HashMap<String, Vec<String>>,
    lines: &'a [&'a str],
    /// First lines of the enclosing statements and fns, innermost last.
    anchors: Vec<usize>,
    references: Vec<Reference>,
    /// Whether the `use` being walked is visible outside the owning feature.
    reexporting: bool,
    /// Re-exports of core or another feature: target, line and the path named.
    reexports: Vec<(String, usize, String)>,
    spawns: Vec<(String, usize)>,
    /// Unmarked statics that hold state, by name, with their lines.
    statics: Vec<(String, usize)>,
}

impl Collector<'_> {
    /// Resolves a path to the library crate, or `None` for external crates,
    /// local items and anything the file's own scope would have to decide.
    fn resolve(&self, segments: &[String], leading_colon: bool) -> Option<Vec<String>> {
        let (first, rest) = segments.split_first()?;
        if first == APP_LIBRARY {
            return Some(rest.to_vec());
        }
        if first == CORE_LIBRARY {
            return None;
        }
        if leading_colon || !self.library {
            return None;
        }
        match first.as_str() {
            "crate" => Some(rest.to_vec()),
            "self" => Some([self.module.as_slice(), rest].concat()),
            "super" => {
                let mut base = self.module.clone();
                let mut remaining = segments;
                while let Some((head, tail)) = remaining.split_first() {
                    if head != "super" {
                        break;
                    }
                    base.pop()?;
                    remaining = tail;
                }
                Some([base.as_slice(), remaining].concat())
            }
            _ => {
                let mut child = self.module.clone();
                child.push(first.clone());
                self.modules
                    .contains(&child)
                    .then(|| [self.module.as_slice(), segments].concat())
            }
        }
    }

    fn reference(&mut self, segments: &[String], leading_colon: bool, glob: bool, line: usize) {
        if let Some(path) = self.resolve(segments, leading_colon) {
            self.references.push(Reference { path, glob, line });
        }
    }

    /// A name a `use` imports: a reference, and a re-export when the `use` is
    /// visible outside the owning feature and names core or another feature.
    fn use_leaf(&mut self, segments: &[String], leading_colon: bool, glob: bool, line: usize) {
        self.reference(segments, leading_colon, glob, line);
        if !self.reexporting {
            return;
        }
        let target = if segments.first().is_some_and(|s| s == CORE_LIBRARY) {
            Some(CORE_LIBRARY.to_string())
        } else {
            self.resolve(segments, leading_colon).and_then(|path| {
                match (path.as_slice(), feature_of(&self.module)) {
                    ([features, other, ..], Some(owner))
                        if features == "features" && other != owner =>
                    {
                        Some(format!("features::{other}"))
                    }
                    _ => None,
                }
            })
        };
        if let Some(target) = target {
            let named = format!("{}{}", segments.join("::"), if glob { "::*" } else { "" });
            self.reexports.push((target, line, named));
        }
    }

    /// Whether code outside the owning feature may name what a `use` with
    /// this visibility imports: `pub`, `pub(crate)`, or a `pub(in ..)` or
    /// `pub(super)` whose scope reaches past the feature.
    fn visible_outside_feature(&self, visibility: &syn::Visibility) -> bool {
        let Some(owner) = feature_of(&self.module) else {
            return false;
        };
        match visibility {
            syn::Visibility::Inherited => false,
            syn::Visibility::Public(_) => true,
            syn::Visibility::Restricted(restricted) => {
                let scope: Vec<String> = restricted
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect();
                self.resolve(&scope, false)
                    .is_none_or(|scope| feature_of(&scope) != Some(owner))
            }
        }
    }

    /// A non-`use` path: a reference, and possibly a spawn call.
    fn expression_path(&mut self, segments: &[String], leading_colon: bool, line: usize) {
        self.reference(segments, leading_colon, false, line);
        let expanded = match segments.split_first() {
            Some((first, rest)) if !leading_colon => match self.aliases.get(first) {
                Some(alias) => [alias.as_slice(), rest].concat(),
                None => segments.to_vec(),
            },
            _ => segments.to_vec(),
        };
        let Some(spawn) = RAW_SPAWNS.iter().find(|spawn| {
            spawn.len() == expanded.len() && spawn.iter().zip(&expanded).all(|(a, b)| a == b)
        }) else {
            return;
        };
        let marked = self
            .anchors
            .iter()
            .chain(std::iter::once(&line))
            .any(|&anchor| marked_above(self.lines, anchor, SPAWN_MARKER));
        if !marked {
            self.spawns.push((spawn.join("::"), line));
        }
    }

    /// A static that holds state, unless marked as deliberately global.
    fn state(&mut self, name: String, line: usize) {
        if !marked_above(self.lines, line, STATE_MARKER) {
            self.statics.push((name, line));
        }
    }

    fn use_tree(&mut self, tree: &UseTree, mut prefix: Vec<String>, leading_colon: bool) {
        match tree {
            UseTree::Path(p) => {
                prefix.push(p.ident.to_string());
                self.use_tree(&p.tree, prefix, leading_colon);
            }
            UseTree::Group(g) => {
                for item in &g.items {
                    self.use_tree(item, prefix.clone(), leading_colon);
                }
            }
            UseTree::Name(n) => {
                if n.ident != "self" {
                    prefix.push(n.ident.to_string());
                }
                self.use_leaf(&prefix, leading_colon, false, n.ident.span().start().line);
            }
            UseTree::Rename(r) => {
                if r.ident != "self" {
                    prefix.push(r.ident.to_string());
                }
                self.use_leaf(&prefix, leading_colon, false, r.ident.span().start().line);
            }
            UseTree::Glob(g) => {
                self.use_leaf(&prefix, leading_colon, true, g.star_token.span.start().line);
            }
        }
    }

    fn anchored(&mut self, line: usize, visit: impl FnOnce(&mut Self)) {
        self.anchors.push(line);
        visit(self);
        self.anchors.pop();
    }
}

/// True when the comment and attribute lines directly above `line` (1-based)
/// include `// <marker> <reason>`, such as `// raw-spawn: <reason>`.
fn marked_above(lines: &[&str], line: usize, marker: &str) -> bool {
    let mut index = line.saturating_sub(1);
    while index > 0 {
        index -= 1;
        let text = lines.get(index).map_or("", |l| l.trim_start());
        if let Some(comment) = text.strip_prefix("//") {
            if comment
                .trim_start()
                .strip_prefix(marker)
                .is_some_and(|reason| !reason.trim().is_empty())
            {
                return true;
            }
        } else if !text.starts_with("#[") {
            return false;
        }
    }
    false
}

impl<'ast> Visit<'ast> for Collector<'_> {
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
        if item.content.is_some() {
            self.module.push(item.ident.to_string());
            visit::visit_item_mod(self, item);
            self.module.pop();
        }
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let line = item.sig.span().start().line;
        self.anchored(line, |this| visit::visit_item_fn(this, item));
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let line = item.sig.span().start().line;
        self.anchored(line, |this| visit::visit_impl_item_fn(this, item));
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        let line = item.sig.span().start().line;
        self.anchored(line, |this| visit::visit_trait_item_fn(this, item));
    }
    fn visit_stmt(&mut self, stmt: &'ast syn::Stmt) {
        if let syn::Stmt::Local(local) = stmt {
            if test_only(&local.attrs) {
                return;
            }
        }
        let line = stmt.span().start().line;
        self.anchored(line, |this| visit::visit_stmt(this, stmt));
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.reexporting = self.visible_outside_feature(&item.vis);
        self.use_tree(&item.tree, Vec::new(), item.leading_colon.is_some());
        self.reexporting = false;
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        let mutable = matches!(item.mutability, syn::StaticMutability::Mut(_));
        if mutable || !(plain_data(&item.ty) || derived_constant(&item.ty)) {
            // The `static` keyword's line, so a marker may sit between the
            // static and its doc comment.
            self.state(item.ident.to_string(), item.static_token.span.start().line);
        }
        visit::visit_item_static(self, item);
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        self.expression_path(
            &segments,
            path.leading_colon.is_some(),
            path.span().start().line,
        );
        visit::visit_path(self, path);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        visit::visit_macro(self, mac);
        if let Some(name) = mac
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .filter(|name| STATE_MACROS.contains(&name.as_str()))
        {
            self.state(format!("{name}!"), mac.path.span().start().line);
        }
        for (segments, leading_colon, line) in token_paths(mac.tokens.clone()) {
            self.expression_path(&segments, leading_colon, line);
        }
    }
}

/// `a::b::c` sequences in unparsed macro input, with their first line.
fn token_paths(tokens: TokenStream) -> Vec<(Vec<String>, bool, usize)> {
    struct Paths {
        out: Vec<(Vec<String>, bool, usize)>,
        current: Vec<String>,
        leading_colon: bool,
        line: usize,
        half_separator: bool,
        separator: bool,
    }
    impl Paths {
        fn flush(&mut self) {
            if !self.current.is_empty() {
                self.out.push((
                    std::mem::take(&mut self.current),
                    self.leading_colon,
                    self.line,
                ));
            }
            self.leading_colon = false;
            self.half_separator = false;
            self.separator = false;
        }
        fn walk(&mut self, tokens: TokenStream) {
            for tree in tokens {
                match tree {
                    TokenTree::Ident(ident) => {
                        let continues =
                            self.separator && (!self.current.is_empty() || self.leading_colon);
                        if !continues {
                            self.flush();
                            self.line = ident.span().start().line;
                        }
                        self.current.push(ident.to_string());
                        self.separator = false;
                    }
                    TokenTree::Punct(punct) if punct.as_char() == ':' => {
                        if self.half_separator {
                            self.half_separator = false;
                            if self.current.is_empty() {
                                self.leading_colon = true;
                                self.line = punct.span().start().line;
                            }
                            self.separator = true;
                        } else if punct.spacing() == Spacing::Joint && !self.separator {
                            self.half_separator = true;
                        } else {
                            self.flush();
                        }
                    }
                    TokenTree::Group(group) => {
                        self.flush();
                        self.walk(group.stream());
                    }
                    _ => self.flush(),
                }
            }
            self.flush();
        }
    }
    let mut paths = Paths {
        out: Vec::new(),
        current: Vec::new(),
        leading_colon: false,
        line: 0,
        half_separator: false,
        separator: false,
    };
    paths.walk(tokens);
    paths.out
}

pub fn counts(findings: &[Finding]) -> Counts {
    let mut counts = Counts::new();
    for finding in findings {
        *counts.entry(finding.key.clone()).or_default() += 1;
    }
    counts
}

/// The feature-edge graph: each feature and the features it references.
fn feature_graph(counts: &Counts) -> BTreeMap<&str, BTreeSet<&str>> {
    let mut graph: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for key in counts.keys().filter(|key| key.rule == FEATURE_EDGE) {
        graph
            .entry(key.subject.as_str())
            .or_default()
            .insert(key.target.as_str());
    }
    graph
}

/// Every feature `from` reaches through one or more edges. `from` itself is
/// included only when a cycle leads back to it.
fn reachable<'a>(graph: &BTreeMap<&'a str, BTreeSet<&'a str>>, from: &'a str) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    let mut queue = vec![from];
    while let Some(node) = queue.pop() {
        for &next in graph.get(node).into_iter().flatten() {
            if seen.insert(next) {
                queue.push(next);
            }
        }
    }
    seen
}

/// `feature-cycle` entries for the feature edges in `counts`: one per edge
/// whose target reaches back to its subject, counted by how many features
/// share the subject's strongly connected component.
pub fn cycle_counts(counts: &Counts) -> Counts {
    let graph = feature_graph(counts);
    let reach: BTreeMap<&str, BTreeSet<&str>> = graph
        .keys()
        .map(|&node| (node, reachable(&graph, node)))
        .collect();
    let reaches = |from: &str, to: &str| reach.get(from).is_some_and(|nodes| nodes.contains(to));
    let mut cycles = Counts::new();
    for (&from, targets) in &graph {
        for &to in targets {
            if !reaches(to, from) {
                continue;
            }
            let component = reach
                .get(from)
                .into_iter()
                .flatten()
                .filter(|&&node| reaches(node, from))
                .count();
            cycles.insert(Key::new(FEATURE_CYCLE, from, to), component);
        }
    }
    cycles
}

/// The shortest route the edge `from -> to` takes back to `from`, written out
/// as `from -> to -> ... -> from`, or `None` when the edge closes no cycle.
pub fn cycle_route(counts: &Counts, from: &str, to: &str) -> Option<String> {
    let graph = feature_graph(counts);
    let mut parent: BTreeMap<&str, &str> = BTreeMap::new();
    let mut queue = std::collections::VecDeque::from([to]);
    let mut seen = BTreeSet::from([to]);
    while let Some(node) = queue.pop_front() {
        if node == from {
            let mut route = vec![from];
            let mut step = from;
            while step != to {
                step = parent.get(step)?;
                route.push(step);
            }
            route.push(from);
            route.reverse();
            return Some(route.join(" -> "));
        }
        for &next in graph.get(node).into_iter().flatten() {
            if seen.insert(next) {
                parent.insert(next, node);
                queue.push_back(next);
            }
        }
    }
    None
}

pub fn parse_baseline(text: &str) -> Result<Counts, String> {
    let mut counts = Counts::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [rule, subject, target, count] = fields[..] else {
            return Err(format!(
                "baseline line {}: expected `<rule> <subject> <target> <count>`",
                index + 1
            ));
        };
        if !RULES.contains(&rule) {
            return Err(format!("baseline line {}: unknown rule {rule}", index + 1));
        }
        if ZERO_RULES.contains(&rule) {
            return Err(format!(
                "baseline line {}: {rule} is held at zero and cannot be baselined",
                index + 1
            ));
        }
        let count = count
            .parse::<usize>()
            .ok()
            .filter(|&count| count > 0)
            .ok_or_else(|| format!("baseline line {}: count must be positive", index + 1))?;
        if counts
            .insert(Key::new(rule, subject, target), count)
            .is_some()
        {
            return Err(format!("baseline line {}: duplicate entry", index + 1));
        }
    }
    Ok(counts)
}

pub fn render_baseline(counts: &Counts) -> String {
    let mut text = HEADER.to_string();
    for rule in RULES.iter().filter(|rule| !ZERO_RULES.contains(rule)) {
        let lines: Vec<String> = counts
            .iter()
            .filter(|(key, _)| key.rule == *rule)
            .map(|(key, count)| format!("{} {} {} {count}\n", key.rule, key.subject, key.target))
            .collect();
        if !lines.is_empty() {
            text.push('\n');
            text.extend(lines);
        }
    }
    text
}

#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    New { now: usize },
    Rose { was: usize, now: usize },
    Shrank { was: usize, now: usize },
}

pub fn compare(baseline: &Counts, current: &Counts) -> Vec<(Key, Change)> {
    let mut changes = Vec::new();
    for (key, &now) in current {
        match baseline.get(key) {
            None => changes.push((key.clone(), Change::New { now })),
            Some(&was) if now > was => changes.push((key.clone(), Change::Rose { was, now })),
            Some(&was) if now < was => changes.push((key.clone(), Change::Shrank { was, now })),
            Some(_) => {}
        }
    }
    for (key, &was) in baseline {
        if !current.contains_key(key) {
            changes.push((key.clone(), Change::Shrank { was, now: 0 }));
        }
    }
    changes.sort_by(|a, b| a.0.cmp(&b.0));
    changes
}

fn summary(counts: &Counts) -> String {
    RULES
        .iter()
        .map(|rule| {
            let (entries, total) = counts
                .iter()
                .filter(|(key, _)| key.rule == *rule)
                .fold((0, 0), |(entries, total), (_, count)| {
                    (entries + 1, total + count)
                });
            if *rule == FEATURE_CYCLE {
                format!("{rule} {entries} edges")
            } else {
                format!("{rule} {total} in {entries} entries")
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The checker's data files.
pub struct Files<'a> {
    pub baseline: &'a Path,
    /// The feature graph allowlist.
    pub feature_graph: &'a Path,
}

/// What the rules find in the tree, before and after the allowlist.
struct Current {
    found: Vec<Finding>,
    /// Counts the baseline is compared with: allowed edges removed, cycles
    /// derived from every edge, allowed or not.
    counts: Counts,
    allowed: Vec<feature_graph::Allowed>,
    /// The references each allowed edge still has.
    allowed_counts: Counts,
}

fn current(roots: &[&Path], feature_graph_path: &Path) -> Result<Current, Error> {
    let allowed = feature_graph::load(feature_graph_path)?;
    let found = findings(roots)?;
    let mut counts = counts(&found);
    let cycles = cycle_counts(&counts);
    let allowed_counts = feature_graph::take_allowed(&mut counts, &allowed);
    counts.extend(cycles);
    Ok(Current {
        found,
        counts,
        allowed,
        allowed_counts,
    })
}

fn read_baseline(path: &Path) -> Result<Counts, Error> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("{}: {e}; run `{REGENERATE}` to create it", path.display()))?;
    Ok(parse_baseline(&text).map_err(|e| format!("{}: {e}", path.display()))?)
}

/// Fails each allowed edge on a cycle of allowed edges, and each allowed edge
/// with no references left.
fn check_feature_graph(current: &Current, path: &Path, failures: &mut usize) {
    for route in feature_graph::cycles(&current.allowed) {
        *failures += 1;
        eprintln!(
            "VIOLATION: {}: the allowed edges form a cycle: {route}",
            path.display()
        );
    }
    for edge in &current.allowed {
        if !current.allowed_counts.contains_key(&edge.key()) {
            *failures += 1;
            eprintln!(
                "STALE: {}: allowed edge {} -> {} has no references left; delete its line",
                path.display(),
                edge.subject,
                edge.target
            );
        }
    }
}

/// Fails every entry of a rule held at zero, with its sites. Nothing is
/// baselined for these rules, so a write reports them too.
fn check_zero_rules(current: &Counts, found: &[Finding], failures: &mut usize) {
    for (key, count) in current.iter().filter(|(key, _)| held_at_zero(key)) {
        *failures += 1;
        eprintln!(
            "VIOLATION: {key}: {count} reference(s); {} is held at zero. Depend on a core \
             port or move the code; only one feature built on a lower-level one may be \
             allowed in feature-graph.txt",
            key.rule
        );
        for finding in found.iter().filter(|finding| finding.key == *key) {
            eprintln!("    at {}", finding.location);
        }
    }
}

fn held_at_zero(key: &Key) -> bool {
    ZERO_RULES.contains(&key.rule.as_str())
}

pub fn check(
    roots: &[&Path],
    files: &Files,
    write: bool,
    failures: &mut usize,
) -> Result<(), Error> {
    let baseline_path = files.baseline;
    let current_tree = current(roots, files.feature_graph)?;
    check_feature_graph(&current_tree, files.feature_graph, failures);
    let Current {
        found,
        counts: current,
        allowed_counts,
        ..
    } = current_tree;
    check_zero_rules(&current, &found, failures);
    if write {
        fs::write(baseline_path, render_baseline(&current))?;
        println!("Wrote {} ({}).", baseline_path.display(), summary(&current));
        return Ok(());
    }
    let baseline = read_baseline(baseline_path)?;
    let ratcheted: Counts = current
        .iter()
        .filter(|(key, _)| !held_at_zero(key))
        .map(|(key, &count)| (key.clone(), count))
        .collect();
    let mut shrank = 0;
    for (key, change) in compare(&baseline, &ratcheted) {
        *failures += 1;
        let cycle = key.rule == FEATURE_CYCLE;
        match change {
            Change::New { now } if cycle => {
                eprintln!(
                    "VIOLATION: {key}: closes a cycle through {now} features, not in the baseline"
                );
            }
            Change::Rose { was, now } if cycle => {
                eprintln!(
                    "VIOLATION: {key}: its cycle now spans {now} features, baseline allows {was}"
                );
            }
            Change::New { now } => {
                eprintln!("VIOLATION: {key}: {now} new reference(s), not in the baseline");
            }
            Change::Rose { was, now } => {
                eprintln!("VIOLATION: {key}: {now} references, baseline allows {was}");
            }
            Change::Shrank { was, now } => {
                shrank += 1;
                eprintln!("STALE: {key}: baseline records {was}, the code now has {now}");
                continue;
            }
        }
        // A cycle entry points at the references behind its edge.
        let site = if cycle {
            if let Some(route) = cycle_route(&current, &key.subject, &key.target) {
                eprintln!("    cycle {route}");
            }
            Key::new(FEATURE_EDGE, key.subject.clone(), key.target.clone())
        } else {
            key
        };
        for finding in found.iter().filter(|finding| finding.key == site) {
            eprintln!("    at {}", finding.location);
        }
    }
    if shrank != 0 {
        eprintln!(
            "{shrank} baseline entr{} shrank. Run `{REGENERATE}` and commit the smaller baseline.",
            if shrank == 1 { "y" } else { "ies" }
        );
    }
    println!(
        "Ratchet: {}; feature-graph allows {} edges ({} references).",
        summary(&current),
        allowed_counts.len(),
        allowed_counts.values().sum::<usize>()
    );
    Ok(())
}

/// Whether a key's subject is the one `--list` asked for: the exact subject,
/// a feature by its bare name, or a directory of files.
fn subject_matches(subject: &str, wanted: &str) -> bool {
    let directory = format!("{}/", wanted.trim_end_matches('/'));
    subject == wanted
        || subject.strip_prefix("features::") == Some(wanted)
        || subject.starts_with(&directory)
}

/// Every finding of `rule`, grouped by entry with its locations, and whether
/// the baseline or the allowlist covers it. Returns the text to print.
pub fn list(
    roots: &[&Path],
    files: &Files,
    rule: &str,
    subject: Option<&str>,
) -> Result<String, Error> {
    if !RULES.contains(&rule) {
        return Err(format!("unknown rule {rule}; the rules are {}", RULES.join(", ")).into());
    }
    let current = current(roots, files.feature_graph)?;
    let baseline = read_baseline(files.baseline)?;
    let wanted =
        |key: &Key| key.rule == rule && subject.is_none_or(|s| subject_matches(&key.subject, s));
    let mut entries: BTreeMap<&Key, usize> = BTreeMap::new();
    for (key, &count) in current.counts.iter().chain(&current.allowed_counts) {
        if wanted(key) {
            entries.insert(key, count);
        }
    }
    let mut text = String::new();
    let mut total = 0;
    for (key, count) in &entries {
        total += count;
        let status = if current.allowed_counts.contains_key(*key) {
            "allowed by feature-graph.txt".to_string()
        } else if held_at_zero(key) {
            "held at zero".to_string()
        } else {
            baseline
                .get(*key)
                .map_or("not in the baseline".to_string(), |was| {
                    format!("baseline {was}")
                })
        };
        text.push_str(&format!("{key}: {count} ({status})\n"));
        // A cycle entry's sites are the references behind its edge.
        let site = if key.rule == FEATURE_CYCLE {
            // Every edge, allowed or not, as the cycles were derived.
            let edges = counts(&current.found);
            if let Some(route) = cycle_route(&edges, &key.subject, &key.target) {
                text.push_str(&format!("    cycle {route}\n"));
            }
            Key::new(FEATURE_EDGE, key.subject.clone(), key.target.clone())
        } else {
            (*key).clone()
        };
        for finding in current.found.iter().filter(|finding| finding.key == site) {
            text.push_str(&format!("    at {}\n", finding.location));
        }
    }
    text.push_str(&if rule == FEATURE_CYCLE {
        format!("{rule}: {} edges.\n", entries.len())
    } else {
        format!("{rule}: {total} in {} entries.\n", entries.len())
    });
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(relative: &str, module: &[&str], text: &str) -> SourceFile {
        SourceFile {
            relative: relative.to_string(),
            module: module.iter().map(|s| s.to_string()).collect(),
            library: true,
            source: text.to_string(),
            ast: syn::parse_file(text).unwrap(),
        }
    }

    fn modules(paths: &[&[&str]]) -> HashSet<Vec<String>> {
        paths
            .iter()
            .map(|path| path.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    fn counted(
        file: &SourceFile,
        known: &HashSet<Vec<String>>,
    ) -> Vec<(String, String, String, usize)> {
        counts(&file_findings(file, known))
            .into_iter()
            .map(|(key, count)| (key.rule, key.subject, key.target, count))
            .collect()
    }

    fn row(
        rule: &str,
        subject: &str,
        target: &str,
        count: usize,
    ) -> (String, String, String, usize) {
        (rule.into(), subject.into(), target.into(), count)
    }

    #[test]
    fn infrastructure_naming_a_feature_counts_per_feature_and_skips_tests() {
        let file = source(
            "infrastructure/events/mod.rs",
            &["infrastructure", "events"],
            r#"
            pub use crate::features::conversation::ChatEvent;
            use crate::{features::{search::Hit, search::Query}, shared::Error};
            fn emit() { super::super::features::web::fetch(); }
            #[cfg(test)] mod tests { use crate::features::qa::Fixture; }
            "#,
        );
        assert_eq!(
            counted(
                &file,
                &modules(&[&["infrastructure"], &["infrastructure", "events"]])
            ),
            vec![
                row(
                    INFRASTRUCTURE_IMPORTS_FEATURE,
                    "infrastructure/events/mod.rs",
                    "features::conversation",
                    1
                ),
                row(
                    INFRASTRUCTURE_IMPORTS_FEATURE,
                    "infrastructure/events/mod.rs",
                    "features::search",
                    2
                ),
                row(
                    INFRASTRUCTURE_IMPORTS_FEATURE,
                    "infrastructure/events/mod.rs",
                    "features::web",
                    1
                ),
            ]
        );
    }

    #[test]
    fn feature_edges_count_cross_feature_references_including_relative_ones() {
        let file = source(
            "features/conversation/chat/turn.rs",
            &["features", "conversation", "chat", "turn"],
            r#"
            use crate::features::{qa::Starter, conversation::Own, web::{Page, Fetch}};
            use super::super::super::web::Archive;
            fn run() { crate::features::qa::answer(); let _ = format!("{}", crate::features::web::NAME); }
            #[cfg(test)] fn helper() { crate::features::search::query(); }
            "#,
        );
        assert_eq!(
            counted(&file, &HashSet::new()),
            vec![
                row(FEATURE_EDGE, "features::conversation", "features::qa", 2),
                row(FEATURE_EDGE, "features::conversation", "features::web", 4),
            ]
        );
    }

    #[test]
    fn container_imports_are_violations_outside_adapter_files() {
        let text = r#"
            use crate::interfaces::di::Container;
            use crate::interfaces::{dto::Request, Container as Wired};
            fn build(c: &crate::Container) {}
        "#;
        let business = source(
            "features/conversation/synthesis.rs",
            &["features", "conversation", "synthesis"],
            text,
        );
        assert_eq!(
            counted(&business, &HashSet::new()),
            vec![row(
                CONTAINER_IMPORT,
                "features/conversation/synthesis.rs",
                CONTAINER_TARGET,
                3
            )]
        );
        let plugin = source(
            "features/conversation/plugin.rs",
            &["features", "conversation", "plugin"],
            text,
        );
        assert!(counted(&plugin, &HashSet::new()).is_empty());
    }

    #[test]
    fn container_allowlist_covers_plugin_command_di_desktop_bootstrap_and_interfaces_files() {
        for allowed in [
            "features/backup/plugin.rs",
            "features/learning/plugin/sources.rs",
            "features/learning/plugin_commands.rs",
            "features/tags/commands.rs",
            "features/backup/commands/create.rs",
            "features/backup/di.rs",
            "features/conversation/di/mod.rs",
            "features/explorer/desktop.rs",
            "bootstrap/app.rs",
            "bootstrap/shutdown.rs",
            "interfaces/di/modules.rs",
            "main.rs",
            "lib.rs",
            "bin/export_bindings/main.rs",
            "desktop_e2e/learning_runtime_self_test.rs",
        ] {
            assert!(may_import_container(allowed), "{allowed} should be allowed");
        }
        for denied in [
            "features/conversation/synthesis.rs",
            "features/qa/starters.rs",
            "infrastructure/services/mod.rs",
            "infrastructure/setup/database.rs",
            "features/explorer/folders.rs",
            "shared/runtime/background.rs",
        ] {
            assert!(!may_import_container(denied), "{denied} should be denied");
        }
    }

    #[test]
    fn composition_root_references_are_violations_outside_entry_points() {
        let text = r#"
            use crate::bootstrap::graceful_shutdown;
            use crate::{bootstrap as root, interfaces::dto::Request};
            fn quit(app: &AppHandle) { crate::bootstrap::graceful_shutdown(app); }
            #[cfg(test)] mod tests { use crate::bootstrap::initialize_app; }
        "#;
        for (relative, module) in [
            (
                "features/download/plugin.rs",
                &["features", "download", "plugin"][..],
            ),
            (
                "infrastructure/setup/database.rs",
                &["infrastructure", "setup", "database"],
            ),
            (
                "interfaces/di/container.rs",
                &["interfaces", "di", "container"],
            ),
        ] {
            assert_eq!(
                counted(&source(relative, module, text), &HashSet::new()),
                vec![row(COMPOSITION_ROOT_IMPORT, relative, COMPOSITION_ROOT, 3)],
                "{relative}"
            );
        }
        let wiring = source("bootstrap/app.rs", &["bootstrap", "app"], text);
        assert!(counted(&wiring, &HashSet::new()).is_empty());

        let entry = "use lattice::bootstrap; fn main() { bootstrap::initialize_app(); }";
        for relative in ["main.rs", "bin/tool.rs"] {
            let mut binary = source(relative, &[], entry);
            binary.library = false;
            assert!(counted(&binary, &HashSet::new()).is_empty(), "{relative}");
        }
        let mut e2e = source(
            "desktop_e2e/self_test.rs",
            &["desktop_e2e", "self_test"],
            "use lattice::bootstrap::graceful_shutdown;",
        );
        e2e.library = false;
        assert_eq!(
            counted(&e2e, &HashSet::new()),
            vec![row(
                COMPOSITION_ROOT_IMPORT,
                "desktop_e2e/self_test.rs",
                COMPOSITION_ROOT,
                1
            )]
        );
    }

    #[test]
    fn composition_root_allowlist_is_the_entry_points_and_the_root_itself() {
        for allowed in [
            "main.rs",
            "lib.rs",
            "bin/tool.rs",
            "bin/export_bindings/main.rs",
            "bootstrap/mod.rs",
            "bootstrap/app.rs",
            "bootstrap.rs",
        ] {
            assert!(
                may_import_composition_root(allowed),
                "{allowed} should be allowed"
            );
        }
        for denied in [
            "features/backup/plugin.rs",
            "features/conversation/di/mod.rs",
            "features/bootstrap/mod.rs",
            "interfaces/di/container.rs",
            "infrastructure/setup/database.rs",
            "desktop_e2e/learning_runtime_self_test.rs",
            "plugins.rs",
        ] {
            assert!(
                !may_import_composition_root(denied),
                "{denied} should be denied"
            );
        }
    }

    #[test]
    fn raw_spawns_are_found_through_imports_and_macros_unless_marked() {
        let file = source(
            "features/search/engine/profiler.rs",
            &["features", "search", "engine", "profiler"],
            r#"
use std::thread;
use tokio::task::{self, JoinHandle};
use tauri::async_runtime::spawn as detach;

fn unmarked() {
    thread::spawn(|| {});
    task::spawn(async {});
    detach(async {});
    tokio::select! { _ = async { tokio::spawn(async {}) } => {} }
}

fn statement_marked() {
    // raw-spawn: the watcher outlives every caller
    let handle = tokio::spawn(async {});
}

/// Docs stay between the marker and the signature.
// raw-spawn: process-wide sampler
#[inline]
fn function_marked() {
    std::thread::spawn(|| {});
}

fn empty_reason_is_not_a_marker() {
    // raw-spawn:
    tokio::spawn(async {});
}

#[cfg(test)]
mod tests { fn run() { tokio::spawn(async {}); } }
            "#,
        );
        let spawns: Vec<_> = file_findings(&file, &HashSet::new())
            .into_iter()
            .map(|finding| format!("{} {}", finding.key.target, finding.location))
            .collect();
        assert_eq!(
            spawns,
            vec![
                "std::thread::spawn features/search/engine/profiler.rs:7",
                "tokio::task::spawn features/search/engine/profiler.rs:8",
                "tauri::async_runtime::spawn features/search/engine/profiler.rs:9",
                "tokio::spawn features/search/engine/profiler.rs:10",
                "tokio::spawn features/search/engine/profiler.rs:27",
            ]
        );
    }

    #[test]
    fn direct_spawn_paths_are_violations_but_test_modules_are_not() {
        let targets = |text: &str| -> Vec<String> {
            file_findings(
                &source("features/x.rs", &["features", "x"], text),
                &HashSet::new(),
            )
            .into_iter()
            .map(|finding| finding.key.target)
            .collect()
        };
        assert_eq!(
            targets("fn run() { tokio::spawn(async {}); }"),
            vec!["tokio::spawn"]
        );
        assert_eq!(
            targets("fn run() { tauri::async_runtime::spawn(async {}); }"),
            vec!["tauri::async_runtime::spawn"]
        );
        assert!(
            targets("#[cfg(test)] mod tests { fn run() { tokio::spawn(async {}); } }").is_empty()
        );
    }

    #[test]
    fn the_shared_runtime_may_spawn_directly() {
        let file = source(
            "shared/runtime/background.rs",
            &["shared", "runtime", "background"],
            "fn run() { tokio::spawn(async {}); }",
        );
        assert!(file_findings(&file, &HashSet::new()).is_empty());
    }

    #[test]
    fn documentation_examples_are_excluded_from_production_spawns() {
        let spawns = |text: &str| {
            file_findings(
                &source("features/x.rs", &["features", "x"], text),
                &HashSet::new(),
            )
            .len()
        };
        assert_eq!(
            spawns("#[cfg(any(test, doc))] mod examples { fn run() { tokio::spawn(async {}); } }"),
            0
        );
        assert_eq!(
            spawns("#[cfg(any(test, unix))] mod platform { fn run() { tokio::spawn(async {}); } }"),
            1
        );
        assert_eq!(
            spawns("impl Job { #[cfg(test)] fn run() { tokio::spawn(async {}); } }"),
            0
        );
    }

    fn global_state(text: &str) -> Vec<String> {
        file_findings(
            &source("features/x.rs", &["features", "x"], text),
            &HashSet::new(),
        )
        .into_iter()
        .filter(|finding| finding.key.rule == GLOBAL_STATE)
        .map(|finding| format!("{} {}", finding.key.target, finding.location))
        .collect()
    }

    #[test]
    fn statics_that_hold_state_are_found_wherever_they_are_declared() {
        assert_eq!(
            global_state(
                r#"
static REGISTRY: Lazy<Mutex<HashMap<String, u64>>> = Lazy::new(Default::default);
pub(crate) static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
static APP: OnceLock<AppHandle> = OnceLock::new();
static HEALTH: PlannerHealth = PlannerHealth::new();
static mut COUNT: u32 = 0;
fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
thread_local! { static SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::new()); }
"#
            ),
            vec![
                "REGISTRY features/x.rs:2",
                "SLOTS features/x.rs:3",
                "APP features/x.rs:4",
                "HEALTH features/x.rs:5",
                "COUNT features/x.rs:6",
                "NEXT features/x.rs:8",
                "thread_local! features/x.rs:11",
            ]
        );
    }

    #[test]
    fn constants_and_lazily_compiled_patterns_are_not_state() {
        assert!(global_state(
            r#"
static NAMES: &[&str] = &["a", "b"];
static TABLE: [(u8, char); 2] = [(1, 'a'), (2, 'b')];
static LIMIT: usize = 4;
static PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new("a").unwrap());
static OPTIONAL: once_cell::sync::Lazy<Option<regex::Regex>> = Lazy::new(|| None);
static WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(HashSet::new);
fn stem() { static STEMMER: Lazy<Stemmer> = Lazy::new(stemmer); }
#[cfg(test)]
static FIXTURE: Mutex<u8> = Mutex::new(0);
"#
        )
        .is_empty());
        // Built lazily, but what it builds can still change or hides a handle.
        assert_eq!(
            global_state(
                r#"
static CACHE: Lazy<Mutex<Vec<u8>>> = Lazy::new(Default::default);
static PROXIES: Lazy<Option<Vec<String>>> = Lazy::new(load);
static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
"#
            ),
            vec![
                "CACHE features/x.rs:2",
                "PROXIES features/x.rs:3",
                "SET features/x.rs:4",
            ]
        );
    }

    #[test]
    fn a_marker_above_the_static_keeps_it_global() {
        assert_eq!(
            global_state(
                r#"
/// The panic hook reads this; nothing can hand it state.
// global-state: the panic hook is process-wide
static CRASHES_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

// global-state: AppKit calls this target with no user data
#[cfg(target_os = "macos")]
static APP: OnceLock<AppHandle> = OnceLock::new();

fn warn_once() {
    // global-state:
    static WARNED: Once = Once::new();
}

// raw-spawn: a different marker
static PENDING: AtomicBool = AtomicBool::new(false);

// global-state: per-thread scratch for the tokenizer
thread_local! { static SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::new()); }
"#
            ),
            vec!["WARNED features/x.rs:12", "PENDING features/x.rs:16"]
        );
    }

    fn too_long(relative: &str, lines: usize) -> Vec<String> {
        file_length(relative, &"fn f() {}\n".repeat(lines))
            .into_iter()
            .map(|finding| {
                format!(
                    "{} {} {}",
                    finding.key.rule, finding.key.subject, finding.key.target
                )
            })
            .collect()
    }

    #[test]
    fn a_file_over_800_lines_is_found_and_one_at_800_is_not() {
        assert_eq!(
            too_long("features/x.rs", 801),
            vec!["file-length features/x.rs max-800-lines"]
        );
        assert!(too_long("features/x.rs", 800).is_empty());
        let finding = file_length("x.rs", &"\n".repeat(801)).unwrap();
        assert_eq!(
            counts(&[finding]).into_values().collect::<Vec<_>>(),
            vec![1]
        );
    }

    #[test]
    fn test_only_files_may_be_any_length() {
        for relative in [
            "features/x/tests.rs",
            "tests.rs",
            "features/x/loop_tests.rs",
            "features/x/tests/fixtures.rs",
            "features/x/tests/cases/long.rs",
        ] {
            assert!(too_long(relative, 2000).is_empty(), "{relative}");
        }
        assert_eq!(too_long("features/x/contests.rs", 801).len(), 1);
        assert_eq!(too_long("features/tests_support.rs", 801).len(), 1);
    }

    #[test]
    fn length_walk_reads_every_rust_file_below_the_root() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lattice-ratchet-length-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("unreached/tests")).unwrap();
        let long = "\n".repeat(801);
        fs::write(root.join("unreached/orphan.rs"), &long).unwrap();
        fs::write(root.join("unreached/tests/case.rs"), &long).unwrap();
        fs::write(root.join("unreached/notes.txt"), &long).unwrap();
        fs::write(root.join("short.rs"), "\n".repeat(800)).unwrap();
        let found: Vec<_> = length_findings(&root)
            .unwrap()
            .into_iter()
            .map(|finding| finding.location)
            .collect();
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(found, vec!["unreached/orphan.rs (801 lines)"]);
    }

    #[test]
    fn production_walk_follows_declared_modules_and_skips_test_files() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lattice-ratchet-walk-{}-{unique}",
            std::process::id()
        ));
        for directory in ["features/conversation", "bin/tool", "elsewhere"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("lib.rs"),
            "pub mod features; #[cfg(test)] mod tests; mod inline { mod nested; }",
        )
        .unwrap();
        fs::write(root.join("features/mod.rs"), "pub mod conversation;").unwrap();
        fs::write(
            root.join("features/conversation/mod.rs"),
            "mod turn; #[cfg(test)] mod tests; #[path = \"../../elsewhere/mapped.rs\"] mod mapped;",
        )
        .unwrap();
        fs::write(root.join("features/conversation/turn.rs"), "").unwrap();
        fs::write(root.join("features/conversation/tests.rs"), "").unwrap();
        fs::write(root.join("elsewhere/mapped.rs"), "").unwrap();
        fs::create_dir_all(root.join("inline")).unwrap();
        fs::write(root.join("inline/nested.rs"), "").unwrap();
        fs::write(root.join("tests.rs"), "").unwrap();
        fs::write(root.join("main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("bin/tool/main.rs"), "fn main() {}").unwrap();
        let (files, modules) = production_files(&root).unwrap();
        let mut found: Vec<_> = files
            .iter()
            .map(|file| format!("{} {}", file.relative, file.module.join("::")))
            .collect();
        found.sort();
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            found,
            vec![
                "bin/tool/main.rs ",
                "elsewhere/mapped.rs features::conversation::mapped",
                "features/conversation/mod.rs features::conversation",
                "features/conversation/turn.rs features::conversation::turn",
                "features/mod.rs features",
                "inline/nested.rs inline::nested",
                "lib.rs ",
                "main.rs ",
            ]
        );
        assert!(modules.contains(&vec!["inline".to_string()]));
        assert!(!modules.contains(&vec!["tests".to_string()]));
    }

    #[test]
    fn child_module_paths_resolve_relative_to_the_declaring_module() {
        let file = source(
            "lib.rs",
            &[],
            "pub use features::search::HybridSearch; pub use tokio::spawn as unused;",
        );
        let known = modules(&[&["features"]]);
        let mut aliases = UseAliases::default();
        aliases.visit_file(&file.ast);
        let mut collector = Collector {
            module: Vec::new(),
            library: true,
            modules: &known,
            aliases: &aliases.0,
            lines: &[],
            anchors: Vec::new(),
            references: Vec::new(),
            reexporting: false,
            reexports: Vec::new(),
            spawns: Vec::new(),
            statics: Vec::new(),
        };
        collector.visit_file(&file.ast);
        let paths: Vec<_> = collector
            .references
            .iter()
            .map(|r| r.path.join("::"))
            .collect();
        assert_eq!(paths, vec!["features::search::HybridSearch"]);
    }

    fn key(rule: &str, subject: &str, target: &str) -> Key {
        Key::new(rule, subject, target)
    }

    #[test]
    fn ratchet_fails_new_pairs_and_rises_and_reports_shrinkage_as_stale() {
        let baseline = Counts::from([
            (key(FEATURE_EDGE, "features::a", "features::b"), 3),
            (key(FEATURE_EDGE, "features::a", "features::c"), 2),
            (key(RAW_SPAWN, "features/a/gone.rs", "tokio::spawn"), 1),
            (
                key(CONTAINER_IMPORT, "features/a/x.rs", CONTAINER_TARGET),
                1,
            ),
        ]);
        let current = Counts::from([
            (key(FEATURE_EDGE, "features::a", "features::b"), 4),
            (key(FEATURE_EDGE, "features::a", "features::c"), 1),
            (key(FEATURE_EDGE, "features::a", "features::d"), 1),
            (
                key(CONTAINER_IMPORT, "features/a/x.rs", CONTAINER_TARGET),
                1,
            ),
        ]);
        assert_eq!(
            compare(&baseline, &current),
            vec![
                (
                    key(FEATURE_EDGE, "features::a", "features::b"),
                    Change::Rose { was: 3, now: 4 }
                ),
                (
                    key(FEATURE_EDGE, "features::a", "features::c"),
                    Change::Shrank { was: 2, now: 1 }
                ),
                (
                    key(FEATURE_EDGE, "features::a", "features::d"),
                    Change::New { now: 1 }
                ),
                (
                    key(RAW_SPAWN, "features/a/gone.rs", "tokio::spawn"),
                    Change::Shrank { was: 1, now: 0 }
                ),
            ]
        );
        assert!(compare(&current, &current).is_empty());
    }

    fn edges(pairs: &[(&str, &str)]) -> Counts {
        pairs
            .iter()
            .map(|(from, to)| {
                (
                    key(
                        FEATURE_EDGE,
                        &format!("features::{from}"),
                        &format!("features::{to}"),
                    ),
                    1,
                )
            })
            .collect()
    }

    fn cycle(from: &str, to: &str) -> Key {
        key(
            FEATURE_CYCLE,
            &format!("features::{from}"),
            &format!("features::{to}"),
        )
    }

    #[test]
    fn feature_cycles_are_the_edges_inside_each_strongly_connected_component() {
        let mut counts = edges(&[
            ("a", "b"),
            ("b", "c"),
            ("c", "a"),
            ("c", "d"),
            ("d", "e"),
            ("e", "d"),
            ("f", "a"),
        ]);
        counts.insert(key(RAW_SPAWN, "features/a/x.rs", "tokio::spawn"), 1);
        assert_eq!(
            cycle_counts(&counts),
            Counts::from([
                (cycle("a", "b"), 3),
                (cycle("b", "c"), 3),
                (cycle("c", "a"), 3),
                (cycle("d", "e"), 2),
                (cycle("e", "d"), 2),
            ])
        );
        assert!(cycle_counts(&edges(&[("a", "b"), ("b", "c"), ("a", "c")])).is_empty());
    }

    #[test]
    fn ratchet_fails_a_new_or_growing_cycle_and_reports_a_broken_one_as_stale() {
        let compare_cycles =
            |baseline: &[(&str, &str)], current: &[(&str, &str)]| -> Vec<(Key, Change)> {
                compare(
                    &cycle_counts(&edges(baseline)),
                    &cycle_counts(&edges(current)),
                )
            };
        assert_eq!(
            compare_cycles(&[("a", "b")], &[("a", "b"), ("b", "a")]),
            vec![
                (cycle("a", "b"), Change::New { now: 2 }),
                (cycle("b", "a"), Change::New { now: 2 }),
            ]
        );
        assert_eq!(
            compare_cycles(
                &[("a", "b"), ("b", "a"), ("b", "c")],
                &[("a", "b"), ("b", "a"), ("b", "c"), ("c", "a")]
            ),
            vec![
                (cycle("a", "b"), Change::Rose { was: 2, now: 3 }),
                (cycle("b", "a"), Change::Rose { was: 2, now: 3 }),
                (cycle("b", "c"), Change::New { now: 3 }),
                (cycle("c", "a"), Change::New { now: 3 }),
            ]
        );
        assert_eq!(
            compare_cycles(&[("a", "b"), ("b", "a")], &[("a", "b")]),
            vec![
                (cycle("a", "b"), Change::Shrank { was: 2, now: 0 }),
                (cycle("b", "a"), Change::Shrank { was: 2, now: 0 }),
            ]
        );
    }

    #[test]
    fn a_cycle_violation_names_the_shortest_route_back() {
        let counts = edges(&[("a", "b"), ("b", "c"), ("c", "a"), ("b", "a"), ("c", "d")]);
        assert_eq!(
            cycle_route(&counts, "features::b", "features::c").as_deref(),
            Some("features::b -> features::c -> features::a -> features::b")
        );
        assert_eq!(
            cycle_route(&counts, "features::a", "features::b").as_deref(),
            Some("features::a -> features::b -> features::a")
        );
        assert_eq!(cycle_route(&counts, "features::c", "features::d"), None);
    }

    #[test]
    fn check_writes_feature_cycles_and_fails_a_cycle_missing_from_the_baseline() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lattice-ratchet-cycle-{}-{unique}",
            std::process::id()
        ));
        for feature in ["a", "b"] {
            fs::create_dir_all(root.join("features").join(feature)).unwrap();
        }
        fs::write(root.join("lib.rs"), "pub mod features;").unwrap();
        fs::write(root.join("features/mod.rs"), "pub mod a; pub mod b;").unwrap();
        fs::write(
            root.join("features/a/mod.rs"),
            "pub fn run() { crate::features::b::run(); }",
        )
        .unwrap();
        fs::write(
            root.join("features/b/mod.rs"),
            "pub fn run() { crate::features::a::run(); }",
        )
        .unwrap();
        let baseline = root.join("baseline.txt");
        let feature_graph = root.join("feature-graph.txt");
        fs::write(&feature_graph, "# nothing allowed\n").unwrap();
        let files = Files {
            baseline: &baseline,
            feature_graph: &feature_graph,
        };

        // The two edges are held at zero, so every run fails them; only the
        // cycle entries are ratcheted.
        let mut writing = 0;
        check(&[&root], &files, true, &mut writing).unwrap();
        let written = fs::read_to_string(&baseline).unwrap();
        let mut recorded = 0;
        check(&[&root], &files, false, &mut recorded).unwrap();

        // A baseline that records nothing but its header: the cycle is missing.
        let without_cycles: String = written
            .lines()
            .filter(|line| !line.starts_with(FEATURE_CYCLE))
            .map(|line| format!("{line}\n"))
            .collect();
        fs::write(&baseline, without_cycles).unwrap();
        let mut missing = 0;
        check(&[&root], &files, false, &mut missing).unwrap();
        fs::remove_dir_all(&root).unwrap();

        assert!(written.contains("feature-cycle features::a features::b 2\n"));
        assert!(written.contains("feature-cycle features::b features::a 2\n"));
        assert!(!written
            .lines()
            .any(|line| line.starts_with("feature-edge ")));
        assert_eq!(writing, 2);
        assert_eq!(recorded, 2);
        assert_eq!(missing, 4);
    }

    #[test]
    fn findings_cover_the_app_and_core_roots_and_count_a_shared_root_once() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let scratch = std::env::temp_dir().join(format!(
            "lattice-ratchet-roots-{}-{unique}",
            std::process::id()
        ));
        let app = scratch.join("app");
        let core = scratch.join("core");
        for directory in ["features/a", "features/b"] {
            fs::create_dir_all(app.join(directory)).unwrap();
        }
        fs::create_dir_all(core.join("shared")).unwrap();
        fs::write(app.join("lib.rs"), "pub mod features;").unwrap();
        fs::write(app.join("features/mod.rs"), "pub mod a; pub mod b;").unwrap();
        fs::write(
            app.join("features/a/mod.rs"),
            "pub fn run() { crate::features::b::run(); }",
        )
        .unwrap();
        fs::write(app.join("features/b/mod.rs"), "pub fn run() {}").unwrap();
        fs::write(core.join("lib.rs"), "pub mod shared;").unwrap();
        fs::write(
            core.join("shared/mod.rs"),
            "static APP: OnceLock<u32> = OnceLock::new();",
        )
        .unwrap();

        let both = counts(&findings(&[&app, &core]).unwrap());
        let shared_root = counts(&findings(&[&app, &app]).unwrap());
        fs::remove_dir_all(&scratch).unwrap();

        assert_eq!(
            both,
            Counts::from([
                (key(FEATURE_EDGE, "features::a", "features::b"), 1),
                (key(GLOBAL_STATE, "shared/mod.rs", "APP"), 1),
            ])
        );
        assert_eq!(
            shared_root,
            Counts::from([(key(FEATURE_EDGE, "features::a", "features::b"), 1)])
        );
    }

    #[test]
    fn binaries_name_the_app_library_and_core_paths_belong_to_another_crate() {
        let mut binary = source(
            "bin/tool.rs",
            &[],
            r#"
            use lattice::interfaces::di::Container;
            use lattice_core::interfaces::di::Wired;
            use lattice_desktop::interfaces::di::Stale;
            fn main() { lattice_core::Container::build(); }
        "#,
        );
        binary.library = false;
        assert_eq!(
            counted(&binary, &HashSet::new()),
            vec![row(CONTAINER_IMPORT, "bin/tool.rs", CONTAINER_TARGET, 1)]
        );

        let feature = source(
            "features/a/mod.rs",
            &["features", "a"],
            "fn run() { lattice_core::features::b::run(); crate::features::b::run(); }",
        );
        let known = modules(&[&["features", "a", "lattice_core"]]);
        assert_eq!(
            counted(&feature, &known),
            vec![row(FEATURE_EDGE, "features::a", "features::b", 1)]
        );
    }

    #[test]
    fn baseline_round_trips_and_rejects_malformed_lines() {
        let counts = Counts::from([
            (key(RAW_SPAWN, "features/a.rs", "tokio::spawn"), 2),
            (
                key(
                    INFRASTRUCTURE_IMPORTS_FEATURE,
                    "infrastructure/x.rs",
                    "features::a",
                ),
                1,
            ),
        ]);
        let text = render_baseline(&counts);
        assert!(text.starts_with("# Architecture ratchet baseline"));
        assert_eq!(parse_baseline(&text).unwrap(), counts);
        assert!(parse_baseline("raw-spawn features/a.rs tokio::spawn").is_err());
        assert!(parse_baseline("raw-spawn features/a.rs tokio::spawn 0").is_err());
        assert!(parse_baseline("raw-spawns features/a.rs tokio::spawn 1").is_err());
        assert!(parse_baseline("raw-spawn a b 1\nraw-spawn a b 2").is_err());
        let edge = parse_baseline("feature-edge features::a features::b 1").unwrap_err();
        assert!(edge.contains("held at zero"), "{edge}");
        assert!(!render_baseline(&Counts::from([(
            key(FEATURE_EDGE, "features::a", "features::b"),
            1
        )]))
        .contains("feature-edge features::a"));
        assert_eq!(
            parse_baseline("global-state shared/runtime/background.rs REGISTRY 1").unwrap(),
            Counts::from([(
                key(GLOBAL_STATE, "shared/runtime/background.rs", "REGISTRY"),
                1
            )])
        );
        assert_eq!(
            parse_baseline("feature-cycle features::a features::b 2").unwrap(),
            Counts::from([(key(FEATURE_CYCLE, "features::a", "features::b"), 2)])
        );
        assert_eq!(
            parse_baseline("composition-root-import features/a.rs bootstrap 1").unwrap(),
            Counts::from([(
                key(COMPOSITION_ROOT_IMPORT, "features/a.rs", "bootstrap"),
                1
            )])
        );
    }

    #[test]
    fn feature_reexports_count_uses_visible_outside_the_feature_that_name_core_or_another_feature()
    {
        let file = source(
            "features/settings/dto/mod.rs",
            &["features", "settings", "dto"],
            r#"
            pub use lattice_core::application::contracts::settings::*;
            pub(crate) use ::lattice_core::domain::{Model, Role as ModelRole};
            pub use crate::features::llm::dto::LlmDto;
            pub(crate) use super::super::web::Page;
            pub use lattice::features::qa::SourceDto;
            pub(in crate::features) use crate::features::search::Hit;
            pub use crate::features::settings::repository::Store;
            pub use super::types::*;
            pub(super) use crate::features::tags::Tag;
            pub(in crate::features::settings) use lattice_core::shared::Text;
            use lattice_core::shared::error::Result;
            use crate::features::download::Progress;
            pub use serde::Serialize;
            #[cfg(test)] pub use crate::features::embedding::Mock;
            mod inner { pub(super) use crate::features::vault::Note; }
            "#,
        );
        let reexports: Vec<_> = file_findings(&file, &HashSet::new())
            .into_iter()
            .filter(|finding| finding.key.rule == FEATURE_REEXPORT)
            .map(|finding| format!("{} {}", finding.key.target, finding.location))
            .collect();
        let at = "features/settings/dto/mod.rs";
        assert_eq!(
            reexports,
            vec![
                format!("lattice_core {at}:2 lattice_core::application::contracts::settings::*"),
                format!("lattice_core {at}:3 lattice_core::domain::Model"),
                format!("lattice_core {at}:3 lattice_core::domain::Role"),
                format!("features::llm {at}:4 crate::features::llm::dto::LlmDto"),
                format!("features::web {at}:5 super::super::web::Page"),
                format!("features::qa {at}:6 lattice::features::qa::SourceDto"),
                format!("features::search {at}:7 crate::features::search::Hit"),
            ]
        );
        let outside = source(
            "interfaces/di/mod.rs",
            &["interfaces", "di"],
            "pub use lattice_core::application::ports::*; pub use crate::features::llm::Engine;",
        );
        assert!(file_findings(&outside, &HashSet::new())
            .iter()
            .all(|finding| finding.key.rule != FEATURE_REEXPORT));
    }

    /// An allowlist covering every edge of `graph_tree` but `c -> a`.
    const LAYERED: &str = "\
allow features::a features::b # a is built on b
allow features::a features::c # a is built on c
allow features::b features::c # b is built on c
";

    /// A tree where `a` uses `b` and `c`, `b` uses `c`, and `c` uses `a`
    /// only when `back` is set.
    fn graph_tree(name: &str, back: bool) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lattice-ratchet-{name}-{}-{unique}",
            std::process::id()
        ));
        for feature in ["a", "b", "c"] {
            fs::create_dir_all(root.join("features").join(feature)).unwrap();
        }
        fs::write(root.join("lib.rs"), "pub mod features;").unwrap();
        fs::write(
            root.join("features/mod.rs"),
            "pub mod a; pub mod b; pub mod c;",
        )
        .unwrap();
        fs::write(
            root.join("features/a/mod.rs"),
            "pub fn run() { crate::features::b::run(); crate::features::b::stop(); crate::features::c::run(); }",
        )
        .unwrap();
        fs::write(
            root.join("features/b/mod.rs"),
            "pub fn run() { crate::features::c::run(); } pub fn stop() {}",
        )
        .unwrap();
        let c = if back {
            "pub fn run() { crate::features::a::run(); }"
        } else {
            "pub fn run() {}"
        };
        fs::write(root.join("features/c/mod.rs"), c).unwrap();
        root
    }

    #[test]
    fn feature_edges_are_held_at_zero_even_while_the_baseline_is_written() {
        let root = graph_tree("zero", false);
        let baseline = root.join("baseline.txt");
        let feature_graph = root.join("feature-graph.txt");
        let files = Files {
            baseline: &baseline,
            feature_graph: &feature_graph,
        };
        fs::write(&feature_graph, "allow features::a features::b # r\n").unwrap();
        let mut writing = 0;
        check(&[&root], &files, true, &mut writing).unwrap();
        let written = fs::read_to_string(&baseline).unwrap();
        let mut checking = 0;
        check(&[&root], &files, false, &mut checking).unwrap();
        // A hand-edited baseline cannot excuse an edge either.
        fs::write(
            &baseline,
            format!("{written}feature-edge features::a features::c 1\n"),
        )
        .unwrap();
        let mut excused = 0;
        let excuse = check(&[&root], &files, false, &mut excused).map_err(|e| e.to_string());
        fs::remove_dir_all(&root).unwrap();

        assert!(!written
            .lines()
            .any(|line| line.starts_with("feature-edge ")));
        // a -> c and b -> c, on both runs; the allowed a -> b is not counted.
        assert_eq!(writing, 2);
        assert_eq!(checking, 2);
        assert!(excuse.unwrap_err().contains("held at zero"));
    }

    #[test]
    fn allowed_edges_leave_feature_edge_but_still_close_cycles() {
        let root = graph_tree("allowed", false);
        let baseline = root.join("baseline.txt");
        let feature_graph = root.join("feature-graph.txt");
        let files = Files {
            baseline: &baseline,
            feature_graph: &feature_graph,
        };
        fs::write(&feature_graph, LAYERED).unwrap();
        let mut failures = 0;
        check(&[&root], &files, true, &mut failures).unwrap();
        let written = fs::read_to_string(&baseline).unwrap();
        check(&[&root], &files, false, &mut failures).unwrap();
        let clean = failures;

        // c now uses a: the allowed a -> b edge is on the new cycle too.
        fs::write(
            root.join("features/c/mod.rs"),
            "pub fn run() { crate::features::a::run(); }",
        )
        .unwrap();
        let cyclic = current(&[&root], &feature_graph).unwrap().counts;
        fs::remove_dir_all(&root).unwrap();

        assert!(!written
            .lines()
            .any(|line| line.starts_with("feature-edge ")));
        assert_eq!(clean, 0);
        assert_eq!(
            cyclic.get(&key(FEATURE_EDGE, "features::a", "features::b")),
            None
        );
        assert_eq!(
            cyclic.get(&key(FEATURE_EDGE, "features::c", "features::a")),
            Some(&1)
        );
        assert_eq!(cyclic.get(&cycle("a", "b")), Some(&3));
        assert_eq!(cyclic.get(&cycle("c", "a")), Some(&3));
    }

    #[test]
    fn an_allowlist_that_forms_a_cycle_or_has_a_stale_edge_fails() {
        let root = graph_tree("allowlist", false);
        let baseline = root.join("baseline.txt");
        let feature_graph = root.join("feature-graph.txt");
        let files = Files {
            baseline: &baseline,
            feature_graph: &feature_graph,
        };
        let run = |allowlist: &str| {
            fs::write(&feature_graph, allowlist).unwrap();
            let mut failures = 0;
            check(&[&root], &files, true, &mut failures)
                .map(|()| failures)
                .map_err(|e| e.to_string())
        };
        let clean = run(LAYERED);
        // c -> a has no references, so it is stale as well as putting all
        // four allowed edges on one cycle.
        let tangled = run(&format!("{LAYERED}allow features::c features::a # r\n"));
        let stale = run(&format!("{LAYERED}allow features::a features::d # r\n"));
        // a -> c and b -> c are not allowed, so they fail as feature edges.
        let partial = run("allow features::a features::b # a is built on b\n");
        let unexplained = run("allow features::a features::b\n");
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(clean, Ok(0));
        assert_eq!(tangled, Ok(5));
        assert_eq!(stale, Ok(1));
        assert_eq!(partial, Ok(2));
        assert!(unexplained.unwrap_err().contains("reason"));
    }

    #[test]
    fn list_prints_every_site_with_its_baseline_or_allowlist_status() {
        let root = graph_tree("list", true);
        let baseline = root.join("baseline.txt");
        let feature_graph = root.join("feature-graph.txt");
        let files = Files {
            baseline: &baseline,
            feature_graph: &feature_graph,
        };
        fs::write(&feature_graph, "allow features::a features::b # r\n").unwrap();
        fs::write(&baseline, "# nothing baselined\n").unwrap();
        let edges = list(&[&root], &files, FEATURE_EDGE, None).unwrap();
        let from_a = list(&[&root], &files, FEATURE_EDGE, Some("a")).unwrap();
        let cycles = list(&[&root], &files, FEATURE_CYCLE, Some("features::c")).unwrap();
        let unknown = list(&[&root], &files, "feature-edges", None);
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(
            edges,
            "\
[feature-edge] features::a -> features::b: 2 (allowed by feature-graph.txt)
    at features/a/mod.rs:1 features::b::run
    at features/a/mod.rs:1 features::b::stop
[feature-edge] features::a -> features::c: 1 (held at zero)
    at features/a/mod.rs:1 features::c::run
[feature-edge] features::b -> features::c: 1 (held at zero)
    at features/b/mod.rs:1 features::c::run
[feature-edge] features::c -> features::a: 1 (held at zero)
    at features/c/mod.rs:1 features::a::run
feature-edge: 5 in 4 entries.
"
        );
        assert!(from_a.starts_with("[feature-edge] features::a -> features::b"));
        assert!(from_a.ends_with("feature-edge: 3 in 2 entries.\n"));
        assert_eq!(
            cycles,
            "\
[feature-cycle] features::c -> features::a: 3 (not in the baseline)
    cycle features::c -> features::a -> features::c
    at features/c/mod.rs:1 features::a::run
feature-cycle: 1 edges.
"
        );
        assert!(unknown.unwrap_err().to_string().contains("unknown rule"));
    }
}
