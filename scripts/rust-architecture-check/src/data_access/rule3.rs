//! Rule 3: a function that runs two or more statements on the pool, one of them
//! a write, does it in one `begin_write()` transaction. Statements reached
//! through same-file helpers count as the caller's.
use super::{is_punct, walk_tokens, Violation};
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
    Expr, FnArg, ImplItem, Item, Pat, TraitItem,
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
const WRITES: [&str; 2] = ["execute", "execute_many"];

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
        write: bool,
        arg: Arg,
        at: Position,
    },
    Helper {
        helper: Helper,
        args: Vec<Arg>,
        at: Position,
    },
    Begin(Position),
    /// A commit, and where the block it sits in ends.
    Commit {
        at: Position,
        until: Position,
    },
}

impl Event {
    fn at(&self) -> Position {
        match self {
            Event::Statement { at, .. }
            | Event::Helper { at, .. }
            | Event::Begin(at)
            | Event::Commit { at, .. } => *at,
        }
    }
}

fn binding(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(ident) => Some(ident.ident.to_string()),
        Pat::Type(typed) => binding(&typed.pat),
        Pat::TupleStruct(wrapper) if wrapper.path.is_ident("Ok") && wrapper.elems.len() == 1 => {
            wrapper.elems.first().and_then(binding)
        }
        _ => None,
    }
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

/// Every statement, helper call, `begin_write()` and `commit()` in a function
/// body, and the connections it checks out.
#[derive(Default)]
struct Body {
    acquired: HashSet<String>,
    events: Vec<Event>,
    blocks: Vec<Position>,
}

