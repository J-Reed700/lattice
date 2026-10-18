# Source-finding eval: label vocabulary

The words a labeller uses in `primary-sources-v1.jsonl`, and what each means.
The labelling rule itself (how a labeller decides, the blind re-label
protocol) is written with the set, in this folder's `README.md`. The names
below are the ones the code uses (`<core>/src/domain/sourcing/`), so a label,
a log line and a tool result spell a thing the same way.

Design: `docs/design/2026-10-18-source-finding-toolbelt.md`.

## Need kinds (`need.kind`)

| Label | Asks for | Example query |
|---|---|---|
| `web` | pages about a question (the chat default) | `retry-after header semantics` |
| `official_docs` | the subject owner's own documentation | `AsyncLocal<T> value flow across await` (subject `.NET`) |
| `package` | a package's registry record, repository and docs | `tokio` (identifier `crates:tokio`) |
| `paper` | a paper by title, DOI or arXiv id, or a measurement it reports | `Attention is all you need` |
| `standard` | a standard from its body (RFC, W3C, WHATWG, ECMA) | `RFC 9110 Retry-After` |
| `law` | a law or regulation from its official publisher | `GDPR article 17` (identifier `celex:32016R0679`) |
| `community` | a question-and-answer or discussion thread | `serde flatten with deny_unknown_fields` |
| `encyclopedia` | an encyclopedia article and its references | `Byzantine fault tolerance` |
| `archive` | a dated archived copy of a URL | a URL plus `as_of` |
| `leads` | the links a page makes, followed to what it cites | a URL plus `near_text` |

## Subject (`need.subject`)

The entity that owns the answer: `name` plus `kind`, one of `software`,
`organisation`, `paper`, `law`, `standard`, `person`, `other`.

Identifiers, as the `find_sources` tool spells them: a DOI (`10.1145/…`), an
arXiv id (`2310.06825`), `crates:<name>`, `pypi:<name>`, `npm:<name>`,
`nuget:<name>`, `rubygems:<name>`, `maven:<group>:<artifact>`, `go:<module>`,
`rfc:<number>`, `celex:<number>`, a Wikidata id (`Q…`), or a URL.

## Gold (`gold`)

- **owner domain** (`owner_domains`): a host, optionally with a path prefix
  (`github.com/dotnet`), that the subject's owner controls. A lead under it
  counts as on the owner's origin; subdomains count, sibling paths do not
  (`github.com/dotnet` does not cover `github.com/dotnetfoundation`).
- **primary URL** (`primary_urls`): a page that *is* the source for the need,
  published by the owner: the API reference, the registry record, the
  publisher's DOI landing page, the RFC on rfc-editor.org, the official
  journal's text of a law.
- **mirror** (`mirrors`): a host that republishes the owner's text without
  being the owner (`learn-microsoft.com`, a scraped docs copy). A mirror in
  the top k is a failure even when the text is identical.
- **archived copy**: a Wayback Machine URL of a page. It scores as the page
  it archives (the same canonical key), never as a mirror.

Two URLs are the same lead when their canonical keys match: case, default
port, fragment, trailing slash, tracking parameters (`utm_*`, `gclid`,
`fbclid`, …), `http`/`https` and Wayback wrapping are ignored, and a page's
declared `rel=canonical` stands for it.

## Lead kinds (what a finder says a lead is)

`page`, `official_site`, `document` (with its identifier), `repository`,
`reference_of` (a link found on another page), `archived_copy`. When fused
scores tie, `document` > `official_site` > `repository` > `page` =
`reference_of` > `archived_copy`.

## Finder results (in `results/` and the outcome lines)

- `answered`: returned at least one lead.
- `empty`: ran and returned none.
- `failed`, with a cause: an HTTP status (with `Retry-After` when sent), a
  bot challenge, a transport failure, an unreadable response, rate-limited
  until a time, or out of time.
- `skipped`, with a reason: turned off, no key, no instance URL, needs a
  contact, paused (breaker open) until a time, allowance spent until a time,
  not applicable, out of budget.

A need is **exhausted** when every applicable finder answered, came back
empty, failed or was skipped.

## Metrics vocabulary

- **primary recall@k**: a gold primary URL, or any URL under a gold owner
  domain, is in the fused top k.
- **owner-hit rate**: owner resolution returned a gold owner domain.
- **mirror rate**: a gold mirror is in the fused top k.
- **time to first primary**: seconds from the need to the first primary lead.
- **requests per need**: provider requests the need spent.
