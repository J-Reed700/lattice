//! The catalogue of problem patterns across six families for coding sessions.
//! A problem statement must never name its pattern; recognition is part of the task.

use serde::{Deserialize, Serialize};

/// The six families of problem patterns.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "snake_case")]
pub enum ProblemPatternFamily {
    Lookup,
    Scanning,
    Ordering,
    Structures,
    Search,
    Building,
}

/// A recurring pattern in coding problems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProblemPattern {
    pub id: &'static str,
    pub name: &'static str,
    pub family: ProblemPatternFamily,
    pub cue: &'static str,
    pub description: &'static str,
}

/// The catalogue of ~20 problem patterns across the 6 families.
pub const PROBLEM_PATTERNS: &[ProblemPattern] = &[
    // Lookup
    ProblemPattern {
        id: "hash_map_lookup",
        name: "Hash Map Lookup",
        family: ProblemPatternFamily::Lookup,
        cue: "pairs that sum to a target → remember what you've seen in a dict",
        description: "Track seen values or frequencies in a hash map to answer queries in O(1).",
    },
    ProblemPattern {
        id: "frequency_counting",
        name: "Frequency Counting",
        family: ProblemPatternFamily::Lookup,
        cue: "counting elements or comparing distributions → map item to count",
        description: "Count occurrences of items to compare multisets or find majorities.",
    },
    ProblemPattern {
        id: "prefix_sum_lookup",
        name: "Prefix Sum Lookup",
        family: ProblemPatternFamily::Lookup,
        cue: "subarray sum equals k → prefix sums in a hash map",
        description: "Store running prefix sums in a hash map to find target subarray sums in linear time.",
    },
    ProblemPattern {
        id: "two_sum",
        name: "Two Sum",
        family: ProblemPatternFamily::Lookup,
        cue: "finding complement in a collection → lookup target minus current",
        description: "Look up the complement of the current element in a set or dictionary.",
    },
    // Scanning
    ProblemPattern {
        id: "two_pointers",
        name: "Two Pointers",
        family: ProblemPatternFamily::Scanning,
        cue: "sorted array pair or palindrome → pointers from opposite ends",
        description: "Advance two pointers inward or in tandem across a sequential container.",
    },
    ProblemPattern {
        id: "sliding_window",
        name: "Sliding Window",
        family: ProblemPatternFamily::Scanning,
        cue: "contiguous subarray or substring with condition → expand right, shrink left",
        description: "Maintain a dynamic window of elements satisfying a monotonic constraint.",
    },
    ProblemPattern {
        id: "fast_slow_pointers",
        name: "Fast and Slow Pointers",
        family: ProblemPatternFamily::Scanning,
        cue: "cycle detection or middle element → move one pointer twice as fast",
        description: "Use two pointers moving at different speeds to detect cycles or find midpoints.",
    },
    ProblemPattern {
        id: "monotonic_stack",
        name: "Monotonic Stack",
        family: ProblemPatternFamily::Scanning,
        cue: "next greater or smaller element → stack with monotonic order",
        description: "Maintain elements in strictly increasing or decreasing order to find nearest extrema.",
    },
    // Ordering
    ProblemPattern {
        id: "sorting",
        name: "Sorting",
        family: ProblemPatternFamily::Ordering,
        cue: "grouping or processing in order → sort first",
        description: "Sort input elements to enable binary search, two pointers, or greedy choices.",
    },
    ProblemPattern {
        id: "custom_comparator",
        name: "Custom Comparator",
        family: ProblemPatternFamily::Ordering,
        cue: "special order or multi-key ranking → sort with key function",
        description: "Define a custom ordering function to sort complex objects or enforce tie-breaking rules.",
    },
    ProblemPattern {
        id: "top_k_elements",
        name: "Top K Elements",
        family: ProblemPatternFamily::Ordering,
        cue: "k largest or smallest items → min-heap of size k",
        description: "Track the top k elements using a priority queue or heap without fully sorting.",
    },
    ProblemPattern {
        id: "interval_merging",
        name: "Interval Merging",
        family: ProblemPatternFamily::Ordering,
        cue: "overlapping time ranges or intervals → sort by start time",
        description: "Sort intervals by start point and merge or sweep through overlapping boundaries.",
    },
    // Structures
    ProblemPattern {
        id: "linked_list_traversal",
        name: "Linked List Traversal",
        family: ProblemPatternFamily::Structures,
        cue: "list mutation or reversal → dummy head and pointer updates",
        description: "Traverse or mutate singly or doubly linked lists using pointer manipulation.",
    },
    ProblemPattern {
        id: "tree_traversal",
        name: "Tree Traversal",
        family: ProblemPatternFamily::Structures,
        cue: "hierarchical nodes → DFS preorder, inorder, or postorder",
        description: "Visit every node of a tree structure recursively or iteratively.",
    },
    ProblemPattern {
        id: "level_order_traversal",
        name: "Level Order Traversal",
        family: ProblemPatternFamily::Structures,
        cue: "shortest path or tree levels → BFS queue",
        description: "Traverse a tree or graph level by level using a FIFO queue.",
    },
    ProblemPattern {
        id: "trie",
        name: "Trie",
        family: ProblemPatternFamily::Structures,
        cue: "prefix matching or dictionary lookups → prefix tree",
        description: "Store strings in a tree where each node represents a common prefix character.",
    },
    // Search
    ProblemPattern {
        id: "binary_search",
        name: "Binary Search",
        family: ProblemPatternFamily::Search,
        cue: "sorted sequence or search space with monotonic predicate → divide in half",
        description: "Find a target or boundary in logarithmic time by bisecting the search range.",
    },
    ProblemPattern {
        id: "depth_first_search",
        name: "Depth-First Search",
        family: ProblemPatternFamily::Search,
        cue: "all paths, subsets, or connected components → recurse with visited set",
        description: "Explore graph or state space by going as deep as possible before backtracking.",
    },
    ProblemPattern {
        id: "breadth_first_search",
        name: "Breadth-First Search",
        family: ProblemPatternFamily::Search,
        cue: "unweighted shortest path or nearest neighbor → queue with visited set",
        description: "Explore graph or grid outwards one distance step at a time using a queue.",
    },
    ProblemPattern {
        id: "backtracking",
        name: "Backtracking",
        family: ProblemPatternFamily::Search,
        cue: "permutations, combinations, or constraint satisfaction → choose, explore, unchoose",
        description: "Build candidates incrementally and abandon paths that fail constraints.",
    },
    // Building
    ProblemPattern {
        id: "dynamic_programming",
        name: "Dynamic Programming",
        family: ProblemPatternFamily::Building,
        cue: "optimal substructure and overlapping subproblems → memoize or table",
        description: "Solve overlapping subproblems and store their results to construct the global optimum.",
    },
    ProblemPattern {
        id: "greedy",
        name: "Greedy Choice",
        family: ProblemPatternFamily::Building,
        cue: "locally optimal choice leads to globally optimal solution → pick best now",
        description: "Make the locally optimal decision at each step without reconsidering prior choices.",
    },
    ProblemPattern {
        id: "in_place_mutation",
        name: "In-Place Mutation",
        family: ProblemPatternFamily::Building,
        cue: "constant extra space required → swap or encode into existing array",
        description: "Modify the input data structure directly to achieve O(1) auxiliary space.",
    },
];

