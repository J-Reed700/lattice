//! Independence (§7): corroboration counts origins, not pages. Copies,
//! mirrors, syndication and archives are folded into one origin cluster
//! before any "two independent sources" rule is applied.
//!
//! Copy detection is deterministic: resemblance and containment of exact
//! shingle sets. Embeddings stay out of it, because they score topical
//! similarity, which is exactly what two *independent* sources on one
//! subject share.

use std::collections::{BTreeMap, HashMap, HashSet};

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use unicode_normalization::UnicodeNormalization;

use super::origin::OriginKey;
use super::policy::Policy;

/// Values in a [`Sketch`]: one minimum per hash function.
pub const SKETCH_VALUES: usize = 128;

/// The shingles of one text, hashed. Exact: inside a course every pair is
/// compared on the full sets; the [`Sketch`] is kept for indexes across
/// courses and chat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShingleSet {
    pub(super) hashes: HashSet<u64>,
}

impl ShingleSet {
    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }
}

/// A MinHash sketch of a shingle set: [`SKETCH_VALUES`] minima, stored per
/// version as little-endian `u32`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sketch(pub [u32; SKETCH_VALUES]);

impl Sketch {
    /// The storage form: each value little-endian, in order.
    pub fn to_le_bytes(&self) -> Vec<u8> {
        self.0
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    /// Reads [`Sketch::to_le_bytes`]; `None` unless `bytes` is exactly
    /// [`SKETCH_VALUES`] little-endian `u32`s.
    pub fn from_le_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != SKETCH_VALUES * 4 {
            return None;
        }
        let mut values = [0u32; SKETCH_VALUES];
        for (value, chunk) in values.iter_mut().zip(bytes.chunks_exact(4)) {
            *value = u32::from_le_bytes(chunk.try_into().ok()?);
        }
        Some(Self(values))
    }
}

impl Serialize for Sketch {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter())
    }
}

impl<'de> Deserialize<'de> for Sketch {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<u32>::deserialize(deserializer)?;
        let values: [u32; SKETCH_VALUES] = values.try_into().map_err(|values: Vec<u32>| {
            serde::de::Error::invalid_length(values.len(), &"128 sketch values")
        })?;
        Ok(Self(values))
    }
}

/// An edge of the provenance graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// `from` is a copy of `to` (resemblance over the whole text); the newer
    /// version points at the older.
    CopyOf,
    /// `from` contains a copy of `to`'s text alongside other material.
    ContainsCopyOf,
    /// `from` is an archived copy of `to`.
    ArchiveOf,
    /// `from` names `to` as its canonical form on another domain
    /// (syndication).
    CanonicalOf,
    /// `from` links to `to` from a span of its text.
    Cites,
    /// `from` was cited for a claim and `to` was fetched to check it.
    CitedForClaim,
}

wire_names!(RelationKind {
    CopyOf => "copy_of",
    ContainsCopyOf => "contains_copy_of",
    ArchiveOf => "archive_of",
    CanonicalOf => "canonical_of",
    Cites => "cites",
    CitedForClaim => "cited_for_claim",
});

/// What a relation points at: a captured version, or a URL not captured.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationTarget {
    Version(String),
    Url(String),
}

/// Why a relation was recorded, by kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "evidence", rename_all = "snake_case")]
pub enum RelationEvidence {
    /// `CopyOf` / `ContainsCopyOf`: the measured overlap. `containment` is
    /// the share of `to`'s shingles found in `from`.
    Text { resemblance: f64, containment: f64 },
    /// `ArchiveOf`.
    Archive {
        archive_host: String,
        #[serde(default)]
        archived_at: Option<i64>,
    },
    /// `CanonicalOf`.
    Canonical { canonical_url: String },
    /// `Cites`: the link's bytes in `from`'s text and its anchor.
    Citation {
        start_byte: usize,
        end_byte: usize,
        anchor: String,
    },
    /// `CitedForClaim`: whether the cited page bore the claim out; `None`
    /// with `fetch_failed` when it could not be read.
    ClaimCitation {
        claim_id: String,
        #[serde(default)]
        borne_out: Option<bool>,
        #[serde(default)]
        fetch_failed: Option<String>,
    },
}

/// One edge: `from` (a version id) relates to `to` as `kind` says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceRelation {
    pub from: String,
    pub to: RelationTarget,
    pub kind: RelationKind,
    pub evidence: RelationEvidence,
}

/// An origin cluster's identity: its smallest member version id, so the
/// same supports always give the same id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClusterId(pub String);

/// Versions that count as one origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cluster {
    pub id: ClusterId,
    /// Member version ids, sorted.
    pub members: Vec<String>,
}

