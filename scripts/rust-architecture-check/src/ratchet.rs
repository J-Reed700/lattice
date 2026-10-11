//! Ratcheted rules. Today's violations live in `baseline.txt`; the check fails
//! when a count rises, a new (subject, target) pair appears, or a baselined
//! entry shrinks without the baseline being regenerated, so the list only
//! ever gets shorter.
//!
//! Only production code counts: the walk follows `mod` declarations from the
//! crate roots and skips `#[cfg(test)]` modules and items, so test files are
//! never read.
use proc_macro2::{Spacing, TokenStream, TokenTree};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, ImplItem, Item, TraitItem, UseTree,
};

use crate::{item_attrs, test_only};

pub const REGENERATE: &str = "bash scripts/check-rust-layer-boundaries.sh --write-baseline";

pub const INFRASTRUCTURE_IMPORTS_FEATURE: &str = "infrastructure-imports-feature";
pub const FEATURE_EDGE: &str = "feature-edge";
pub const CONTAINER_IMPORT: &str = "container-import";
pub const RAW_SPAWN: &str = "raw-spawn";
const RULES: [&str; 4] = [
    INFRASTRUCTURE_IMPORTS_FEATURE,
    FEATURE_EDGE,
    CONTAINER_IMPORT,
    RAW_SPAWN,
];

const RAW_SPAWNS: [&[&str]; 4] = [
    &["tokio", "spawn"],
    &["tokio", "task", "spawn"],
    &["tauri", "async_runtime", "spawn"],
    &["std", "thread", "spawn"],
];
const SPAWN_MARKER: &str = "raw-spawn:";
const SUPERVISOR: &str = "shared/runtime/";
const CONTAINER_TARGET: &str = "interfaces::di";

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
#
#   infrastructure-imports-feature <file> features::<b>   infrastructure naming crate::features
#   feature-edge <features::a> <features::b>              references from one feature into another
#   container-import <file> interfaces::di                the DI Container outside plugin, command,
#                                                         DI, desktop, setup and interfaces files
#   raw-spawn <file> <spawn fn>                           detached spawns outside shared/runtime
";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub rule: String,
    pub subject: String,
    pub target: String,
}

impl Key {
    fn new(rule: &str, subject: impl Into<String>, target: impl Into<String>) -> Self {
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

pub fn findings(root: &Path) -> Result<Vec<Finding>, Error> {
    let (files, modules) = production_files(root)?;
    Ok(files
        .iter()
        .flat_map(|file| file_findings(file, &modules))
        .collect())
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
        spawns: Vec::new(),
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
            if file.module.first().is_some_and(|s| s == "infrastructure") {
                found.push(Finding {
                    key: Key::new(INFRASTRUCTURE_IMPORTS_FEATURE, relative, target.clone()),
                    location: at(reference.line),
                });
            }
            if let (Some(owner), Some(other)) = (feature_of(&file.module), path.get(1)) {
                if owner != other {
                    found.push(Finding {
                        key: Key::new(FEATURE_EDGE, format!("features::{owner}"), target),
                        location: at(reference.line),
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
    }
    if !relative.starts_with(SUPERVISOR) {
        for (target, line) in collector.spawns {
            found.push(Finding {
                key: Key::new(RAW_SPAWN, relative, target),
                location: at(line),
            });
        }
    }
    found
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
        || matches!(directories.first(), Some(&"interfaces" | &"desktop_e2e"))
        || directories.starts_with(&["infrastructure", "setup"])
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
    spawns: Vec<(String, usize)>,
}

impl Collector<'_> {
    /// Resolves a path to the library crate, or `None` for external crates,
    /// local items and anything the file's own scope would have to decide.
    fn resolve(&self, segments: &[String], leading_colon: bool) -> Option<Vec<String>> {
        let (first, rest) = segments.split_first()?;
        if first == "lattice_desktop" {
            return Some(rest.to_vec());
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
            .any(|&anchor| marked_above(self.lines, anchor));
        if !marked {
            self.spawns.push((spawn.join("::"), line));
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
                self.reference(&prefix, leading_colon, false, n.ident.span().start().line);
            }
            UseTree::Rename(r) => {
                if r.ident != "self" {
                    prefix.push(r.ident.to_string());
                }
                self.reference(&prefix, leading_colon, false, r.ident.span().start().line);
            }
            UseTree::Glob(g) => {
                self.reference(&prefix, leading_colon, true, g.star_token.span.start().line);
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
/// include `// raw-spawn: <reason>`.
fn marked_above(lines: &[&str], line: usize) -> bool {
    let mut index = line.saturating_sub(1);
    while index > 0 {
        index -= 1;
        let text = lines.get(index).map_or("", |l| l.trim_start());
        if let Some(comment) = text.strip_prefix("//") {
            if comment
                .trim_start()
                .strip_prefix(SPAWN_MARKER)
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
        self.use_tree(&item.tree, Vec::new(), item.leading_colon.is_some());
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
    for rule in RULES {
        let lines: Vec<String> = counts
            .iter()
            .filter(|(key, _)| key.rule == rule)
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
            format!("{rule} {total} in {entries} entries")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn check(
    root: &Path,
    baseline_path: &Path,
    write: bool,
    failures: &mut usize,
) -> Result<(), Error> {
    let found = findings(root)?;
    let current = counts(&found);
    if write {
        fs::write(baseline_path, render_baseline(&current))?;
        println!("Wrote {} ({}).", baseline_path.display(), summary(&current));
        return Ok(());
    }
    let text = fs::read_to_string(baseline_path).map_err(|e| {
        format!(
            "{}: {e}; run `{REGENERATE}` to create it",
            baseline_path.display()
        )
    })?;
    let baseline =
        parse_baseline(&text).map_err(|e| format!("{}: {e}", baseline_path.display()))?;
    let mut shrank = 0;
    for (key, change) in compare(&baseline, &current) {
        *failures += 1;
        match change {
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
        for finding in found.iter().filter(|finding| finding.key == key) {
            eprintln!("    at {}", finding.location);
        }
    }
    if shrank != 0 {
        eprintln!(
            "{shrank} baseline entr{} shrank. Run `{REGENERATE}` and commit the smaller baseline.",
            if shrank == 1 { "y" } else { "ies" }
        );
    }
    println!("Ratchet: {}.", summary(&current));
    Ok(())
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
    fn container_allowlist_covers_plugin_command_di_desktop_setup_and_interfaces_files() {
        for allowed in [
            "features/backup/plugin.rs",
            "features/learning/plugin/sources.rs",
            "features/learning/plugin_commands.rs",
            "features/tags/commands.rs",
            "features/backup/commands/create.rs",
            "features/backup/di.rs",
            "features/conversation/di/mod.rs",
            "features/explorer/desktop.rs",
            "infrastructure/setup/app.rs",
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
            "features/explorer/folders.rs",
            "shared/runtime/background.rs",
        ] {
            assert!(!may_import_container(denied), "{denied} should be denied");
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
            spawns: Vec::new(),
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
    }
}