/// Looks up a pattern by its identifier.
pub fn find_pattern(id: &str) -> Option<&'static ProblemPattern> {
    PROBLEM_PATTERNS.iter().find(|pattern| pattern.id == id)
}

fn normalize(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push(' ');
    for c in s.chars() {
        if c.is_alphanumeric() {
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(' ');
        }
    }
    result.push(' ');
    result
}

/// Returns the pattern name if `statement` names any pattern in `patterns` (or any catalogue pattern name) case-insensitively.
pub fn statement_names_pattern(statement: &str, patterns: &[String]) -> Option<&'static str> {
    let haystack = normalize(statement);

    // First check specifically requested patterns
    for pattern_id in patterns {
        if let Some(pattern) = find_pattern(pattern_id) {
            let needle = normalize(pattern.name);
            if haystack.contains(&needle) {
                return Some(pattern.name);
            }
            let id_needle = normalize(pattern.id);
            if haystack.contains(&id_needle) {
                return Some(pattern.name);
            }
        }
    }

    // Then check all catalogue patterns
    for pattern in PROBLEM_PATTERNS {
        let needle = normalize(pattern.name);
        if haystack.contains(&needle) {
            return Some(pattern.name);
        }
        let id_needle = normalize(pattern.id);
        if haystack.contains(&id_needle) {
            return Some(pattern.name);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statement_names_pattern_refused() {
        let patterns = vec!["hash_map_lookup".to_string(), "two_pointers".to_string()];

        let statement = "You should use a hash map lookup to solve this problem.";
        let named = statement_names_pattern(statement, &patterns);
        assert_eq!(named, Some("Hash Map Lookup"));

        let statement2 = "Apply sliding window technique on the string.";
        let named2 = statement_names_pattern(statement2, &[]);
        assert_eq!(named2, Some("Sliding Window"));

        let clean = "Find the maximum sum of any contiguous subarray of size k.";
        let clean_named = statement_names_pattern(clean, &patterns);
        assert_eq!(clean_named, None);
    }
}
