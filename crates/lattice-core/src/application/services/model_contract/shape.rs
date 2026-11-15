//! What an answer may look like: each shape supplies its schema and its
//! reading, and nothing else is read from a model.
//!
//! An answer is made of statements, and a statement settles whole. Presence is
//! evidence; absence is evidence only in an answer read to its end. A pointer
//! counts only after it resolved against what its request shows.
//!
//! Every schema is written in one dialect, the subset every backend's
//! translator enforces as written: closed objects, every property required,
//! string `enum`s, lists with no count keyword but `minItems: 1` (except the
//! length a request sets for a list of new things, written only for a backend
//! that holds lists to it), `anyOf` only between closed objects. No optional property, no nullable, no
//! integer, no range, no `$ref`. Absence is the named member `"none"`, never
//! a missing field.

use std::marker::PhantomData;

use serde::Serialize;

use super::edit::{Change, Consistent, Content};
use super::shown::{Kind, Picked, Showing};

mod document;
mod fields;
pub(super) mod form;
mod shapes;

#[cfg(test)]
mod tests;

pub use fields::{Fields, Place};
pub use form::{Count, Form};

/// The answer schema a request carries. Made by a shape, by a tool's
/// parameters and, until every call has moved, by `legacy::schema`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Schema(serde_json::Value);

impl Schema {
    pub(super) fn made(json: serde_json::Value) -> Self {
        Self(json)
    }

    /// The schema as a provider's translator reads it.
    pub fn as_json(&self) -> &serde_json::Value {
        &self.0
    }

    pub(super) fn into_json(self) -> serde_json::Value {
        self.0
    }
}

/// How one statement ended. It has no default: a caller must decide what an
/// unanswered one means.
#[derive(Debug, Clone, PartialEq)]
pub enum Settled<V> {
    Answered(V),
    Unanswered(Why),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// The answer could not be read at all.
    NotRead,
    /// The answer ended before this statement closed.
    CutOff,
    /// A complete answer said nothing about this item.
    Missing,
    /// A placing pointer in the statement did not resolve.
    Unplaced,
    /// Differing entries for one item; an order that is none; a change that
    /// is inconsistent or meets an earlier one. Holds what contradicts, in
    /// words a correction tells the model.
    Contradicted(String),
    /// The model declined to answer.
    Declined,
    /// What the model authored was refused for this reason: by the contract's
    /// `accept`, or by a field that must hold something and holds nothing.
    Refused(String),
    /// The item, with what it needs, does not fit the model's window: it was not asked.
    NoRoom,
}

/// How the text of an answer ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Complete,
    CutOff,
    Declined,
}

/// The text a model returned and how it ended.
#[derive(Debug, Clone, Copy)]
pub struct Written<'a> {
    pub text: &'a str,
    pub ended: Ended,
}

/// What the backend holds an answer to, so how a schema is written: pointers
/// as the `enum` of their valid handles on a backend that enforces one of that
/// size, a plain string otherwise; and, where it holds lists to a length, the
/// length a request sets. The reading is the same in all three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// Enumerated pointers and the lengths a request sets.
    Counts,
    /// Enumerated pointers.
    Enums,
    Plain,
}

/// The `items` list of a `New` answer.
pub const STATEMENTS: Count = Count("statements");

/// A fixed vocabulary of a judgement: the words a model may write and what each means.
pub trait Vocabulary: Sized + Copy + 'static {
    const WORDS: &'static [(&'static str, Self)];
}

/// What the model authors in one statement: fixed named fields of judgement,
/// new text, pointers, and nested shapes.
pub trait Authored: Sized {
    /// The fields, in the order the model writes them.
    fn form() -> Form;
    /// The value from its fields, pointers resolved. `None` when a field could
    /// not be used; `fields` holds why, and the statement is unanswered.
    fn read(fields: &mut Fields<'_, '_>) -> Option<Self>;
}

/// An answer that is one of several forms, told apart by the tag the model
/// writes in `kind`. Choosing the form is the model's judgement.
pub trait Union: Sized {
    /// Each form's tag and fields.
    fn forms() -> Vec<(&'static str, Form)>;
    /// The value of the form tagged `kind`.
    fn read(kind: &str, fields: &mut Fields<'_, '_>) -> Option<Self>;
}

/// A shape: the only definition of a call's schema and reading.
pub trait Shape {
    /// What settles whole, and what a contract's `accept` speaks about.
    type Statement;

    /// The schema of an answer to a request that shows `showing`. `None` for a text form.
    fn schema(showing: &Showing<'_>, pointers: Held) -> Option<Schema>;
}

/// A shape whose answer is one statement about the call's one subject.
pub trait Single: Shape {
    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Settled<Self::Statement>;
}

/// A shape whose answer is a list of new things, each a statement of its own.
pub trait Listed: Shape {
    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Found<Self::Statement>;
}

/// What an answer said about each item a request shows.
pub type Rows<K, S> = Vec<(Picked<K>, Settled<S>)>;

/// A shape whose answer speaks about the items the request shows, one by one.
pub trait PerItem: Shape {
    type Item: Kind;

