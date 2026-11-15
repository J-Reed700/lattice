//! Applying edits: one applier. A change is applied whole or not at all, and
//! its result is defined for every set of edits.
//!
//! The edits of a change all address the text as it was shown: handles do not
//! shift, so the edits are a set and not a sequence.
//!
//! 1. A unit is claimed by a replace, a remove or a move. A change that claims
//!    one unit twice is inconsistent.
//! 2. Units that are not moved keep the order they were shown in. A replaced
//!    one is its `with`; a removed one leaves only its place.
//! 3. What is anchored after a unit follows it wherever it ends up: first the
//!    inserts, in written order, then the moved units, in written order, each
//!    followed by what is anchored after it. An insert after a moved unit
//!    travels with it; an insert or a move after a removed unit lands where
//!    that unit was.
//! 4. A moved unit whose anchors lead back to itself has no place: the change
//!    is inconsistent.
//!
//! Changes applied one after another all address the text as it was shown.
//! A later one that claims, or is anchored on, a unit an earlier one claimed
//! is refused; every other pair combines as if written as one change, the
//! earlier one's edits first.

use super::shape::form::Field;
use super::shape::{Authored, Fields, Settled, Why};
use super::shown::{Kind, Picked};

#[cfg(test)]
mod tests;

/// Where an insert or a move lands: at the start, or after a shown thing.
#[derive(Debug, Clone, PartialEq)]
pub enum Anchor<K: Kind> {
    Start,
    After(Picked<K>),
}

/// New content of a unit's or a node's type: what `with` holds.
pub trait Content: Sized {
    #[doc(hidden)]
    fn field() -> Field;
    /// The content from what the model wrote in `with`.
    #[doc(hidden)]
    fn read(value: &serde_json::Value, fields: &mut Fields<'_, '_>) -> Option<Self>;
}

/// New text.
impl Content for String {
    fn field() -> Field {
        Field::Text
    }

    fn read(value: &serde_json::Value, _fields: &mut Fields<'_, '_>) -> Option<Self> {
        value.as_str().map(str::to_owned)
    }
}

/// New content with fields of its own: a lesson of an outline, an item of a quiz.
#[derive(Debug, Clone, PartialEq)]
pub struct Made<V>(pub V);

impl<V: Authored> Content for Made<V> {
    fn field() -> Field {
        Field::Group(V::form())
    }

    fn read(value: &serde_json::Value, fields: &mut Fields<'_, '_>) -> Option<Self> {
        fields.made(value).map(Made)
    }
}

/// The edits a model wrote over the things of kind `K` a request shows, each
/// pointer resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Change<K: Kind, W> {
    pub replace: Vec<(Picked<K>, W)>,
    pub insert: Vec<(Anchor<K>, W)>,
    pub remove: Vec<Picked<K>>,
    pub moved: Vec<(Picked<K>, Anchor<K>)>,
}

impl<K: Kind, W> Change<K, W> {
    /// A change that changes nothing: the model declined to edit.
    pub fn is_empty(&self) -> bool {
        self.replace.is_empty()
            && self.insert.is_empty()
            && self.remove.is_empty()
            && self.moved.is_empty()
    }
}

/// A change that claims no unit twice and moves none onto itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Consistent<K: Kind, W>(Change<K, W>);

impl<K: Kind, W> Consistent<K, W> {
    pub fn change(&self) -> &Change<K, W> {
        &self.0
    }

    /// The same edits, each new content made into another.
    pub fn map<V>(self, mut to: impl FnMut(W) -> V) -> Consistent<K, V> {
        let Change {
            replace,
            insert,
            remove,
            moved,
        } = self.0;
        Consistent(Change {
            replace: replace
                .into_iter()
                .map(|(thing, with)| (thing, to(with)))
                .collect(),
            insert: insert
                .into_iter()
                .map(|(anchor, with)| (anchor, to(with)))
                .collect(),
            remove,
            moved,
        })
    }

