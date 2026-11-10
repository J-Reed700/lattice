//! Rule 3: a function whose statements, its own plus those of same-file helpers
//! it calls, number two or more, or that is a unit of work in any module (it
//! begins a transaction or takes a `WriteTx` / `ReadTx`), runs none on
//! the pool, begins at most one write and one read transaction, reads before it
//! begins the write transaction, and runs nothing after the write transaction
//! commits.
mod pool;

use super::{is_punct, walk_tokens, Exception, Exempt, Violation};
use crate::{
    item_attrs,
    ratchet::{impl_item_attrs, trait_item_attrs},
    test_only,
};
use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use quote::ToTokens;
use std::collections::{HashMap, HashSet};
use syn::{
    visit::{self, Visit},
    Expr, FnArg, ImplItem, Item, Pat, TraitItem, Type,
};

const STATEMENTS: [&str; 7] = [
    "execute",
    "execute_many",
    "fetch",
    "fetch_one",
    "fetch_all",
    "fetch_optional",
    "fetch_many",
];

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Write,
    Read,
}

/// What a statement runs on.
#[derive(Clone, Copy, PartialEq)]
enum Target {
    Pool,
    Transaction(Kind),
    Other,
}

type Position = (usize, usize);

fn position(span: Span) -> Position {
    let start = span.start();
    (start.line, start.column)
}

/// The tokens of an argument list or of one argument.
struct Arg {
    squashed: String,
    words: HashSet<String>,
}

impl Arg {
    fn new(tokens: &str) -> Self {
        Self {
            squashed: tokens.split_whitespace().collect(),
            words: tokens
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .filter(|word| !word.is_empty())
                .map(str::to_string)
                .collect(),
        }
    }
}

/// A `let` binding: its name, its annotated type and its initializer when that only copies
/// something.
struct Let {
    name: String,
    ty: Option<Type>,
    core: Option<Arg>,
}

/// A call to a function that may be defined in the same file.
struct Helper {
    name: String,
    /// `self.name(..)` or `Self::name(..)`: a method of the caller's own type.
    same_type: bool,
    /// `self.name(..)`: the receiver is not among the arguments.
    method: bool,
}

enum Event {
    Statement {
        arg: Arg,
        at: Position,
    },
    Helper {
        helper: Helper,
        args: Vec<Arg>,
        at: Position,
    },
    /// An awaited call outside a data module: a data operation if it passes a unit or a pool.
    Operation {
        arg: Arg,
        at: Position,
    },
    Begin {
        kind: Kind,
        at: Position,
    },
    Commit {
        kind: Kind,
        /// The block it sits in, and whether that block is inside the body's own.
        block: usize,
        nested: bool,
        at: Position,
    },
}

impl Event {
    fn at(&self) -> Position {
        match self {
            Event::Statement { at, .. }
            | Event::Helper { at, .. }
            | Event::Operation { at, .. }
            | Event::Begin { at, .. }
            | Event::Commit { at, .. } => *at,
        }
    }
}

pub(super) fn binding(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(ident) => Some(ident.ident.to_string()),
        Pat::Type(typed) => binding(&typed.pat),
        Pat::TupleStruct(wrapper) if wrapper.path.is_ident("Ok") && wrapper.elems.len() == 1 => {
            wrapper.elems.first().and_then(binding)
        }
        _ => None,
    }
}

/// Parameters typed `WriteTx` or `ReadTx`, by value or `&mut`, matched on the last path
/// segment. The flag says the function owns the unit.
pub(super) fn unit_params(signature: &syn::Signature) -> Vec<(String, Kind, bool)> {
    signature
        .inputs
        .iter()
        .filter_map(|input| {
            let FnArg::Typed(typed) = input else {
                return None;
            };
            let (ty, owned) = match &*typed.ty {
                Type::Reference(reference) => {
                    reference.mutability?;
                    (&*reference.elem, false)
                }
                ty => (ty, true),
            };
            let Type::Path(path) = ty else {
                return None;
            };
            let kind = match path.path.segments.last()?.ident.to_string().as_str() {
                "WriteTx" => Kind::Write,
                "ReadTx" => Kind::Read,
                _ => return None,
            };
            Some((binding(&typed.pat)?, kind, owned))
        })
        .collect()
}

