# Labelling rule (shared by perturbation writers and labellers)

Each item is one claim and one passage. The passage is a verbatim span of a real
web page (documentation, blog, tutorial). Judge entailment from the passage
alone. Outside knowledge never counts, for or against: a claim that is true in
the world but absent from the passage is unsupported.

| label | meaning |
| --- | --- |
| `supported` | Every factual part of the claim (numbers, versions, dates, units, names, negations, scope, qualifiers) is stated in the passage or follows from it necessarily. A faithful paraphrase is supported. So is arithmetic on stated values. |
| `contradicted` | The passage states something incompatible with the claim, or explicitly rules it out. |
| `unsupported` | Neither: the passage is silent on at least one part of the claim. One clause backed and another missing is unsupported. |

Close calls:
- A value carried across a scope boundary (the passage gives it for a different
  version, product, setting or case) is unsupported, not contradicted, unless the
  passage says it applies only there.
- Evaluative or advisory wording ("significant", "critical", "best practice",
  "you should", "important for interviews") is a factual part. Supported only if
  the passage says it or something that necessarily implies it.
- Over-generalised scope ("always", "all", "guarantees", "never", dropping a
  stated condition) is unsupported when the passage states the narrower scope
  without ruling out the broader one, contradicted when the passage states an
  exception that rules it out.
- An unstated causal link ("because", "so that", "which is why") between facts
  the passage states separately is unsupported.
- Page chrome in the passage (links, headings, navigation text) is still passage
  text and may support a claim if it states the fact.

Difficulty tags (use only these; one or more per item):
verbatim, paraphrase, number_drift, date_drift, unit_drift, negation, reversal,
wrong_entity, partial, multi_clause, multi_sentence, world_knowledge,
distractor_numbers, lexical_trap, causal_addition, scope, hedge, table,
arithmetic, noisy_passage, cross_lingual

- `partial` is for one clause supported and one missing (label unsupported, or
  supported when the extra clause turns out to be backed). A clause the passage
  rules out makes the item contradicted and is tagged `multi_clause` instead.
- `world_knowledge`: true or plausible in the world, absent from the passage.
- `lexical_trap`: high word overlap with the passage but a different meaning.
- `distractor_numbers`: the passage holds several numbers or versions and the
  claim uses the right one or a neighbour's.
