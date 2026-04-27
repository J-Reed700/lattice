# Chat — Voice & Microcopy Audit

**Scope:** all user-facing text in the Chat feature (composer, thread, sidebar, spotlight, citations, source preview, linked documents).
**Goal:** make copy match the app's aesthetic — quiet, editorial, precise — and stop sounding like shipped-at-2am placeholder text.

---

## 1. Voice diagnosis

The current copy is not *bad* — it is *unauthored*. It reads like a working draft that nobody has done a pass on. The register changes from panel to panel: the sidebar says "No conversations yet." while the composer empty state says "This conversation is empty." while ChatPanel's empty state says "No conversation selected" and offers a capitalized CTA "New Conversation" — three different registers (terse, declarative, sentence-fragment-plus-Title-Case-button) in three adjacent surfaces.

There is no single voice. Instead there are three tics that keep leaking in:

1. **Engineering-speak.** "Invalid hex color", "Failed to load file", "No evaluable claims for this message.", "Force KB retrieval for this turn", "URL sources linked to this conversation context. Ingest to index." This is copy written from inside the state machine, not from the user's side of the glass.
2. **Over-explaining.** "Pick a conversation from the sidebar, or start a new one." "Start a new conversation above." "No journals yet. Create one to organize selected conversations." The interface is already showing the user a sidebar, a "New Conversation" button, and a form — narrating what they see is noise.
3. **Inconsistent capitalization and terminology.** "New Conversation" (Title Case button) sits next to "Saved References" (Title Case section) sits next to "Space Scope" (Title Case sub-label) sits next to "Turn mode" / "Tools" (Sentence case). "KB" / "Knowledge base" / "Knowledge Capture" all coexist. "References" is a filter tab, "Reference Inbox" is a route, and "Saved References" is a header — three capitalizations for one concept.

What it lacks, relative to the aesthetic guide:

- **Confidence.** Linear-grade copy is declarative; ours apologizes and explains. Empty states should state what this space *is*, not instruct the user on what to click.
- **Precision of register.** Every place the app says "can't" vs "cannot", "failed to X" vs "couldn't X", sentence-case vs Title-case, "chat" vs "conversation" — the inconsistency itself reads as inattention.
- **A point of view on domain terms.** "Thread" never appears, but the component named `ChatPanel` and the store named `conversationsStore` hint at an unresolved debate. We should pick "conversation" (the API contract already has) and use it everywhere the user sees chrome, reserving "message" for the unit inside.
- **Empty states with any soul.** Empty states are the cheapest opportunity to set voice — ours all say some version of "No X yet." with a generic icon. That is Bootstrap-admin copy.

The fix is not a rewrite — it is a pass. Pick a register (sentence case, declarative, no exclamation marks), pick a vocabulary, and apply it.

---

## 2. Terminology audit

### Canonical vocabulary — Chat

| Term | Meaning | Use where |
|---|---|---|
| **Conversation** | A single chat thread (one or more messages, one topic, one title). | Sidebar items, "New conversation", page labels. The API already calls this `Conversation` — mirror it. |
| **Message** | One turn inside a conversation (either `You` or `Assistant`). | Message actions, message count, "Delete message". |
| **Assistant** / **You** | The two roles in a message header. | Header labels only. Never "AI" in UI chrome; "AI" is too fuzzy and is already ambiguous with the "AI"/"You"/"System" filter in the References panel. |
| **Space** | A scoped context (documents, defaults, system prompt) that groups conversations. | Sidebar space scope, space editor, "All spaces". |
| **Journal** | A space kind that lives on a calendar, with daily entries. | Only where the user is in journal scope. Do not mix "Journal" and "Space" in the same label without "Journal space" as the disambiguator. |
| **Source** | A document or web page the assistant cited. | Citation footnotes, source list. |
| **Citation** | The numbered inline marker `[1]` linking to a source. | Tooltip and aria-labels only; not user-facing body text. |
| **Reference** | A bookmarked message saved for later capture in the Reference Inbox. | Sidebar filter tab, snippet detail, "Save reference". |
| **Bookmark** | A conversation-level marker (the star/flag on a conversation in the sidebar). | Use "Bookmarked" only for conversation-level state. |
| **Save** (verb) | The *action* of bookmarking a message as a reference. Past tense: "Saved". | Message action row. |
| **Tool** | An optional retrieval/augmentation the assistant can invoke (web, KB, wiki, custom). | Composer controls popover. |
| **Turn mode** | The pre-committed routing mode for the next message (Auto / Follow-up / Query). | Composer controls popover. |
| **Deep research** | A multi-step tool mode (slower). Always lowercase when used in body; proper-noun-cased only when used as a feature name. | Composer controls, warning toast. |

### Inconsistencies to fix

- **"Chat" never surfaces in UI copy, but the feature is called Chat internally.** Good — keep it that way. Do not let "chat" leak into labels.
- **"Thread" is absent. Good — do not reintroduce it.**
- **"Saved / Saved References / References" (three capitalizations for one concept).** Unify: the filter tab is `References`, the section header is `Saved references`, the action button is `Save`, the verb past tense is `Saved`.
- **"Bookmark" vs "Save" collide at the conversation level.** Today, a sidebar conversation has *both* a `Star` (called `Save`) and a `Bookmark` (called `Bookmark`), and there is *also* a message-level "Save" / "Saved" action that really means "Add to References". Three different controls are all called some flavor of "save/star/bookmark". Proposal: the conversation-level star becomes **Pin to favorites** (sentence in tooltip: "Pin to favorites") OR we drop one of Save/Bookmark entirely. This is a P0 clarity bug, not just a copy issue — flag for Josh.
- **"Journal" vs "Notebook" vs "Notebook Mode".** The sidebar says "Notebook Mode" in one place and "Notebook" in another, for the same feature. Unify to `Journal` in user-facing copy. "Notebook" is an implementation detail.
- **"KB" vs "Knowledge base".** `KB Default` button, `Knowledge base` label, `Force KB retrieval for this turn` description. Use **Knowledge base** everywhere user-visible; `KB` is acceptable only inside dev tooltips, which we don't need.
- **"Space" vs "Spaces" vs "All Spaces" (Title Case).** "All Spaces" and "Space Scope" are Title Case in the sidebar while "Turn mode" and "Tools" are sentence case two panels away. Unify to sentence case: `All spaces`, `Space scope`, `Per-space context`.
- **"Cited source" vs "Source" vs "Web source".** The `SourceCitations` summary says "3 cited sources · 7 excerpts". Keep it simple: `3 sources · 7 excerpts` (the fact that they are *cited* is implied by the UI context).

---

## 3. Findings — full table

Legend: G = Good, W = Weak, M = Missing, T = Wrong tone.
"Priority" column: **P0** = wrong, misleading, or absent and needs a rewrite; **P1** = weak copy that a tier-1 app would not ship; **P2** = polish pass.