    /// Every item of kind `Item` the request shows, exactly once, in the order shown.
    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Rows<Self::Item, Self::Statement>;
}

/// The new things an answer held.
#[derive(Debug, Clone, PartialEq)]
pub struct Found<S> {
    /// Every item that closed and could be read, in written order. Each stands alone.
    pub items: Vec<S>,
    /// `None` when the answer was read to its end: only then is it known that
    /// there are no others. Otherwise why it is not known.
    pub rest: Option<Why>,
    /// How many items the model wrote that are not in `items`, whatever the cause.
    pub left_out: usize,
    /// Why the first item that could not be read was not, when it had a reason of its own.
    pub unread_why: Option<Why>,
}

/// `V`: judgement and text about the one subject of the call. No identity exists to get wrong.
pub struct Value<V>(PhantomData<V>);

/// One handle, or `"none"`. Written `{ "pick": "s4" }`.
pub struct Pick<K>(PhantomData<K>);

/// A list of handles, one judgement: cut off before it closed, or holding a
/// handle that does not resolve, it is unanswered. A handle written twice
/// counts once. Written `{ "picks": ["s4", "s7"] }`.
pub struct PickSome<K>(PhantomData<K>);

/// A list of new things, each a `V`. Written `{ "items": [V, …] }`. The
/// number asked for is a request, never a rule of the reading.
pub struct New<V>(PhantomData<V>);

/// One entry per item. Written `{ "entries": [{ "of": "c2", …V }] }`. Equal
/// entries for one item count once; entries that differ are no answer; an item
/// with no entry is unanswered.
pub struct Each<K, V>(PhantomData<(K, V)>);

/// Only the marked items. Written `{ "marks": [{ "at": "f4", …V }] }`. An
/// unmarked item is unmarked only in an answer read to its end.
pub struct Flagged<K, V>(PhantomData<(K, V)>);

/// A verdict word first, then free text, then exactly one pointer line
/// (`none` is a member). A text form: its verdict's confidence is read from
/// the probabilities of the first token.
pub struct Lead<W, K>(PhantomData<(W, K)>);

/// What a [`Lead`] answer said.
#[derive(Debug, Clone, PartialEq)]
pub struct Led<W, K: Kind> {
    pub verdict: W,
    pub text: String,
    pub pointer: Option<Picked<K>>,
}

impl<V: Authored> Shape for Value<V> {
    type Statement = V;

    fn schema(showing: &Showing<'_>, pointers: Held) -> Option<Schema> {
        shapes::value_schema::<V>(showing, pointers)
    }
}

impl<V: Authored> Single for Value<V> {
    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Settled<V> {
        shapes::read_value(written, showing)
    }
}

impl<K: Kind> Shape for Pick<K> {
    type Statement = Option<Picked<K>>;

    fn schema(showing: &Showing<'_>, pointers: Held) -> Option<Schema> {
        shapes::pick_schema::<K>(showing, pointers)
    }
}

impl<K: Kind> Single for Pick<K> {
    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Settled<Option<Picked<K>>> {
        shapes::read_pick(written, showing)
    }
}

impl<K: Kind> Shape for PickSome<K> {
    type Statement = Vec<Picked<K>>;

    fn schema(showing: &Showing<'_>, pointers: Held) -> Option<Schema> {
        shapes::pick_some_schema::<K>(showing, pointers)
    }
}

impl<K: Kind> Single for PickSome<K> {
    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Settled<Vec<Picked<K>>> {
        shapes::read_pick_some(written, showing)
    }
}

impl<V: Authored> Shape for New<V> {
    type Statement = V;

    fn schema(showing: &Showing<'_>, pointers: Held) -> Option<Schema> {
        shapes::new_schema::<V>(showing, pointers)
    }
}

impl<V: Authored> Listed for New<V> {
    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Found<V> {
        shapes::read_new(written, showing)
    }
}

impl<K: Kind, V: Authored + PartialEq> Shape for Each<K, V> {
    type Statement = V;

    fn schema(showing: &Showing<'_>, pointers: Held) -> Option<Schema> {
        shapes::each_schema::<K, V>(showing, pointers)
    }
}

impl<K: Kind, V: Authored + PartialEq> PerItem for Each<K, V> {
    type Item = K;

    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Rows<K, V> {
        shapes::read_each(written, showing)
    }
}

impl<K: Kind, V: Authored + PartialEq> Shape for Flagged<K, V> {
    /// The mark, or `None` for an item a complete answer left unmarked.
    type Statement = Option<V>;

    fn schema(showing: &Showing<'_>, pointers: Held) -> Option<Schema> {
        shapes::flagged_schema::<K, V>(showing, pointers)
    }
}

impl<K: Kind, V: Authored + PartialEq> PerItem for Flagged<K, V> {
    type Item = K;

    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Rows<K, Option<V>> {
        shapes::read_flagged(written, showing)
    }
}

/// A change over the units or nodes the request shows. Written `{ "replace":
/// [{ "at", "with" }], "insert": [{ "after", "with" }], "remove": ["u3"],
/// "move": [{ "at", "after" }] }`; all four lists are required and may be
/// empty, and an anchor is a shown thing or `"start"`. One statement: it is
/// read whole and checked by [`super::edit::consistent`], or it is unanswered.
impl<K: Kind, W: Content> Shape for Change<K, W> {
    type Statement = Consistent<K, W>;

    fn schema(showing: &Showing<'_>, pointers: Held) -> Option<Schema> {
        shapes::change_schema::<K, W>(showing, pointers)
    }
}

impl<K: Kind, W: Content> Single for Change<K, W> {
    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Settled<Consistent<K, W>> {
        shapes::read_change(written, showing)
    }
}

impl<W: Vocabulary, K: Kind> Shape for Lead<W, K> {
    type Statement = Led<W, K>;

    fn schema(_showing: &Showing<'_>, _pointers: Held) -> Option<Schema> {
        None
    }
}

impl<W: Vocabulary, K: Kind> Single for Lead<W, K> {
    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Settled<Led<W, K>> {
        shapes::read_lead(written, showing)
    }
}
