//! Dictionary modules for query expansion.
//!
//! Provides term-signal helpers shared by chat retrieval and HyDE.

pub mod stopwords;

pub use stopwords::select_informative_terms;
