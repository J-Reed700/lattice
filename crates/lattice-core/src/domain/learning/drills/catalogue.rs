//! The pattern catalogue: the recurring shapes of defect that a drill plants
//! and a learner is asked to recognise, as data. Each pattern has a family, a
//! one-sentence definition, the material it applies to and the source that
//! names it.

use serde::{Deserialize, Serialize};

use super::drill::Material;

/// The kind of problem a pattern is. The catalogue lists families in this
/// order, and a pattern's id begins with its family's slug. The first nine are
/// the families of code, the last five those of a passage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Correctness,
    Errors,
    Concurrency,
    Resources,
    Security,
    Performance,
    Design,
    Readability,
    Testing,
    Fidelity,
    Scope,
    Reasoning,
    Definition,
    Consistency,
}

impl Family {
    /// Every family, in the catalogue's order.
    pub const ALL: [Family; 14] = [
        Family::Correctness,
        Family::Errors,
        Family::Concurrency,
        Family::Resources,
        Family::Security,
        Family::Performance,
        Family::Design,
        Family::Readability,
        Family::Testing,
        Family::Fidelity,
        Family::Scope,
        Family::Reasoning,
        Family::Definition,
        Family::Consistency,
    ];

    /// The families of code, in the catalogue's order.
    pub const CODE: [Family; 9] = [
        Family::Correctness,
        Family::Errors,
        Family::Concurrency,
        Family::Resources,
        Family::Security,
        Family::Performance,
        Family::Design,
        Family::Readability,
        Family::Testing,
    ];

    /// The families of a passage, in the catalogue's order.
    pub const PASSAGE: [Family; 5] = [
        Family::Fidelity,
        Family::Scope,
        Family::Reasoning,
        Family::Definition,
        Family::Consistency,
    ];

    /// The family's name as it begins a pattern id: `security` in
    /// `security.sql_injection`.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Correctness => "correctness",
            Self::Errors => "errors",
            Self::Concurrency => "concurrency",
            Self::Resources => "resources",
            Self::Security => "security",
            Self::Performance => "performance",
            Self::Design => "design",
            Self::Readability => "readability",
            Self::Testing => "testing",
            Self::Fidelity => "fidelity",
            Self::Scope => "scope",
            Self::Reasoning => "reasoning",
            Self::Definition => "definition",
            Self::Consistency => "consistency",
        }
    }
}

/// What a pattern can be planted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Languages {
    /// Code in every language of the catalogue.
    Any,
    /// Only these: the pattern exists only where the language has the thing it
    /// is about.
    Only(&'static [&'static str]),
    /// Only an explanation of a passage, never code.
    Passage,
}

impl Languages {
    /// Whether a pattern with these languages applies to code in `language`.
    pub fn includes(self, language: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Only(languages) => languages.contains(&language),
            Self::Passage => false,
        }
    }
}

/// One recurring shape of defect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Pattern {
    /// `family.slug`, unique in the catalogue: the family's slug, a dot, then
    /// the pattern's own name in lowercase with underscores.
    pub id: &'static str,
    pub family: Family,
    pub name: &'static str,
    /// One sentence: what the pattern is and why it is a problem.
    pub definition: &'static str,
    pub languages: Languages,
    /// Where the pattern is named: a book and its chapter or entry, a paper, a
    /// CWE id, or the standard term.
    pub source: &'static str,
}

impl Pattern {
    /// Whether the pattern can be planted in `material`.
    pub fn applies_to(&self, material: &Material) -> bool {
        match material {
            Material::Code { language } => self.languages.includes(language),
            Material::Passage => self.languages == Languages::Passage,
        }
    }
}

/// The language ids a drill can be written in.
pub const LANGUAGES: [&str; 15] = [
    "python",
    "javascript",
    "typescript",
    "rust",
    "go",
    "java",
    "csharp",
    "cpp",
    "c",
    "kotlin",
    "swift",
    "ruby",
    "php",
    "sql",
    "shell",
];

/// The language of `LANGUAGES` most of the fenced code in `texts` is written in, ties to the one listed first; `None` when no fence names one.
pub fn language_of_code(texts: &[&str]) -> Option<&'static str> {
    let mut counts: Vec<(&'static str, usize)> = LANGUAGES.iter().map(|&name| (name, 0)).collect();
    for text in texts {
        let mut open: Option<char> = None;
        for line in text.lines() {
            let fence = line.trim_start_matches(' ');
            if line.len() - fence.len() > 3 {
                continue;
            }
            let Some(mark) = fence
                .chars()
                .next()
                .filter(|&mark| mark == '`' || mark == '~')
            else {
                continue;
            };
            let info = fence.trim_start_matches(mark);
            if fence.len() - info.len() < 3 {
                continue;
            }
            if open == Some(mark) {
                open = None;
                continue;
            }
            if open.is_some() {
                continue;
            }
            open = Some(mark);
            let word = info
                .trim_start()
                .split(|c: char| c.is_whitespace() || c == '{')
                .next()
                .unwrap_or_default()
                .trim_start_matches('.')
                .to_lowercase();
            if let Some(language) = language_named(&word) {
                if let Some((_, count)) = counts.iter_mut().find(|(name, _)| *name == language) {
                    *count += 1;
                }
            }
        }
    }
    let mut best: Option<(&'static str, usize)> = None;
    for &(name, count) in &counts {
        if count > best.map_or(0, |(_, most)| most) {
            best = Some((name, count));
        }
    }
    best.map(|(name, _)| name)
}

/// The language of `LANGUAGES` a fence's first word names, in lower case.
fn language_named(word: &str) -> Option<&'static str> {
    Some(match word {
        "python" | "py" | "python3" | "py3" => "python",
        "javascript" | "js" | "jsx" | "mjs" | "cjs" | "node" => "javascript",
        "typescript" | "ts" | "tsx" => "typescript",
        "rust" | "rs" => "rust",
        "go" | "golang" => "go",
        "java" => "java",
        "csharp" | "cs" | "c#" => "csharp",
        "cpp" | "c++" | "cc" | "cxx" | "hpp" => "cpp",
        "c" | "h" => "c",
        "kotlin" | "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "ruby" | "rb" => "ruby",
        "php" => "php",
        "sql" | "postgresql" | "postgres" | "mysql" | "sqlite" | "plsql" => "sql",
        "shell" | "sh" | "bash" | "zsh" | "console" => "shell",
        _ => return None,
    })
}