fn is_self(expr: &Expr) -> bool {
    matches!(expr, Expr::Path(path) if path.path.is_ident("self"))
}

fn split_arguments(stream: TokenStream) -> Vec<Arg> {
    let mut args = Vec::new();
    let mut current: Vec<TokenTree> = Vec::new();
    for token in stream {
        if is_punct(Some(&token), ',') {
            let tokens: TokenStream = current.drain(..).collect();
            args.push(Arg::new(&tokens.to_string()));
        } else {
            current.push(token);
        }
    }
    if !current.is_empty() {
        let tokens: TokenStream = current.into_iter().collect();
        args.push(Arg::new(&tokens.to_string()));
    }
    args
}

/// Every statement, helper call, `begin_write()`, `begin_read()` and `commit()`
/// in a function body, the connections it checks out and the transactions it
/// begins.
#[derive(Default)]
struct Body {
    lets: Vec<Let>,
    acquired: HashSet<String>,
    writes: HashSet<String>,
    reads: HashSet<String>,
    events: Vec<Event>,
    blocks: Vec<usize>,
    next_block: usize,
    /// Blocks with a `return` among their own statements.
    returning: HashSet<usize>,
    /// Whether awaited calls that pass a unit or a pool are data operations (outside data modules).
    operations: bool,
}

/// Calls that begin, end or check out a unit of work: never data operations themselves.
const UNIT_CALLS: [&str; 7] = [
    "begin_write",
    "begin_read",
    "begin",
    "begin_with",
    "commit",
    "rollback",
    "acquire",
];

impl Body {
    fn call(&mut self, method: &str, arguments: &str, receiver: Option<&str>, at: Position) {
        if STATEMENTS.contains(&method) {
            self.events.push(Event::Statement {
                arg: Arg::new(arguments),
                at,
            });
        }
        match method {
            "begin_write" => self.events.push(Event::Begin {
                kind: Kind::Write,
                at,
            }),
            "begin_read" => self.events.push(Event::Begin {
                kind: Kind::Read,
                at,
            }),
            "commit" => {
                let reads = receiver.is_some_and(|receiver| {
                    receiver
                        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .any(|word| self.reads.contains(word))
                });
                self.events.push(Event::Commit {
                    kind: if reads { Kind::Read } else { Kind::Write },
                    block: self.blocks.last().copied().unwrap_or(0),
                    nested: self.blocks.len() > 1,
                    at,
                });
            }
            _ => {}
        }
    }

    fn operation(&mut self, base: &Expr) {
        let (name, args, at) = match base {
            Expr::MethodCall(call) => (
                call.method.to_string(),
                call.args.iter().collect::<Vec<_>>(),
                position(call.method.span()),
            ),
            Expr::Call(call) => match &*call.func {
                Expr::Path(function) => match function.path.segments.last() {
                    Some(last) => (
                        last.ident.to_string(),
                        call.args.iter().collect(),
                        position(last.ident.span()),
                    ),
                    None => return,
                },
                _ => return,
            },
            _ => return,
        };
        if STATEMENTS.contains(&name.as_str()) || UNIT_CALLS.contains(&name.as_str()) {
            return;
        }
        let text = args
            .iter()
            .filter(|arg| !matches!(arg, Expr::Closure(_) | Expr::Async(_)))
            .map(|arg| arg.to_token_stream().to_string())
            .collect::<Vec<_>>()
            .join(" , ");
        self.events.push(Event::Operation {
            arg: Arg::new(&text),
            at,
        });
    }

    fn helper(&mut self, helper: Helper, args: Vec<Arg>, at: Position) {
        if !STATEMENTS.contains(&helper.name.as_str()) {
            self.events.push(Event::Helper { helper, args, at });
        }
    }
}

