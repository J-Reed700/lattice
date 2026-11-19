//! The fields of one statement, as a schema writes them.

use super::super::edit::Content;
use super::super::shown::Kind;
use super::{Authored, Union, Vocabulary};

/// The fields the model writes in one statement, in order. Each is one of the
/// four kinds an answer may hold (judgement, new text, pointer, structure).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Form {
    pub(in super::super) fields: Vec<(&'static str, Field)>,
}

/// The name of a list whose length the request sets. Equal by name.
#[derive(Debug, PartialEq, Eq)]
pub struct Count(pub &'static str);

/// Where a pointer's valid set comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Among {
    /// Every thing of the kind the request shows.
    Shown,
    /// The own things of the entry's item, labelled within it.
    Own,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    Text,
    Word(Vec<&'static str>),
    /// A quantity the model judges, such as minutes. Never a position, an id or a count.
    Number,
    /// A placing pointer. `none` adds the member `"none"`.
    Point {
        prefix: &'static str,
        among: Among,
        none: bool,
    },
    /// Evidence pointers: a list of handles.
    Cite {
        prefix: &'static str,
        among: Among,
        /// Whether at least one must be given whenever the request shows any.
        some: bool,
    },
    /// Every shown thing of a kind, each once, in the order the model judges.
    Order {
        prefix: &'static str,
    },
    /// A closed object of fixed fields.
    Group(Form),
    /// A list of new things; `count` names the length its request sets.
    List {
        form: Form,
        count: Option<&'static Count>,
    },
    /// One entry per own thing of the item: `[{ "of": "o2", …fields }]`.
    EachOwn {
        prefix: &'static str,
        form: Form,
    },
    /// A change over the shown things of a kind; `with` is the content's field.
    Change {
        prefix: &'static str,
        with: Box<Field>,
        /// Whether an insert or a move may anchor at `"start"`.
        start: bool,
    },
    /// One of several forms, told apart by `kind`.
    OneOf(Vec<(&'static str, Form)>),
}

impl Form {
    pub fn new() -> Self {
        Self::default()
    }

    fn with(mut self, name: &'static str, field: Field) -> Self {
        self.fields.push((name, field));
        self
    }

    /// Text the model authors.
    pub fn text(self, name: &'static str) -> Self {
        self.with(name, Field::Text)
    }

    /// A judgement from a fixed vocabulary.
    pub fn word<W: Vocabulary>(self, name: &'static str) -> Self {
        self.with(
            name,
            Field::Word(W::WORDS.iter().map(|(word, _)| *word).collect()),
        )
    }

    /// A whole number the model judges.
    pub fn number(self, name: &'static str) -> Self {
        self.with(name, Field::Number)
    }

    /// A placing pointer at one of the things of kind `K` the request shows.
    pub fn point<K: Kind>(self, name: &'static str) -> Self {
        self.with(name, point::<K>(Among::Shown, false))
    }

    /// A placing pointer at a shown `K`, or `"none"`.
    pub fn point_or_none<K: Kind>(self, name: &'static str) -> Self {
        self.with(name, point::<K>(Among::Shown, true))
    }

    /// A placing pointer at one of the own things of the entry's item.
    pub fn own<C: Kind>(self, name: &'static str) -> Self {
        self.with(name, point::<C>(Among::Own, false))
    }

    /// A placing pointer at one of the item's own things, or `"none"`.
    pub fn own_or_none<C: Kind>(self, name: &'static str) -> Self {
        self.with(name, point::<C>(Among::Own, true))
    }

    /// Evidence: the shown things of kind `K` that support the text beside it.
    pub fn cite<K: Kind>(self, name: &'static str) -> Self {
        self.with(
            name,
            Field::Cite {
                prefix: K::PREFIX,
                among: Among::Shown,
                some: false,
            },
        )
    }

    /// Evidence that must be given: at least one shown `K` whenever the request shows any.
    pub fn cite_some<K: Kind>(self, name: &'static str) -> Self {
        self.with(
            name,
            Field::Cite {
                prefix: K::PREFIX,
                among: Among::Shown,
                some: true,
            },
        )
    }

    /// Every shown `K`, each once, in the order the model judges.
    pub fn order<K: Kind>(self, name: &'static str) -> Self {
        self.with(name, Field::Order { prefix: K::PREFIX })
    }

    /// Evidence among the own things of the entry's item.
    pub fn cite_own<C: Kind>(self, name: &'static str) -> Self {
        self.with(
            name,
            Field::Cite {
                prefix: C::PREFIX,
                among: Among::Own,
                some: false,
            },
        )
    }

    /// A fixed set of named fields.
    pub fn group<V: Authored>(self, name: &'static str) -> Self {
        self.with(name, Field::Group(V::form()))
    }

    /// A list of new things, each a `V`.
    pub fn list<V: Authored>(self, name: &'static str) -> Self {
        self.with(
            name,
            Field::List {
                form: V::form(),
                count: None,
            },
        )
    }

    /// A list of new things, each a `V`, whose length the request sets under `count`.
    pub fn counted<V: Authored>(self, name: &'static str, count: &'static Count) -> Self {
        self.with(
            name,
            Field::List {
                form: V::form(),
                count: Some(count),
            },
        )
    }

    /// One entry for each own thing of the entry's item.
    pub fn each_own<C: Kind, V: Authored>(self, name: &'static str) -> Self {
        self.with(
            name,
            Field::EachOwn {
                prefix: C::PREFIX,
                form: V::form(),
            },
        )
    }

    /// A change over the shown things of kind `K`.
    pub fn change<K: Kind, W: Content>(self, name: &'static str) -> Self {
        self.with(
            name,
            Field::Change {
                prefix: K::PREFIX,
                with: Box::new(W::field()),
                start: true,
            },
        )
    }

    /// A change whose inserts and moves always follow a shown thing: for the units of several texts shown in
    /// one request, where `"start"` names no place.
    pub fn change_after<K: Kind, W: Content>(self, name: &'static str) -> Self {
        self.with(
            name,
            Field::Change {
                prefix: K::PREFIX,
                with: Box::new(W::field()),
                start: false,
            },
        )
    }

    /// One of several forms.
    pub fn one_of<U: Union>(self, name: &'static str) -> Self {
        self.with(name, Field::OneOf(U::forms()))
    }
}

fn point<K: Kind>(among: Among, none: bool) -> Field {
    Field::Point {
        prefix: K::PREFIX,
        among,
        none,
    }
}
