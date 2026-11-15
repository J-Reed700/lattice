//! Reading the fields of one statement: every accessor is total, and a
//! pointer is used only after it resolved against what the request shows.

use serde_json::{Map, Value};

use super::super::edit::{consistent, Anchor, Change, Consistent, Content};
use super::super::shown::{Kind, Name, Picked, Showing};
use super::{Authored, Union, Vocabulary, Why};

/// The fields of one statement as the model wrote them. An accessor returns
/// `None` when its field cannot be used and records why; the first reason
/// recorded is the statement's.
pub struct Fields<'r, 's> {
    pub(in super::super) object: &'r serde_json::Map<String, serde_json::Value>,
    pub(in super::super) showing: &'r Showing<'s>,
    /// The item the statement is an entry of, when it is one.
    pub(in super::super) item: Option<&'r Name>,
    pub(super) why: Option<Why>,
    /// Evidence pointers that did not resolve: traced, and they cost nothing else.
    pub(super) lost_cites: usize,
}

impl<'r, 's> Fields<'r, 's> {
    pub(in super::super) fn of(
        object: &'r serde_json::Map<String, serde_json::Value>,
        showing: &'r Showing<'s>,
        item: Option<&'r Name>,
    ) -> Self {
        Self {
            object,
            showing,
            item,
            why: None,
            lost_cites: 0,
        }
    }

    /// Why the statement is unanswered, once an accessor returned `None`.
    pub(super) fn why(&self) -> Why {
        self.why.clone().unwrap_or(Why::NotRead)
    }

    /// Text the model authored. Never scanned and never edited by the app.
    pub fn text(&mut self, name: &'static str) -> Option<String> {
        self.string(name).map(str::to_owned)
    }

    /// The meaning of the word written in `name`.
    pub fn word<W: Vocabulary>(&mut self, name: &'static str) -> Option<W> {
        let written = self.string(name)?;
        match W::WORDS.iter().find(|(word, _)| *word == written) {
            Some((_, meaning)) => Some(*meaning),
            None => self.fail(Why::NotRead),
        }
    }

    /// The whole number written in `name`.
    pub fn number(&mut self, name: &'static str) -> Option<u32> {
        let written = self
            .value(name)?
            .as_u64()
            .and_then(|n| u32::try_from(n).ok());
        match written {
            Some(number) => Some(number),
            None => self.fail(Why::NotRead),
        }
    }

    /// The shown `K` the field points at. Unresolved: `Why::Unplaced`.
    pub fn point<K: Kind>(&mut self, name: &'static str) -> Option<Picked<K>> {
        let written = self.string(name)?;
        self.resolved(written)
    }

    /// The shown `K` the field points at, or `Some(None)` for `"none"`.
    pub fn point_or_none<K: Kind>(&mut self, name: &'static str) -> Option<Option<Picked<K>>> {
        let written = self.string(name)?;
        if written == "none" {
            return Some(None);
        }
        self.resolved(written).map(Some)
    }

    /// The own thing of the entry's item the field points at. A label the
    /// item does not have names nothing: `Why::Unplaced`.
    pub fn own<C: Kind>(&mut self, name: &'static str) -> Option<Picked<C>> {
        let item = self.item()?;
        let written = self.string(name)?;
        self.resolved_own(item, written)
    }

    pub fn own_or_none<C: Kind>(&mut self, name: &'static str) -> Option<Option<Picked<C>>> {
        let item = self.item()?;
        let written = self.string(name)?;
        if written == "none" {
            return Some(None);
        }
        self.resolved_own(item, written).map(Some)
    }

    /// The cites that resolve, each once. One that does not is not a cite: it
    /// is counted for the trace and the statement stands with the cites it has left.
    pub fn cite<K: Kind>(&mut self, name: &'static str) -> Vec<Picked<K>> {
        let showing = self.showing;
        self.cited(name, |written| showing.resolve::<K>(written))
    }

    pub fn cite_own<C: Kind>(&mut self, name: &'static str) -> Vec<Picked<C>> {
        let Some(item) = self.item() else {
            return Vec::new();
        };
        let showing = self.showing;
        self.cited(name, |written| showing.resolve_under::<C>(item, written))
    }

    pub fn group<V: Authored>(&mut self, name: &'static str) -> Option<V> {
        let Some(Value::Object(object)) = self.value(name) else {
            return self.fail(Why::NotRead);
        };
        self.nested(object)
    }

    /// The new things of a nested list. Nested in a statement, the list
    /// settles with it: one item that cannot be read leaves the statement unanswered.
    pub fn list<V: Authored>(&mut self, name: &'static str) -> Option<Vec<V>> {
        let Some(Value::Array(elements)) = self.value(name) else {
            return self.fail(Why::NotRead);
        };
        let mut read = Vec::with_capacity(elements.len());
        for element in elements {
            let Some(object) = element.as_object() else {
                return self.fail(Why::NotRead);
            };
            read.push(self.nested::<V>(object)?);
        }
        Some(read)
    }

