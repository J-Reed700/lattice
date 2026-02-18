//! Dictionary modules for query expansion.
//!
//! Provides domain-specific synonyms, term-signal helpers, and user-defined synonym management.

pub mod domain;
pub mod stopwords;
pub mod user_synonyms;

pub use domain::load_domain_dict;
pub use stopwords::select_informative_terms;
pub use user_synonyms::{get_user_synonyms_path, load_user_synonyms, save_user_synonyms};