impl<'ast> Visit<'ast> for Body {
    fn visit_item(&mut self, item: &'ast Item) {
        if !item_attrs(item).is_some_and(test_only) {
            visit::visit_item(self, item);
        }
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        let id = self.next_block;
        self.next_block += 1;
        if block
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, syn::Stmt::Expr(Expr::Return(_), _)))
        {
            self.returning.insert(id);
        }
        self.blocks.push(id);
        visit::visit_block(self, block);
        self.blocks.pop();
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if test_only(&local.attrs) {
            return;
        }
        if let Some(init) = &local.init {
            let text: String = init
                .expr
                .to_token_stream()
                .to_string()
                .split_whitespace()
                .collect();
            if let Some(name) = binding(&local.pat) {
                if text.contains("acquire()") {
                    self.acquired.insert(name.clone());
                }
                if text.contains("begin_write(") {
                    self.writes.insert(name.clone());
                }
                if text.contains("begin_read(") {
                    self.reads.insert(name);
                }
            }
        }
        if let Some(name) = binding(&local.pat) {
            let ty = match &local.pat {
                Pat::Type(typed) => Some((*typed.ty).clone()),
                _ => None,
            };
            let core = local.init.as_ref().and_then(|init| pool::alias(&init.expr));
            self.lets.push(Let { name, ty, core });
        }
        visit::visit_local(self, local);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let method = call.method.to_string();
        let at = position(call.method.span());
        let receiver = call.receiver.to_token_stream().to_string();
        self.call(
            &method,
            &call.args.to_token_stream().to_string(),
            Some(&receiver),
            at,
        );
        if is_self(&call.receiver) {
            let helper = Helper {
                name: method,
                same_type: true,
                method: true,
            };
            let args = call
                .args
                .iter()
                .map(|arg| Arg::new(&arg.to_token_stream().to_string()))
                .collect();
            self.helper(helper, args, at);
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_expr_await(&mut self, expr: &'ast syn::ExprAwait) {
        if self.operations {
            self.operation(&expr.base);
        }
        visit::visit_expr_await(self, expr);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(function) = &*call.func {
            if let Some(last) = function.path.segments.last() {
                let name = last.ident.to_string();
                if name == "begin_write" || name == "begin_read" {
                    self.call(&name, "", None, position(last.ident.span()));
                }
            }
            let names: Vec<&syn::Ident> = function
                .path
                .segments
                .iter()
                .map(|segment| &segment.ident)
                .collect();
            let target = match (function.qself.is_none(), names.as_slice()) {
                (true, [name]) => Some((false, *name)),
                (true, [first, name]) if *first == "Self" => Some((true, *name)),
                _ => None,
            };
            if let Some((same_type, name)) = target {
                let helper = Helper {
                    name: name.to_string(),
                    same_type,
                    method: false,
                };
                let args = call
                    .args
                    .iter()
                    .map(|arg| Arg::new(&arg.to_token_stream().to_string()))
                    .collect();
                self.helper(helper, args, position(name.span()));
            }
        }
        visit::visit_expr_call(self, call);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        walk_tokens(mac.tokens.clone(), &mut |tokens, index| {
            let (Some(TokenTree::Ident(ident)), Some(TokenTree::Group(arguments))) =
                (tokens.get(index), tokens.get(index + 1))
            else {
                return;
            };
            if arguments.delimiter() != Delimiter::Parenthesis {
                return;
            }
            let at = position(ident.span());
            let name = ident.to_string();
            let before = |distance: usize| {
                index
                    .checked_sub(distance)
                    .and_then(|earlier| tokens.get(earlier))
            };
            let named = |token: Option<&TokenTree>, word: &str| matches!(token, Some(TokenTree::Ident(ident)) if ident == word);
            if is_punct(before(1), '.') {
                let receiver = match before(2) {
                    Some(TokenTree::Ident(ident)) => Some(ident.to_string()),
                    _ => None,
                };
                self.call(
                    &name,
                    &arguments.stream().to_string(),
                    receiver.as_deref(),
                    at,
                );
                if named(before(2), "self") {
                    let helper = Helper {
                        name,
                        same_type: true,
                        method: true,
                    };
                    self.helper(helper, split_arguments(arguments.stream()), at);
                }
            } else if is_punct(before(1), ':') {
                if is_punct(before(2), ':') && named(before(3), "Self") {
                    let helper = Helper {
                        name,
                        same_type: true,
                        method: false,
                    };
                    self.helper(helper, split_arguments(arguments.stream()), at);
                }
            } else {
                let helper = Helper {
                    name,
                    same_type: false,
                    method: false,
                };
                self.helper(helper, split_arguments(arguments.stream()), at);
            }
        });
    }
}