    /// One value for each own thing of the entry's item, in the order shown.
    /// Lacking one of them, or holding entries that differ for one, the
    /// statement is unanswered.
    pub fn each_own<C: Kind, V: Authored + PartialEq>(
        &mut self,
        name: &'static str,
    ) -> Option<Vec<(Picked<C>, V)>> {
        let item = self.item()?;
        let Some(Value::Array(entries)) = self.value(name) else {
            return self.fail(Why::NotRead);
        };
        let mut read: Vec<(Picked<C>, V)> = Vec::new();
        for entry in entries {
            let Some(object) = entry.as_object() else {
                return self.fail(Why::NotRead);
            };
            let Some(written) = object.get("of").and_then(Value::as_str) else {
                return self.fail(Why::NotRead);
            };
            let Some(own) = self.showing.resolve_under::<C>(item, written) else {
                trace("unplaced-entry", Some(written));
                continue;
            };
            let value = self.nested::<V>(object)?;
            match read.iter().find(|(held, _)| *held == own) {
                Some((_, first)) if *first != value => {
                    let why =
                        Why::Contradicted(format!("{} has two entries that differ.", own.handle));
                    return self.fail(why);
                }
                Some(_) => {}
                None => read.push((own, value)),
            }
        }
        let mut ordered = Vec::with_capacity(read.len());
        for shown in self.showing.shown_under::<C>(item) {
            match read.iter().position(|(held, _)| *held == shown) {
                Some(at) => ordered.push(read.swap_remove(at)),
                None => return self.fail(Why::Missing),
            }
        }
        Some(ordered)
    }

    /// A nested change, read whole and checked for consistency.
    pub fn change<K: Kind, W: Content>(&mut self, name: &'static str) -> Option<Consistent<K, W>> {
        let Some(Value::Object(object)) = self.value(name) else {
            return self.fail(Why::NotRead);
        };
        self.change_object(object)
    }

    pub fn one_of<U: Union>(&mut self, name: &'static str) -> Option<U> {
        let Some(Value::Object(object)) = self.value(name) else {
            return self.fail(Why::NotRead);
        };
        let Some(kind) = object.get("kind").and_then(Value::as_str) else {
            return self.fail(Why::NotRead);
        };
        if !U::forms().iter().any(|(tag, _)| *tag == kind) {
            return self.fail(Why::NotRead);
        }
        let mut inner = Fields::of(object, self.showing, self.item);
        let read = U::read(kind, &mut inner);
        self.absorb(&inner, read)
    }

    /// The change an object holds: its four lists, each pointer resolved, then
    /// checked by [`consistent`].
    pub(super) fn change_object<K: Kind, W: Content>(
        &mut self,
        object: &Map<String, Value>,
    ) -> Option<Consistent<K, W>> {
        let mut change = Change {
            replace: Vec::new(),
            insert: Vec::new(),
            remove: Vec::new(),
            moved: Vec::new(),
        };
        for entry in self.entries(object, "replace")? {
            let at = self.shown_at::<K>(entry, "at")?;
            let with = self.content::<W>(entry)?;
            change.replace.push((at, with));
        }
        for entry in self.entries(object, "insert")? {
            let after = self.anchor::<K>(entry, "after")?;
            let with = self.content::<W>(entry)?;
            change.insert.push((after, with));
        }
        for written in self.strings(object, "remove")? {
            change.remove.push(self.resolved::<K>(written)?);
        }
        for entry in self.entries(object, "move")? {
            let at = self.shown_at::<K>(entry, "at")?;
            let after = self.anchor::<K>(entry, "after")?;
            change.moved.push((at, after));
        }
        match consistent(change) {
            Ok(change) => Some(change),
            Err(contradiction) => self.fail(Why::Contradicted(contradiction.told())),
        }
    }

    /// Records why the statement is unanswered, unless a reason is already recorded.
    fn fail<T>(&mut self, why: Why) -> Option<T> {
        if self.why.is_none() {
            self.why = Some(why);
        }
        None
    }