    /// One change for each group of the things it names, the groups in the order first named (replace,
    /// insert, remove, move, as written). For a change over the units of several texts shown in one request.
    pub fn split_by<G: PartialEq>(
        self,
        group: impl Fn(&K::Ref) -> Option<G>,
    ) -> std::result::Result<Vec<(G, Consistent<K, W>)>, String> {
        let Change {
            replace,
            insert,
            remove,
            moved,
        } = self.0;
        let after_start = insert
            .iter()
            .map(|(anchor, _)| anchor)
            .chain(moved.iter().map(|(_, anchor)| anchor))
            .any(|anchor| matches!(anchor, Anchor::Start));
        if after_start {
            return Err(
                "start does not say which text is meant: give the sentence the new place follows."
                    .to_owned(),
            );
        }
        for (thing, anchor) in &moved {
            let Some(after) = anchored_on(anchor) else {
                continue;
            };
            if let (Some(of), Some(to)) = (group(&thing.reference), group(&after.reference)) {
                if of != to {
                    return Err(format!(
                        "{} cannot move after {}: they are in different texts.",
                        thing.handle.written(),
                        after.handle.written()
                    ));
                }
            }
        }
        let named = replace
            .iter()
            .map(|(thing, _)| thing)
            .chain(insert.iter().filter_map(|(anchor, _)| anchored_on(anchor)))
            .chain(remove.iter())
            .chain(
                moved
                    .iter()
                    .flat_map(|(thing, anchor)| std::iter::once(thing).chain(anchored_on(anchor))),
            );
        for thing in named {
            if group(&thing.reference).is_none() {
                return Err(format!(
                    "{} cannot be changed here.",
                    thing.handle.written()
                ));
            }
        }
        let mut parts: Vec<(G, Change<K, W>)> = Vec::new();
        for (thing, with) in replace {
            if let Some(part) = part_of(&mut parts, group(&thing.reference)) {
                part.replace.push((thing, with));
            }
        }
        for (anchor, with) in insert {
            let of = anchored_on(&anchor).and_then(|thing| group(&thing.reference));
            if let Some(part) = part_of(&mut parts, of) {
                part.insert.push((anchor, with));
            }
        }
        for thing in remove {
            if let Some(part) = part_of(&mut parts, group(&thing.reference)) {
                part.remove.push(thing);
            }
        }
        for (thing, anchor) in moved {
            if let Some(part) = part_of(&mut parts, group(&thing.reference)) {
                part.moved.push((thing, anchor));
            }
        }
        Ok(parts
            .into_iter()
            .map(|(of, change)| (of, Consistent(change)))
            .collect())
    }
}

/// The change of `group` among `parts`, added empty when it is the first of its group.
fn part_of<K: Kind, W, G: PartialEq>(
    parts: &mut Vec<(G, Change<K, W>)>,
    group: Option<G>,
) -> Option<&mut Change<K, W>> {
    let group = group?;
    let at = match parts.iter().position(|(held, _)| *held == group) {
        Some(at) => at,
        None => {
            parts.push((
                group,
                Change {
                    replace: Vec::new(),
                    insert: Vec::new(),
                    remove: Vec::new(),
                    moved: Vec::new(),
                },
            ));
            parts.len().saturating_sub(1)
        }
    };
    parts.get_mut(at).map(|(_, change)| change)
}

/// Why a change contradicts itself, naming the units for the correction.
#[derive(Debug, Clone, PartialEq)]
pub enum Contradiction<K: Kind> {
    /// Claimed by more than one replace, remove or move.
    ClaimedTwice(Vec<Picked<K>>),
    /// Moved units whose anchors lead back to themselves.
    Cycle(Vec<Picked<K>>),
}

impl<K: Kind> Contradiction<K> {
    /// What the correction says, naming the units by their handles.
    pub fn told(&self) -> String {
        match self {
            Self::ClaimedTwice(things) => {
                let listed = written(things.iter()).join(", ");
                match things.as_slice() {
                    [_] => format!("{listed} is changed more than once."),
                    _ => format!("{listed} are changed more than once."),
                }
            }
            Self::Cycle(things) => match things.as_slice() {
                [only] => format!("{} is moved after itself.", only.handle.written()),
                [first, second] => format!(
                    "{} and {} are each moved after the other.",
                    first.handle.written(),
                    second.handle.written()
                ),
                _ => format!(
                    "{} are moved after one another in a circle.",
                    written(things.iter()).join(", ")
                ),
            },
        }
    }
}