struct Def {
    name: String,
    owner: Option<String>,
    line: usize,
    /// Parameter names, without the receiver.
    params: Vec<String>,
    receiver: bool,
    /// The parameters and bindings that hold the pool, by type or by what they copy.
    pool_names: HashSet<String>,
    /// The fields of `self` that hold the pool.
    pool_fields: HashSet<String>,
    /// Begins a transaction or takes a `WriteTx` / `ReadTx`.
    unit: bool,
    body: Body,
}

/// Every function with a body in the file, and the type whose impl it sits in.
#[derive(Default)]
struct Collect {
    operations: bool,
    owner: Option<String>,
    defs: Vec<Def>,
    /// Per struct of the file, its fields that hold a pool.
    fields: HashMap<String, HashSet<String>>,
    /// The pool fields of the struct the current impl is for.
    current: HashSet<String>,
    /// The generics of the current impl that are bounded by `DriverPool`.
    bounds: HashSet<String>,
}

impl Collect {
    fn function(&mut self, signature: &syn::Signature, block: &syn::Block) {
        let mut body = Body {
            operations: self.operations,
            ..Body::default()
        };
        body.visit_block(block);
        body.events.sort_by_key(Event::at);
        let units = unit_params(signature);
        for (name, kind, _) in &units {
            if *kind == Kind::Write {
                body.writes.insert(name.clone());
            } else {
                body.reads.insert(name.clone());
            }
        }
        let unit = !units.is_empty()
            || body
                .events
                .iter()
                .any(|event| matches!(event, Event::Begin { .. }));
        let params = signature
            .inputs
            .iter()
            .filter_map(|input| match input {
                FnArg::Receiver(_) => None,
                FnArg::Typed(typed) => Some(binding(&typed.pat).unwrap_or_else(|| "_".into())),
            })
            .collect();
        let mut bounded = pool::bounded(&signature.generics);
        bounded.extend(self.bounds.iter().cloned());
        let mut pool_names: HashSet<String> = signature
            .inputs
            .iter()
            .filter_map(|input| match input {
                FnArg::Typed(typed) if pool::pool_type(&typed.ty, &bounded) => binding(&typed.pat),
                _ => None,
            })
            .collect();
        let pool_fields = if signature.receiver().is_some() {
            self.current.clone()
        } else {
            HashSet::new()
        };
        for each in &body.lets {
            let pool = each
                .ty
                .as_ref()
                .is_some_and(|ty| pool::pool_type(ty, &bounded))
                || each
                    .core
                    .as_ref()
                    .is_some_and(|arg| pool::pool_text(arg, &pool_names, &pool_fields));
            if pool {
                pool_names.insert(each.name.clone());
            }
        }
        self.defs.push(Def {
            name: signature.ident.to_string(),
            owner: self.owner.clone(),
            line: signature.fn_token.span.start().line,
            params,
            receiver: signature.receiver().is_some(),
            pool_names,
            pool_fields,
            unit,
            body,
        });
    }
}

impl<'ast> Visit<'ast> for Collect {
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
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let outer = self
            .owner
            .replace(item.self_ty.to_token_stream().to_string());
        let fields = match &*item.self_ty {
            Type::Path(ty) => ty
                .path
                .segments
                .last()
                .and_then(|last| self.fields.get(&last.ident.to_string()))
                .cloned()
                .unwrap_or_default(),
            _ => HashSet::new(),
        };
        let outer_fields = std::mem::replace(&mut self.current, fields);
        let outer_bounds = std::mem::replace(&mut self.bounds, pool::bounded(&item.generics));
        visit::visit_item_impl(self, item);
        self.current = outer_fields;
        self.bounds = outer_bounds;
        self.owner = outer;
    }
    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        let outer = self.owner.replace(item.ident.to_string());
        visit::visit_item_trait(self, item);
        self.owner = outer;
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.function(&item.sig, &item.block);
        let outer = self.owner.take();
        visit::visit_item_fn(self, item);
        self.owner = outer;
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.function(&item.sig, &item.block);
        let outer = self.owner.take();
        visit::visit_impl_item_fn(self, item);
        self.owner = outer;
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if let Some(block) = &item.default {
            self.function(&item.sig, block);
        }
        let outer = self.owner.take();
        visit::visit_trait_item_fn(self, item);
        self.owner = outer;
    }
}

