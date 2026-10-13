# Prompt formatting and prompt engineering for local open-weight LLMs (4B-32B), state as of Oct 2026

Scope: Qwen 3.5 / Gemma / Llama / Mistral / gpt-oss class run via llama.cpp on Apple Silicon; RAG chat, web research, tools, summarisation, course generation. Evidence grade noted inline: [peer-rev], [preprint], [vendor], [practitioner], [older = pre-2025]. Several 2026 arXiv items were read at abstract/snippet level only; flagged "(abstract only)".

## 1. Does prompt format (XML vs Markdown vs plain vs JSON) measurably matter?

### Takeaway
Yes, and more for small/open models than for frontier closed ones, but no format wins universally; the measurable cost comes mostly from asking for structured OUTPUT, not from how INPUT sections are delimited. Pick one consistent delimiter style, test it on your exact model, and keep reasoning out of rigid output formats.

### Cited Findings
- [older] Sclar et al.: semantically equivalent formatting changes (separators, casing, spacing) swing few-shot accuracy by up to 76 points on LLaMA-2-13B; sensitivity persists with size and instruction tuning — [arXiv 2310.11324](https://arxiv.org/abs/2310.11324)
- [older, late 2024] He et al. (Microsoft): plain text vs Markdown vs JSON vs YAML templates moved GPT-3.5-turbo by up to 40% on code translation; GPT-4 far more robust; no single format best across tasks/models, and best format rarely transfers even within a family — [arXiv 2411.10541](https://arxiv.org/abs/2411.10541)
- [preprint 2026] "The Format Tax" (Lee, D'Antoni, Berg-Kirkpatrick; 6 open-weight + 4 API models; JSON/XML/LaTeX/Markdown output): requiring structured output "substantially degrades" reasoning and writing for open-weight models while most closed models show little/no tax; most of the loss comes from the format-requesting instruction itself, not the constrained decoder; decoupling (free-form first then reformat, or thinking enabled) recovers most of it (abstract only) — [arXiv 2604.03616](https://arxiv.org/abs/2604.03616)
- [preprint 2026] "Format Sensitivity Index" treats prompt-wrapper choice as a variance axis to report alongside decoding settings; notes prompt-only JSON instructions are brittle (snippet only) — [arXiv 2607.09665](https://arxiv.org/pdf/2607.09665)
- [vendor] Anthropic: "Separate prompt sections with XML tags or Markdown headers", but "exact formatting ... is likely becoming less important as models become more capable" — [Anthropic, Sep 29 2025](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- [vendor] Anthropic Claude docs recommend XML tags (`<instructions>`, `<context>`, `<input>`) to reduce misinterpretation in mixed prompts and "match your prompt style to the desired output" — [Claude prompting best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)
- [vendor] gpt-oss "will not work correctly" without the harmony format; instructions and tool definitions go in the developer message, reasoning effort in the system message; output split into analysis / commentary / final channels — [OpenAI harmony cookbook](https://developers.openai.com/cookbook/articles/openai-harmony)
- [vendor] Gemma instruction-tuned models have only `user` and `model` roles; system-level instructions go inside the first user turn — [Gemma prompt structure](https://ai.google.dev/gemma/docs/core/prompt-structure)
- [vendor] Qwen 3.5 trained on XML-style tool calls (`qwen3_coder` parser); Qwen's card gives no system-prompt or delimiter guidance — [Qwen3.5-27B card](https://huggingface.co/Qwen/Qwen3.5-27B)

### Inferences
- Anthropic's XML guidance is Claude-specific training advice; no study shows XML input delimiters beat Markdown for Qwen/Gemma. What transfers is the principle: unambiguous, consistent section boundaries; XML is a safe choice for document blocks because retrieved text often contains Markdown headers that would collide with Markdown-delimited sections.
- Chat-template correctness (roles, special tokens, Gemma's missing system role, harmony channels) matters far more than tag choice; a wrong template is a bigger error than a suboptimal delimiter.

### Gaps
- No controlled 2025-26 study of input-delimiter style (XML vs Markdown) on Qwen 3.5 / Gemma 3 specifically. Mistral's current system-prompt placement was not verified (search returned nothing authoritative).

## 2. Qwen 3.5 specifics

### Takeaway
Qwen 3.5 (Feb 2026) changes several Qwen3 habits: the `/think` `/no_think` soft switches are gone, thinking is toggled only via `enable_thinking` in the template, presence_penalty 1.5 is the default recommendation, prior-turn reasoning must be stripped, and tool calls use the Qwen3-Coder XML format, which several runtimes mis-wire.

### Cited Findings
- Thinking on by default for the 27B card, reasoning in `<think>`; disable via `chat_template_kwargs: {"enable_thinking": false}`; "Qwen3.5 does not officially support the soft switch of Qwen3" (`/think`, `/no_think`) — [Qwen3.5-27B card](https://huggingface.co/Qwen/Qwen3.5-27B)
- Conflict: Unsloth says reasoning is disabled by default for the small 0.8B-9B models and shows enabling it via llama-server template kwargs — [Unsloth Qwen3.5 guide](https://unsloth.ai/docs/models/qwen3.5); [Unsloth 9B GGUF discussion](https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/discussions/2)
- Sampling (official): thinking-general T=1.0, top_p 0.95, top_k 20, min_p 0, presence_penalty 1.5; thinking-precise-coding T=0.6, presence 0.0; non-thinking-general T=0.7, top_p 0.8, top_k 20, presence 1.5; non-thinking-reasoning T=1.0, top_p 0.95 (card's best-practices section gives top_p 1.0, top_k 40, presence 2.0 — internal inconsistency); repetition_penalty 1.0; high presence penalty can cause language mixing — [Qwen3.5-27B card](https://huggingface.co/Qwen/Qwen3.5-27B)
- Output budget: ~32,768 tokens for most queries, up to 81,920 for hard math/code — same card
- Multi-turn: history should contain only final answers, not thinking; the Jinja template does this, other frameworks must do it themselves — same card
- Context: native 262,144, extensible to ~1.01M via YaRN; static YaRN may hurt short-text performance so enable only when needed; keep >=128K to preserve thinking capability — same card
- Standardised output prompts: math "Please reason step by step, and put your final answer within \boxed{}."; multiple choice via an `"answer"` JSON field — same card
- [older, Qwen3] Qwen3 advised against greedy decoding (repetition) and supported `/think` `/no_think` — [Qwen3-8B card](https://huggingface.co/Qwen/Qwen3-8B)
- [practitioner] Ollama issue: Qwen 3.5 trained on Qwen3-Coder XML tool format (`<function=name><parameter=x>`), not Hermes JSON; unclosed `</think>` corrupted multi-turn tool prompts — [ollama #14493](https://github.com/ollama/ollama/issues/14493)
- [practitioner] llama.cpp issue: Qwen3.5-9B emits tool call as XML text then stops; worked with thinking disabled — [llama.cpp #20837](https://github.com/ggml-org/llama.cpp/issues/20837)
- [practitioner] Community "fixed" templates revert tool calls to native XML — [Qwen3.5 Fixed Chat Templates](https://huggingface.co/ansulev/Qwen3.5-Fixed-Chat-Templates)
- [practitioner, CN] Local Qwen3.5 pitfalls: no `developer` role in template, prior reasoning not hidden, string-typed tool args fail parsing, long tool descriptions bloat context; run llama.cpp with `--jinja --chat-template-file` — [CSDN write-up](https://adg.csdn.net/6a311754662f9a54cb7fcfc5.html)

### Inferences
- Any app sending `/no_think` to Qwen 3.5 is relying on unsupported behaviour; use the template kwarg per request.
- A single sampling profile for both modes is off-spec; llama.cpp's default presence_penalty (0) differs from Qwen's 1.5.

### Gaps
- No Qwen-official RAG prompt template, system-prompt length guidance, or reasoning-budget (thinking token cap) control found for Qwen 3.5. Qwen docs (qwen.readthedocs.io) were not fetched.

## 3. RAG prompting patterns

### Takeaway
Evidence first, question/instructions last; wrap each passage in a delimited block with an id and source metadata; ask for quotes before the answer on long inputs; expect small models to cite passages they did not use, so verify citations post hoc rather than trusting prompt wording.

### Cited Findings
- [vendor] Put long documents at the top, query at the end: "Queries at the end can improve response quality by up to 30 percent in tests, especially with complex, multidocument inputs"; wrap each document in `<document>` with `<source>` and `<document_content>`; "ground responses in quotes" (extract quotes into `<quotes>` then answer) — [Claude best practices, long context](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)
- [preprint 2026] "Lost in the Prompt Order": across 10 instruction-tuned decoder-only models, evidence before question beat question-before-evidence by +15.14 F1 in RAG with BM25 distractors; attributed to causal attention (snippet only) — [arXiv 2601.14152](https://arxiv.org/pdf/2601.14152)
- [older] Lost in the middle: accuracy highest when relevant info is at the start or end of context — [Liu et al. 2307.03172](https://arxiv.org/abs/2307.03172)
- [practitioner/vendor research, Jul 2025] Chroma "Context Rot" (18 models incl. Qwen3): reliability falls with input length even on trivial retrieval/copy tasks; single distractors hurt unevenly; shuffled haystacks beat coherent ones — [Chroma](https://research.trychroma.com/context-rot) (primary not fetched; via [summary](https://www.beri.net/learning/chroma-context-rot-report))
- [preprint 2026] Citation prompt requiring one bracketed citation per sentence + "I don't know" fallback still yields "post-rationalization": models cite passages they never used — [arXiv 2609.23053](https://arxiv.org/html/2609.23053v1)
- [peer-rev 2025] Citation correctness != faithfulness; a correct-looking citation may not reflect the passage actually used — [Wallat et al. ICTIR 2025](https://staff.fnwi.uva.nl/m.derijke/wp-content/papercite-data/pdf/wallat-2025-correctness.pdf)
- [practitioner] Smaller/faster models struggle with citation adherence and leak outside knowledge — [Woolridge 2025](http://www.danieljwoolridge.com/blog/2025/4/28/the-crucial-hand-off-integrating-retrieval-results-with-llms-in-rag)
- [vendor, older Sep 2024] Contextual retrieval (prepend 50-100 tokens of LLM-written chunk context before embedding/BM25) cut failed retrievals 35% / 49% with BM25 / 67% with reranking — [Anthropic](https://www.anthropic.com/news/contextual-retrieval)

### Inferences
- Contextual retrieval is an indexing-time prompt, model-agnostic; a 4B-9B local model can write chunk context, cost is ingest time not quality of the chat model (untested here).
- With lost-in-the-middle plus causal-order effects, place best passages first in the evidence block and the question + output rules after it (also cache-friendly, see 5).
- Short numeric ids (`[1]`, `[S3]`) are the cheapest marker for small models to copy; verification should be programmatic.

### Gaps
- No controlled study comparing citation-marker formats or abstention wording on 4B-30B open models; no Qwen-specific RAG prompt guidance found.

## 4. Few-shot, instruction placement, negatives, roles, CoT vs native thinking

### Takeaway
For thinking models: zero-shot, high-level goals, no "think step by step" boilerplate. For non-thinking calls: CoT and prompt repetition still help. Positive instructions with reasons beat bare prohibitions; shouting (CRITICAL/MUST) over-triggers newer models.

### Cited Findings
- [peer-rev/preprint 2025] Wharton Prompting Science Report 2: explicit CoT gives reasoning models only marginal gains with 20-80% more time; still helps non-reasoning models on average — [SSRN 5285532](https://papers.ssrn.com/abstract=5285532)
- [peer-rev ICML 2025] "Mind Your Step (by Step)": CoT/reasoning drops accuracy up to 36.3% absolute (o1-preview vs GPT-4o) on tasks where deliberation hurts humans — [arXiv 2410.21333](https://arxiv.org/html/2410.21333v1)
- [peer-rev NeurIPS 2025] Reasoning models often underperform non-reasoning counterparts on inductive rule learning; reasoning amplifies errors — [Jin et al.](https://mlanthology.org/neurips/2025/jin2025neurips-evaluating)
- [preprint 2025] DeepSeek-R1: "few-shot prompting consistently degrades its performance"; recommend zero-shot with direct problem + output format — [arXiv 2501.12948](https://arxiv.org/abs/2501.12948)
- [vendor] Anthropic: "think thoroughly" often beats a hand-written step plan; worked multishot examples still shape thinking; 3-5 diverse examples in `<example>` tags; "Tell Claude what to do instead of what not to do"; give motivation; dial back "CRITICAL: You MUST" (overtriggers) — [Claude best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)
- [vendor] Few-shot: "a small set of diverse, canonical examples" rather than a laundry list of edge cases — [Anthropic context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- [practitioner] Manus: repeated similar examples make agents fall into a "few-shot rut"; inject structured variation — [Manus blog](https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus)
- [preprint Dec 2025] Google "Prompt Repetition": repeating the input prompt twice wins 47/70 tests with 0 losses for non-reasoning use, no extra output tokens; little benefit with reasoning (tested on API models: Gemini, GPT-4o, Claude, DeepSeek V3) — [arXiv 2512.14982](https://arxiv.org/abs/2512.14982)
- [vendor] Qwen 3.5 still recommends "reason step by step" phrasing in standardized math prompts — [Qwen3.5-27B card](https://huggingface.co/Qwen/Qwen3.5-27B)

### Inferences
- In Qwen 3.5 non-thinking mode (e.g., query rewriting, classification), restating the key instruction after the evidence is the cheap analogue of prompt repetition; untested on small open models.
- Role prompting: vendor-endorsed for tone only; I found no 2025-26 evidence it improves accuracy on open models.

### Gaps
- No SLM-specific (4B-30B) replication of prompt repetition or few-shot-vs-zero-shot with Qwen 3.5 thinking mode found.

## 5. Prompt layout for KV-cache reuse

### Takeaway
Stable, byte-identical prefix (system prompt, tool schemas) first; append-only history; volatile content (date, retrieved evidence, the query) last; deterministic serialization. On llama.cpp this is the difference between re-prefilling thousands of tokens per turn and reusing them.

### Cited Findings
- [practitioner] Manus: KV-cache hit rate is "the single most important metric" for production agents; cached vs uncached input differs 10x in API price; keep prefix stable (no timestamps at the start), append-only context, deterministic JSON key ordering; mask tools via decoding constraints rather than removing them mid-loop (removal invalidates cache); recite goals (todo.md) at the end of context; keep errors in context — [Manus blog](https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus)
- [vendor] llama-server reuses the prompt prefix per slot (`cache_prompt`, on by default) and offers `--cache-reuse` for chunk reuse via KV shifting — [llama.cpp server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)
- [vendor] Qwen 3.5 template strips prior thinking from history — [Qwen3.5-27B card](https://huggingface.co/Qwen/Qwen3.5-27B); harmony drops prior CoT once a turn ends on the final channel — [harmony cookbook](https://developers.openai.com/cookbook/articles/openai-harmony)

### Inferences
- Template-side thinking stripping rewrites earlier assistant turns, so the cached prefix ends before the last assistant message each turn; budget for re-prefill of the last turn.
- Retrieved evidence placed in the system prompt breaks the cache every turn; put it in the latest user turn.

### Gaps
- No vendor (Qwen/Google/Meta) guidance on cache-friendly prompt layout found; no measurements of local prefill savings on Apple Silicon.

## 6. Structured output prompting

### Takeaway
Grammar-constrained decoding fixes syntax for small models but does not fix (and can worsen) answer quality; the tax is mostly from forcing reasoning into the format. Think first (native thinking or a free-text field/pass), emit the schema last.

### Cited Findings
- [peer-rev EMNLP 2024, older] "Let Me Speak Freely?": JSON mode cut Llama-3-8B-Instruct GSM8K exact match by 17% and up to 63% on Last Letter — [ACL Anthology](https://aclanthology.org/2024.emnlp-industry.91.pdf)
- [preprint 2026] "Constraint Tax" (SLMs): answer-only hard schema decoding raised validity 61.5% -> 100% but cut accuracy 19.7% -> 11.0%; parse-success metrics mislead (abstract only) — [arXiv 2605.26128](https://arxiv.org/html/2605.26128v1)
- [preprint 2026] "Repair, Not Improvement": JSON instruction alone costs ~-3.9 points; decode-time enforcement ~-1.6 more (snippet only) — [arXiv 2608.13959](https://arxiv.org/pdf/2608.13959)
- [preprint 2026] Constrained decoding eliminates structural failures in small LLMs (baseline schema validity 78.6-92.9%) but leaves a scale-dependent semantic gap (snippet only) — [arXiv 2609.23742](https://arxiv.org/pdf/2609.23742)
- [preprint 2026] Constrained decoding adds 3.6x-8.2x latency in some settings and can degrade task performance (snippet only) — [arXiv 2605.02363](https://arxiv.org/html/2605.02363v1)
- [preprint 2026] Format Tax mitigation: free-form then reformat, or enable thinking — [arXiv 2604.03616](https://arxiv.org/abs/2604.03616)

### Inferences
- Schema field order matters: put a `reasoning`/`evidence` string before decision fields, or let Qwen think in `<think>` and constrain only the final answer.

### Gaps
- Whether llama.cpp grammar applies inside Qwen `<think>` blocks with the reasoning parser was not verified.

## 7. SLM-specific findings and 2025-26 context engineering

### Takeaway
Small open models pay more for format, distractors and long context than frontier ones; "context engineering" practice is: smallest high-signal context, just-in-time retrieval, compaction, notes, sub-agents with clean windows.

### Cited Findings
- [vendor] Anthropic: smallest set of high-signal tokens; system prompt at "right altitude" (no brittle if-else, no vague guidance); start minimal on the best model and add for observed failures; just-in-time retrieval via references; compaction (maximize recall then precision; clearing old tool results is the low-risk first step); structured notes; sub-agents return ~1-2k-token summaries — [Anthropic, Sep 2025](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- [preprint 2026] Format tax concentrated in open-weight models — [arXiv 2604.03616](https://arxiv.org/abs/2604.03616)
- [preprint 2025] "Don't Adapt Small Language Models for Tools; Adapt Tool Schemas to the Models" (title only seen) — [arXiv 2510.07248](https://arxiv.org/pdf/2510.07248)
- [practitioner] Context rot affects all tested models including Qwen3 — [Chroma](https://research.trychroma.com/context-rot)

### Inferences
- The "test on the best model first" advice is cloud-centric; for a local app, test on the smallest shipped tier, which is where format/position sensitivity bites.

### Gaps
- LangChain and Cognition 2025-26 context-engineering posts were not fetched within budget.

## 8. Tool description and tool-result formatting

### Takeaway
Few, non-overlapping tools; unambiguous parameter descriptions; token-efficient results; and, for Qwen 3.5, the native XML call format through the official template.

### Cited Findings
- [vendor] If a human can't tell which tool applies, the agent can't; tools self-contained, error-robust, token-efficient — [Anthropic](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- [vendor] gpt-oss: tool definitions in the developer message; tool calls on the commentary channel — [harmony cookbook](https://developers.openai.com/cookbook/articles/openai-harmony)
- [vendor] Qwen 3.5: `qwen3_coder` parser; Qwen-Agent recommended — [Qwen3.5-27B card](https://huggingface.co/Qwen/Qwen3.5-27B)
- [practitioner] Qwen 3.5 string-typed args break parsers; long tool descriptions consume context — [CSDN](https://adg.csdn.net/6a311754662f9a54cb7fcfc5.html)
- [practitioner] Manus: mask unavailable tools via constrained decoding/prefill rather than editing tool list; consistent tool name prefixes (e.g., `browser_`) — [Manus blog](https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus)

### Gaps
- BFCL leaderboard/blog guidance on tool-description style was not fetched; no measured evidence on JSON vs text tool-result formatting for small models found.