impl Body {
    fn call(&mut self, method: &str, arguments: &str, at: Position) {
        if STATEMENTS.contains(&method) {
            self.events.push(Event::Statement {
                write: WRITES.contains(&method),
                arg: Arg::new(arguments),
                at,
            });
        }
        match method {
            "begin_write" => self.events.push(Event::Begin(at)),
            "commit" => {
                let until = self
                    .blocks
                    .last()
                    .copied()
                    .unwrap_or((usize::MAX, usize::MAX));
                self.events.push(Event::Commit { at, until });
            }
            _ => {}
        }
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
        self.blocks.push(position(block.brace_token.span.close()));
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
            if text.contains("acquire()") {
                if let Some(name) = binding(&local.pat) {
                    self.acquired.insert(name);
                }
            }
        }
        visit::visit_local(self, local);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let method = call.method.to_string();
        let at = position(call.method.span());
        self.call(&method, &call.args.to_token_stream().to_string(), at);
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
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(function) = &*call.func {
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
                self.call(&name, &arguments.stream().to_string(), at);
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
    body: Body,
}

/// Every function with a body in the file, and the type whose impl it sits in.
#[derive(Default)]
struct Collect {
    owner: Option<String>,
    defs: Vec<Def>,
}

impl Collect {
    fn function(&mut self, signature: &syn::Signature, block: &syn::Block) {
        let mut body = Body::default();
        body.visit_block(block);
        body.events.sort_by_key(Event::at);
        let params = signature
            .inputs
            .iter()
            .filter_map(|input| match input {
                FnArg::Receiver(_) => None,
                FnArg::Typed(typed) => Some(binding(&typed.pat).unwrap_or_else(|| "_".into())),
            })
            .collect();
        self.defs.push(Def {
            name: signature.ident.to_string(),
            owner: self.owner.clone(),
            line: signature.fn_token.span.start().line,
            params,
            receiver: signature.receiver().is_some(),
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
        visit::visit_item_impl(self, item);
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

/// A statement that runs on the pool, and the helpers it was reached through.
struct Counted {
    write: bool,
    at: Position,
    via: Vec<String>,
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

    /// Whether `arg` is the pool inside `def`. `bound` holds the parameters that
    /// received the pool; it is `None` for the function being checked, where
    /// the text decides.
    fn is_pool(def: &Def, bound: Option<&HashSet<String>>, arg: &Arg) -> bool {
        let received = |word: &String| bound.is_some_and(|bound| bound.contains(word));
        let uses_parameter = bound.is_some() && arg.words.iter().any(|w| def.params.contains(w));
        arg.squashed.contains("self.pool")
            || arg.squashed.contains("acquire()")
            || arg.words.iter().any(|w| def.body.acquired.contains(w))
            || arg.words.iter().any(received)
            || (!uses_parameter && arg.squashed.contains("pool"))
    }

    fn collect(
        &self,
        at: usize,
        bound: Option<&HashSet<String>>,
        stack: &mut Vec<usize>,
        site: Option<Position>,
        via: &mut Vec<String>,
        out: &mut Vec<Counted>,
    ) {
        let Some(def) = self.defs.get(at) else {
            return;
        };
        for event in &def.body.events {
            match event {
                Event::Statement { write, arg, at } if Self::is_pool(def, bound, arg) => {
                    out.push(Counted {
                        write: *write,
                        at: site.unwrap_or(*at),
                        via: via.clone(),
                    });
                }
                Event::Helper { helper, args, at } => {
                    let owner = if helper.same_type {
                        def.owner.as_deref()
                    } else {
                        None
                    };
                    let Some(&next) = self.index.get(&(owner, helper.name.as_str())) else {
                        continue;
                    };
                    let Some(callee) = self.defs.get(next) else {
                        continue;
                    };
                    if stack.contains(&next) || (helper.method && !callee.receiver) {
                        continue;
                    }
                    let skip = usize::from(callee.receiver && !helper.method);
                    let received: HashSet<String> = callee
                        .params
                        .iter()
                        .zip(args.iter().skip(skip))
                        .filter(|(_, arg)| Self::is_pool(def, bound, arg))
                        .map(|(param, _)| param.clone())
                        .collect();
                    stack.push(next);
                    via.push(helper.name.clone());
                    self.collect(next, Some(&received), stack, site.or(Some(*at)), via, out);
                    via.pop();
                    stack.pop();
                }
                Event::Statement { .. } | Event::Begin(_) | Event::Commit { .. } => {}
            }
        }
    }
}

pub fn violations(
    path: &str,
    ast: &syn::File,
    exceptions: &[(&str, &str, &str)],
) -> Vec<Violation> {
    let mut collect = Collect::default();
    collect.visit_file(ast);
    let resolver = Resolver::new(&collect.defs);
    let mut found = Vec::new();
    for (position, def) in collect.defs.iter().enumerate() {
        let excepted = exceptions
            .iter()
            .any(|(file, function, _)| *file == path && *function == def.name);
        if excepted {
            continue;
        }
        let mut counted = Vec::new();
        resolver.collect(
            position,
            None,
            &mut vec![position],
            None,
            &mut Vec::new(),
            &mut counted,
        );
        let writes = counted.iter().filter(|call| call.write).count();
        let mut add = |message: String| {
            found.push(Violation {
                rule: 3,
                path: path.to_string(),
                line: def.line,
                message,
            });
        };
        if counted.len() >= 2 && writes >= 1 {
            let mut helpers: Vec<&str> = Vec::new();
            for name in counted.iter().flat_map(|call| &call.via) {
                if !helpers.contains(&name.as_str()) {
                    helpers.push(name);
                }
            }
            let through = if helpers.is_empty() {
                String::new()
            } else {
                format!(" (through {})", helpers.join(", "))
            };
            add(format!(
                "{} runs {} statements on the pool{through}, {writes} of them writes, \
                 outside a transaction",
                def.name,
                counted.len()
            ));
        }
        let begins = def
            .body
            .events
            .iter()
            .any(|event| matches!(event, Event::Begin(_)));
        let commits: Vec<(Position, Position)> = def
            .body
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Commit { at, until } => Some((*at, *until)),
                _ => None,
            })
            .collect();
        let last_commit = commits.iter().map(|(at, _)| *at).max();
        if let (true, Some(last)) = (begins, last_commit) {
            let inside = |call: &Counted| {
                call.at < last
                    && !commits
                        .iter()
                        .any(|(at, until)| *at < call.at && call.at <= *until)
            };
            if counted.iter().any(inside) {
                add(format!(
                    "{} runs a statement on the pool before committing its write \
                     transaction; run it in the transaction",
                    def.name
                ));
            }
        }
    }
    found
}
