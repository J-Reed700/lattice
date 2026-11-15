//! Naming what a model is shown: one mint, one written form, one resolver.
//!
//! A handle is a short prefix and a 1-based ordinal (`s4`). In a field it is
//! the whole string; in prose it is the same string in square brackets.
//! Nothing else is ever a handle, and no artefact stores one: lessons, cards
//! and notes store references ([`Kind::Ref`]).

use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::shared::error::{AppError, Result};

/// A kind of thing that can be shown. The feature that owns the type declares
/// it; the core names no feature's vocabulary.
pub trait Kind {
    /// The letters a handle of this kind starts with. Unique within a scope.
    const PREFIX: &'static str;
    /// How a person sees a reference to one.
    const DRAWN: Drawn;
    /// The thing's internal identity: recorded, never rendered.
    type Ref: Clone + PartialEq + fmt::Debug + Serialize + DeserializeOwned + Send + Sync + 'static;

    fn reference(&self) -> Self::Ref;
}

/// How a person sees a reference: as its number, by its name, or as the thing itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drawn {
    Number,
    Name,
    Itself,
}

/// Context the model reads. Always rendered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Told<T>(pub T);

/// Machine identity: recorded with the call and never rendered. It has no
/// `Display`, and its `Debug` does not print the value.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Internal<T>(T);

impl<T> Internal<T> {
    pub fn of(value: T) -> Self {
        Self(value)
    }

    pub fn value(&self) -> &T {
        &self.0
    }
}

impl<T> fmt::Debug for Internal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Internal(..)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(super) struct Name {
    pub(super) prefix: String,
    pub(super) ordinal: u32,
}

impl Name {
    fn written(&self) -> String {
        format!("{}{}", self.prefix, self.ordinal)
    }

    /// `s4` as a name. `None` for anything that is not a prefix of letters and a 1-based ordinal.
    fn read(written: &str) -> Option<Self> {
        let digits = written.find(|c: char| c.is_ascii_digit())?;
        let (prefix, ordinal) = written.split_at(digits);
        let well_formed = !prefix.is_empty()
            && prefix.chars().all(|c| c.is_ascii_lowercase())
            && !ordinal.starts_with('0')
            && ordinal.chars().all(|c| c.is_ascii_digit());
        let ordinal = ordinal.parse().ok().filter(|_| well_formed)?;
        Some(Self {
            prefix: prefix.to_owned(),
            ordinal,
        })
    }
}

/// The name one shown thing has in its scope. Minted and resolved only here.
pub struct Handle<K> {
    name: Name,
    /// The item this thing is one of the own things of, when it is labelled within an item.
    of: Option<Name>,
    kind: PhantomData<fn() -> K>,
}

impl<K> Handle<K> {
    fn new(name: Name, of: Option<Name>) -> Self {
        Self {
            name,
            of,
            kind: PhantomData,
        }
    }

    /// The handle as a field holds it: `s4`.
    pub fn written(&self) -> String {
        self.name.written()
    }

    /// The handle as prose holds it: `[s4]`.
    pub fn bracketed(&self) -> String {
        format!("[{}]", self.name.written())
    }

    /// The number a person sees for a kind drawn as a number.
    pub fn number(&self) -> u32 {
        self.name.ordinal
    }

    pub(super) fn name(&self) -> &Name {
        &self.name
    }
}

impl<K> Clone for Handle<K> {
    fn clone(&self) -> Self {
        Self::new(self.name.clone(), self.of.clone())
    }
}

impl<K> PartialEq for Handle<K> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.of == other.of
    }
}

impl<K> Eq for Handle<K> {}

impl<K> PartialOrd for Handle<K> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K> Ord for Handle<K> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.of, &self.name).cmp(&(&other.of, &other.name))
    }
}

impl<K> fmt::Debug for Handle<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.of {
            Some(of) => write!(f, "{}/{}", of.written(), self.name.written()),
            None => f.write_str(&self.name.written()),
        }
    }
}

impl<K> fmt::Display for Handle<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name.written())
    }
}