enum Step {
    Statement { target: Target, via: Vec<String> },
    Begin(Kind),
    Commit { kind: Kind, early: bool },
}

struct Resolver<'a> {
    defs: &'a [Def],
    index: HashMap<(Option<&'a str>, &'a str), usize>,
}

impl<'a> Resolver<'a> {
    fn new(defs: &'a [Def]) -> Self {
        let index = defs
            .iter()
            .enumerate()
            .map(|(at, def)| ((def.owner.as_deref(), def.name.as_str()), at))
            .collect();
        Self { defs, index }
    }

    /// The same-file function a helper call reaches, when there is one.
    fn resolve(&self, def: &Def, helper: &Helper) -> Option<usize> {
        let owner = if helper.same_type {
            def.owner.as_deref()
        } else {
            None
        };
        let next = *self.index.get(&(owner, helper.name.as_str()))?;
        let callee = self.defs.get(next)?;
        (!helper.method || callee.receiver).then_some(next)
    }

    /// What `arg` is inside `def`. `bound` holds what each parameter received;
    /// it is `None` for the function being checked, where the text decides.
    fn target(def: &Def, bound: Option<&HashMap<String, Target>>, arg: &Arg) -> Target {
        let named = |names: &HashSet<String>| arg.words.iter().any(|word| names.contains(word));
        if named(&def.body.writes) {
            return Target::Transaction(Kind::Write);
        }
        if named(&def.body.reads) {
            return Target::Transaction(Kind::Read);
        }
        if let Some(target) = bound.and_then(|bound| arg.words.iter().find_map(|w| bound.get(w))) {
            return *target;
        }
        if pool::pool_text(arg, &def.pool_names, &def.pool_fields)
            || named(&def.body.acquired)
            || arg.squashed.contains("pool")
        {
            Target::Pool
        } else {
            Target::Other
        }
    }

    /// The statements, begins and commits of a function in the order they run,
    /// with those of the same-file helpers it calls in place.
    fn flatten(
        &self,
        at: usize,
        bound: Option<&HashMap<String, Target>>,
        stack: &mut Vec<usize>,
        via: &mut Vec<String>,
        out: &mut Vec<Step>,
    ) {
        let Some(def) = self.defs.get(at) else {
            return;
        };
        let resolved: HashSet<Position> = def
            .body
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Helper { helper, at, .. } => self.resolve(def, helper).map(|_| *at),
                _ => None,
            })
            .collect();
        for event in &def.body.events {
            match event {
                Event::Statement { arg, .. } => out.push(Step::Statement {
                    target: Self::target(def, bound, arg),
                    via: via.clone(),
                }),
                Event::Operation { arg, at } => {
                    let target = Self::target(def, bound, arg);
                    if target != Target::Other && !resolved.contains(at) {
                        out.push(Step::Statement {
                            target,
                            via: via.clone(),
                        });
                    }
                }
                Event::Begin { kind, at } => {
                    if !resolved.contains(at) {
                        out.push(Step::Begin(*kind));
                    }
                }
                Event::Commit { at, .. } if resolved.contains(at) => {}
                Event::Commit {
                    kind,
                    block,
                    nested,
                    ..
                } => out.push(Step::Commit {
                    kind: *kind,
                    early: *nested && def.body.returning.contains(block),
                }),
                Event::Helper { helper, args, .. } => {
                    let Some(next) = self.resolve(def, helper) else {
                        continue;
                    };
                    let Some(callee) = self.defs.get(next) else {
                        continue;
                    };
                    if stack.contains(&next) {
                        continue;
                    }
                    let skip = usize::from(callee.receiver && !helper.method);
                    let mut received: HashMap<String, Target> = callee
                        .params
                        .iter()
                        .map(|param| (param.clone(), Target::Other))
                        .collect();
                    for (param, arg) in callee.params.iter().zip(args.iter().skip(skip)) {
                        received.insert(param.clone(), Self::target(def, bound, arg));
                    }
                    stack.push(next);
                    via.push(helper.name.clone());
                    self.flatten(next, Some(&received), stack, via, out);
                    via.pop();
                    stack.pop();
                }
            }
        }
    }
}