/// FNV-1a 64-bit offset basis and prime: a fixed, documented hash, so a
/// shingle hashes the same on every machine and in every release.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Seeds the sketch's hash family (SplitMix64 over this constant): any
/// fixed value works; changing it invalidates stored sketches.
const SKETCH_SEED: u64 = 0x5eed_0f5a_174e_55aa;

/// The shingle set of `text` (§7): markdown link targets and bare URLs
/// removed (anchors kept), Unicode NFKC, lower case, punctuation and
/// markdown syntax read as spaces; then `policy.shingle_words`-word
/// shingles, or `policy.shingle_chars`-character shingles when fewer than
/// `policy.word_break_ratio` of the characters are whitespace (scripts
/// without word breaks). A text shorter than one shingle is one shingle.
pub fn shingles(text: &str, policy: &Policy) -> ShingleSet {
    let text = strip_link_targets(text);
    let normalised: String = text.nfkc().flat_map(char::to_lowercase).collect();
    let visible = normalised.chars().filter(|c| !c.is_control()).count();
    let spaces = normalised.chars().filter(|c| c.is_whitespace()).count();
    let words: Vec<String> = normalised
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    let mut hashes = HashSet::new();
    if visible == 0 || words.is_empty() {
        return ShingleSet { hashes };
    }
    if (spaces as f64) < policy.word_break_ratio * visible as f64 {
        let chars: Vec<char> = words.concat().chars().collect();
        let width = policy.shingle_chars.max(1);
        if chars.len() <= width {
            hashes.insert(fnv1a(&chars.iter().collect::<String>()));
        }
        for window in chars.windows(width) {
            hashes.insert(fnv1a(&window.iter().collect::<String>()));
        }
    } else {
        let width = policy.shingle_words.max(1);
        if words.len() <= width {
            hashes.insert(fnv1a(&words.join(" ")));
        }
        for window in words.windows(width) {
            hashes.insert(fnv1a(&window.join(" ")));
        }
    }
    ShingleSet { hashes }
}

/// Jaccard resemblance `|A ∩ B| / |A ∪ B|`; 0 when both are empty.
pub fn resemblance(a: &ShingleSet, b: &ShingleSet) -> f64 {
    let shared = a.hashes.intersection(&b.hashes).count();
    let union = a.len() + b.len() - shared;
    if union == 0 {
        0.0
    } else {
        shared as f64 / union as f64
    }
}

/// Containment of `small` in `large`: `|S ∩ L| / |S|`; 0 when `small` is
/// empty.
pub fn containment(small: &ShingleSet, large: &ShingleSet) -> f64 {
    if small.is_empty() {
        return 0.0;
    }
    small.hashes.intersection(&large.hashes).count() as f64 / small.len() as f64
}

impl Sketch {
    /// The MinHash sketch of a shingle set: for each of [`SKETCH_VALUES`]
    /// hash functions, the least hash over the set (all `u32::MAX` for an
    /// empty set).
    pub fn of(set: &ShingleSet) -> Self {
        let mut values = [u32::MAX; SKETCH_VALUES];
        let mut seed = SKETCH_SEED;
        for value in values.iter_mut() {
            seed = splitmix64(seed);
            *value = set
                .hashes
                .iter()
                .map(|hash| (splitmix64(hash ^ seed) >> 32) as u32)
                .min()
                .unwrap_or(u32::MAX);
        }
        Self(values)
    }

    /// The estimated resemblance of the two sets: the share of equal
    /// minima.
    pub fn resemblance(&self, other: &Self) -> f64 {
        let equal = self
            .0
            .iter()
            .zip(other.0.iter())
            .filter(|(a, b)| a == b)
            .count();
        equal as f64 / SKETCH_VALUES as f64
    }
}

/// Page-level relations between a newly captured version and a course's
/// active versions (§7): `CopyOf` (the newer points at the older) at
/// resemblance `policy.resemblance_copy`, else `ContainsCopyOf` in
/// whichever direction reaches `policy.containment_copy`.
pub fn page_relations(
    new_id: &str,
    new: &ShingleSet,
    existing: &[(&str, &ShingleSet)],
    policy: &Policy,
) -> Vec<SourceRelation> {
    let mut relations = Vec::new();
    for (id, set) in existing {
        if *id == new_id {
            continue;
        }
        let r = resemblance(new, set);
        let old_in_new = containment(set, new);
        let new_in_old = containment(new, set);
        let text = |containment: f64| RelationEvidence::Text {
            resemblance: r,
            containment,
        };
        if r >= policy.resemblance_copy {
            relations.push(SourceRelation {
                from: new_id.to_owned(),
                to: RelationTarget::Version((*id).to_owned()),
                kind: RelationKind::CopyOf,
                evidence: text(old_in_new),
            });
            continue;
        }
        if old_in_new >= policy.containment_copy {
            relations.push(SourceRelation {
                from: new_id.to_owned(),
                to: RelationTarget::Version((*id).to_owned()),
                kind: RelationKind::ContainsCopyOf,
                evidence: text(old_in_new),
            });
        }
        if new_in_old >= policy.containment_copy {
            relations.push(SourceRelation {
                from: (*id).to_owned(),
                to: RelationTarget::Version(new_id.to_owned()),
                kind: RelationKind::ContainsCopyOf,
                evidence: text(new_in_old),
            });
        }
    }
    relations
}