/// The handles of `things`, each once, in the order given.
fn written<'a, K: Kind + 'a>(things: impl Iterator<Item = &'a Picked<K>>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for thing in things {
        let name = thing.handle.written();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The thing an anchor names, when it names one.
fn anchored_on<K: Kind>(anchor: &Anchor<K>) -> Option<&Picked<K>> {
    match anchor {
        Anchor::Start => None,
        Anchor::After(thing) => Some(thing),
    }
}

/// Rules 1 and 4. Called by the reading, so a change that contradicts itself
/// is settled and corrected by `ask` and [`Edited::apply`] only ever receives a consistent one.
pub fn consistent<K: Kind, W>(
    change: Change<K, W>,
) -> std::result::Result<Consistent<K, W>, Contradiction<K>> {
    let claims: Vec<&Picked<K>> = change
        .replace
        .iter()
        .map(|(thing, _)| thing)
        .chain(change.remove.iter())
        .chain(change.moved.iter().map(|(thing, _)| thing))
        .collect();
    let mut twice: Vec<Picked<K>> = Vec::new();
    for (at, claim) in claims.iter().enumerate() {
        let repeated = claims
            .iter()
            .enumerate()
            .any(|(other_at, other)| other_at != at && other.reference == claim.reference);
        if repeated && !twice.iter().any(|seen| seen.reference == claim.reference) {
            twice.push((*claim).clone());
        }
    }
    if !twice.is_empty() {
        return Err(Contradiction::ClaimedTwice(twice));
    }
    let circle = in_a_circle(&change.moved);
    if !circle.is_empty() {
        return Err(Contradiction::Cycle(circle));
    }
    Ok(Consistent(change))
}

/// The moved things whose chain of anchors comes back to themselves, in the order written.
fn in_a_circle<K: Kind>(moved: &[(Picked<K>, Anchor<K>)]) -> Vec<Picked<K>> {
    let mut members = Vec::new();
    for (start, _) in moved {
        let mut at = &start.reference;
        // A chain longer than the number of moved things has entered a circle that does not hold `start`.
        for _ in 0..=moved.len() {
            let Some((_, Anchor::After(anchor))) =
                moved.iter().find(|(thing, _)| thing.reference == *at)
            else {
                break;
            };
            if anchor.reference == start.reference {
                members.push(start.clone());
                break;
            }
            at = &anchor.reference;
        }
    }
    members
}

/// The things a request showed and every change applied to them since: text
/// in units, or a feature's tree (an outline's lessons, a quiz's items).
///
/// Every applied change addresses the things as they were shown, so the
/// result is rules 2 and 3 over all of them together, the changes in the
/// order they were applied. Nothing is recomputed from an earlier result:
/// what an earlier change anchored after a thing still follows it when a
/// later change moves that thing.
#[derive(Debug, Clone)]
pub struct Edited<K: Kind, W> {
    shown: Vec<K::Ref>,
    replace: Vec<(K::Ref, W)>,
    /// New content and the shown thing it follows; `None` is the start.
    insert: Vec<(Option<K::Ref>, W)>,
    remove: Vec<K::Ref>,
    /// A moved thing and the shown thing it follows; `None` is the start.
    moved: Vec<(K::Ref, Option<K::Ref>)>,
}

/// One place of the result.
#[derive(Debug, Clone, PartialEq)]
pub struct Place<'a, R, W> {
    /// The shown thing this place stands at or follows, by its position in
    /// the order shown; `None` before the first. A thing in its own place
    /// stands at itself; what is anchored after a thing, and what follows
    /// that, stands at the thing the chain starts from.
    pub at: Option<usize>,
    pub laid: Laid<'a, R, W>,
}

/// What a place holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Laid<'a, R, W> {
    /// A shown thing as it was: in its own place, or moved here.
    Kept(&'a R),
    /// New content, and the shown thing it replaces when it replaces one.
    Wrote {
        content: &'a W,
        replaces: Option<&'a R>,
    },
}

/// What a change did.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied<K: Kind> {
    /// The shown things the change claimed: replaced, removed, moved, as written.
    pub claimed: Vec<Picked<K>>,
    /// How many places hold content the change wrote.
    pub wrote: usize,
}

impl<K: Kind, W> Edited<K, W> {
    /// The things a request showed, in the order shown, with no change applied.
    pub fn of(shown: Vec<K::Ref>) -> Self {
        Self {
            shown,
            replace: Vec::new(),
            insert: Vec::new(),
            remove: Vec::new(),
            moved: Vec::new(),
        }
    }

    /// Whether any change has been applied.
    pub fn changed(&self) -> bool {
        !(self.replace.is_empty()
            && self.insert.is_empty()
            && self.remove.is_empty()
            && self.moved.is_empty())
    }

    /// Whether an applied change replaced, removed or moved `thing`, or anchored anything after it.
    pub fn touches(&self, thing: &K::Ref) -> bool {
        self.claims(thing)
            || self
                .insert
                .iter()
                .any(|(anchor, _)| anchor.as_ref() == Some(thing))
            || self
                .moved
                .iter()
                .any(|(_, anchor)| anchor.as_ref() == Some(thing))
    }