/// The pattern with this id.
pub fn pattern(id: &str) -> Option<&'static Pattern> {
    CATALOGUE.iter().find(|pattern| pattern.id == id)
}

/// The patterns that apply to `material`, in the catalogue's order.
pub fn patterns_for(material: &Material) -> impl Iterator<Item = &'static Pattern> + '_ {
    CATALOGUE
        .iter()
        .filter(move |pattern| pattern.applies_to(material))
}

/// The general-purpose programming languages: every language but SQL and shell.
const PROGRAMMING: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "rust",
    "go",
    "java",
    "csharp",
    "cpp",
    "c",
    "kotlin",
    "swift",
    "ruby",
    "php",
];

/// The languages where a value can be null: all of them but Rust, whose
/// absent values are an `Option` that has to be handled.
const NULLABLE: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "go",
    "java",
    "csharp",
    "cpp",
    "c",
    "kotlin",
    "swift",
    "ruby",
    "php",
];

/// The languages whose integers have a fixed width.
const FIXED_WIDTH_INTEGERS: &[&str] = &[
    "c", "cpp", "csharp", "go", "java", "kotlin", "rust", "swift",
];

/// The languages with `async` and `await`, or a deferred value to await.
const ASYNC_AWAIT: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "csharp",
    "rust",
    "swift",
    "kotlin",
];

/// The languages where a call reports failure by a returned status or value,
/// not only by an exception.
const ERRORS_AS_VALUES: &[&str] = &["c", "cpp", "go", "rust", "php", "shell"];

/// The languages a web application is commonly written in.
const WEB: &[&str] = &[
    "javascript",
    "typescript",
    "python",
    "ruby",
    "php",
    "java",
    "csharp",
    "kotlin",
    "go",
];

/// The languages whose standard libraries or common formats deserialise
/// arbitrary types.
const OBJECT_DESERIALISATION: &[&str] = &[
    "python",
    "java",
    "csharp",
    "php",
    "ruby",
    "kotlin",
    "javascript",
    "typescript",
];

/// The languages that evaluate a string as code.
const EVAL: &[&str] = &["python", "javascript", "typescript", "ruby", "php", "shell"];

/// The languages with manual memory management.
const MANUAL_MEMORY: &[&str] = &["c", "cpp"];

/// The languages with exceptions.
const EXCEPTIONS: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "java",
    "csharp",
    "cpp",
    "kotlin",
    "swift",
    "ruby",
    "php",
];

/// The languages with classes, or types that carry methods: the programming
/// languages but C.
const CLASSES: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "rust",
    "go",
    "java",
    "csharp",
    "cpp",
    "kotlin",
    "swift",
    "ruby",
    "php",
];

/// The languages with collection pipelines: filter, map and reduce over a
/// collection.
const PIPELINES: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "rust",
    "java",
    "csharp",
    "kotlin",
    "swift",
    "ruby",
    "php",
];