/// Pairs of a claim's supporting passages that are copies of each other
/// (§7, passage level): a syndicated paragraph inside otherwise different
/// pages merges their clusters for that claim only.
pub fn passage_merges(passages: &[(&str, &str)], policy: &Policy) -> Vec<(String, String)> {
    let sets: Vec<(&str, ShingleSet)> = passages
        .iter()
        .map(|(id, text)| (*id, shingles(text, policy)))
        .collect();
    let mut merges = Vec::new();
    for (index, (a, set_a)) in sets.iter().enumerate() {
        for (b, set_b) in sets.iter().skip(index + 1) {
            if a != b && resemblance(set_a, set_b) >= policy.resemblance_copy {
                merges.push(((*a).to_owned(), (*b).to_owned()));
            }
        }
    }
    merges
}

/// Origin clusters of a claim's supports (§7): union-find over the same
/// origin key, copy, archive and canonical relations in either direction
/// (also through versions that are not supports themselves, so two copies
/// of one page fall together) and passage-level merges. Distrusted
/// supports are dropped by the caller before clustering. Clusters come out
/// sorted by id, each id its smallest member.
pub fn clusters(
    supports: &[&str],
    relations: &[SourceRelation],
    origins: &HashMap<String, OriginKey>,
    passage_merges: &[(String, String)],
) -> Vec<Cluster> {
    let mut forest = Forest::default();
    for support in supports {
        forest.node(&format!("v:{support}"));
        if let Some(origin) = origins.get(*support) {
            forest.union(&format!("v:{support}"), &format!("o:{}", origin.0));
        }
    }
    for relation in relations {
        if !matches!(
            relation.kind,
            RelationKind::CopyOf
                | RelationKind::ContainsCopyOf
                | RelationKind::ArchiveOf
                | RelationKind::CanonicalOf
        ) {
            continue;
        }
        let to = match &relation.to {
            RelationTarget::Version(id) => format!("v:{id}"),
            RelationTarget::Url(url) => format!("u:{url}"),
        };
        forest.union(&format!("v:{}", relation.from), &to);
    }
    for (a, b) in passage_merges {
        forest.union(&format!("v:{a}"), &format!("v:{b}"));
    }
    let mut groups: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for support in supports {
        let root = forest.find(&format!("v:{support}"));
        let members = groups.entry(root).or_default();
        if !members.iter().any(|member| member == support) {
            members.push((*support).to_owned());
        }
    }
    let mut clusters: Vec<Cluster> = groups
        .into_values()
        .filter_map(|mut members| {
            members.sort();
            Some(Cluster {
                id: ClusterId(members.first()?.clone()),
                members,
            })
        })
        .collect();
    clusters.sort_by(|a, b| a.id.cmp(&b.id));
    clusters
}

/// Union-find over string-named nodes.
#[derive(Default)]
struct Forest {
    index: HashMap<String, usize>,
    parent: Vec<usize>,
}

impl Forest {
    fn node(&mut self, name: &str) -> usize {
        if let Some(&id) = self.index.get(name) {
            return id;
        }
        let id = self.parent.len();
        self.parent.push(id);
        self.index.insert(name.to_owned(), id);
        id
    }

    fn root(&mut self, mut id: usize) -> usize {
        while let Some(&parent) = self.parent.get(id) {
            if parent == id {
                break;
            }
            let grandparent = self.parent.get(parent).copied().unwrap_or(parent);
            if let Some(slot) = self.parent.get_mut(id) {
                *slot = grandparent;
            }
            id = parent;
        }
        id
    }

    fn find(&mut self, name: &str) -> usize {
        let id = self.node(name);
        self.root(id)
    }

    fn union(&mut self, a: &str, b: &str) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            if let Some(slot) = self.parent.get_mut(a.max(b)) {
                *slot = a.min(b);
            }
        }
    }
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    })
}

fn splitmix64(state: u64) -> u64 {
    let mut z = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Markdown links become their anchors; bare URLs go. A link target is
/// where text points, not text it shares.
fn strip_link_targets(text: &str) -> String {
    let links = Regex::new(r"\[([^\]]*)\]\([^)]*\)");
    let urls = Regex::new(r"https?://\S+");
    match (links, urls) {
        (Ok(links), Ok(urls)) => urls
            .replace_all(&links.replace_all(text, "$1"), " ")
            .into_owned(),
        _ => text.to_owned(),
    }
}

#[cfg(test)]
#[path = "independence_tests.rs"]
mod tests;