/// A pointer the app resolved: the handle the model wrote and the thing it names.
pub struct Picked<K: Kind> {
    pub handle: Handle<K>,
    pub reference: K::Ref,
}

impl<K: Kind> Clone for Picked<K> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            reference: self.reference.clone(),
        }
    }
}

impl<K: Kind> PartialEq for Picked<K> {
    fn eq(&self, other: &Self) -> bool {
        self.handle == other.handle
    }
}

impl<K: Kind> fmt::Debug for Picked<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} -> {:?}", self.handle, self.reference)
    }
}

/// One minted name as a stored scope keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub prefix: String,
    pub ordinal: u32,
    /// The item this thing is labelled within, as (prefix, ordinal).
    pub under: Option<(String, u32)>,
    pub reference: serde_json::Value,
}

/// One kind's minted things: the ordinal of a reference is its place here plus one.
#[derive(Debug, Default)]
struct Minted {
    /// The Rust type the prefix belongs to, once a thing of it was shown in this process.
    kind: Option<&'static str>,
    references: Vec<serde_json::Value>,
    by_reference: BTreeMap<String, u32>,
}

impl Minted {
    fn claim<K: Kind>(&mut self) -> Result<()> {
        let kind = std::any::type_name::<K>();
        match self.kind {
            Some(held) if held != kind => Err(AppError::InternalError(format!(
                "The handle prefix '{}' is used by {held} and by {kind} in one scope.",
                K::PREFIX
            ))),
            _ => {
                self.kind = Some(kind);
                Ok(())
            }
        }
    }

    /// The ordinal `reference` has, minting one when it has none. The flag says whether it was minted.
    fn ordinal(&mut self, reference: serde_json::Value) -> (u32, bool) {
        let key = reference.to_string();
        if let Some(ordinal) = self.by_reference.get(&key) {
            return (*ordinal, false);
        }
        self.references.push(reference);
        let ordinal = u32::try_from(self.references.len()).unwrap_or(u32::MAX);
        self.by_reference.insert(key, ordinal);
        (ordinal, true)
    }

    fn restore(&mut self, ordinal: u32, reference: serde_json::Value) -> Result<()> {
        if u32::try_from(self.references.len() + 1).ok() != Some(ordinal) {
            return Err(AppError::InvalidData(
                "A stored scope's names are not in the order they were given.".into(),
            ));
        }
        self.by_reference.insert(reference.to_string(), ordinal);
        self.references.push(reference);
        Ok(())
    }

    fn reference(&self, ordinal: u32) -> Option<&serde_json::Value> {
        let index = usize::try_from(ordinal.checked_sub(1)?).ok()?;
        self.references.get(index)
    }
}

type Kinds = BTreeMap<String, Minted>;

/// Everything one conversation with a model has been shown. The only mint and
/// the only resolver.
///
/// A question's scope ends with the question: a handle is stable across its
/// correction, its draft and answer steps and its smaller re-asks. A stored
/// scope lives with its conversation: a thing gets its name the first time it
/// is shown and keeps it, and no name is reused.
#[derive(Debug, Default)]
pub struct Scope {
    stored: bool,
    top: Kinds,
    under: BTreeMap<Name, Kinds>,
    /// The one kind of a stored scope a person sees as a bare number.
    numbered: Option<String>,
    fresh: Vec<Record>,
}

impl Scope {
    /// A scope that ends with the question and its re-asks.
    pub fn for_question() -> Self {
        Self::default()
    }

    /// A conversation's scope, from the names it has been given so far, in the order they were given.
    pub fn stored(records: Vec<Record>) -> Result<Self> {
        let mut scope = Self {
            stored: true,
            ..Self::default()
        };
        for record in records {
            let kinds = match record.under {
                Some((prefix, ordinal)) => scope.under.entry(Name { prefix, ordinal }).or_default(),
                None => &mut scope.top,
            };
            kinds
                .entry(record.prefix)
                .or_default()
                .restore(record.ordinal, record.reference)?;
        }
        Ok(scope)
    }

