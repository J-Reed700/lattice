//! The names a lesson's sections and questions keep wherever they move.
use super::{part_sha256, FindingSite, ReviewFinding};
use lattice_core::domain::learning::claim_review::text_sha256;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Hex characters of a unit's id: 64 bits, enough that no two units of one lesson meet and short enough to read in a log.
const UNIT_ID_LENGTH: usize = 16;

/// The name of one section or question, kept by the application: it follows
/// the part wherever it moves in the lesson.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitId(String);

impl UnitId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The id a part with `digest` is born with, the `occurrence`th time that digest is taken.
    fn born_from(digest: &str, occurrence: usize) -> Self {
        let text = if occurrence == 0 {
            digest.to_owned()
        } else {
            text_sha256(&format!("{digest}#{}", occurrence + 1))
        };
        Self(text.chars().take(UNIT_ID_LENGTH).collect())
    }

    /// A unit id read back from text; none when the text is not one.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let hex = text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        (text.len() == UNIT_ID_LENGTH && hex).then(|| Self(text.to_owned()))
    }
}

/// Every section and question of `candidate` with its site, sections first.
fn parts(candidate: &Value) -> Vec<(FindingSite, &Value)> {
    let list = |field: &str| {
        candidate
            .get(field)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    let sections = list("blocks")
        .iter()
        .enumerate()
        .map(|(index, part)| (FindingSite::Section(index), part));
    let questions = list("questions")
        .iter()
        .enumerate()
        .map(|(index, part)| (FindingSite::Question(index), part));
    sections.chain(questions).collect()
}

/// Which unit id each section and question has: the digest of a part's text
/// (`part_sha256`) leads to the ids of the parts that had it. A part keeps its
/// id wherever it moves; an edit made in place hands it on (`carry`), and a
/// part a rewrite changes is a new part (`carry_rewrite`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Lineage {
    units: BTreeMap<String, Vec<UnitId>>,
}

/// What a rewrite of the whole lesson left: the ids its parts have, and those
/// of them no part of the lesson before it had.
pub(crate) struct Rewritten {
    pub(crate) standing: BTreeSet<UnitId>,
    pub(crate) created: BTreeSet<UnitId>,
}

impl Lineage {
    fn knows(&self, id: &UnitId) -> bool {
        self.units.values().any(|ids| ids.contains(id))
    }

    /// The first id of `digest` that `used` does not hold.
    fn known(&self, digest: &str, used: &BTreeSet<UnitId>) -> Option<UnitId> {
        self.units
            .get(digest)?
            .iter()
            .find(|id| !used.contains(*id))
            .cloned()
    }

    /// An id no part has had and `used` does not hold, derived from `digest`.
    fn born(&self, digest: &str, used: &BTreeSet<UnitId>) -> UnitId {
        (0usize..)
            .map(|occurrence| UnitId::born_from(digest, occurrence))
            .find(|id| !self.knows(id) && !used.contains(id))
            .unwrap_or_else(|| UnitId::born_from(digest, 0))
    }

    /// The id of every section and question of `candidate`, born where it has none.
    fn resolve(&mut self, candidate: &Value) -> BTreeMap<FindingSite, UnitId> {
        let mut used = BTreeSet::new();
        let mut placed = BTreeMap::new();
        for (site, part) in parts(candidate) {
            let digest = part_sha256(part);
            let id = match self.known(&digest, &used) {
                Some(id) => id,
                None => {
                    let id = self.born(&digest, &used);
                    self.units.entry(digest).or_default().push(id.clone());
                    id
                }
            };
            used.insert(id.clone());
            placed.insert(site, id);
        }
        placed
    }

    /// Records that the parts of `after` at the sites of `chosen` now have those ids.
    fn assign(&mut self, after: &Value, chosen: &BTreeMap<FindingSite, UnitId>) {
        let taken: BTreeSet<&UnitId> = chosen.values().collect();
        for ids in self.units.values_mut() {
            ids.retain(|id| !taken.contains(id));
        }
        let mut occurrences: BTreeMap<String, usize> = BTreeMap::new();
        for (site, part) in parts(after) {
            let Some(id) = chosen.get(&site) else {
                continue;
            };
            let digest = part_sha256(part);
            let rank = occurrences.entry(digest.clone()).or_default();
            let ids = self.units.entry(digest).or_default();
            ids.insert((*rank).min(ids.len()), id.clone());
            *rank += 1;
        }
        self.units.retain(|_, ids| !ids.is_empty());
    }

    /// Sets the unit of every finding sited at a section or question of `candidate`.
    pub(crate) fn place(&mut self, candidate: &Value, findings: &mut [ReviewFinding]) {
        let placed = self.resolve(candidate);
        for finding in findings {
            finding.unit = match finding.site {
                Some(site @ (FindingSite::Section(_) | FindingSite::Question(_))) => {
                    placed.get(&site).cloned()
                }
                _ => None,
            };
        }
    }

    /// A repair that edited parts in place: the part at each position after it
    /// is the part that stood there before. One that added or removed a part
    /// is carried as a rewrite.
    pub(crate) fn carry(&mut self, before: &Value, after: &Value) {
        let sites = |candidate: &Value| -> Vec<FindingSite> {
            parts(candidate).into_iter().map(|(site, _)| site).collect()
        };
        if sites(before) != sites(after) {
            self.carry_rewrite(before, after);
            return;
        }
        let ids = self.resolve(before);
        self.assign(after, &ids);
    }

    /// A rewrite of the whole lesson: a part of `after` whose text the lesson
    /// has had keeps that text's id wherever it now stands; any other part is
    /// new.
    pub(crate) fn carry_rewrite(&mut self, before: &Value, after: &Value) -> Rewritten {
        let had: BTreeSet<UnitId> = self.resolve(before).into_values().collect();
        let mut chosen: BTreeMap<FindingSite, UnitId> = BTreeMap::new();
        let mut used = BTreeSet::new();
        for (site, part) in parts(after) {
            let digest = part_sha256(part);
            let id = self
                .known(&digest, &used)
                .unwrap_or_else(|| self.born(&digest, &used));
            used.insert(id.clone());
            chosen.insert(site, id);
        }
        self.assign(after, &chosen);
        Rewritten {
            created: used.difference(&had).cloned().collect(),
            standing: used,
        }
    }

    /// Takes up that the part with `digest` has `id`, unless `id` is already known.
    pub(crate) fn seed(&mut self, digest: &str, id: UnitId) {
        if !self.knows(&id) {
            self.units.entry(digest.to_owned()).or_default().push(id);
        }
    }

    /// Takes up every id of `other` that this lineage does not know.
    pub(crate) fn absorb(&mut self, other: &Self) {
        for (digest, ids) in &other.units {
            for id in ids {
                self.seed(digest, id.clone());
            }
        }
    }
}