### ChatPanel.tsx

| Location | Current | Grade | Proposed | Why | Priority |
|---|---|---|---|---|---|
| `ChatPanel.tsx:383-388` placeholder (auto) | `message...` | W | `Ask anything` | Lowercase sentence fragment with trailing ellipsis is the weakest placeholder convention. "Ask anything" (Notion, Perplexity) is specific about the verb and invites. | P1 |
| `ChatPanel.tsx:383-388` placeholder (followup) | `follow up on this conversation...` | W | `Follow up...` | Verbose and grammatically awkward (starts with "follow" as imperative but then describes). Be brief. | P1 |
| `ChatPanel.tsx:383-388` placeholder (query) | `search and answer...` | T | `Search sources and answer` | Current sounds like a command to the machine, not a hint to the human. Drop the ellipsis; placeholders should not pretend to be continuations. | P1 |
| `ChatPanel.tsx:408-411` empty state title | `No conversation selected` | T | `No conversation open` | "Selected" is engineering language (what the app sees); "open" is what the user thinks. | P2 |
| `ChatPanel.tsx:410-412` empty state body | `Pick a conversation from the sidebar, or start a new one.` | W | (delete entirely, keep button only) OR `Open one from the sidebar, or start fresh.` | Over-explains visible UI. The sidebar is right there. Prefer silence; if we must say something, make it shorter. | P2 |
| `ChatPanel.tsx:421` CTA button label | `New Conversation` (Title Case) | T | `New conversation` | Title Case for buttons is out of step with the sidebar's own buttons further down (some are Title Case, some are not) and with the Linear/Vercel register. Sentence case everywhere. | P0 |
| `ChatPanel.tsx:417` aria-label | `Create new conversation` | G | keep | Good — matches proposed visible label in intent. | — |
| `ChatPanel.tsx:437-442` empty-thread state | `This conversation is empty.` / `Ask a question to begin.` | W | `Empty conversation.` / `Type anything to start.` — or delete the subtitle entirely. | "Ask a question to begin" is two statements of the obvious (there's a composer right below, already placeholder-hinting). Kill subtitle. | P1 |
| `ChatPanel.tsx:475` textarea aria-label | `Message input` | W | `Message composer` | "Input" is DOM terminology; screen reader users deserve "composer". | P2 |
| `ChatPanel.tsx:485` controls trigger aria-label | `Composer controls` | G | keep | Good. | — |
| `ChatPanel.tsx:486` controls trigger title | `Turn mode, tools, model` | W | `Turn mode and tools` | There is no model picker here (the code lies about what this popover contains — see ComposerControls). Also sentence case. | P0 |
| `ChatPanel.tsx:524-525` stop button aria/title | `Stop generation` / `Stop generation` | G | `Stop` (title), `Stop generating response` (aria) | Title on an icon button should be short because tooltips truncate at ~~6 words visually; aria-label can be longer. | P2 |
| `ChatPanel.tsx:534-535` send button | aria: `Send message`, title: `Send message (Enter)` | W | aria: `Send message`, title: `Send · Enter` | `Enter` in parentheses is fine but a bullet-separated shortcut reads cleaner and matches the footer below. See §5. | P1 |
| `ChatPanel.tsx:543` shortcut hint | `⌘⏎ to send · ⇧⏎ for new line` | T | `Enter to send · Shift + Enter for new line` | The current shortcut is wrong: code at line 371-378 sends on plain Enter, AND on ⌘+Enter. The hint says only ⌘⏎, which is misleading — most users will hit Enter and be surprised when it sends without the modifier. Pick one behavior (I recommend Enter to send, ⇧Enter for newline) and document that. See §5 for full shortcut standard. | **P0 — bug** |

### Message.tsx

| Location | Current | Grade | Proposed | Why | Priority |
|---|---|---|---|---|---|
| `Message.tsx:190` role label | `You` / `Assistant` | G | keep | Good, consistent, classic. | — |
| `Message.tsx:195` status | `Sending` | G | keep | — | — |
| `Message.tsx:201` fallback error | `Failed to send` | W | `Message didn't send` | "Failed" is a technical register; "didn't send" is human. Same information. | P1 |
| `Message.tsx:96-98` badge label | `Verification off` | G | keep | — | — |
| `Message.tsx:103` badge label | `No verifiable claims` | W | `Nothing to verify` | Current sounds like the claims are *unverifiable*, which is different. Proposed is neutral. | P1 |
| `Message.tsx:111` badge label | `Verified` | G | keep | — | — |
| `Message.tsx:118` badge label | `Partially verified (3)` | W | `3 unverified` or `Partially verified · 3` | Badge count in parens is awkward when it gets long. Use a bullet. | P2 |
| `Message.tsx:218-222` badge title tooltips | `Grounding verification is disabled in settings.` / `Click to view verification details.` / `No evaluable claims for this message.` | T | `Verification is off. Turn on in settings.` / `Show verification details.` / `Nothing in this message to verify.` | "Grounding verification" is a feature name nobody outside the codebase uses. "Click to" violates the no-instructional rule. | P1 |
| `Message.tsx:269` panel label | `Verified: 3` | G | keep | — | — |
| `Message.tsx:273` panel label | `Unverified: 1` | G | keep | — | — |
| `Message.tsx:276` panel label | `Coverage 82% (5 claims)` | W | `82% coverage · 5 claims` | Percentage-first reads better; parens is for asides. | P2 |
| `Message.tsx:283` section header | `Verified Claims` (Title Case) | T | `Verified claims` | Sentence case. | P1 |
| `Message.tsx:318` section header | `Unverified Claims` (Title Case) | T | `Unverified claims` | Sentence case. | P1 |
| `Message.tsx:303-305` expand link | `Show all verified claims (5)` / `Show fewer verified claims` | W | `Show all 5` / `Show fewer` | Redundant repetition of "verified claims" when we're under a `Verified claims` header. | P2 |
| `Message.tsx:310-312` empty | `Verified claim text is not available for this message.` | T | `No verified claim text available.` | Strip "for this message" — the context is obvious. | P2 |
| `Message.tsx:345-347` empty | `All evaluated claims were grounded in the cited context.` | T | `Every claim is grounded.` | Current is passive-voice, jargon-heavy ("grounded in the cited context"). | P1 |
| `Message.tsx:151` confirm dialog | `Delete this message from the conversation? This cannot be undone.` | W | `Delete this message? This can't be undone.` | Drop "from the conversation" (where else would it be deleted from?). `can't` reads warmer; both are fine. | P2 |

### MessageActions.tsx

| Location | Current | Grade | Proposed | Why | Priority |
|---|---|---|---|---|---|
| `MessageActions.tsx:35` aria-label | `Copy message to clipboard` | G | keep | — | — |
| `MessageActions.tsx:41, 46` visible label | `Copy` / `Copied` | G | keep | — | — |
| `MessageActions.tsx:54` aria-label | `Save message as reference` / `Remove saved reference` | W | `Save to references` / `Remove from references` | The actions *add to* / *remove from* a place called "References" (sidebar filter). "Save message as reference" is accurate but doesn't point at the destination. | P1 |
| `MessageActions.tsx:62-63` visible label | `Save` / `Saved` | W | `Save` / `Saved` — keep, BUT see terminology audit: this collides with conversation-level "save". Safer long-term: label the message-level action **`Reference`** / **`Referenced`** with the bookmark icon, reserving "save" for conversation-level. | Collision with sidebar's conversation-level "Save" star. | P1 |
| `MessageActions.tsx:69` aria-label | `Delete message` | G | keep | — | — |
| `MessageActions.tsx:73` visible label | `Delete` | G | keep | — | — |

### ComposerControls.tsx

| Location | Current | Grade | Proposed | Why | Priority |
|---|---|---|---|---|---|
| `ComposerControls.tsx:53` header | `Turn mode` | G | keep (sentence case ✓) | — | — |
| `ComposerControls.tsx:60` option desc | `Let the assistant infer whether this is a new topic or follow-up` | W | `Auto-detect new topic or follow-up` | 13 words → 5. | P2 |
| `ComposerControls.tsx:61` option desc | `Treat this turn as context-dependent` | T | `Continue the current topic` | "Context-dependent" is implementation jargon. What the user actually wants is "continue". | P1 |
| `ComposerControls.tsx:62` option desc | `Force retrieval mode (KB + tools)` | T | `Always search sources before answering` | "Force retrieval mode" is terminology nobody outside the RAG literature uses. "KB + tools" means nothing to the user without a legend. | P0 |
| `ComposerControls.tsx:89` header | `Tools` | G | keep | — | — |
| `ComposerControls.tsx:94-96` tool label/desc | `Knowledge base` / `Force KB retrieval for this turn` | T | `Knowledge base` / `Search indexed documents` | See above — "force retrieval" is jargon. Describe what it does, not the state machine. | P0 |
| `ComposerControls.tsx:100-103` tool label/desc | `Web` / `Enable and force web retrieval` | T | `Web` / `Search the web` | Same jargon. Two words is enough. | P0 |
| `ComposerControls.tsx:107-109` tool label/desc | `Wikipedia` / `Enable Wikipedia search and summary` | W | `Wikipedia` / `Search and summarize Wikipedia` | "Enable" is user-visible bureaucracy — the toggle already communicates enablement. | P2 |
| `ComposerControls.tsx:114-116` tool label/desc | `Deep research` / `Run recursive multi-step retrieval (slower)` | T | `Deep research` / `Multi-step research across sources. Slower.` | "Recursive" doesn't help users decide. Two-sentence form separates the what from the caveat. | P1 |
| `ComposerControls.tsx:10-11` warning message | `Deep Research runs recursive multi-step retrieval and can take noticeably longer than standard replies.` | T | `Deep research runs multiple rounds of search. Expect a longer wait.` | "Noticeably longer than standard replies" is committee prose. Also "Deep Research" is title case here but sentence case in the toggle above. | P1 |

### ConversationSidebar.tsx

| Location | Current | Grade | Proposed | Why | Priority |
|---|---|---|---|---|---|
| `ConversationSidebar.tsx:1164` aria-label/title | `Toggle sidebar` / `Toggle sidebar` | G | keep | — | — |
| `ConversationSidebar.tsx:1183` button | `New Conversation` / `New Journal Entry` (Title Case) | T | `New conversation` / `New entry` | Sentence case. For journal mode, drop "Journal" — the user is already *in* a journal, don't repeat it. | P1 |
| `ConversationSidebar.tsx:1188` section label | `Space Scope` (Title Case) | T | `Scope` | Two-word all-caps mini-label. Just "Scope" carries the meaning. | P2 |
| `ConversationSidebar.tsx:1194` value | `All Spaces` (Title Case) | T | `All spaces` | Sentence case. | P1 |
| `ConversationSidebar.tsx:1213` button | `Spaces` | G | keep | — | — |
| `ConversationSidebar.tsx:1221` label | `Notebook Mode` (Title Case) | T | `Journal` | Terminology + case: "Notebook Mode" doesn't match "Journal" used elsewhere. | P0 |
| `ConversationSidebar.tsx:1223` body | `Journal v2 is active: entries, pinned highlights, and notebook pages.` | T | (delete entirely) or `Entries, pinned highlights, and notebook pages.` | "Journal v2 is active" is a feature-flag message that leaked into UI. Users don't know v1. | P0 |
| `ConversationSidebar.tsx:1247` aria-label | `All conversations` / `Saved conversations` / etc. | G | keep | Good pattern. | — |
| `ConversationSidebar.tsx:1268` placeholder | `Search conversations...` / `Search journal entries...` | W | `Search conversations` / `Search entries` | Drop the ellipsis. Placeholders shouldn't pretend to be sentences in progress. | P2 |
| `ConversationSidebar.tsx:1299` button | `Select multiple` | G | keep | Specific verb. Good. | — |
| `ConversationSidebar.tsx:1292` button | `Done` | G | keep | — | — |
| `ConversationSidebar.tsx:1308` button | `Clear` / `Select all` | G | keep | — | — |
| `ConversationSidebar.tsx:1315` status | `5 selected` | G | keep | — | — |
| `ConversationSidebar.tsx:1326` select option | `Loading journals...` | W | `Loading...` | The user already sees this is a journal picker (the label above it). | P2 |
| `ConversationSidebar.tsx:1330` select option | `Failed to load journals` | T | `Couldn't load journals` | "Failed" → "Couldn't". Warmer, not apologetic. | P1 |
| `ConversationSidebar.tsx:1334` select option | `No journals yet` | G | keep | — | — |
| `ConversationSidebar.tsx:1339` select option | `Choose journal...` | W | `Choose a journal` | Drop the ellipsis. Add the article. | P2 |
| `ConversationSidebar.tsx:1366` button | `Add` | G | keep | — | — |
| `ConversationSidebar.tsx:1370-1372` error | `Failed to load journals: <error>` | T | `Couldn't load journals. <error>` | "Failed" is the codebase's stock verb. We can do better. | P1 |
| `ConversationSidebar.tsx:1376` body | `No journals yet. Create one to organize selected conversations.` | G | keep — but consider dropping "to organize selected conversations" (redundant with the selection context). | — | — |
| `ConversationSidebar.tsx:1391` button | `Create journal` | G | keep | — | — |
| `ConversationSidebar.tsx:1398` button | `Open journals` | G | keep | — | — |
| `ConversationSidebar.tsx:1418` aria-label | `Clear error` | W | `Dismiss error` | "Clear" implies fixing the underlying problem; "dismiss" is honest about what the X does. | P2 |
| `ConversationSidebar.tsx:1432-1438` section | `Knowledge Capture` / `Saved References` / `Review and process references in Reference Inbox.` | T | `Reference inbox` / `Saved references` / `Review and process them below.` — or better: drop the eyebrow line entirely. | "Knowledge Capture" is a title-cased feature slogan. Users don't need feature names on the sidebar; they need function. Also "Reference Inbox" vs "Reference inbox" inconsistency. | P1 |
| `ConversationSidebar.tsx:1451-1453` role filter | `All` / `AI` / `You` / `System` | W | `All` / `Assistant` / `You` / `System` | Use "Assistant" to match the message header label. "AI" is inconsistent. | P0 |
| `ConversationSidebar.tsx:1475` button | `Inbox` | G | keep | — | — |
| `ConversationSidebar.tsx:1481` status | `3 pending · 12 captured` | G | keep | — | — |
| `ConversationSidebar.tsx:1493-1494` empty state | `No saved references yet.` / `No references match the selected role filter.` | W | `No saved references.` / `No references match that filter.` | Drop "yet" (the app already implies state-over-time). "The selected role filter" → "that filter". | P2 |
| `ConversationSidebar.tsx:1526` role display | `assistant` / `user` / `system` (raw enum) | T | `Assistant` / `You` / `System` | Raw enum values are leaking to the UI — this is in lowercase with no capitalization. | P0 |
| `ConversationSidebar.tsx:1534-1535` capture badge | `Captured` / `Pending` | G | keep | — | — |
| `ConversationSidebar.tsx:1570` button | `Open in chat` | G | keep | Specific, gerund-free. | — |
| `ConversationSidebar.tsx:1580-1589` detail | `Selected reference` / `Captured in <note>.` / `Pending capture.` | G | keep | — | — |
| `ConversationSidebar.tsx:1595-1596` button | `Manage in Reference Inbox` (Title Case "Inbox") | T | `Manage references` | "Reference Inbox" is a Title Case proper noun here, lowercase two lines above. Pick one: I recommend treating it as a route name, sentence case, and calling this button `Manage references`. | P1 |
| `ConversationSidebar.tsx:1612-1613` empty state | `No conversations yet.` / `Start a new conversation above.` | T | See §4 — redesign this empty state. Current is weakest copy in the app. | — | P0 |
| `ConversationSidebar.tsx:1627` journal group label | `Today` / `Yesterday` / `Tue, Feb 11` | G | keep | — | — |
| `ConversationSidebar.tsx:1633` time bucket label | `Today` / `Yesterday` / `Last 7 days` / `Last 30 days` / `Older` | G | keep | — | — |
| `ConversationSidebar.tsx:1694` prefix | `Page 1`, `Page 2` (journal scope) | W | `Entry 1` or just the timestamp | "Page" mixes notebook metaphors. If we're using "entry" as the canonical journal unit, use `Entry 1`. | P1 |
| `ConversationSidebar.tsx:1816` aria-label | `Rename conversation: <title>` | G | keep | Good screen-reader clarity. | — |
| `ConversationSidebar.tsx:1818` title tooltip | `Rename conversation` | G | keep | — | — |
| `ConversationSidebar.tsx:1828` aria-label | `Save conversation: <title>` | W | `<Save/Unsave> conversation: <title>` (use `isSaved` state like the bookmark one does) | Aria-label doesn't reflect state. Screen reader users don't know if clicking will save or unsave. | P1 |
| `ConversationSidebar.tsx:1834` title tooltip | `Save` / `Unsave` | W | See terminology audit: this control is currently called "save" on hover but uses a Star icon. Rename to `Pin to favorites` / `Unpin from favorites` — OR drop the star entirely and let the bookmark do the job. | Three overlapping "save/bookmark/pin" actions confuse. | P0 |
| `ConversationSidebar.tsx:1844` aria-label | `Bookmark conversation: <title>` | W | `<Bookmark/Remove bookmark from> conversation: <title>` | Same as above — aria doesn't reflect state. | P2 |
| `ConversationSidebar.tsx:1850` title tooltip | `Remove bookmark` / `Bookmark` | G | keep | — | — |
| `ConversationSidebar.tsx:1860` aria-label | `Pin conversation: <title>` | W | `<Pin/Unpin> conversation: <title>` | Same state-reflection issue. | P2 |
| `ConversationSidebar.tsx:1866` title tooltip | `Unpin` / `Pin` | G | keep | — | — |
| `ConversationSidebar.tsx:1876` aria-label | `Unarchive conversation: <title>` / `Archive conversation: <title>` | G | keep | — | — |
| `ConversationSidebar.tsx:1878` title tooltip | `Unarchive` / `Archive` | G | keep | — | — |
| `ConversationSidebar.tsx:1890` aria-label | `Delete conversation: <title>` | G | keep | — | — |
| `ConversationSidebar.tsx:1892` title tooltip | `Delete conversation` | G | keep | — | — |
| `ConversationSidebar.tsx:1920` footer | `5 conversations` / `5 entries` | G | keep | — | — |
| `ConversationSidebar.tsx:1939-1940` section | `Per-Space Context` / `Spaces` | T | `Per-space context` / `Spaces` | Title Case for eyebrow labels is inconsistent with other uses. | P2 |
| `ConversationSidebar.tsx:1958` button | `Cancel` / `New Space` (Title Case) | T | `Cancel` / `New space` | Sentence case. | P1 |
| `ConversationSidebar.tsx:1966` button | `Environment` | W | `Edit space` or `Settings` | "Environment" is an internal word (system-prompt + defaults + model). Users don't have an "environment" mental model for chat. | P1 |
| `ConversationSidebar.tsx:1977-1979` placeholder | `Journal name (e.g. Food Research, Weekly Notes)` / `Space name (e.g. Product, Research, Personal)` | G | keep — this is genuinely good copy. Examples-in-placeholder is a strong pattern. | — | — |
| `ConversationSidebar.tsx:1993, 2003` segmented control | `Standard` / `Journal` | G | keep | — | — |
| `ConversationSidebar.tsx:2028` button | `Create` | G | keep | — | — |
| `ConversationSidebar.tsx:2036` section header | `Space Selector` (Title Case) | T | `Spaces` | The label contradicts the larger header "Spaces" right above. Also Title Case. If we need a label, use `Choose space` in sentence case, but the header above is probably enough. | P1 |
| `ConversationSidebar.tsx:2056-2059` "All Spaces" item | `All Spaces` / `View every conversation` | T | `All spaces` / `Every conversation, all scopes` | Sentence case; body is slightly warmer/more specific. | P1 |
| `ConversationSidebar.tsx:2117-2119` fallback | `Custom space context` | T | `No description` | "Custom space context" is engineering-speak for "this space has a system prompt or defaults". Users reading an empty description field want to know it's empty. | P1 |
| `ConversationSidebar.tsx:2131-2133` section header | `Space Environment` (Title Case) | T | `Environment` or better — `Settings` | See above. | P1 |
| `ConversationSidebar.tsx:2136` summary | `Basics` | G | keep | — | — |
| `ConversationSidebar.tsx:2143-2165` placeholders | `Icon` / `Space name` / `#8b72ff` / `Description` | G | keep | — | — |
| `ConversationSidebar.tsx:2184` summary | `AI Defaults` (Title Case) | T | `Defaults` (under a "Settings" section, the AI context is obvious) | — | P2 |
| `ConversationSidebar.tsx:2189` placeholder | `Default model id (optional)` | G | keep, but consider `e.g. claude-opus-4.7` as example in placeholder | More concrete helps. | P2 |
| `ConversationSidebar.tsx:2201` placeholder | `Space prompt (prepended for this environment)` | T | `System prompt for this space` | "Prepended" is terminology only a dev uses. Users recognize "system prompt". | P1 |
| `ConversationSidebar.tsx:2215-2238` toggles | `KB Default` / `Web Default` / `Deep Research` (Title Case, abbreviated) | T | `Knowledge base by default` / `Web by default` / `Deep research by default` | These three buttons are set-a-default toggles, but their current labels look like named feature flags. Say what they do. | P1 |
| `ConversationSidebar.tsx:2244-2247` warning body | `Deep Research default is on for this space, so responses can take longer.` | W | `Deep research is on for this space. Responses will be slower.` | Two short sentences > one comma-spliced. "Can" → "will" is more confident. | P2 |
| `ConversationSidebar.tsx:2284` summary | `Archived Spaces (3)` (Title Case) | T | `Archived spaces · 3` | Sentence case; bullet separator instead of parens. | P2 |
| `ConversationSidebar.tsx:2300` button | `Restore` | G | keep | — | — |
| `ConversationSidebar.tsx:2313` footer | `5 spaces` | G | keep | — | — |

### ConversationSpotlight.tsx

| Location | Current | Grade | Proposed | Why | Priority |
|---|---|---|---|---|---|
| `ConversationSpotlight.tsx:196` Dialog title (sr-only) | `Search conversations and references` | G | keep | — | — |
| `ConversationSpotlight.tsx:204` placeholder | `Search conversations and references...` | W | `Search conversations and references` | Drop the ellipsis per convention. | P2 |
| `ConversationSpotlight.tsx:220` empty state | `No results` | G | keep | Appropriately terse — this is a high-frequency empty state, don't burden it. | — |
| `ConversationSpotlight.tsx:262` meta line | `Conversation · <raw ISO timestamp>` | T | `Conversation · 2h ago` (use `formatDistanceToNow`) | **Currently displays raw ISO strings** like `2026-04-17T11:42:03.123Z`. This is a bug disguised as copy. | **P0 — bug** |
| `ConversationSpotlight.tsx:300` meta line | `<space> · <conv title> · <message preview>` | G | keep structure | Consider wrapping message preview in quotes for scannability. | P2 |

### CitationFootnote.tsx

| Location | Current | Grade | Proposed | Why | Priority |
|---|---|---|---|---|---|
| `CitationFootnote.tsx:37` aria-label | `Citation 1: <filename>` | G | keep | — | — |
| `CitationFootnote.tsx:67` button | `View source` | G | keep | — | — |

### SourceCitations.tsx

| Location | Current | Grade | Proposed | Why | Priority |
|---|---|---|---|---|---|
| `SourceCitations.tsx:225` metric | `Rank #3` | G | keep | — | — |
| `SourceCitations.tsx:227` metric | `Relevance N/A` | T | `No score` | "N/A" is a stock spreadsheet value. | P2 |
| `SourceCitations.tsx:229` metric | `Relevance 73.2%` | W | `73.2% relevance` | Word order — number-first reads better as metadata. | P2 |
| `SourceCitations.tsx:235-237` summary | `3 cited sources · 7 excerpts` | W | `3 sources · 7 excerpts` | "Cited" is redundant — the UI context already established they are citations. | P2 |
| `SourceCitations.tsx:289` tooltip | `View <filename>` | W | `Open <filename>` | "View" (modal) vs "Open" (file viewer) — we already use "Open" for the file-viewer action; use it consistently. | P2 |
| `SourceCitations.tsx:291` button | `View source` | G | keep as `View source` for modal preview; reserve `Open` for opening the file externally. | This is a two-button distinction — flag for explicit alignment. | P2 |
| `SourceCitations.tsx:300` meta | `Chunk 3` | T | `Section 3` or drop entirely | "Chunk" is embedding-pipeline vocabulary. Users don't know what a chunk is. If we have a section title, use it; otherwise hide. | P1 |

### ConversationLinkedDocumentsPanel.tsx

| Location | Current | Grade | Proposed | Why | Priority |
|---|---|---|---|---|---|
| `ConversationLinkedDocumentsPanel.tsx:281` header | `Sources in this conversation · 5` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:288` empty | `No sources are linked to this conversation yet.` | W | `No linked sources.` | Passive voice and redundant. | P2 |
| `ConversationLinkedDocumentsPanel.tsx:298` label | `Assign spaces` / `3 spaces` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:312-317` meta | `3 references · 2 hours ago` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:329` button | `Open` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:338` button | `Remove` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:353` summary | `Scope: <n> spaces` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:357` loading | `Loading spaces...` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:396-399` header + body | `Web sources` / `URL sources linked to this conversation context. Ingest to index.` | T | `Web sources` / `URLs cited in this conversation. Ingest to include them in search.` | "Ingest to index" is pure engineering jargon — ingest into *what*? Index *what*? The user needs to know *why*. | P0 |
| `ConversationLinkedDocumentsPanel.tsx:408` button | `Ingest all` | T | `Index all` | "Ingest" is a database term. "Index" is at least closer to common use (Google, search). Best: `Add all to search` — verbose but clear. Compromise on `Index all`. | P1 |
| `ConversationLinkedDocumentsPanel.tsx:416` empty | `No pending web citation sources.` | T | `Nothing pending.` | "Web citation sources" is the fifth noun in four words. | P2 |
| `ConversationLinkedDocumentsPanel.tsx:433, 373, 427` button | `Open URL` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:443` button | `Remove link` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:452` button | `Link source` | W | `Link` or `Save link` | "Link source" is noun-as-verb and awkward. | P2 |
| `ConversationLinkedDocumentsPanel.tsx:461` button | `Ingest` | T | `Index` | Same as "Ingest all". | P1 |
| `ConversationLinkedDocumentsPanel.tsx:129` toast | `Failed to open document` | T | `Couldn't open document` | — | P1 |
| `ConversationLinkedDocumentsPanel.tsx:140` toast | `Removed linked document` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:191` toast | `Linked source` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:202` toast | `Removed source link` | G | keep | — | — |
| `ConversationLinkedDocumentsPanel.tsx:222` toast | `Failed to ingest <label>` | T | `Couldn't index <label>` | — | P1 |
| `ConversationLinkedDocumentsPanel.tsx:256` toast | `Ingested 3 sources` | T | `Indexed 3 sources` | — | P1 |

### FilePreviewModal.tsx

| Location | Current | Grade | Proposed | Why | Priority |
|---|---|---|---|---|---|
| `FilePreviewModal.tsx:108` label | `Scope` (UPPERCASE tracking) | G | keep | — | — |
| `FilePreviewModal.tsx:114` option | `No space scope` | W | `No space` or `Unscoped` | "No space scope" is two words for what "Unscoped" does in one. | P2 |
| `FilePreviewModal.tsx:176` dynamic label | `Open URL` / `Open File` | T | `Open URL` / `Open file` | "Open File" is title case but "Open URL" isn't (URL is an acronym); we should say `Open file` (sentence case) — "URL" stays caps because it's an initialism. | P2 |
| `FilePreviewModal.tsx:188` toast | `Failed to open URL` | T | `Couldn't open URL` | — | P1 |
| `FilePreviewModal.tsx:199` toast | `Failed to open file` | T | `Couldn't open file` | — | P1 |
| `FilePreviewModal.tsx:214` toast | `Failed to open archived content` | T | `Couldn't open archive` | — | P1 |
| `FilePreviewModal.tsx:224` toast | `Failed to open file` | T | `Couldn't open file` | Duplicate. | P1 |
| `FilePreviewModal.tsx:235` toast | `Failed to locate file path` | T | `Couldn't locate file` | — | P1 |
| `FilePreviewModal.tsx:241` toast | `Failed to show in folder` | T | `Couldn't reveal in Finder/Explorer` | Platform-aware copy; "show in folder" is Windows-speak bleeding everywhere. | P2 |
| `FilePreviewModal.tsx:269` toast | `Failed to import source URL` | T | `Couldn't import URL` | — | P1 |
| `FilePreviewModal.tsx:274` toast title | `Source imported` | G | keep | — | — |
| `FilePreviewModal.tsx:282` toast | `Imported source saved, but failed to open` | T | `Saved, but couldn't open` | Removed redundancy. | P1 |
| `FilePreviewModal.tsx:311-314` empty | `File Too Large for Preview` / `This file (12 MB) exceeds the 10 MB preview limit.` | T | `File too large to preview` / `This file is 12 MB. Preview limit is 10 MB.` | Title Case header ("File Too Large for Preview"); rewrite in sentence case and split the explanation. | P1 |
| `FilePreviewModal.tsx:320` button | `Open in External Viewer` (Title Case) | T | `Open externally` or `Open in default viewer` | Title Case; "External Viewer" is an abstraction nobody uses. | P1 |
| `FilePreviewModal.tsx:332` loading | `Loading large file (12 MB)...` | G | keep | Ellipsis is fine here because this genuinely is a continuing action. | — |
| `FilePreviewModal.tsx:346, 400` label | `Web Source` (Title Case) | T | `Web source` | — | P2 |
| `FilePreviewModal.tsx:381` fallback | `Preview unavailable for this web source.` | W | `No preview available.` | — | P2 |
| `FilePreviewModal.tsx:435` fallback | `Preview unavailable. Open the source URL.` | G | keep | — | — |
| `FilePreviewModal.tsx:445` error title | `Failed to load file` | T | `Couldn't load file` | — | P1 |
| `FilePreviewModal.tsx:518` button | `Show in Folder` (Title Case, platform-mismatched) | T | `Reveal in Finder` on mac, `Show in Explorer` on Win, `Show in file manager` on Linux (platform-aware) | Title Case; platform-mismatched on mac. Finder says "Reveal in Finder". | P1 |

---

## 4. Empty state redesigns

Empty states are the single biggest missed opportunity. Currently they're all variations of `No X yet. Do Y.` with a muted icon. Five rewrites follow; pick the ones that fit best.

### 4.1. No conversation selected (ChatPanel.tsx:402-426)

**Current:**
> No conversation selected
> Pick a conversation from the sidebar, or start a new one.
> [New Conversation]

**Proposed (Option A — quiet confidence):**
> Nothing open.
> Pick one on the left, or start fresh.
> [New conversation]

**Proposed (Option B — editorial, my recommendation):**
> No conversation open.
> [New conversation]
>
> *(no subtitle — let the button do the work)*

**Why:** The sidebar is adjacent and obvious. The subtitle currently states what is already visible. Linear, Arc, and Things 3 all let the button speak for itself in this pattern.

### 4.2. Empty conversation (ChatPanel.tsx:433-444)

**Current:**
> This conversation is empty.
> Ask a question to begin.

**Proposed (Option A — direct):**
> *(delete entirely; the composer's placeholder `Ask anything` already communicates this)*

**Proposed (Option B — editorial):**
> No messages yet.

**Why:** There is a composer with a placeholder at the bottom of the screen. Adding a second empty-state card that says the same thing is redundant. If we *must* fill the space, one line is enough.

### 4.3. No conversations in sidebar (ConversationSidebar.tsx:1609-1614)

**Current:**
> No conversations yet.
> Start a new conversation above.

**Proposed (my showcase — this is the one to ship):**
> No conversations here.
> Every question you ask lives in a conversation.
> Start one with `⌘N`.

**Why:** The current copy wastes the single most valuable empty state in the app — a new user's first moment with the sidebar. It tells them nothing about what this space is, explains nothing about the model, and points at a button that is already visible one line away. The proposed version does three things at once: it names the concept (conversation), it teaches the mental model (every question lives in one), and it teaches a shortcut. Things 3–grade.

For the journal-scope variant:
> No entries in this journal.
> Each entry is a day's conversation.
> Start one with `⌘N`.

### 4.4. No results (ConversationSpotlight.tsx:217-221)

**Current:**
> *[search icon]*
> No results

**Proposed:**
> *[search icon]*
> No matches.
> Try a different term, or `⌘N` for a new conversation.

**Why:** Spotlight is a power-user surface and a search-returns-nothing state is the most common dead-end; it should offer a way forward.

### 4.5. No citations for a message

No explicit empty state exists today (the `SourceCitations` returns `null` if `sources.length === 0`). That's correct behavior — no orphan card needed. Leave it.

### 4.6. No linked documents / web sources (ConversationLinkedDocumentsPanel.tsx:287-291)

**Current:**
> No sources are linked to this conversation yet.

**Proposed:**
> Nothing linked.
> Sources cited in answers will appear here.

**Why:** Teaches what this space is for. Current is passive and silent on the mechanism.

### 4.7. No saved references, filtered (ConversationSidebar.tsx:1491-1495)

**Current:**
> No saved references yet.
> *(or)* No references match the selected role filter.

**Proposed:**
> No references yet.
> Save any message from a conversation with the bookmark icon.
>
> *(filtered variant):* No matches in this filter.

**Why:** The current copy doesn't teach *how* to create a reference. The bookmark icon is the mechanism and we should name it.

---

## 5. Keyboard shortcut format standard

### Current state (a mess)

- `ChatPanel.tsx:543` — `<kbd>⌘⏎</kbd> to send · <kbd>⇧⏎</kbd> for new line` (glyph-only, compact)
- `ChatPanel.tsx:535` — `Send message (Enter)` (word form, in parens)
- `ConversationSpotlight.tsx:207-209` — `ESC` (all caps, bare)

Three formats in one feature.

### Actual behavior discrepancy (bug)

`ChatPanel.tsx:370-378` sends on **plain Enter** AND on **⌘/Ctrl + Enter**. The footer label only mentions `⌘⏎`. Users who hit plain Enter will send unexpectedly.

**Fix this first:** pick one behavior. Recommendation — match Linear/Arc:
- `Enter` sends.
- `Shift + Enter` inserts a newline.
- `⌘ + Enter` *also* sends (power-user convenience, documented as such).

### Standard — apply everywhere

**Written copy (body text, help text):** spelled-out modifiers joined by `+`, Enter/Esc as words, mac glyphs for `⌘` and `⇧` and `⌥`.

```
Enter                  → Enter
Shift + Enter          → Shift + Enter
Cmd + Enter (mac)      → ⌘ + Enter
Cmd + K (mac)          → ⌘K        (no plus for single-key modifier chords)
Escape                 → Esc
```

**Footer hint / composer hint:**
```
Enter to send · Shift + Enter for new line
```
(NOT `⌘⏎ to send` — that is user-hostile without a legend, and contradicts the code.)

**`<kbd>` tags in body:** wrap each key separately, space-separated on the page, `+` as literal text outside the `<kbd>`:
```html
<kbd>Enter</kbd> to send · <kbd>Shift</kbd> + <kbd>Enter</kbd> for new line
```

**Tooltip on a button:** `Action name · Shortcut` using the spelled-out form.
```
Send · Enter
Stop · Esc
```

**Spotlight esc hint:** replace the current bare `ESC` with `<kbd>Esc</kbd>` in `Esc` not `ESC` (initial cap only). The all-caps pill reads like a 1990s help system.

---

## 6. Tooltip coverage

Every icon-only button needs a `title` attribute (for pointer) and an `aria-label` (for assistive tech). Below is the current state. Where both exist and match, we're good. Where one is missing, or they're wrong-tone, fix.

### Icon-only buttons audit

| File:line | Icon | aria-label | title | Status |
|---|---|---|---|---|
| `ChatPanel.tsx:484` | `Settings2` (composer popover) | `Composer controls` | `Turn mode, tools, model` | title mentions missing "model" — fix to `Turn mode and tools` |
| `ChatPanel.tsx:523` | `Square` (stop) | `Stop generation` | `Stop generation` | OK — shorten title to `Stop` |
| `ChatPanel.tsx:533` | `Send` | `Send message` | `Send message (Enter)` | OK — update to `Send · Enter` |
| `ConversationSidebar.tsx:1164` | `PanelLeft` | `Toggle sidebar` | `Toggle sidebar` | OK |
| `ConversationSidebar.tsx:1246` | filter chips (not icon-only — labels present) | `<name> conversations` | `<name>` | OK |
| `ConversationSidebar.tsx:1418` | `X` (clear error) | `Clear error` | — | **MISSING title** — add `Dismiss` |
| `ConversationSidebar.tsx:1759` | `Check` (save rename) | `Save conversation title` | `Save title` | OK |
| `ConversationSidebar.tsx:1769` | `X` (cancel rename) | `Cancel rename` | `Cancel` | OK |
| `ConversationSidebar.tsx:1817` | `Pencil` | `Rename conversation: <title>` | `Rename conversation` | OK |
| `ConversationSidebar.tsx:1828` | `Star` | `Save conversation: <title>` | `Save` / `Unsave` | aria doesn't reflect state — fix |
| `ConversationSidebar.tsx:1844` | `Bookmark` | `Bookmark conversation: <title>` | `Bookmark` / `Remove bookmark` | aria doesn't reflect state — fix |
| `ConversationSidebar.tsx:1860` | `Pin` | `Pin conversation: <title>` | `Pin` / `Unpin` | aria doesn't reflect state — fix |
| `ConversationSidebar.tsx:1876` | `Archive` / `RotateCcw` | `<Archive/Unarchive> conversation: <title>` | `Archive` / `Unarchive` | OK |
| `ConversationSidebar.tsx:1892` | `Trash2` | `Delete conversation: <title>` | `Delete conversation` | OK |
| `ConversationSidebar.tsx:1944` | `X` (close spaces panel) | `Close spaces panel` | — | **MISSING title** — add `Close` |
| `ConversationSidebar.tsx:2168` | `Clear` button (accent color) | — | — | **MISSING aria-label and title** — add `Clear accent color` |
| `CitationFootnote.tsx:37` | superscript `[n]` | `Citation n: <filename>` | — | title MISSING (only aria); popover handles it so OK |
| `SourceCitations.tsx:285-292` | `View source` | — | `View <filename>` | button has visible label, no aria needed — OK |
| `MessageActions.tsx:33-48` | `Copy` / `Check` | `Copy message to clipboard` | — | visible label — OK but add `title="Copy"` for hover-clarity on the icon |
| `MessageActions.tsx:52-63` | `Bookmark` | `Save message as reference` / `Remove saved reference` | — | **MISSING title** — add `Save to references` / `Remove from references` |
| `MessageActions.tsx:67-75` | `Trash2` | `Delete message` | — | **MISSING title** — add `Delete message` |
| `Message.tsx:205-232` | verification badge (icon + label) | — | long sentence titles | titles are too verbose — see §3 findings |
| `FilePreviewModal.tsx:500-502` | `X` close | — | — | **MISSING both aria-label and title** — add `aria-label="Close"` and `title="Close · Esc"` |
| `FilePreviewModal.tsx:513` | `Show in Folder` | — | — | has visible label; OK but consider `title="Reveal in Finder"` on mac |
| `ConversationLinkedDocumentsPanel.tsx:322-338` | `ExternalLink` / `Trash2` | — | — | buttons have visible labels; OK |

**Summary:** 7 missing tooltips or aria-labels need to be added; 3 aria-labels don't reflect state (star/bookmark/pin); 2 titles are too verbose.

---

## 7. Punch-list

### P0 — wrong, missing, or misleading (fix first)

1. Keyboard shortcut hint contradicts actual send behavior (`ChatPanel.tsx:543` says `⌘⏎`, code at `:370-378` accepts plain Enter too). **Bug.**
2. Spotlight shows raw ISO timestamps like `2026-04-17T11:42:03.123Z` in meta (`ConversationSpotlight.tsx:262`). **Bug disguised as copy.**
3. Raw lowercase enum values (`assistant` / `user` / `system`) leaking into sidebar reference snippets (`ConversationSidebar.tsx:1526`). **Bug.**
4. Title Case inconsistency for button labels across `ChatPanel.tsx:421`, `ConversationSidebar.tsx:1183, 1958`, `FilePreviewModal.tsx:311, 320, 518` — systemic.
5. "Notebook Mode" / "Journal v2 is active" / "Journal" / "Notebook" — four names for one feature (`ConversationSidebar.tsx:1221-1237`). **Terminology bug.**
6. Turn-mode descriptions (`ComposerControls.tsx:60-62`) use engineer language ("Force retrieval mode (KB + tools)") that won't parse for end users.
7. "Save" exists on a message, "Save" exists on a conversation (star), "Bookmark" exists on a conversation, "References" filters the result — four overlapping names for adjacent concepts (`MessageActions.tsx`, `ConversationSidebar.tsx:1828, 1844`). **Terminology bug.**
8. `ConversationSidebar.tsx:1451-1453` role filter uses `AI` instead of `Assistant`, breaking consistency with the message header.
9. `ConversationLinkedDocumentsPanel.tsx:397-399` "URL sources linked to this conversation context. Ingest to index." is opaque engineering prose shown in a user-facing body.
10. Sidebar "No conversations yet." empty state is the weakest piece of copy in the feature (`ConversationSidebar.tsx:1612-1613`).

**Count: 10 P0 items.**

### P1 — weak copy a top-tier app wouldn't ship

1. "Message input" aria-label → "Message composer".
2. Composer placeholders: "message..." / "follow up on this conversation..." / "search and answer..." all trailing-ellipsis and unspecific.
3. "This conversation is empty." / "Ask a question to begin." duplicates the composer placeholder.
4. Tool descriptions in `ComposerControls.tsx:94-116` use "force retrieval", "enable and force", "recursive multi-step retrieval" — jargon.
5. "Deep Research runs recursive multi-step retrieval and can take noticeably longer than standard replies" — committee prose.
6. All `Failed to X` toasts across `FilePreviewModal.tsx`, `ConversationLinkedDocumentsPanel.tsx` (~10 occurrences) — rewrite to `Couldn't X`.
7. "New Conversation" / "New Journal Entry" / "New Space" / "New Space" all Title Case — sentence case everywhere.
8. "All Spaces" / "Space Scope" / "Space Selector" / "Space Environment" all Title Case labels — sentence case.
9. "Ingest" / "Ingest all" — database terminology used as UI verbs.
10. "Knowledge Capture" eyebrow + "Saved References" header + "Reference Inbox" route — three capitalizations for one concept.
11. "Space prompt (prepended for this environment)" — "prepended" leaks to UI.
12. "KB Default" / "Web Default" / "Deep Research" button labels don't say what they do.
13. "Custom space context" fallback for empty description — engineering-speak.
14. Aria-labels on star/bookmark/pin buttons don't reflect state.
15. "Failed to load journals" error text.
16. "Manage in Reference Inbox" Title Case.
17. Verification badge titles too long (`Message.tsx:218-222`).
18. "Verified Claims" / "Unverified Claims" headers Title Case.
19. "Partially verified (3)" parens awkward.
20. "All evaluated claims were grounded in the cited context." passive voice.
21. "View every conversation" → "Every conversation, all scopes".
22. "File Too Large for Preview" Title Case.
23. "Open in External Viewer" Title Case and abstract.
24. "Show in Folder" platform-mismatched on mac.
25. "Chunk N" in source metadata.
26. "Environment" button label is internal terminology.
27. "Page 1" in journal conversation groups — mixes notebook metaphor.
28. `MessageActions.tsx` "Save" / "Saved" collides with conversation-level "Save" (star).
29. Composer trigger title `Turn mode, tools, model` mentions a non-existent model picker.
30. `Ingest` / `Indexed` / `Index` — pick one verb for this flow.

**Count: 30 P1 items.**

### P2 — polish pass (nice-to-have)

1. Drop trailing ellipses from placeholders (`Search conversations...`, `Search conversations and references...`, `Choose journal...`, etc.) — 6+ instances.
2. Drop "yet" from empty states — implied by the state.
3. "Coverage 82% (5 claims)" → "82% coverage · 5 claims".
4. "Show all verified claims (5)" → "Show all 5".
5. "Verified claim text is not available for this message." → "No verified claim text available."
6. "Delete this message from the conversation? This cannot be undone." → shorten.
7. "Rank #3" → keep or simplify.
8. "Relevance N/A" → "No score".
9. "Relevance 73.2%" → "73.2% relevance" (number first).
10. "3 cited sources · 7 excerpts" → "3 sources · 7 excerpts".
11. "No sources are linked to this conversation yet." → "No linked sources."
12. "Web citation sources" noun stacking.
13. "Link source" button → "Link" or "Save link".
14. "No space scope" → "Unscoped".
15. Loading text in select boxes ("Loading journals...") → just "Loading...".
16. "The selected role filter" → "that filter".
17. "Clear error" → "Dismiss error".
18. "Per-Space Context" / "Space Selector" / "AI Defaults" Title Case → sentence case.
19. "Archived Spaces (3)" → "Archived spaces · 3".
20. "Enable Wikipedia search and summary" → "Search and summarize Wikipedia".
21. Verbose option descriptions in turn mode.
22. "Default model id (optional)" placeholder — add example.
23. Consistent `<kbd>` styling across spotlight / composer footer (see §5).
24. Message wrap-in-quotes for message previews in spotlight.
25. "View <filename>" vs "Open <filename>" action verb clarity in citations.

**Count: 25 P2 items.**

### Totals

| Priority | Count |
|---|---|
| P0 (wrong / missing / misleading) | **10** |
| P1 (weak) | **30** |
| P2 (polish) | **25** |
| **Total findings** | **65** |

---

## 8. Implementation notes

- **Sentence case is the single biggest win.** Most P1 items collapse to one rule: buttons, tabs, eyebrows, and section headers are sentence case; proper nouns stay capitalized (Recall, Wikipedia, Finder). One codemod-friendly pass clears ~15 findings.
- **Stop writing `Failed to X`.** Grep for `'Failed to` in `src/app/websrc/components/Chat/**`; replace with `Couldn't `. That knocks out another ~10.
- **Terminology doc belongs at `/Users/joshreed/Code/Recall/src/app/websrc/.design/CHAT-VOCABULARY.md`** (not in code). This audit's §2 is the starting draft.
- **The save/bookmark/pin/reference collision (P0 #7) is a product decision, not a copy decision** — flag it to Josh separately. I've proposed renaming the conversation-level "Save" star to "Pin to favorites", but the cleaner fix is to delete one of the three and pick.
- **Keyboard shortcut bug (P0 #1) is one-line:** either remove plain-Enter send (match the hint) or update the hint to reflect actual behavior. Recommend the latter + match Linear.
