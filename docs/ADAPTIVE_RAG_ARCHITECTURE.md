# Adaptive RAG Architecture (Router-Optional)

## Goal

Provide a modern, low-latency, citation-grounded architecture for Recall that:

- Works reliably for first-turn conversations with no prior document context.
- Avoids unnecessary orchestration complexity in the default path.
- Keeps advanced routing and multi-agent behavior available as optional extensions.

## Principles

1. Deterministic policy gates before LLM orchestration.
2. Retrieval quality and citation grounding over speculative routing.
3. Optional router, not mandatory router.
4. Fail-open to useful behavior: when uncertain, search and answer conservatively.

## Default Runtime Flow

1. Scope gate
- Apply space and collection constraints first.

2. Retrieval
- Run hybrid retrieval with semantic + lexical signals.
- Apply rerank/document support filters.

3. Confidence policy
- If confidence is strong, answer with citations.
- If confidence is weak and no recent document exists, do not clarify about "previous document"; continue with search fallback.
- If confidence is weak and a valid recent document exists, clarification is allowed.

4. Generation
- Produce grounded answer when sources exist.
- Use no-context template for general guidance when sources are insufficient.

5. Verification
- Attach grounding metadata and citation checks.

## Router Positioning

- Router is optional and disabled by default.
- Router may be enabled for advanced follow-up disambiguation in conversations with valid recent document context.
- Router should not run when recent document metadata cannot be resolved.

## Phase 1 Implementation (Completed)

- Router disabled by default in settings.
- Router validation no longer requires `router.enabled = true`.
- Conversation flow bypasses router when:
  - router is disabled, or
  - there is no resolvable recent document.
- Clarify responses for no-recent-document scenarios use generic guidance instead of "previous document".

## Future Phases

1. Add explicit policy telemetry
- Emit metrics for fallback reason (`no_recent_doc`, `low_confidence`, `router_disabled`).

2. Improve confidence policy
- Use calibrated thresholds from offline evaluation datasets.

3. Optional orchestration mode
- Introduce agentic routing only for multi-source/multi-agent scenarios.

4. Continuous eval
- Track groundedness, citation precision, abstention quality, latency, and cost per answer.