pub fn violations(
    path: &str,
    ast: &syn::File,
    exceptions: &[Exception],
    data: bool,
) -> Vec<Violation> {
    let mut collect = Collect {
        operations: !data,
        ..Collect::default()
    };
    collect.fields = pool::pool_fields(ast);
    collect.visit_file(ast);
    let resolver = Resolver::new(&collect.defs);
    let mut found = Vec::new();
    for (position, def) in collect.defs.iter().enumerate() {
        let excepted = exceptions.iter().any(|(file, function, exempt, _)| {
            *file == path && *function == def.name && *exempt == Exempt::Shape
        });
        if excepted || (!data && !def.unit) {
            continue;
        }
        let mut items = Vec::new();
        resolver.flatten(
            position,
            None,
            &mut vec![position],
            &mut Vec::new(),
            &mut items,
        );
        let statements = items
            .iter()
            .filter(|item| matches!(item, Step::Statement { .. }))
            .count();
        if !def.unit && statements < 2 {
            continue;
        }
        let mut add = |message: String| {
            found.push(Violation {
                rule: 3,
                path: path.to_string(),
                line: def.line,
                message,
            });
        };
        let name = &def.name;

        let mut on_pool = 0;
        let mut helpers: Vec<&str> = Vec::new();
        for item in &items {
            if let Step::Statement {
                target: Target::Pool,
                via,
            } = item
            {
                on_pool += 1;
                for helper in via {
                    if !helpers.contains(&helper.as_str()) {
                        helpers.push(helper);
                    }
                }
            }
        }
        if on_pool > 0 {
            let through = if helpers.is_empty() {
                String::new()
            } else {
                format!(" (through {})", helpers.join(", "))
            };
            add(format!(
                "{name} runs {on_pool} of its {statements} statements on the pool{through}; \
                 run them in one transaction"
            ));
        }

        let begun = |wanted: Kind| {
            items
                .iter()
                .filter(|item| matches!(item, Step::Begin(kind) if *kind == wanted))
                .count()
        };
        let (writes, reads) = (begun(Kind::Write), begun(Kind::Read));
        if writes > 1 || reads > 1 {
            add(format!(
                "{name} begins {writes} write and {reads} read transactions; \
                 begin at most one of each"
            ));
        }

        let begin_write = items
            .iter()
            .position(|item| matches!(item, Step::Begin(Kind::Write)));
        let read_after_begin = begin_write.is_some_and(|begin| {
            items.iter().skip(begin).any(|item| {
                matches!(
                    item,
                    Step::Statement {
                        target: Target::Transaction(Kind::Read),
                        ..
                    }
                )
            })
        });
        if read_after_begin {
            add(format!(
                "{name} reads in a read transaction after beginning its write \
                 transaction; do the reads first"
            ));
        }

        let commits = |early: bool| {
            items
                .iter()
                .enumerate()
                .filter_map(|(at, item)| match item {
                    Step::Commit {
                        kind: Kind::Write,
                        early: skipped,
                    } if early || !*skipped => Some(at),
                    _ => None,
                })
                .max()
        };
        if let Some(last) = commits(false).or_else(|| commits(true)) {
            let after = items
                .iter()
                .skip(last + 1)
                .any(|item| matches!(item, Step::Statement { .. }));
            if after {
                add(format!(
                    "{name} runs a statement after committing its write transaction; \
                     run it in the transaction"
                ));
            }
        }
    }
    found
}