    /// Whether an applied change replaced, removed or moved `thing`.
    fn claims(&self, thing: &K::Ref) -> bool {
        self.replace.iter().any(|(claimed, _)| claimed == thing)
            || self.remove.contains(thing)
            || self.moved.iter().any(|(claimed, _)| claimed == thing)
    }

    /// Applies `change` whole or not at all. The one thing it refuses is a
    /// change that claims, or is anchored on, a thing an earlier change
    /// claimed: `Unanswered(Why::Contradicted)`, and nothing is changed. Its
    /// author saw that thing as it was shown, and it is no longer that.
    pub fn apply(&mut self, change: Consistent<K, W>) -> Settled<Applied<K>> {
        let Change {
            replace,
            insert,
            remove,
            moved,
        } = change.0;
        let claimed: Vec<Picked<K>> = replace
            .iter()
            .map(|(thing, _)| thing.clone())
            .chain(remove.iter().cloned())
            .chain(moved.iter().map(|(thing, _)| thing.clone()))
            .collect();
        let touched = || {
            claimed
                .iter()
                .chain(insert.iter().filter_map(|(anchor, _)| anchored_on(anchor)))
                .chain(moved.iter().filter_map(|(_, anchor)| anchored_on(anchor)))
        };
        let blocked = written(touched().filter(|thing| self.claims(&thing.reference)));
        match blocked.as_slice() {
            [] => {}
            [only] => return refusal(format!("{only} was already changed by an earlier edit.")),
            _ => {
                return refusal(format!(
                    "{} were already changed by earlier edits.",
                    blocked.join(", ")
                ))
            }
        }
        // A pointer that resolved in the request and names nothing this target shows.
        let elsewhere = written(touched().filter(|thing| !self.shown.contains(&thing.reference)));
        if !elsewhere.is_empty() {
            return refusal(format!("{} cannot be placed.", elsewhere.join(", ")));
        }

        let wrote = replace.len().saturating_add(insert.len());
        self.replace.extend(
            replace
                .into_iter()
                .map(|(thing, content)| (thing.reference, content)),
        );
        self.insert.extend(
            insert
                .into_iter()
                .map(|(anchor, content)| (followed(anchor), content)),
        );
        self.remove
            .extend(remove.into_iter().map(|thing| thing.reference));
        self.moved.extend(
            moved
                .into_iter()
                .map(|(thing, anchor)| (thing.reference, followed(anchor))),
        );
        Settled::Answered(Applied { claimed, wrote })
    }

    /// The result, in order: rules 2 and 3 over every change applied.
    pub fn places(&self) -> Vec<Place<'_, K::Ref, W>> {
        let mut places = Vec::new();
        self.attach(None, None, &mut places);
        for (index, thing) in self.shown.iter().enumerate() {
            if self.moved.iter().any(|(moved, _)| moved == thing) {
                continue;
            }
            if !self.remove.contains(thing) {
                let laid = match self.replace.iter().find(|(replaced, _)| replaced == thing) {
                    Some((_, content)) => Laid::Wrote {
                        content,
                        replaces: Some(thing),
                    },
                    None => Laid::Kept(thing),
                };
                places.push(Place {
                    at: Some(index),
                    laid,
                });
            }
            self.attach(Some(thing), Some(index), &mut places);
        }
        places
    }

    /// Rule 3: what is anchored after `to` (`None` is the start), standing at
    /// `at`. No change holds a circle (rule 4) and none is anchored on a thing
    /// an earlier change moved, so no chain of anchors returns to where it
    /// began and the walk ends.
    fn attach<'a>(
        &'a self,
        to: Option<&K::Ref>,
        at: Option<usize>,
        places: &mut Vec<Place<'a, K::Ref, W>>,
    ) {
        for (anchor, content) in &self.insert {
            if anchor.as_ref() == to {
                places.push(Place {
                    at,
                    laid: Laid::Wrote {
                        content,
                        replaces: None,
                    },
                });
            }
        }
        for (thing, anchor) in &self.moved {
            if anchor.as_ref() == to {
                places.push(Place {
                    at,
                    laid: Laid::Kept(thing),
                });
                self.attach(Some(thing), at, places);
            }
        }
    }
}

/// The shown thing an anchor names; `None` is the start.
fn followed<K: Kind>(anchor: Anchor<K>) -> Option<K::Ref> {
    match anchor {
        Anchor::Start => None,
        Anchor::After(thing) => Some(thing.reference),
    }
}

fn refusal<V>(text: String) -> Settled<V> {
    Settled::Unanswered(Why::Contradicted(text))
}