    fn value(&self, name: &str) -> Option<&'r Value> {
        let object = self.object;
        object.get(name)
    }

    fn string(&mut self, name: &str) -> Option<&'r str> {
        match self.value(name).and_then(Value::as_str) {
            Some(written) => Some(written),
            None => self.fail(Why::NotRead),
        }
    }

    fn item(&mut self) -> Option<&'r Name> {
        match self.item {
            Some(item) => Some(item),
            None => self.fail(Why::NotRead),
        }
    }

    fn resolved<K: Kind>(&mut self, written: &str) -> Option<Picked<K>> {
        match self.showing.resolve::<K>(written) {
            Some(picked) => Some(picked),
            None => self.fail(Why::Unplaced),
        }
    }

    fn resolved_own<C: Kind>(&mut self, item: &Name, written: &str) -> Option<Picked<C>> {
        match self.showing.resolve_under::<C>(item, written) {
            Some(picked) => Some(picked),
            None => self.fail(Why::Unplaced),
        }
    }

    /// The handles of an evidence list that resolve, each once.
    fn cited<K: Kind>(
        &mut self,
        name: &str,
        resolve: impl Fn(&str) -> Option<Picked<K>>,
    ) -> Vec<Picked<K>> {
        let Some(Value::Array(written)) = self.value(name) else {
            self.lose(None);
            return Vec::new();
        };
        let mut kept: Vec<Picked<K>> = Vec::new();
        for element in written {
            match element.as_str().and_then(&resolve) {
                Some(picked) => {
                    if !kept.contains(&picked) {
                        kept.push(picked);
                    }
                }
                None => self.lose(element.as_str()),
            }
        }
        kept
    }

    fn lose(&mut self, written: Option<&str>) {
        self.lost_cites = self.lost_cites.saturating_add(1);
        trace("lost-cite", written);
    }

    fn nested<V: Authored>(&mut self, object: &'r Map<String, Value>) -> Option<V> {
        let mut inner = Fields::of(object, self.showing, self.item);
        let read = V::read(&mut inner);
        self.absorb(&inner, read)
    }

    /// New content with fields of its own, read as a statement nested in this one.
    pub(in super::super) fn made<V: Authored>(&mut self, value: &Value) -> Option<V> {
        let Some(object) = value.as_object() else {
            return self.fail(Why::NotRead);
        };
        let mut inner = Fields::of(object, self.showing, self.item);
        let read = V::read(&mut inner);
        self.absorb(&inner, read)
    }

    /// Takes what a nested statement read: its lost cites, and its reason when it failed.
    fn absorb<T>(&mut self, inner: &Fields<'_, '_>, read: Option<T>) -> Option<T> {
        self.lost_cites = self.lost_cites.saturating_add(inner.lost_cites);
        match read {
            Some(value) => Some(value),
            None => self.fail(inner.why()),
        }
    }

    fn entries<'o>(
        &mut self,
        object: &'o Map<String, Value>,
        name: &str,
    ) -> Option<Vec<&'o Map<String, Value>>> {
        let Some(Value::Array(elements)) = object.get(name) else {
            return self.fail(Why::NotRead);
        };
        let mut entries = Vec::with_capacity(elements.len());
        for element in elements {
            let Some(entry) = element.as_object() else {
                return self.fail(Why::NotRead);
            };
            entries.push(entry);
        }
        Some(entries)
    }

    fn strings<'o>(&mut self, object: &'o Map<String, Value>, name: &str) -> Option<Vec<&'o str>> {
        let Some(Value::Array(elements)) = object.get(name) else {
            return self.fail(Why::NotRead);
        };
        let mut strings = Vec::with_capacity(elements.len());
        for element in elements {
            let Some(written) = element.as_str() else {
                return self.fail(Why::NotRead);
            };
            strings.push(written);
        }
        Some(strings)
    }

    fn shown_at<K: Kind>(&mut self, entry: &Map<String, Value>, name: &str) -> Option<Picked<K>> {
        let Some(written) = entry.get(name).and_then(Value::as_str) else {
            return self.fail(Why::NotRead);
        };
        self.resolved(written)
    }

    fn anchor<K: Kind>(&mut self, entry: &Map<String, Value>, name: &str) -> Option<Anchor<K>> {
        let Some(written) = entry.get(name).and_then(Value::as_str) else {
            return self.fail(Why::NotRead);
        };
        if written == "start" {
            return Some(Anchor::Start);
        }
        self.resolved(written).map(Anchor::After)
    }

    fn content<W: Content>(&mut self, entry: &Map<String, Value>) -> Option<W> {
        let Some(with) = entry.get("with") else {
            return self.fail(Why::NotRead);
        };
        match W::read(with, self) {
            Some(content) => Some(content),
            None => self.fail(Why::NotRead),
        }
    }
}

/// One `lattice::answers` event for a pointer or statement that did not count.
/// The written token is named only when it has the form of a handle.
pub(super) fn trace(reason: &'static str, written: Option<&str>) {
    match written.filter(|written| has_handle_form(written)) {
        Some(handle) => tracing::debug!(target: "lattice::answers", reason, handle),
        None => tracing::debug!(target: "lattice::answers", reason),
    }
}

/// Letters, then digits.
fn has_handle_form(written: &str) -> bool {
    match written.find(|c: char| !c.is_ascii_alphabetic()) {
        Some(at) if at > 0 => written
            .get(at..)
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())),
        _ => false,
    }
}