    /// The names minted since the scope was made, for a stored scope's store to keep.
    pub fn fresh(&self) -> &[Record] {
        &self.fresh
    }

    /// Starts a request. What the request shows is what its answer may point at.
    pub fn request(&mut self) -> Showing<'_> {
        Showing {
            scope: self,
            top: BTreeMap::new(),
            under: BTreeMap::new(),
        }
    }

    /// What a reply or a tool call wrote, against everything the scope holds:
    /// the transcript still shows it.
    pub(super) fn resolve<K: Kind>(&self, written: &str) -> Option<Picked<K>> {
        let name = Name::read(written).filter(|name| name.prefix == K::PREFIX)?;
        let reference = self.top.get(K::PREFIX)?.reference(name.ordinal)?;
        picked(name, None, reference)
    }

    /// What a reply wrote, whatever its kind: the handle's prefix and number
    /// and the reference it names. A reply's markers are told apart by prefix.
    pub(super) fn resolve_any(&self, written: &str) -> Option<(String, u32, serde_json::Value)> {
        let name = Name::read(written)?;
        let reference = self.top.get(&name.prefix)?.reference(name.ordinal)?.clone();
        Some((name.prefix, name.ordinal, reference))
    }

    fn mint<K: Kind>(&mut self, of: Option<&Name>, thing: &K) -> Result<Name> {
        if self.stored && of.is_none() && K::DRAWN == Drawn::Number {
            match &self.numbered {
                Some(prefix) if prefix != K::PREFIX => {
                    return Err(AppError::InternalError(format!(
                        "A stored scope draws one kind as a number; '{prefix}' and '{}' both ask to be.",
                        K::PREFIX
                    )))
                }
                _ => self.numbered = Some(K::PREFIX.to_owned()),
            }
        }
        let reference = serde_json::to_value(thing.reference()).map_err(|error| {
            AppError::InternalError(format!(
                "A reference of kind '{}' could not be recorded: {error}",
                K::PREFIX
            ))
        })?;
        let kinds = match of {
            Some(of) => self.under.entry(of.clone()).or_default(),
            None => &mut self.top,
        };
        let minted = kinds.entry(K::PREFIX.to_owned()).or_default();
        minted.claim::<K>()?;
        let (ordinal, new) = minted.ordinal(reference.clone());
        if new {
            self.fresh.push(Record {
                prefix: K::PREFIX.to_owned(),
                ordinal,
                under: of.map(|of| (of.prefix.clone(), of.ordinal)),
                reference,
            });
        }
        Ok(Name {
            prefix: K::PREFIX.to_owned(),
            ordinal,
        })
    }
}

fn picked<K: Kind>(
    name: Name,
    of: Option<Name>,
    reference: &serde_json::Value,
) -> Option<Picked<K>> {
    let reference = serde_json::from_value(reference.clone()).ok()?;
    Some(Picked {
        handle: Handle::new(name, of),
        reference,
    })
}

/// What one request shows. A structured answer is resolved against this and
/// nothing else: a handle minted for an earlier request of the question, for
/// an item since settled, names nothing here.
#[derive(Debug)]
pub struct Showing<'s> {
    scope: &'s mut Scope,
    /// The ordinals this request shows as pointable, by prefix, in the order shown.
    top: BTreeMap<String, Vec<u32>>,
    under: BTreeMap<Name, BTreeMap<String, Vec<u32>>>,
}

