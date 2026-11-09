//! The catalogue of problem patterns for coding sessions. A problem never names its pattern before the debrief.

use serde::{Deserialize, Serialize};

/// The six families of problem patterns.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "snake_case")]
pub enum PatternFamily {
    Lookup,
    Scanning,
    Ordering,
    Structures,
    Search,
    Building,
}

impl PatternFamily {
    /// Every family, in catalogue order.
    pub const ALL: [Self; 6] = [
        Self::Lookup,
        Self::Scanning,
        Self::Ordering,
        Self::Structures,
        Self::Search,
        Self::Building,
    ];

    /// The family's name as the app stores and shows it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lookup => "lookup",
            Self::Scanning => "scanning",
            Self::Ordering => "ordering",
            Self::Structures => "structures",
            Self::Search => "search",
            Self::Building => "building",
        }
    }
}

/// A recurring pattern in coding problems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProblemPattern {
    pub id: &'static str,
    pub name: &'static str,
    /// Other names the pattern goes by; a statement naming one gives the pattern away as much as its name does.
    pub aliases: &'static [&'static str],
    pub family: PatternFamily,
    /// What to notice in a problem that calls for the pattern.
    pub cue: &'static str,
}

const fn pattern(
    id: &'static str,
    name: &'static str,
    aliases: &'static [&'static str],
    family: PatternFamily,
    cue: &'static str,
) -> ProblemPattern {
    ProblemPattern {
        id,
        name,
        aliases,
        family,
        cue,
    }
}

/// The patterns of the catalogue, grouped by family.
pub const PROBLEM_PATTERNS: &[ProblemPattern] = &[
    pattern(
        "hash_map_lookup",
        "Hash map lookup",
        &["hash table", "hash table lookup", "dictionary lookup"],
        PatternFamily::Lookup,
        "Something is asked again and again about values already seen, and remembering them by key answers it at once.",
    ),
    pattern(
        "frequency_counting",
        "Frequency counting",
        &["frequency map", "frequency table", "counting occurrences"],
        PatternFamily::Lookup,
        "The answer depends on how many times each value occurs, not on where it occurs.",
    ),
    pattern(
        "prefix_sums",
        "Prefix sums",
        &["prefix sum", "cumulative sum"],
        PatternFamily::Lookup,
        "Many questions about the total of a stretch of values, each of which one running total settles.",
    ),
    pattern(
        "two_pointers",
        "Two pointers",
        &["two pointer", "opposite end pointers"],
        PatternFamily::Scanning,
        "Two positions move through the input by a rule, and each move rules out part of it.",
    ),
    pattern(
        "sliding_window",
        "Sliding window",
        &["moving window"],
        PatternFamily::Scanning,
        "The answer is about a contiguous stretch that grows at one end and shrinks at the other.",
    ),
    pattern(
        "fast_slow_pointers",
        "Fast and slow pointers",
        &["tortoise and hare", "floyd cycle detection"],
        PatternFamily::Scanning,
        "A walk through linked or repeating data where two walkers at different speeds meet or part.",
    ),
    pattern(
        "monotonic_stack",
        "Monotonic stack",
        &["monotone stack"],
        PatternFamily::Scanning,
        "Each element wants the nearest earlier or later element that is larger or smaller than it.",
    ),
    pattern(
        "sort_first",
        "Sort first",
        &["sorting first", "sort then scan"],
        PatternFamily::Ordering,
        "The input has no useful order, but once it has one, the answer is found in a single pass.",
    ),
    pattern(
        "interval_merging",
        "Interval merging",
        &["sweep line"],
        PatternFamily::Ordering,
        "Ranges overlap or touch, and ordering them by start makes the overlaps adjacent.",
    ),
    pattern(
        "heap_selection",
        "Heap selection",
        &["priority queue", "top k with a heap"],
        PatternFamily::Ordering,
        "Only the best few of a stream matter, and the next best must be available as items come and go.",
    ),
    pattern(
        "bucket_counting",
        "Bucket counting",
        &["counting sort", "bucket sort"],
        PatternFamily::Ordering,
        "The values fall in a small known range, so counting them in slots orders them without comparing.",
    ),
    pattern(
        "trie",
        "Trie",
        &["prefix tree"],
        PatternFamily::Structures,
        "Many strings share beginnings, and queries are about prefixes.",
    ),
    pattern(
        "union_find",
        "Union find",
        &["disjoint set", "disjoint set union"],
        PatternFamily::Structures,
        "Items are joined into groups as the input goes on, and the question is whether two are in one group.",
    ),
    pattern(
        "tree_traversal",
        "Tree traversal",
        &["depth first traversal", "level order traversal"],
        PatternFamily::Structures,
        "The answer is gathered by visiting every node of a hierarchy, with each node's answer built from its children's.",
    ),
    pattern(
        "graph_traversal",
        "Graph traversal",
        &["breadth first search", "depth first search", "bfs", "dfs"],
        PatternFamily::Structures,
        "Items are linked, and the question is what can be reached from where, or by how few steps.",
    ),
    pattern(
        "binary_search",
        "Binary search",
        &["bisection", "bisect"],
        PatternFamily::Search,
        "The input is ordered, or the answer is on one side of a line that a single check can find.",
    ),
    pattern(
        "binary_search_on_answer",
        "Binary search on the answer",
        &["parametric search"],
        PatternFamily::Search,
        "The answer is a number, and for any guess it is cheap to tell whether the guess is too small or too large.",
    ),
    pattern(
        "backtracking",
        "Backtracking",
        &["exhaustive search with pruning"],
        PatternFamily::Search,
        "Every way of making a series of choices must be tried, and a bad partial choice can be abandoned early.",
    ),
    pattern(
        "greedy_choice",
        "Greedy choice",
        &["greedy algorithm", "greedy strategy"],
        PatternFamily::Building,
        "At each step one choice is never worse than the others, so it can be made and never revisited.",
    ),
    pattern(
        "dynamic_programming",
        "Dynamic programming",
        &["memoization", "tabulation", "dp"],
        PatternFamily::Building,
        "The same smaller questions come up again and again, and each answer is built from answers to those.",
    ),
    pattern(
        "divide_and_conquer",
        "Divide and conquer",
        &["recursive splitting"],
        PatternFamily::Building,
        "The input splits into independent parts that are solved alike and then combined.",
    ),
    pattern(
        "bit_manipulation",
        "Bit manipulation",
        &["bitmask", "bitwise tricks"],
        PatternFamily::Building,
        "The values are small sets or flags, and operating on their bits does what a loop over members would.",
    ),
];

/// The pattern with this id.
pub fn problem_pattern(id: &str) -> Option<&'static ProblemPattern> {
    PROBLEM_PATTERNS.iter().find(|pattern| pattern.id == id)
}

/// The first of `pattern_ids` whose name or an alias appears in `text`, ignoring case, as a whole phrase.
pub fn named_in(text: &str, pattern_ids: &[String]) -> Option<&'static ProblemPattern> {
    let words = words_of(text);
    pattern_ids
        .iter()
        .filter_map(|id| problem_pattern(id))
        .find(|pattern| {
            std::iter::once(&pattern.name)
                .chain(pattern.aliases.iter())
                .any(|phrase| appears(&words, &words_of(phrase)))
        })
}

/// The words of a text, lowercase: runs of letters and digits, so punctuation and hyphens only separate them.
fn words_of(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn appears(words: &[String], phrase: &[String]) -> bool {
    !phrase.is_empty() && words.windows(phrase.len()).any(|window| window == phrase)
}

#[cfg(test)]
mod tests;