/// Every pattern, grouped by family in the order of `Family::ALL`. Within a
/// family the patterns that come up most often in code review interviews come
/// first, and `targets` offers the patterns a learner has never practised in
/// this order.
pub const CATALOGUE: &[Pattern] = &[
    // Correctness
    Pattern {
        id: "correctness.null_dereference",
        family: Family::Correctness,
        name: "Missing null check",
        definition: "A value that can be null, None or undefined is used without a check, so the program fails or misbehaves when the value is absent.",
        languages: Languages::Only(NULLABLE),
        source: "CWE-476",
    },
    Pattern {
        id: "correctness.off_by_one",
        family: Family::Correctness,
        name: "Off-by-one error",
        definition: "A loop bound, index or count is one too high or one too low, so the first or last element is skipped, repeated or read past the end.",
        languages: Languages::Any,
        source: "CWE-193",
    },
    Pattern {
        id: "correctness.boundary_and_empty_input",
        family: Family::Correctness,
        name: "Unhandled boundary or empty input",
        definition: "Code that is right for typical input fails on an empty collection, a single element, zero, a negative number or the largest allowed value.",
        languages: Languages::Any,
        source: "Myers, The Art of Software Testing, boundary value analysis",
    },
    Pattern {
        id: "correctness.wrong_operator",
        family: Family::Correctness,
        name: "Wrong operator or comparison",
        definition: "A comparison, logical or arithmetic operator is not the one the intent calls for, such as `<` for `<=` or `&&` for `||`, so the code computes something else.",
        languages: Languages::Any,
        source: "CWE-480",
    },
    Pattern {
        id: "correctness.mutated_argument",
        family: Family::Correctness,
        name: "Argument mutated in place",
        definition: "A function changes a collection or object it was passed, so the caller's data silently changes as a side effect.",
        languages: Languages::Only(PROGRAMMING),
        source: "Fowler, Refactoring (2nd ed.), Mutable Data",
    },
    Pattern {
        id: "correctness.mutable_default_argument",
        family: Family::Correctness,
        name: "Mutable default argument",
        definition: "A default argument that is a list, a dict or another mutable object is created once, when the function is defined, and shared by every call that leaves it out.",
        languages: Languages::Only(&["python"]),
        source: "Python tutorial, More on Defining Functions: Default Argument Values",
    },
    Pattern {
        id: "correctness.integer_overflow",
        family: Family::Correctness,
        name: "Integer overflow",
        definition: "Arithmetic on a fixed-width integer exceeds its range and wraps or traps, so a size, a count or a price silently becomes wrong.",
        languages: Languages::Only(FIXED_WIDTH_INTEGERS),
        source: "CWE-190",
    },
    Pattern {
        id: "correctness.reference_equality",
        family: Family::Correctness,
        name: "Identity compared where value was meant",
        definition: "Two references are compared with an operator that tests identity, such as `==` on Java strings, `is` on Python values or `===` on JavaScript objects, so equal values compare as different.",
        languages: Languages::Only(&["java", "csharp", "python", "javascript", "typescript"]),
        source: "Java Language Specification, section 15.21.3: Reference Equality Operators",
    },
    Pattern {
        id: "correctness.stale_state",
        family: Family::Correctness,
        name: "Stale state",
        definition: "A value is read from a cache, a copy or an earlier lookup after the data it came from has changed, so a decision is made on out-of-date information.",
        languages: Languages::Any,
        source: "Standard term: stale read",
    },
    Pattern {
        id: "correctness.float_for_exact_values",
        family: Family::Correctness,
        name: "Floating point for exact values",
        definition: "Money or another quantity that must be exact is held in binary floating point, which cannot represent most decimal fractions, so sums drift by fractions of a unit.",
        languages: Languages::Any,
        source: "Bloch, Effective Java (3rd ed.), Item 60",
    },
    Pattern {
        id: "correctness.modified_while_iterating",
        family: Family::Correctness,
        name: "Collection modified while iterating",
        definition: "A collection is added to or removed from while a loop is walking it, so elements are skipped or the iterator fails partway.",
        languages: Languages::Only(PROGRAMMING),
        source: "Java SE API, ConcurrentModificationException",
    },
    Pattern {
        id: "correctness.loop_variable_capture",
        family: Family::Correctness,
        name: "Loop variable captured by a closure",
        definition: "Closures made in a loop capture the loop variable itself rather than its value on each turn, so all of them see the last value.",
        languages: Languages::Only(&["python", "javascript", "typescript", "csharp"]),
        source: "Python Programming FAQ, Why do lambdas defined in a loop with different values all return the same result?",
    },
    Pattern {
        id: "correctness.loose_equality",
        family: Family::Correctness,
        name: "Loose equality coercion",
        definition: "A comparison with `==` converts its operands to a common type first, so values that differ, such as `0` and an empty string, compare equal.",
        languages: Languages::Only(&["javascript", "typescript"]),
        source: "MDN, Equality comparisons and sameness",
    },
    Pattern {
        id: "correctness.naive_datetime",
        family: Family::Correctness,
        name: "Naive date and time",
        definition: "A time is stored or compared with no time zone, or in local time where UTC was meant, so its meaning changes with the zone of the machine and with daylight saving.",
        languages: Languages::Any,
        source: "Python docs, datetime, Aware and Naive Objects",
    },
    Pattern {
        id: "correctness.null_comparison_in_sql",
        family: Family::Correctness,
        name: "Comparison with NULL",
        definition: "In SQL a comparison with NULL using `=` or `<>` evaluates to unknown rather than true, so a condition that reads correctly silently matches no rows.",
        languages: Languages::Only(&["sql"]),
        source: "ISO/IEC 9075 (SQL), three-valued logic",
    },
    // Errors
    Pattern {
        id: "errors.swallowed_exception",
        family: Family::Errors,
        name: "Swallowed exception",
        definition: "An error is caught and then ignored, so the failure goes unnoticed and execution carries on as if the operation had worked.",
        languages: Languages::Any,
        source: "CWE-390",
    },
    Pattern {
        id: "errors.error_as_success",
        family: Family::Errors,
        name: "Error reported as success",
        definition: "A failure is turned into a normal result, such as an empty list, a default value or a success status, so callers cannot tell that the operation failed.",
        languages: Languages::Any,
        source: "CWE-393",
    },
    Pattern {
        id: "errors.overly_broad_catch",
        family: Family::Errors,
        name: "Overly broad catch",
        definition: "A handler catches every exception, or a base type such as Exception, so it also swallows errors it cannot handle, including programming mistakes and cancellation.",
        languages: Languages::Only(EXCEPTIONS),
        source: "CWE-396",
    },
    Pattern {
        id: "errors.partial_update_without_rollback",
        family: Family::Errors,
        name: "Partial update without rollback",
        definition: "A change made in several steps has no transaction or compensation, so a failure partway leaves the data half updated.",
        languages: Languages::Any,
        source: "Gray and Reuter, Transaction Processing: Concepts and Techniques (1992), atomicity",
    },
    Pattern {
        id: "errors.lost_context",
        family: Family::Errors,
        name: "Error context lost",
        definition: "An error is re-raised or wrapped without its cause, its message or its stack, so whoever handles it cannot tell what failed or why.",
        languages: Languages::Only(PROGRAMMING),
        source: "Java Tutorials, Chained Exceptions",
    },
    Pattern {
        id: "errors.unchecked_return_value",
        family: Family::Errors,
        name: "Ignored error return value",
        definition: "The status or error that a call returns is discarded, so the code goes on as though the call had succeeded.",
        languages: Languages::Only(ERRORS_AS_VALUES),
        source: "CWE-252",
    },
    Pattern {
        id: "errors.retry_without_backoff",
        family: Family::Errors,
        name: "Retry without backoff",
        definition: "A failed call is retried at once, with no growing delay and no limit on attempts, so a struggling dependency is hit harder and a blip can become an outage.",
        languages: Languages::Any,
        source: "Brooker, Timeouts, retries and backoff with jitter (Amazon Builders' Library)",
    },
    Pattern {
        id: "errors.retry_not_idempotent",
        family: Family::Errors,
        name: "Retry of a non-idempotent operation",
        definition: "An operation with side effects, such as a payment or an insert, is retried without an idempotency key or a check, so a timeout that hid a success makes it happen twice.",
        languages: Languages::Any,
        source: "RFC 9110, section 9.2.2, Idempotent Methods",
    },
    Pattern {
        id: "errors.unwrap_on_fallible",
        family: Family::Errors,
        name: "Unwrap on a fallible value",
        definition: "Calling `unwrap()` or `expect()` on a Result or Option that input or the environment can make fail turns an ordinary error into a panic that ends the thread or the program.",
        languages: Languages::Only(&["rust"]),
        source: "The Rust Programming Language, ch. 9.3: To panic! or Not to panic!",
    },
    Pattern {
        id: "errors.script_continues_after_failure",
        family: Family::Errors,
        name: "Script continues after a failed command",
        definition: "A shell script without `set -e` or explicit status checks carries on after a command fails, and a failure inside a pipeline is hidden by the status of its last command.",
        languages: Languages::Only(&["shell"]),
        source: "GNU Bash Reference Manual, The Set Builtin",
    },
    // Concurrency
    Pattern {
        id: "concurrency.check_then_act",
        family: Family::Concurrency,
        name: "Check-then-act race",
        definition: "A condition is checked and then acted on while nothing stops it changing in between, so two concurrent callers can both pass the check.",
        languages: Languages::Any,
        source: "Lu et al. (2008), Learning from mistakes: atomicity violation",
    },
    Pattern {
        id: "concurrency.unsynchronised_shared_state",
        family: Family::Concurrency,
        name: "Unsynchronised shared state",
        definition: "Data that more than one thread, task or request can reach is read and written without a lock, an atomic or other synchronisation, so updates are lost or seen half done.",
        languages: Languages::Only(PROGRAMMING),
        source: "Lu et al. (2008), Learning from mistakes: atomicity and order violations",
    },
    Pattern {
        id: "concurrency.lock_ordering",
        family: Family::Concurrency,
        name: "Inconsistent lock ordering",
        definition: "Two code paths take the same locks in different orders, so each can end up holding one lock while waiting for the other and both wait forever.",
        languages: Languages::Any,
        source: "Lu et al. (2008), Learning from mistakes: deadlock",
    },
    Pattern {
        id: "concurrency.missing_await",
        family: Family::Concurrency,
        name: "Missing await",
        definition: "An asynchronous call is made without awaiting its result, so the code runs on before the work is done and its value or its error is lost.",
        languages: Languages::Only(ASYNC_AWAIT),
        source: "Python docs, Developing with asyncio: Detect never-awaited coroutines; typescript-eslint, no-floating-promises",
    },
    Pattern {
        id: "concurrency.blocking_in_async",
        family: Family::Concurrency,
        name: "Blocking call in async code",
        definition: "A blocking call, such as synchronous I/O, a sleep or heavy computation, runs on a thread that serves asynchronous tasks, so every other task on it stalls until the call returns.",
        languages: Languages::Only(ASYNC_AWAIT),
        source: "Node.js guides, Don't Block the Event Loop (or the Worker Pool)",
    },
    Pattern {
        id: "concurrency.lock_held_across_slow_call",
        family: Family::Concurrency,
        name: "Lock held across a slow call",
        definition: "A lock stays held while the code does I/O, calls over the network or awaits, so every other caller waits on the slowest operation and a stall can become a deadlock.",
        languages: Languages::Only(PROGRAMMING),
        source: "Goetz et al., Java Concurrency in Practice (2006), section 11.4.1: Narrowing Lock Scope",
    },
    Pattern {
        id: "concurrency.double_checked_locking",
        family: Family::Concurrency,
        name: "Broken double-checked locking",
        definition: "A lazily created shared object is checked without a lock and then created under one, and without a memory barrier another thread can see it before it is fully built.",
        languages: Languages::Only(&["java", "cpp", "csharp", "kotlin"]),
        source: "Bacon et al., The Double-Checked Locking is Broken Declaration",
    },
    // Resources
    Pattern {
        id: "resources.leaked_handle",
        family: Family::Resources,
        name: "Leaked handle or connection",
        definition: "A file, socket, connection or lock is acquired and not released on every path, an error path above all, so the process runs out of handles over time.",
        languages: Languages::Only(PROGRAMMING),
        source: "CWE-772",
    },
    Pattern {
        id: "resources.unbounded_growth",
        family: Family::Resources,
        name: "Unbounded growth",
        definition: "A cache, queue, buffer or list grows with every request or event and is never trimmed, so memory use climbs until the process is killed.",
        languages: Languages::Only(PROGRAMMING),
        source: "CWE-770",
    },
    Pattern {
        id: "resources.missing_timeout",
        family: Family::Resources,
        name: "Missing timeout",
        definition: "A call to a network service, a lock or a process has no time limit, so one stuck dependency makes the caller wait forever and keeps its resources tied up.",
        languages: Languages::Any,
        source: "CWE-1088",
    },
    Pattern {
        id: "resources.unbounded_result_set",
        family: Family::Resources,
        name: "Unbounded result set",
        definition: "A query or request reads every matching record at once, with no limit or paging, so its cost grows with the data and a large table exhausts memory.",
        languages: Languages::Any,
        source: "CWE-400",
    },
    Pattern {
        id: "resources.lapsed_listener",
        family: Family::Resources,
        name: "Lapsed listener",
        definition: "A listener or subscription is registered and never removed, so the object it points to cannot be freed for as long as the source lives.",
        languages: Languages::Only(&["javascript", "typescript", "java", "csharp", "kotlin", "swift"]),
        source: "Standard term: lapsed listener problem",
    },
    Pattern {
        id: "resources.use_after_free",
        family: Family::Resources,
        name: "Use after free",
        definition: "Memory is used after it has been freed, so the program reads or writes storage that now belongs to something else.",
        languages: Languages::Only(MANUAL_MEMORY),
        source: "CWE-416",
    },
    // Security
    Pattern {
        id: "security.sql_injection",
        family: Family::Security,
        name: "SQL injection",
        definition: "Input is joined into a SQL statement instead of being passed as a parameter, so an attacker can change what the query means.",
        languages: Languages::Any,
        source: "CWE-89",
    },
    Pattern {
        id: "security.missing_authorisation",
        family: Family::Security,
        name: "Missing authorisation check",
        definition: "An action or a record can be reached without checking that the caller may use it, so any user who finds the request can read or change what belongs to others.",
        languages: Languages::Only(PROGRAMMING),
        source: "CWE-862",
    },
    Pattern {
        id: "security.hardcoded_secret",
        family: Family::Security,
        name: "Secret in code",
        definition: "A password, token or key is written into the source, so anyone who can read the repository or the build can use it and it cannot be rotated without a release.",
        languages: Languages::Any,
        source: "CWE-798",
    },
    Pattern {
        id: "security.xss",
        family: Family::Security,
        name: "Cross-site scripting",
        definition: "Input is written into a page without escaping for the context it lands in, so an attacker's markup or script runs in other users' browsers.",
        languages: Languages::Only(WEB),
        source: "CWE-79",
    },
    Pattern {
        id: "security.command_injection",
        family: Family::Security,
        name: "OS command injection",
        definition: "Input is placed into a command line that a shell runs, so an attacker can add commands of their own.",
        languages: Languages::Any,
        source: "CWE-78",
    },
    Pattern {
        id: "security.path_traversal",
        family: Family::Security,
        name: "Path traversal",
        definition: "A file path is built from input without confining it to the intended directory, so `../` sequences or an absolute path reach files outside it.",
        languages: Languages::Only(PROGRAMMING),
        source: "CWE-22",
    },
    Pattern {
        id: "security.sensitive_data_in_logs",
        family: Family::Security,
        name: "Sensitive data in logs",
        definition: "A password, token or piece of personal data is written to a log or an error message, so anyone with access to the logs can read it.",
        languages: Languages::Any,
        source: "CWE-532",
    },
    Pattern {
        id: "security.weak_cryptography",
        family: Family::Security,
        name: "Broken or risky cryptography",
        definition: "A weak or obsolete algorithm, mode or key size protects passwords or data, so the protection can be bypassed or broken.",
        languages: Languages::Only(PROGRAMMING),
        source: "CWE-327",
    },
    Pattern {
        id: "security.ssrf",
        family: Family::Security,
        name: "Server-side request forgery",
        definition: "The server fetches a URL that input controls without restricting where it may point, so an attacker can make it call internal services or cloud metadata endpoints.",
        languages: Languages::Only(PROGRAMMING),
        source: "CWE-918",
    },
    Pattern {
        id: "security.unsafe_deserialisation",
        family: Family::Security,
        name: "Unsafe deserialisation",
        definition: "Data from an untrusted source is deserialised with a format that can create arbitrary types or run code, so an attacker decides what the program builds.",
        languages: Languages::Only(OBJECT_DESERIALISATION),
        source: "CWE-502",
    },
    Pattern {
        id: "security.missing_csrf_protection",
        family: Family::Security,
        name: "Missing CSRF protection",
        definition: "A state-changing request is accepted on the strength of a session cookie alone, with no token or origin check, so another site can make the browser send it.",
        languages: Languages::Only(WEB),
        source: "CWE-352",
    },
    Pattern {
        id: "security.weak_randomness",
        family: Family::Security,
        name: "Predictable randomness for secrets",
        definition: "A token, key or identifier that must be unguessable comes from a generator that is not cryptographically secure, so an attacker can predict it.",
        languages: Languages::Only(PROGRAMMING),
        source: "CWE-338",
    },
    Pattern {
        id: "security.mass_assignment",
        family: Family::Security,
        name: "Mass assignment",
        definition: "Request fields are bound straight onto a model or record, so a caller can set fields it should not, such as a role or an owner.",
        languages: Languages::Only(WEB),
        source: "CWE-915",
    },
    Pattern {
        id: "security.open_redirect",
        family: Family::Security,
        name: "Open redirect",
        definition: "A redirect target is taken from the request without a check, so a link on the site can send users to a page the attacker chooses.",
        languages: Languages::Only(WEB),
        source: "CWE-601",
    },
    Pattern {
        id: "security.missing_certificate_validation",
        family: Family::Security,
        name: "Disabled certificate validation",
        definition: "TLS certificate or host name verification is turned off or skipped, so an attacker in the middle can impersonate the server.",
        languages: Languages::Only(PROGRAMMING),
        source: "CWE-295",
    },
    Pattern {
        id: "security.timing_unsafe_comparison",
        family: Family::Security,
        name: "Timing-unsafe comparison",
        definition: "A secret such as a token or a MAC is compared with an operator that stops at the first difference, so response times reveal how much of a guess was right.",
        languages: Languages::Only(PROGRAMMING),
        source: "CWE-208",
    },
    Pattern {
        id: "security.code_injection",
        family: Family::Security,
        name: "Code injection through eval",
        definition: "Input reaches `eval`, `exec` or a template that compiles code, so an attacker can run code of their own inside the application.",
        languages: Languages::Only(EVAL),
        source: "CWE-94",
    },
    Pattern {
        id: "security.out_of_bounds_write",
        family: Family::Security,
        name: "Out-of-bounds write",
        definition: "Data is written past the end of a buffer or before its start, so it overwrites neighbouring memory and can corrupt state or give an attacker control.",
        languages: Languages::Only(MANUAL_MEMORY),
        source: "CWE-787",
    },
    // Performance
    Pattern {
        id: "performance.n_plus_one_queries",
        family: Family::Performance,
        name: "N+1 queries",
        definition: "Code runs one query to list records and then one more for each record, so the number of round trips grows with the data instead of staying constant.",
        languages: Languages::Any,
        source: "Standard term: N+1 selects; see CWE-1060",
    },
    Pattern {
        id: "performance.quadratic_loop",
        family: Family::Performance,
        name: "Quadratic loop",
        definition: "A scan, a membership test on a list or a nested loop runs once per element, so the work grows with the square of the input and a larger input is far slower.",
        languages: Languages::Any,
        source: "CWE-407",
    },
    Pattern {
        id: "performance.repeated_work_in_loop",
        family: Family::Performance,
        name: "Repeated work in a loop",
        definition: "A value that does not change from one iteration to the next, such as a compiled regular expression, a lookup or a file read, is computed again on every iteration.",
        languages: Languages::Any,
        source: "Standard term: loop-invariant code motion",
    },
    Pattern {
        id: "performance.string_building_in_loop",
        family: Family::Performance,
        name: "String built by repeated concatenation",
        definition: "A string is grown by concatenation in a loop and everything built so far is copied on each turn, so the work grows with the square of the length.",
        languages: Languages::Only(&["java", "csharp", "kotlin", "go", "python"]),
        source: "Bloch, Effective Java (3rd ed.), Item 63",
    },
    Pattern {
        id: "performance.sequential_awaits",
        family: Family::Performance,
        name: "Independent awaits in sequence",
        definition: "Independent asynchronous calls are awaited one after another, so their waits add up instead of overlapping.",
        languages: Languages::Only(ASYNC_AWAIT),
        source: "MDN, Promise.all()",
    },
    Pattern {
        id: "performance.needless_allocation",
        family: Family::Performance,
        name: "Needless allocation",
        definition: "Objects, strings or copies of collections that nothing needs are created, often on a hot path, so the allocator and the garbage collector do avoidable work.",
        languages: Languages::Only(PROGRAMMING),
        source: "Bloch, Effective Java (3rd ed.), Item 6",
    },
    Pattern {
        id: "performance.unindexed_lookup",
        family: Family::Performance,
        name: "Query the index cannot serve",
        definition: "A filter applies a function or a conversion to an indexed column, or starts with a wildcard, so the database scans the whole table instead of using the index.",
        languages: Languages::Only(&["sql"]),
        source: "Winand, SQL Performance Explained",
    },
    Pattern {
        id: "performance.catastrophic_regex",
        family: Family::Performance,
        name: "Catastrophic regular expression",
        definition: "A regular expression with nested or overlapping repetition takes exponential time on some inputs, so a short crafted string can hang the process.",
        languages: Languages::Only(PROGRAMMING),
        source: "CWE-1333",
    },
    // Design
    Pattern {
        id: "design.long_function",
        family: Family::Design,
        name: "Long function",
        definition: "A function does so much that it cannot be read at a glance, so its parts cannot be named, tested or reused on their own.",
        languages: Languages::Any,
        source: "Fowler, Refactoring (2nd ed.), Long Function",
    },
    Pattern {
        id: "design.large_class",
        family: Family::Design,
        name: "Large class",
        definition: "A class holds so many fields and responsibilities that it changes for many reasons and no one can keep it in their head.",
        languages: Languages::Only(CLASSES),
        source: "Fowler, Refactoring (2nd ed.), Large Class",
    },
    Pattern {
        id: "design.hardwired_dependency",
        family: Family::Design,
        name: "Hardwired dependency",
        definition: "A unit builds its own collaborators, such as a database client, the clock or an HTTP client, instead of receiving them, so it cannot be tested or swapped.",
        languages: Languages::Only(PROGRAMMING),
        source: "Fowler (2004), Inversion of Control Containers and the Dependency Injection pattern",
    },
    Pattern {
        id: "design.global_mutable_state",
        family: Family::Design,
        name: "Global mutable state",
        definition: "Code reads and writes a variable shared across the whole program, so behaviour depends on what ran before and callers and tests interfere with each other.",
        languages: Languages::Only(PROGRAMMING),
        source: "Fowler, Refactoring (2nd ed.), Global Data",
    },
    Pattern {
        id: "design.long_parameter_list",
        family: Family::Design,
        name: "Long parameter list",
        definition: "A function takes so many parameters that callers pass them in the wrong order or get them wrong, and every new need changes every call.",
        languages: Languages::Only(PROGRAMMING),
        source: "Fowler, Refactoring (2nd ed.), Long Parameter List",
    },
    Pattern {
        id: "design.repeated_switches",
        family: Family::Design,
        name: "Repeated switches",
        definition: "The same switch or if-chain on a type or kind appears in several places, so adding a case means finding and changing every one of them.",
        languages: Languages::Only(PROGRAMMING),
        source: "Fowler, Refactoring (2nd ed.), Repeated Switches",
    },
    Pattern {
        id: "design.primitive_obsession",
        family: Family::Design,
        name: "Primitive obsession",
        definition: "Domain concepts such as money, an identifier or a range travel as bare strings and numbers, so nothing stops a caller mixing them up and the rules for them are scattered.",
        languages: Languages::Only(PROGRAMMING),
        source: "Fowler, Refactoring (2nd ed.), Primitive Obsession",
    },
    Pattern {
        id: "design.feature_envy",
        family: Family::Design,
        name: "Feature envy",
        definition: "A function uses the data and methods of another class more than its own, so the logic lives far from the data it depends on.",
        languages: Languages::Only(PROGRAMMING),
        source: "Fowler, Refactoring (2nd ed.), Feature Envy",
    },
    Pattern {
        id: "design.flag_argument",
        family: Family::Design,
        name: "Flag argument",
        definition: "A boolean or mode parameter selects between two behaviours inside a function, so the call site reads as an unexplained true or false and the function does two jobs.",
        languages: Languages::Only(PROGRAMMING),
        source: "Fowler, Refactoring (2nd ed.), Remove Flag Argument",
    },
    Pattern {
        id: "design.leaky_abstraction",
        family: Family::Design,
        name: "Leaky abstraction",
        definition: "An interface exposes details of its implementation, such as storage types, error codes or ordering, so callers come to depend on them and the implementation can no longer change.",
        languages: Languages::Only(PROGRAMMING),
        source: "Spolsky (2002), The Law of Leaky Abstractions",
    },
    Pattern {
        id: "design.data_clumps",
        family: Family::Design,
        name: "Data clumps",
        definition: "The same few values travel together through fields and parameter lists, so they want to be an object of their own and every change touches every place they appear.",
        languages: Languages::Only(PROGRAMMING),
        source: "Fowler, Refactoring (2nd ed.), Data Clumps",
    },
    Pattern {
        id: "design.temporal_coupling",
        family: Family::Design,
        name: "Temporal coupling",
        definition: "Calls must be made in an order that nothing enforces, such as initialising before use, so a caller who gets the order wrong gets a failure or a wrong result.",
        languages: Languages::Only(PROGRAMMING),
        source: "Martin, Clean Code, Hidden Temporal Couplings",
    },
    Pattern {
        id: "design.message_chains",
        family: Family::Design,
        name: "Message chain",
        definition: "Code reaches through a chain of objects to get what it needs, such as `a.b().c().d()`, so it depends on the structure of every object along the way.",
        languages: Languages::Only(PROGRAMMING),
        source: "Fowler, Refactoring (2nd ed.), Message Chains",
    },
    Pattern {
        id: "design.speculative_generality",
        family: Family::Design,
        name: "Speculative generality",
        definition: "Hooks, parameters, abstract classes or options exist for needs that have not arrived, so the code carries complexity that nothing uses.",
        languages: Languages::Only(PROGRAMMING),
        source: "Fowler, Refactoring (2nd ed.), Speculative Generality",
    },
    // Readability
    Pattern {
        id: "readability.misleading_name",
        family: Family::Readability,
        name: "Misleading name",
        definition: "A name says something other than what the thing is or does, so readers form a wrong picture of the code and make wrong changes.",
        languages: Languages::Any,
        source: "Fowler, Refactoring (2nd ed.), Mysterious Name",
    },
    Pattern {
        id: "readability.duplicated_logic",
        family: Family::Readability,
        name: "Duplicated logic",
        definition: "The same logic is written out in more than one place, so a fix or a change made in one copy is missed in the others.",
        languages: Languages::Any,
        source: "Fowler, Refactoring (2nd ed.), Duplicated Code",
    },
    Pattern {
        id: "readability.magic_number",
        family: Family::Readability,
        name: "Magic number",
        definition: "A literal whose meaning is not obvious, such as a limit, a status code or a rate, appears unnamed in the code, so readers cannot tell what it means or where else it must change.",
        languages: Languages::Any,
        source: "Fowler, Refactoring (2nd ed.), Replace Magic Literal",
    },
    Pattern {
        id: "readability.deep_nesting",
        family: Family::Readability,
        name: "Deep nesting",
        definition: "Conditions and loops are nested several levels deep, so the main path is hard to follow and the conditions it depends on are hard to keep in mind.",
        languages: Languages::Any,
        source: "Fowler, Refactoring (2nd ed.), Replace Nested Conditional with Guard Clauses",
    },
    Pattern {
        id: "readability.complex_conditional",
        family: Family::Readability,
        name: "Complex conditional",
        definition: "A condition combines many clauses, negations or nested branches in one expression, so its meaning has to be worked out again each time it is read.",
        languages: Languages::Any,
        source: "Fowler, Refactoring (2nd ed.), Decompose Conditional",
    },
    Pattern {
        id: "readability.dead_code",
        family: Family::Readability,
        name: "Dead code",
        definition: "Code that can never run, or whose result is never used, stays in the file, so readers spend time on it and may think it matters.",
        languages: Languages::Any,
        source: "Fowler, Refactoring (2nd ed.), Remove Dead Code",
    },
    Pattern {
        id: "readability.misleading_comment",
        family: Family::Readability,
        name: "Misleading comment",
        definition: "A comment says something the code does not do, usually because the code changed and the comment did not, so readers trust the wrong description.",
        languages: Languages::Any,
        source: "Martin, Clean Code, ch. 4: Misleading Comments",
    },
    Pattern {
        id: "readability.loop_instead_of_pipeline",
        family: Family::Readability,
        name: "Loop where a pipeline reads better",
        definition: "A loop with a mutable accumulator does what a filter, map or reduce would say directly, so the reader has to simulate the loop to see what it computes.",
        languages: Languages::Only(PIPELINES),
        source: "Fowler, Refactoring (2nd ed.), Loops; Replace Loop with Pipeline",
    },
    // Testing
    Pattern {
        id: "testing.untested_branch",
        family: Family::Testing,
        name: "Untested branch",
        definition: "A branch, an error path or a boundary of the code under test is never run by any test, so a defect there goes unnoticed until it reaches production.",
        languages: Languages::Any,
        source: "Myers, The Art of Software Testing, decision coverage",
    },
    Pattern {
        id: "testing.assertion_free_test",
        family: Family::Testing,
        name: "Assertion-free test",
        definition: "A test runs the code but asserts nothing about the result, so it passes whatever the code does and gives only a false sense of coverage.",
        languages: Languages::Any,
        source: "Meszaros, xUnit Test Patterns, Self-Checking Test",
    },
    Pattern {
        id: "testing.coupled_to_implementation",
        family: Family::Testing,
        name: "Test coupled to implementation",
        definition: "A test checks how the code works, such as which helpers or mocks were called in what order, instead of what it produces, so a harmless refactoring breaks it.",
        languages: Languages::Any,
        source: "Meszaros, xUnit Test Patterns, Fragile Test",
    },
    Pattern {
        id: "testing.nondeterministic_test",
        family: Family::Testing,
        name: "Nondeterministic test",
        definition: "A test depends on the clock, on randomness, on the network or on the order of a hash map, so it passes and fails with no change to the code.",
        languages: Languages::Any,
        source: "Meszaros, xUnit Test Patterns, Erratic Test",
    },
    Pattern {
        id: "testing.shared_state_between_tests",
        family: Family::Testing,
        name: "Shared state between tests",
        definition: "Tests share mutable state such as a global, a file or a database row, so a result depends on which tests ran before and the suite fails when the order changes.",
        languages: Languages::Any,
        source: "Meszaros, xUnit Test Patterns, Interacting Tests",
    },
    // Fidelity
    Pattern {
        id: "fidelity.misquoted_figure",
        family: Family::Fidelity,
        name: "Misquoted figure",
        definition: "A number, percentage or other figure differs from the one the source gives, so a reader takes away a result the source does not report.",
        languages: Languages::Passage,
        source: "Lewandowsky et al., Misinformation and its Correction, Psychological Science in the Public Interest 13(3), 2012",
    },
    Pattern {
        id: "fidelity.wrong_unit_or_magnitude",
        family: Family::Fidelity,
        name: "Wrong unit or magnitude",
        definition: "A quantity keeps its number but has the wrong unit or the wrong order of magnitude, such as milligrams for micrograms or thousands for millions, so it is off by a large factor.",
        languages: Languages::Passage,
        source: "Huff, How to Lie with Statistics, ch. 5, The Gee-Whiz Graph",
    },
    Pattern {
        id: "fidelity.swapped_entities",
        family: Family::Fidelity,
        name: "Swapped entities",
        definition: "Two people, groups, methods or things the source distinguishes trade places, so what the source says of one is said of the other.",
        languages: Languages::Passage,
        source: "Lewandowsky et al., Misinformation and its Correction, Psychological Science in the Public Interest 13(3), 2012",
    },
    Pattern {
        id: "fidelity.wrong_attribution",
        family: Family::Fidelity,
        name: "Wrong attribution",
        definition: "A claim, finding or quotation is credited to a person, study or institution other than the one the source credits it to, so a reader trusts it for the wrong reason.",
        languages: Languages::Passage,
        source: "Lewandowsky et al., Misinformation and its Correction, Psychological Science in the Public Interest 13(3), 2012",
    },
    Pattern {
        id: "fidelity.wrong_date_or_order",
        family: Family::Fidelity,
        name: "Wrong date or order",
        definition: "A date, period or sequence of events differs from the source's, so a cause appears to come after its effect or a step happens out of order.",
        languages: Languages::Passage,
        source: "Grosse and Renkl, Finding and Fixing Errors in Worked Examples, Journal of Educational Psychology 99(4), 2007",
    },
    // Scope
    Pattern {
        id: "scope.overgeneralisation",
        family: Family::Scope,
        name: "Overgeneralisation",
        definition: "A claim the source makes about some cases, some people or one setting is stated about all of them, so it asserts more than the evidence reaches.",
        languages: Languages::Passage,
        source: "Damer, Attacking Faulty Reasoning, Hasty Generalization",
    },
    Pattern {
        id: "scope.dropped_qualifier",
        family: Family::Scope,
        name: "Dropped qualifier or hedge",
        definition: "A word such as may, often, in most cases or under these conditions is left out, so a tentative or conditional finding reads as a certain one.",
        languages: Languages::Passage,
        source: "Sumner et al., The Association between Exaggeration in Health Related Science News and Academic Press Releases, BMJ 349, 2014",
    },
    Pattern {
        id: "scope.shifted_population",
        family: Family::Scope,
        name: "Shifted population or condition",
        definition: "A result is moved to a group, a setting or a condition other than the one the source studied, such as from mice to people or from adults to children.",
        languages: Languages::Passage,
        source: "Henrich, Heine and Norenzayan, The Weirdest People in the World?, Behavioral and Brain Sciences 33, 2010",
    },
    // Reasoning
    Pattern {
        id: "reasoning.correlation_as_cause",
        family: Family::Reasoning,
        name: "Correlation stated as cause",
        definition: "Two things the source reports as associated are said to be cause and effect, although the source gives no evidence that one produces the other.",
        languages: Languages::Passage,
        source: "Huff, How to Lie with Statistics, ch. 8, Post Hoc Rides Again",
    },
    Pattern {
        id: "reasoning.reversed_relation",
        family: Family::Reasoning,
        name: "Reversed relation or direction",
        definition: "The direction of a relation is turned round, so that what the source says increases, causes or precedes something is said to decrease, result from or follow it.",
        languages: Languages::Passage,
        source: "Damer, Attacking Faulty Reasoning, Questionable Cause",
    },
    Pattern {
        id: "reasoning.unsupported_mechanism",
        family: Family::Reasoning,
        name: "Unsupported mechanism",
        definition: "A reason for why something happens is added that the source does not give, so the explanation sounds complete but rests on a claim nothing supports.",
        languages: Languages::Passage,
        source: "Pearl and Mackenzie, The Book of Why, ch. 1, The Ladder of Causation",
    },
    Pattern {
        id: "reasoning.non_sequitur",
        family: Family::Reasoning,
        name: "Non sequitur",
        definition: "A conclusion is drawn that does not follow from the sentences before it, even if each of them is true.",
        languages: Languages::Passage,
        source: "Copi and Cohen, Introduction to Logic, Fallacies of Relevance",
    },
    // Definition
    Pattern {
        id: "definition.wrong_definition",
        family: Family::Definition,
        name: "Wrong definition",
        definition: "A term is defined in a way that differs from how the source defines it, so every later use of the term carries the wrong meaning.",
        languages: Languages::Passage,
        source: "Tippett, Refutation Text in Science Education, International Journal of Science and Mathematics Education 8, 2010",
    },
    Pattern {
        id: "definition.conflated_terms",
        family: Family::Definition,
        name: "Conflated terms",
        definition: "Two terms the source keeps apart, such as accuracy and precision, are treated as one or used for each other, so a distinction the subject depends on is lost.",
        languages: Languages::Passage,
        source: "Chi, Slotta and de Leeuw, From Things to Processes: A Theory of Conceptual Change for Learning Science Concepts, Learning and Instruction 4, 1994",
    },
    // Consistency
    Pattern {
        id: "consistency.self_contradiction",
        family: Family::Consistency,
        name: "Contradicts another sentence",
        definition: "A sentence says the opposite of another sentence of the same explanation, so the explanation cannot be right in both places.",
        languages: Languages::Passage,
        source: "Copi and Cohen, Introduction to Logic, the law of non-contradiction",
    },
];