impl Showing<'_> {
    /// Shows `things` as pointable: mints a name for each, or takes the one it has.
    pub fn show<'a, K: Kind>(&mut self, things: &'a [K]) -> Result<Shown<'a, K>> {
        let mut handles = Vec::with_capacity(things.len());
        for thing in things {
            let name = self.scope.mint(None, thing)?;
            let shown = self.top.entry(name.prefix.clone()).or_default();
            if !shown.contains(&name.ordinal) {
                shown.push(name.ordinal);
            }
            handles.push(Handle::new(name, None));
        }
        Ok(Shown { things, handles })
    }

    /// Shows an item's own things, labelled within it: `o2` under question 3
    /// is question 3's second option.
    pub fn under<'a, K: Kind, C: Kind>(
        &mut self,
        of: &Handle<K>,
        own: &'a [C],
    ) -> Result<Shown<'a, C>> {
        let mut handles = Vec::with_capacity(own.len());
        for thing in own {
            let name = self.scope.mint(Some(of.name()), thing)?;
            let shown = self
                .under
                .entry(of.name().clone())
                .or_default()
                .entry(name.prefix.clone())
                .or_default();
            if !shown.contains(&name.ordinal) {
                shown.push(name.ordinal);
            }
            handles.push(Handle::new(name, Some(of.name().clone())));
        }
        Ok(Shown {
            things: own,
            handles,
        })
    }

    /// The thing of kind `K` this request shows under `written`.
    pub(super) fn resolve<K: Kind>(&self, written: &str) -> Option<Picked<K>> {
        let name = Name::read(written).filter(|name| name.prefix == K::PREFIX)?;
        self.top
            .get(K::PREFIX)?
            .contains(&name.ordinal)
            .then_some(())?;
        let reference = self.scope.top.get(K::PREFIX)?.reference(name.ordinal)?;
        picked(name, None, reference)
    }

    /// The own thing of `of` this request shows under `written`.
    pub(super) fn resolve_under<C: Kind>(&self, of: &Name, written: &str) -> Option<Picked<C>> {
        let name = Name::read(written).filter(|name| name.prefix == C::PREFIX)?;
        self.under
            .get(of)?
            .get(C::PREFIX)?
            .contains(&name.ordinal)
            .then_some(())?;
        let reference = self
            .scope
            .under
            .get(of)?
            .get(C::PREFIX)?
            .reference(name.ordinal)?;
        picked(name, Some(of.clone()), reference)
    }

    /// Every thing of kind `K` this request shows, in the order shown.
    pub(super) fn shown<K: Kind>(&self) -> Vec<Picked<K>> {
        self.top
            .get(K::PREFIX)
            .into_iter()
            .flatten()
            .filter_map(|ordinal| self.resolve(&format!("{}{ordinal}", K::PREFIX)))
            .collect()
    }

    /// Every own thing of kind `C` this request shows under `of`, in the order shown.
    pub(super) fn shown_under<C: Kind>(&self, of: &Name) -> Vec<Picked<C>> {
        self.under
            .get(of)
            .and_then(|kinds| kinds.get(C::PREFIX))
            .into_iter()
            .flatten()
            .filter_map(|ordinal| self.resolve_under(of, &format!("{}{ordinal}", C::PREFIX)))
            .collect()
    }

    /// The written handles a pointer to a thing of the kind with `prefix`
    /// may hold, in the order shown: the `enum` of the field.
    pub(super) fn written(&self, prefix: &str) -> Vec<String> {
        self.top
            .get(prefix)
            .into_iter()
            .flatten()
            .map(|ordinal| format!("{prefix}{ordinal}"))
            .collect()
    }

    /// The labels any item of this request has for its own things of the
    /// kind with `prefix`: the `enum` of a pointer inside an entry.
    pub(super) fn written_under(&self, prefix: &str) -> Vec<String> {
        let mut ordinals: Vec<u32> = self
            .under
            .values()
            .filter_map(|kinds| kinds.get(prefix))
            .flatten()
            .copied()
            .collect();
        ordinals.sort_unstable();
        ordinals.dedup();
        ordinals
            .into_iter()
            .map(|ordinal| format!("{prefix}{ordinal}"))
            .collect()
    }
}

/// One kind's things as a request shows them, in order, each with its handle.
#[derive(Debug)]
pub struct Shown<'a, K: Kind> {
    things: &'a [K],
    handles: Vec<Handle<K>>,
}

impl<'a, K: Kind> Shown<'a, K> {
    pub fn iter(&self) -> impl Iterator<Item = (&Handle<K>, &'a K)> + '_ {
        self.handles.iter().zip(self.things)
    }

    pub fn handles(&self) -> &[Handle<K>] {
        &self.handles
    }

    pub fn len(&self) -> usize {
        self.handles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }
}
