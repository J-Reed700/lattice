//! Query-term salience helpers.
//!
//! What remains of a larger dictionary-based query expander: the closed-class
//! stoplist and the salience ranking every retrieval path uses to pick which
//! of a question's words are worth searching for. The synonym dictionaries and
//! the expander itself were never wired into a search path and are gone.

pub mod dictionaries;
