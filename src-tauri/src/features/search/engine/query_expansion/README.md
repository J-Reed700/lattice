# Query-Term Salience (`query_expansion`)

Despite the module name, nothing here expands queries any more. The
dictionary-based synonym expander (`QueryExpander`, `QueryExpansionConfig`,
domain and user synonym dictionaries) was never wired into a search path and
has been deleted. What remains is the helper every retrieval path uses to pick
which words of a question are worth searching for.

## Structure

```
query_expansion/
├── mod.rs
└── dictionaries/
    ├── mod.rs          # re-exports select_informative_terms
    └── stopwords.rs    # stoplist, is_stopword, select_informative_terms
```

## API

```rust
use crate::features::search::engine::query_expansion::dictionaries::select_informative_terms;

let terms = vec!["What".into(), "is".into(), "going".into(), "on".into(), "with".into(), "NASA".into()];
let picked = select_informative_terms(terms, 3); // keeps "nasa", drops function words
```

- `is_stopword(term)`: membership in a closed-class stoplist (articles,
  pronouns, auxiliaries, contentless modifiers). Domain nouns are never listed.
- `select_informative_terms(terms, max_terms)`: lowercases and de-duplicates,
  drops terms shorter than two characters, terms with no letters, and
  stopwords, then ranks the rest by
  a character-entropy and length salience score and returns at most
  `max_terms`.

The stoplist pass is deliberate: scoring by entropy and length alone kept long
adverbs ("specifically") and dropped short subjects ("NASA").

## Callers

- `features/conversation/chat/retrieval/mod.rs`
- `features/conversation/chat/retrieval/external_query.rs`
- `features/conversation/chat/prior_evidence.rs`
- `features/qa/hyde/hyde_service.rs`

## Testing

```bash
cd src-tauri
cargo test --lib query_expansion
```
