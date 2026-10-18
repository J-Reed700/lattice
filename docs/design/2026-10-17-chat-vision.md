# Chat vision: the model sees pictures

Status: design, 2026-10-17. Builds on `2026-10-17-chat-images-and-diagrams.md` (pictures in answers, `chat_images`, `lattice-image://`) and replaces its §9. Implemented by Opus agents straight from this document; every contract below is meant to be typed in as written.

## 0. The ask, and the answer in one paragraph

The chat model should be able to look at pictures: the ones `find_image` fetches, so it keeps the one that shows the subject and drops the rest, and the ones a reader attaches to a message, so a question about a photo or a screenshot is answered from the pixels and not from a filename. Vision is a declared capability of a model, never probed: curated local models carry their projector file and encoder geometry in the catalog, a remote or Ollama model carries what the user declares for it in Settings, and a cloud model carries documented provider defaults the user may override. One new request shape, `CompletionInput::Multimodal`, carries image parts through the core port to every provider adapter; an adapter whose model is not declared to see refuses them, and the chat feature never sends them to such a model in the first place — it falls back to text. Every picture is decoded once, fitted to the declared geometry (never upscaled), re-encoded, cached beside the original, and costs the tokens that geometry says it costs; those tokens are counted in the context budget and in the pre-send window check (#85) like any other prompt tokens, and are the first thing shrunk, then dropped, when a request does not fit. `find_image` relevance is a small side request that shows the candidates to the same model and gets a structured verdict, so the conversation's own prompt prefix never carries candidate pictures. Attached pictures ride in the turn they were sent in; later turns, memory, handoff and the grounding check see captions. The bundled sidecar gains vision in this build: the catalog knows each chat model's `mmproj` file, the user downloads it from the model row, and the launcher passes `--mmproj`; mainline llama.cpp has allowed speculative decoding alongside a projector since April 2026 (#19493), which the pinned v0.6.0 includes, so nothing is silently dropped.

## 1. What exists today (verified)

- Chat requests are text-only. `CompletionInput` (`crates/lattice-core/src/application/ports/llm_port.rs:246`) has `Native`, `Message { role, content: String }`, `ToolCall`, `ToolResult`. 126 sites match `CompletionInput::Message`; 22 non-test files. Adding a *variant* breaks only exhaustive matches (the five provider builders and four counters listed in §6.5); adding a field would touch all 126.
- Providers that implement `LLMPort` for real models: `features/llm/llama_cpp/mod.rs` (bundled sidecar and remote llama.cpp; `chat_messages` at line 258 builds OpenAI Chat Completions messages), `features/llm/engine/ollama_client.rs` (`OllamaChatMessage.images: Option<Vec<String>>` exists at `engine/types.rs:92` and is always `None`), `features/llm/cloud.rs` (OpenAI Responses `input`, Anthropic Messages, and Google through an OpenAI-compatible chat body). Wrappers that must delegate: `scheduler/scheduled.rs`, `instances/pinned.rs`, `learning/patient_model.rs`, `engine/factory.rs::MockLLMPort`.
- The OCR feature already sends pictures as OpenAI `image_url` data URIs (`features/ocr/client.rs:74`, `ocr/render.rs:36`) to a dedicated llama-server launched with `--mmproj` (`ocr/launcher.rs:163`). Its comment (`launcher.rs:5`, `ocr_port.rs:34`) says a projector rules out speculative decoding; upstream fixed that (§12.3).
- #85 (`1a04a7f1`): `ScheduledLlm::ensure_fits` (`scheduler/scheduled.rs:68`) and `LlamaCppLlm::measured_body` (`llama_cpp/request.rs:85`) refuse a request the configured window cannot hold, counting text with the calibrated estimate and, when that does not fit, the server tokenizer. Both deliberately ignore `Native` items ("opaque provider state (images) is not text"). `plain_prompt_tokens` and `prefill_allowance` (`llama_cpp/retry.rs:33`) count `message.to_string()` characters: a base64 image inside a message would be counted as text and refuse every vision request. §8 fixes this.
- Attachments: `features/conversation/chat/attachments.rs` loads attached *documents* by id and carries their text; images are not importable documents at all (`indexing/engine/extraction/mime.rs` lists no image type; an attached PNG is `UnsupportedFileType` and reaches the prompt only as an "unreadable" name). The frontend stages paths from drag-drop or a dialog (`useChatFileDrop.ts`, `useAttachmentImport.ts`) and sends `attachmentNames` + `attachmentDocumentIds` (`plugin_impl.rs:185`).
- Pictures: `features/web/images/*` stores validated files at `~/.lattice/chat-images/<id>.<ext>` with `chat_images` rows (`20261017120000_chat_images.sql`, `origin IN ('wikimedia_commons','page')`), serves them over `lattice-image://`, leases them to the turn's `request_id`, attaches on persist (`chat/persistence.rs:422`), sweeps at startup (`gc.rs`), copies on fork (`copy_for_messages`), exports and backs them up. `tool_loop/images.rs` runs `find_image` through `ChatImagesPort`.
- Model identity and capability: `ModelRef` (`domain/models/model_ref.rs`), `ModelRequirements { min_context_tokens, tools }`, `LLMPort::supports_tool_calling()`; the local sidecar's tool support comes from the catalog file name or GGUF header (`engine/gguf_metadata.rs:280`), its thinking policy from `engine/thinking_policy.rs`. The catalog entry struct is `CuratedChatModel` (`domain/models/curated/chat.rs:107`); `ModelMetadata.capabilities: Vec<String>` is what the UI sees. The OCR catalog (`curated/ocr.rs`) already downloads a weights file plus `mmproj.gguf` through `ModelMetadata.files`.
- Sidecar launch: `sidecar_manager/readiness.rs:70 build_server_args` (`--jinja`, `--ctx-checkpoints`, `--cache-reuse 256`, `-np 1 --spec-type draft-mtp --spec-draft-n-max N` when `config.mtp_draft()`); `engine/factory.rs:361 sidecar_config`. The pinned binary is upstream tag `v0.6.0` (`src-tauri/scripts/llama-server.lock`), built with `LLAMA_BUILD_SERVER=ON` and default options, which include the `mtmd` multimodal library.
- The context budget: `context_assembler/budget.rs` (`ModelCapacity`, `BudgetRequest { system, tools, current_input, history }`), built per turn in `chat/turn/budget.rs:45`. The bounded-memory path assembles `Vec<CompletionInput>` in lattice-core (`context_assembler/{plan,render}.rs`); the plain path uses `completion_input::from_context` (`turn/assemble.rs:509`). Research jobs persist their round transcripts as `Vec<CompletionInput>` (`turn/research.rs:26`).
- History replay turns an answer's picture lines into `[Picture: caption]` in `context_window_builder.rs:156`, `context_assembler/render.rs:82`, `history_tools/read.rs:125`, `memory/summarize.rs:289`, `handoff.rs:48`; the lexical grounding check strips them (`verification/lexical.rs:74`).
- Frontend settings: `features/settings/components/AITab/{LlamaCppConnection,ModelsTab,TuningTab}.tsx`, `modelRoles/{LocalModelRow,LocalModelConfigModal,LlamaCppMetaRow,OllamaMetaRow}.tsx`; the chat model picker shows a "no tools" note from `supportsTools` (`ModelPickerPopover.tsx:246`, `utils/chatModelSelection.ts:26`). Views reach the backend through `features/*/api/client.ts`; bindings are generated (`export_bindings --check`).

## 2. Goals and non-goals

Goals
1. A model declared to see gets pictures as pixels: attached images in the turn they were sent in, and `find_image` candidates in a side pick.
2. One capability contract for every backend: bundled sidecar, remote llama.cpp, Ollama, OpenAI, Anthropic, Google. Declared, never probed.
3. Encoder geometry per model drives downscaling and token cost; both are derived, not constants. A conservative default exists for an undeclared geometry and says so in the UI.
4. Image tokens go through the same accounting as text: the turn budget, `ensure_fits`, `measured_body`, the prefill allowance.
5. Prompt prefix stays stable: pictures only in the turn that needs them, captions afterwards; tool list unchanged across turns; no candidate pictures in the conversation transcript.
6. Local sidecar vision first-class: projector in the catalog, downloadable per model, `--mmproj` at launch, speculative decoding kept.
7. Architecture ratchets unchanged: new cross-feature calls go through lattice-core ports; no file over 800 lines; bindings regenerated; UI on the app's tokens in both themes.

Non-goals
- Image generation. Video or audio. OCR replacement (the OCR pipeline stays as it is).
- Images in tool-role messages (llama.cpp's OpenAI-compatible path documents image parts for user messages only; nothing here depends on more).
- Pixels replayed across turns (§11 explains the trade and the explicit "show again" path).
- Vision for uncurated GGUFs from the Hugging Face browser (follow-up, §20).
- Measuring vision quality (an eval set is a follow-up).

## 3. User-visible behaviour

- Settings → Models: a local chat model whose catalog entry has a projector shows **Vision** on its row: "Download projector (0.8 GB)" until downloaded, then a toggle "Let this model see images" (on by default) in the model's configuration modal. A model without a projector in the catalog shows nothing. The remote llama.cpp connection, each Ollama model and each cloud model row show a **Vision** declaration: Off / a preset (Qwen 3.5–3.8 VL · Qwen 2.5 VL · Gemma 3/4 · OpenAI · Anthropic · Gemini · Custom) with the geometry fields visible and editable under "Advanced", and a one-line cost preview ("a 1280×960 photo costs about 1,100 tokens on this model"). Cloud rows start on their provider preset; an unknown geometry shows "Conservative default (up to 1,026 tokens per picture)".
- Chat composer: dropping or choosing an image file (`png`, `jpg`/`jpeg`, `gif`, `webp`) stages a thumbnail, not a document chip. Sending stores the pictures with the message; the user bubble shows them as figures (same `lattice-image://` path as answer pictures). If the model of the turn is not declared to see, each figure carries a small note "Not shown to <model>" and the model is told the picture exists.
- The model picker shows "sees images" beside a model whose declaration is on, the way it shows "no tools" today.
- `find_image`: with a seeing model, the model is shown the stored candidates at pick size and keeps the ones that show the subject; the answer then has fewer, better pictures, or none ("No suitable picture found"). Without vision, behaviour is unchanged.
- A request that cannot fit even after pictures are shrunk to the geometry's minimum is refused with the #85 message, which now says how many tokens the pictures cost and that they were estimated from the model's declared geometry.
- A follow-up question about an earlier picture is answered from the caption unless the reader re-attaches it: each figure in a user bubble has "Show to the model again", which stages a copy on the composer.

## 4. Architecture

```mermaid
flowchart LR
  subgraph fe[frontend]
    CP[composer · useAttachmentImport\nimage files → chatApi.attachImages] --> CMD
    UB[Message.tsx user figures\n"Not shown to model" chip] 
    VD[VisionDeclarationEditor\nLocalModelConfigModal vision toggle\nLocalModelRow projector download]
  end
  subgraph conv[features::conversation]
    CMD[commands/images.rs\nattach_message_images] -->|ChatImagesPort.store_attachment| CI
    PIC[turn/pictures.rs\nfit · charge · shrink · drop] -->|ChatImagesPort.prepared| CI
    PIC --> REQ[plan_request\nattach_images_to_last_user]
    PK[tool_loop/image_pick.rs\nside request, JSON verdict] -->|ChatImagesPort.prepared| CI
    PK -->|LLMPort.complete| LLM
    REQ -->|LLMPort.complete| LLM
    PS[persistence.rs\nattach_ids_to_message] --> CI
  end
  subgraph core[lattice-core]
    VG[(domain::models::vision\nVisionGeometry · fit · tokens)]
    CI[(ports::ChatImagesPort\ncontracts::images)]
    LP[(ports::llm_port\nCompletionInput::Multimodal\nLLMPort::vision)]
  end
  subgraph web[features::web::images]
    SVC[service.rs] --> PREP[prepare.rs\ndecode · orient · fit · encode · cache]
    SVC --> ST[store.rs\nchat-images/id.ext + prepared/]
    SVC --> RP[repository.rs\norigin=attachment]
  end
  subgraph llm[features::llm]
    LLM[ScheduledLlm → LlamaCppLlm · OllamaClient · CloudLlm\nimage parts → wire format\nimage tokens → ensure_fits · measured_body]
    SC[sidecar_manager\n--mmproj + draft-mtp]
    ROLES[di/roles.rs\ndeclaration → adapter.vision]
  end
  CI -.impl.-> SVC
  PREP --> VG
  PIC --> VG
  LLM --> VG
  VD -->|settings · model commands| ROLES
```

Three boundaries, all through lattice-core: the conversation feature reaches pictures through `ChatImagesPort` (extended, §10.3) and models through `LLMPort` (extended, §6.2); `features::llm` and `features::web` share only `domain::models::vision`. `feature-edge` counts do not change.

## 5. The capability contract

### 5.1 `VisionGeometry` — `crates/lattice-core/src/domain/models/vision.rs` (new; `pub mod vision;` in `domain/models/mod.rs`; tests in `vision/tests.rs`; provider defaults in `vision/cloud_defaults.rs`)

```rust
/// How a model's vision encoder takes a picture: what size it is resized to
/// and what that costs in prompt tokens. Declared per model (catalog, Settings
/// or provider default), never measured from a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VisionGeometry {
    /// Dynamic resolution on a token grid (Qwen2-VL, Qwen2.5-VL, Qwen3-VL and
    /// the Qwen3.5+ chat models built on them). The picture is resized so both
    /// sides are multiples of `token_side` pixels (vision patch × spatial
    /// merge: 28 for Qwen2/2.5-VL, 32 for Qwen3-VL and later) with its area
    /// held within `min_pixels..=max_pixels`; one token per `token_side²`
    /// square plus `overhead_tokens` (vision start/end markers).
    Grid { token_side: u32, min_pixels: u32, max_pixels: u32, overhead_tokens: u32 },
    /// A fixed input (SigLIP-style encoders: Gemma 3/4, PaliGemma, most LLaVA
    /// builds): resized to fit `side`×`side`, always `tokens` tokens.
    Fixed { side: u32, tokens: u32 },
    /// Tiled encoders (OpenAI "high" detail, Gemini, Llama 3.2 Vision): the
    /// picture is scaled so its long side is at most `max_side`, cut into
    /// `tile_side` tiles (at most `max_tiles`), `tokens_per_tile` each plus
    /// `base_tokens` for the overview.
    Tiles { tile_side: u32, tokens_per_tile: u32, base_tokens: u32, max_tiles: u32, max_side: u32 },
}

impl VisionGeometry {
    /// The size to send for a `width`×`height` source: downscaled to the
    /// geometry, never upscaled, aspect ratio kept (rounded to the grid for
    /// `Grid`; letterboxed by the encoder itself for `Fixed`).
    pub fn fit(self, width: u32, height: u32) -> (u32, u32);
    /// Prompt tokens for a picture already at `fit` size.
    pub fn tokens(self, width: u32, height: u32) -> u32;
    /// `fit` and `tokens` together, for the common call.
    pub fn plan(self, width: u32, height: u32) -> FittedImage { FittedImage { width, height, tokens } }
    /// The largest fit that costs at most `max_tokens`, or `None` when even
    /// the geometry's smallest rendering costs more. `Grid` shrinks the area
    /// down to `min_pixels`; `Tiles` reduces tiles down to the overview alone;
    /// `Fixed` cannot shrink.
    pub fn fit_within(self, width: u32, height: u32, max_tokens: u32) -> Option<FittedImage>;
    /// The most a single picture can cost on this geometry.
    pub fn max_tokens(self) -> u32;
    /// The least a picture can cost (the overview alone, the minimum area, or
    /// the fixed count). What the `find_image` pick is sized from (§9.3).
    pub fn min_tokens(self) -> u32;
    /// For a model whose geometry nobody declared. Conservative in both
    /// directions: pictures are sent no larger than 1,024 grid squares of
    /// 28 px (≈ 896×896) and charged 1,026 tokens at that size, which is at or
    /// above what every family above costs for the same pixels (a 32 px grid
    /// costs fewer, a Fixed 896 encoder 256–280, Anthropic ≈ 1,070, OpenAI ≈ 765).
    pub const CONSERVATIVE_DEFAULT: Self = Self::Grid { token_side: 28, min_pixels: 4 * 28 * 28, max_pixels: 1024 * 28 * 28, overhead_tokens: 2 };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FittedImage { pub width: u32, pub height: u32, pub tokens: u32 }

/// Named geometries the Settings editor offers. Each is a documented reading
/// of a model family's published preprocessing; "Custom" is the raw enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum VisionPreset { Qwen3Vl, Qwen25Vl, GemmaSiglip, OpenAi, Anthropic, Gemini }
impl VisionPreset { pub fn geometry(self) -> VisionGeometry; pub fn label(self) -> &'static str; }

/// The catalog capability tag, beside `tool-calling`.
pub const VISION_CAPABILITY: &str = "vision";
```

Preset values (the agent writes each as a `const` with a doc comment citing the source read when implementing; these are the readings at design time):
- `Qwen3Vl`: `Grid { token_side: 32, min_pixels: 64·32², max_pixels: 2048·32² (≈ 2.1 MP), overhead_tokens: 2 }`. Confirm `token_side` from the projector's GGUF header for each catalog entry (§12.1); the catalog entries carry their own values, the preset is for user-declared models.
- `Qwen25Vl`: `Grid { token_side: 28, min_pixels: 4·28², max_pixels: 1280·28² (≈ 1.0 MP), overhead_tokens: 2 }` (the 1280-token cap is the model card's recommended `max_pixels`).
- `GemmaSiglip`: `Fixed { side: 896, tokens: 256 }` (Gemma 3; verify Gemma 4's token count from its mmproj header — if it differs, the catalog entry says so).
- `OpenAi`: `Tiles { tile_side: 512, tokens_per_tile: 170, base_tokens: 85, max_tiles: 16, max_side: 2048 }`.
- `Anthropic`: `Grid { token_side: 27, min_pixels: 0, max_pixels: 1568·1568, overhead_tokens: 0 }` — their documented estimate is `width·height / 750`, which a 27-px grid reproduces within 3 %.
- `Gemini`: `Tiles { tile_side: 768, tokens_per_tile: 258, base_tokens: 0, max_tiles: 6, max_side: 3072 }`.

`cloud_defaults.rs`: `pub fn cloud_vision_default(provider: CloudProvider, model: &str) -> Option<VisionGeometry>` — `Openai` for `gpt-4o*`, `gpt-4.1*`, `gpt-5*`, `o3*`, `o4*`; `Anthropic` for every `claude-*`; `Google` for every `gemini-*`; `None` otherwise (the user may still declare one). Unit-tested on a handful of names.

Arithmetic (`Grid`): `fit` scales so that `w·h ≤ max_pixels`, then rounds each side to the nearest multiple of `token_side` (at least one square), never above the source; `tokens = (w/token_side)·(h/token_side) + overhead_tokens`. `Tiles`: `fit` scales the long side to `max_side`, then `tokens = base_tokens + tokens_per_tile · min(max_tiles, ceil(w/tile_side)·ceil(h/tile_side))`. `Fixed`: `fit` scales to fit inside `side`×`side`, never up; `tokens` is constant.

### 5.2 Where a declaration lives

| Model | Declared by | Stored in |
|---|---|---|
| Curated local GGUF | the catalog entry (`CuratedVision`, §12.1): projector file + geometry | code |
| whether the local model *uses* it | the user; on by default once the projector is downloaded | `LocalModelConfigDto.vision: Option<bool>` (`None` = on when the projector is present) |
| Uncurated local GGUF | nobody in this build (§20) | — |
| Remote llama.cpp, Ollama, cloud | the user, per model, in Settings | `LLMSettingsDto.vision_declarations: BTreeMap<String, VisionDeclarationDto>` keyed by `ModelRef::to_string()` (`llamacpp:<model>`, `ollama:<tag>`, the cloud `Display` form) |
| Cloud, undeclared | `cloud_vision_default` | code |
| Anything else | `None` — the model cannot see; attached pictures fall back to text | — |

`contracts/settings/llm.rs`:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VisionDeclarationDto { pub enabled: bool, pub geometry: VisionGeometry }
// in LLMSettingsDto:
#[serde(default)] pub vision_declarations: BTreeMap<String, VisionDeclarationDto>,
```
`contracts/settings/generation.rs`: `LocalModelConfigDto.vision: Option<bool>` (`#[serde(default)]`). The settings validator (`problems`) rejects a `Grid` with `token_side == 0` or `min_pixels > max_pixels`, a `Fixed` with `side == 0`, a `Tiles` with `tile_side == 0`.

Resolution (`features/llm/di/roles.rs`, when an adapter is built for a `ModelRef`): local → `engine/vision.rs::local_vision(model_path, local_config) -> Option<(PathBuf, VisionGeometry)>` (catalog entry has `vision`, projector file present, config not `Some(false)`); `LlamaCpp`/`Ollama` → the declarations map, `enabled` only; `Cloud` → the map, else `cloud_vision_default`. The adapter is constructed with the result (`ServerConfig.vision`, `OllamaClient.vision`, `CloudLlm.vision`) and logs `llm.vision_declared { model, kind, max_tokens }` once. Nothing is read from any server.

### 5.3 UI

- `src/features/settings/components/AITab/VisionDeclarationEditor.tsx` (new, + `.test.tsx`): props `{ value: VisionDeclaration | null, defaultGeometry: VisionGeometry | null, onChange }`. A select (Off, presets, Custom) and, under a disclosure "Geometry", the enum's fields as numeric inputs with the `kind` as a segmented control; a live preview line using the same arithmetic ported to TypeScript in `src/utils/visionGeometry.ts` (+ test mirroring the Rust cases: the two must agree on the example rows). Used by `LlamaCppMetaRow.tsx`, `OllamaMetaRow.tsx` and the cloud model rows in `ModelsTab.tsx` (one row per configured cloud model); saves through `useLlmSettings().saveLlmUpdates({ visionDeclarations })`.
- `modelRoles/LocalModelConfigModal.tsx`: a "Vision" section: the projector's download state (from `DownloadedModel.vision`, §12.2) with a "Download projector" button calling `modelApi.startVisionProjectorDownload(modelId)`, and the "Let this model see images" switch bound to `localModelConfigs[id].vision`. Hidden when the catalog declares no projector.
- `modelRoles/LocalModelRow.tsx`: a `Vision` badge when the projector is downloaded and enabled; the composer's `ModelPickerPopover.tsx` shows "sees images" from a new `seesImages: boolean | null` on `EffectiveChatModel` (`utils/chatModelSelection.ts`), fed by `TurnModelDto.sees_images` and the downloaded-model DTO.
- Tokens only: `--color-text-muted`, `--color-accent-muted` for the badge, `--color-warning-muted` for the "not shown" chip; checked in both themes.

## 6. Message shape

### 6.1 `CompletionInput::Multimodal` — `ports/llm_port.rs`

```rust
pub enum CompletionInput {
    Native { value: serde_json::Value },
    Message { role: String, content: String },
    /// A message with pictures. Only `role == "user"` may carry images; every
    /// adapter refuses any other role with `AppError::InvalidInput`.
    Multimodal { role: String, parts: Vec<ContentPart> },
    ToolCall { id: String, name: String, arguments: serde_json::Value },
    ToolResult { id: String, output: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart { Text { text: String }, Image(ImagePart) }

/// One picture as the model receives it: already fitted to the model's
/// declared geometry (§7). `tokens` is what that geometry charges for
/// `width`×`height`; every counter below adds it instead of the base64.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePart {
    pub mime: String,          // "image/jpeg" | "image/png"
    pub data_base64: String,   // standard alphabet, no data: prefix
    pub width: u32, pub height: u32,
    pub tokens: u32,
    /// What the picture is, for text fallbacks and logs: the file name, or the
    /// `find_image` caption.
    pub alt: String,
}

impl CompletionInput {
    /// The words a tokenizer would read: `Message` content, `Multimodal` text
    /// parts and each image's alt, call names/arguments, results. Empty for `Native`.
    pub fn text(&self) -> std::borrow::Cow<'_, str>;
    pub fn image_tokens(&self) -> usize;
    /// `Multimodal` → `Message` whose content is the text parts followed by one
    /// `[Picture: alt (W×H)]` line per image. Other variants unchanged.
    pub fn without_images(&self) -> CompletionInput;
    pub fn is_user(&self) -> bool;
}
impl CompletionRequest {
    pub fn image_tokens(&self) -> usize;
    pub fn has_images(&self) -> bool;
}
```

`application/services/completion_input.rs`:
```rust
/// Give the newest user message `images`: `Message` → `Multimodal` with the
/// images first and the text last (how the model families above are prompted).
/// Returns `false` when the input has no user message (nothing attached).
pub fn attach_images_to_last_user(input: &mut Vec<CompletionInput>, images: Vec<ImagePart>) -> bool;
/// Every item through `without_images`, for anything persisted or replayed.
pub fn strip_images(input: &[CompletionInput]) -> Vec<CompletionInput>;
```

### 6.2 `LLMPort::vision`

```rust
/// The geometry this model's vision encoder was declared with, or `None` for a
/// model that cannot see. Declared (catalog, Settings, provider default), never probed.
fn vision(&self) -> Option<VisionGeometry> { None }
```
Implemented by `LlamaCppLlm` (`ServerConfig.vision: Option<VisionGeometry>`), `OllamaClient`, `CloudLlm`; delegated by `ScheduledLlm`, `PinnedLlm`, `PatientModel`; `MockLLMPort` and the test doubles take it from a builder (`with_vision`).

### 6.3 Wire formats

- llama.cpp (`llama_cpp/mod.rs::chat_messages`): `Multimodal` → `{"role":"user","content":[{"type":"image_url","image_url":{"url":"data:<mime>;base64,<data>"}}, …, {"type":"text","text":…}]}`. `coalesce_system_messages` is untouched (it only reads system messages). If `self.config.vision.is_none()` and the request `has_images()` → `AppError::InvalidConfig("<model> is not set up to see images (Settings → Models → Vision)")`, mirroring the `supports_tools` refusal.
- Ollama (`engine/ollama_client.rs::chat_request`): `OllamaChatMessage { role: "user", content: <text parts joined by "\n">, images: Some(vec![data_base64…]) }`; same refusal.
- OpenAI Responses (`cloud.rs`): `{"role":"user","content":[{"type":"input_image","image_url":"data:…","detail":"auto"}, …, {"type":"input_text","text":…}]}`.
- Anthropic: `{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":mime,"data":data}}, …, {"type":"text","text":…}]}`.
- Google (OpenAI-compatible chat path in `cloud.rs`): as llama.cpp.
- `cloud.rs` is 710 lines; the message builders move to `cloud/messages.rs` (`fn openai_input`, `fn chat_messages`, `fn anthropic_messages`) so neither file passes 800.

### 6.4 Text fallback

The chat feature decides before building the request: `turn.llm.vision()` is `None` → §10.6 appends a `[Picture attached: name (W×H). This model cannot see pictures; say so if the answer depends on it.]` line per picture and sends no image part. The adapter refusal is a safety net, not the path.

### 6.5 Every counter that reads a message

Each of these must treat `Multimodal` as its text parts (`CompletionInput::text()`) plus declared `image_tokens`, and must never count `data_base64` as characters: `llama_cpp/request.rs::{plain_prompt_tokens, exact_prompt_tokens}` (work on the typed input, not the JSON body — pass `&request.input` through; the body is built after), `llama_cpp/retry.rs::prefill_allowance` (same), `scheduler/rejection.rs::request_texts` (skip image parts), `scheduler/scheduled.rs::{prompt_texts, prompt_chars}`, `context_assembler/plan.rs::tokenizable_text`, `context_assembler/anchor.rs:151` (`is_user`), `chat/tool_loop.rs::chars_in_flight`. `exact_prompt_tokens` adds `TEMPLATE_TOKENS_PER_MESSAGE` once per message as today, plus the image tokens.

### 6.6 Persistence of requests

Research jobs save round transcripts (`turn/research.rs`). They save `strip_images(&transcript)`: a resumed job replays captions, as a later turn would. `CompletionRequest`'s `Serialize` is otherwise unchanged; `data_base64` is a string, so nothing else needs a serde attribute.

## 7. Image preparation

`features/web/images/prepare.rs` (new):
```rust
pub struct PreparedImage { pub mime: &'static str, pub bytes: Vec<u8>, pub width: u32, pub height: u32, pub tokens: u32 }
/// Decode `source` (already validated by magic bytes), apply its EXIF
/// orientation, downscale to `geometry.fit` (or `fit_within(max_tokens)`),
/// re-encode. Blocking: run on the blocking pool.
pub fn prepare(source: &[u8], mime: &str, geometry: VisionGeometry, max_tokens: Option<u32>) -> Result<PreparedImage, ImageRejected>;
```
- Decoder: the `image` crate (already in `Cargo.lock` at 0.25.10 transitively; add it to `src-tauri/Cargo.toml` with `default-features = false, features = ["jpeg","png","gif","webp"]`). `ImageReader::with_guessed_format` with `Limits { max_image_width: 16_384, max_image_height: 16_384, max_alloc: 512 MiB }` — a **security bound** against decompression bombs (a 16k×16k RGBA decode is 1 GiB; the limit refuses before allocating). A rejection is `ImageRejected::TooLarge`, surfaced to the composer as "This picture is too large to prepare".
- Orientation from the decoder's EXIF (`ImageDecoder::orientation`, `DynamicImage::apply_orientation`) before resizing. Re-encoding drops all metadata: an attached photo's GPS and camera tags never reach a server. Stated as a privacy property in the module doc.
- Resize: `imageops::resize(FilterType::Lanczos3)` to the fitted size; never upscale (`fit` guarantees it).
- Encode: PNG when the source is PNG or GIF (screenshots, diagrams, text: edges matter; alpha is flattened onto white first since encoders read RGB), JPEG quality 85 otherwise. `mime` reports what was written.
- Cache: `~/.lattice/chat-images/prepared/<id>-<w>x<h>.<ext>`; `ImageStore::prepared_path(id, w, h, ext)`. `ChatImageService::prepared` (§10.3) checks the cache, else prepares and writes atomically (the store's `write_atomically`). `gc.rs` removes `prepared/<id>-*` whenever `<id>` is removed, and sweeps orphans whose `<id>` has no row. The pick size and the full size are two files; a second request at the same size is a read.
- Memory in the request: a 2 MP JPEG at quality 85 is ≈ 300–500 KB, ≈ 0.5–0.7 MB as base64. Six pick candidates at ≈ 340 tokens each are under 100 KB each.

## 8. Token accounting and what gives way

- Planning (`chat/turn/budget.rs::plan`): `BudgetRequest::current_input` gains the pictures' tokens, computed from the rows' stored `width`/`height` through `geometry.plan` — no decoding needed to budget. Pictures are the reader's own material, paid with the message before evidence (passages, folder block, memory) is sized, as attachments are.
- Shrink, then drop (`turn/pictures.rs`): if the fixed parts (system + tools + message + pictures) exceed capacity, re-fit pictures from the last attached to the first with `fit_within(remaining)`, down to `min_tokens`; if still over, drop pictures last-first (each becomes its caption line plus a `status` event "picture not shown: the window has no room"); if still over, the turn is refused by #85 as today. The counts are logged: `chat.pictures_carried { count, tokens, shrunk, dropped }`.
- `ensure_fits` and `measured_body`: `prompt_tokens += request.image_tokens()` in both the estimate and the exact path; `RequestSize` gains `image_tokens: usize` and `describe()` reads "a prompt of 7,900 (text counted exactly; 2,052 tokens for 2 pictures estimated from the model's declared geometry)". The `counted_exactly` flag stays about the text.
- `prefill_allowance`: image tokens count as prompt tokens. Encoding a picture costs more wall time per token than text does; the allowance is what the live checklist watches (§19.3), and raising it is derived from the measured tokens, never a fixed add.
- The side pick (§9) is its own request through the same scheduler, so it gets the same check.

## 9. Use A — `find_image` keeps what shows the subject

### 9.1 Why a side request

Three places could show the candidates: inside the tool result (tool role), in a follow-up user message in the same round, or in a separate request. The tool role is out: llama.cpp documents image parts for user messages only and nothing confirms tool-role images (§2). A follow-up user message puts candidate pictures into the conversation's own transcript: they would sit in the prefix for the rest of the turn, be stripped afterwards (so the prefix changes at that point anyway), and invent a user turn the reader never wrote. A separate request keeps the conversation's transcript as it is, gets a structured verdict, works identically on every provider, and is bounded by the turn's deadline and Stop like any other call. Its cost is one small request; with llama-server's prompt checkpoints the conversation's slot cache is swapped out and back, not rebuilt, and on a multi-slot server a distinct `cache_key` keeps it off the conversation's slot.

### 9.2 Flow (`tool_loop/images.rs` → new `tool_loop/image_pick.rs`)

After `find_and_store` returns `(output, dtos)` with ≥ 1 candidate and `scope.llm.vision()` is `Some(geometry)`:
1. Per-candidate budget: `per = max(geometry.min_tokens(), geometry.max_tokens() / candidates)` — the whole pick costs the model about what one picture in the answer would. For each candidate, `container.chat_images().prepared(id, geometry, Some(per))`; a candidate that cannot be prepared is kept unjudged (it stays in the result as today).
2. Request: `input = [Message{system: PICK_SYSTEM}, Multimodal{user: [images…, Text(PICK_USER)]}]`, `json_schema = {"type":"object","properties":{"keep":{"type":"array","items":{"type":"string","enum":[refs…]}}},"required":["keep"]}`, `sampling: SamplingOverride::deterministic()`, `priority: scope.priority`, `cancel: scope.cancel`, `cache_key: Some(format!("{conversation_id}:image-pick"))`, `tools: []`, `thinking: Off`. `PICK_SYSTEM` is constant text (the same across calls, so a server caches it): "You judge candidate pictures for a reader. Keep a candidate only if it clearly shows the subject asked for. Keep none when none does." `PICK_USER`: "Subject: `<query>`\nContext: `<the turn's subject digest, as attachments.rs::subject_digest produces>`\nCandidates in order: img-1 "caption" … Return JSON." Run under `run_tool_bounded` with the turn clock.
3. Verdict: parse `keep`; filter `output.images` and the stream `images` to the kept refs; rewrite `note` to "N candidates were shown to you and M kept" or to `NO_PICTURE_NOTE` when none. Rows not kept stay leased and are released at persist as today (`attach_to_message` releases unreferenced refs) — no new lifecycle. Logged: `chat.image_pick { candidates, kept, tokens, elapsed_ms }`.
4. Failure (refused, timed out, unparseable, cancelled): the unfiltered result is returned, logged `chat.image_pick_skipped { reason }`. The pick never fails the tool.

### 9.3 Sizes

Nothing fixed: the pick size comes from the geometry (`fit_within(per)`), the full-answer picture is the stored file (the reader's webview shows the original); the model never needs the full size for a pick.

## 10. Use B — attached pictures

### 10.1 Frontend

- `useAttachmentImport.importStaged` splits staged paths by extension: images (`png jpg jpeg gif webp`) → `chatApi.attachImages(conversationId, paths)` → `ChatImageDto[]`; everything else → the existing import. It returns `{ names, documentIds, imageIds }`. Staged images render as thumbnails from the file path (Tauri `convertFileSrc`) before send and from `lattice-image://` after.
- `useComposer` → `sendMessage(..., attachmentNames, attachmentDocumentIds, attachmentImageIds)`; `chatApi.sendMessage` gains `attachmentImageIds?: string[]`; `useConversationTurnLifecycle.ts` carries it. Removing a staged picture or discarding the draft calls `chatApi.releaseImages(ids)`.
- `Message.tsx`: a user message with `metadata.images` renders them through the same figure path as answers (`TiptapViewer images=` already exists; the user bubble passes the list and the plain text). Each figure gets a "Show to the model again" action (stages a copy: `chatApi.restageImage(conversationId, imageId)` → new row via the service's copy) and, when `metadata.picturesSeen === false`, a chip "Not shown to <model>".
- `ChatImage` type (`src/types/conversation.ts`): `sourceUrl: string | null`, `origin: 'wikimedia_commons' | 'page' | 'attachment'`, `fileName?: string`. The figure omits the citation chip for `attachment`.

### 10.2 Backend command — `features/conversation/commands/images.rs` (new; registered in `plugin.rs`)

```rust
#[tauri::command] async fn attach_message_images(conversation_id: String, paths: Vec<String>) -> Result<Vec<ChatImageDto>>;
#[tauri::command] async fn release_message_images(conversation_id: String, image_ids: Vec<String>) -> Result<()>;
#[tauri::command] async fn restage_message_image(conversation_id: String, image_id: String) -> Result<ChatImageDto>;
```
`attach_message_images` reads each path (user-chosen, from the drop or dialog; refused unless it is a regular file under the fetch byte bound already used for web pictures), validates by magic bytes (`fetch::validate_image`), and calls `store_attachment` with `ImageTurnScope { conversation_id, request_id: format!("draft:{ulid}") }` — one draft id per call. Refs are `att-1…` within the draft (the `UNIQUE(request_id, ref)` rule holds).

### 10.3 `ChatImagesPort` additions — `ports/chat_images.rs`

```rust
/// Store a picture the reader attached, from bytes already validated. The row
/// is leased to `scope.request_id` (a draft) until a message claims it.
async fn store_attachment(&self, scope: &ImageTurnScope, file_name: &str, bytes: Vec<u8>) -> Result<ChatImageDto>;
/// The picture fitted to `geometry` (within `max_tokens` when given), from the
/// prepared cache or prepared now. `None` when the row is gone.
async fn prepared(&self, image_id: &str, geometry: VisionGeometry, max_tokens: Option<u32>) -> Result<Option<PreparedImageDto>>;
/// Rows the conversation holds for these ids, in the order given; ids of other
/// conversations are dropped, not errors.
async fn rows_for_ids(&self, conversation_id: &str, image_ids: &[String]) -> Result<Vec<ChatImageDto>>;
/// Claim draft rows for a persisted user message (`message_id` set, `request_id`
/// rewritten to the turn's). Idempotent for a regenerate.
async fn attach_ids_to_message(&self, conversation_id: &str, image_ids: &[String], message_id: &str, request_id: &str) -> Result<Vec<ChatImageDto>>;
/// Release draft rows the composer discarded. Ignores rows already attached.
async fn release_images(&self, conversation_id: &str, image_ids: &[String]) -> Result<()>;
/// A new draft row with the same file, for "show to the model again".
async fn restage(&self, conversation_id: &str, image_id: &str) -> Result<ChatImageDto>;
```
`contracts/images.rs`: `ImageOrigin::Attachment`; `ChatImageDto.source_url: Option<String>`, `file_name: Option<String>`; `PreparedImageDto { mime, data_base64, width, height, tokens }` (`From<PreparedImageDto> for ImagePart` with `alt` supplied by the caller). The fake in `chat/test_runtime.rs` records each call as it does today.

### 10.4 Migration `20261018090000_chat_image_attachments.sql`

Rebuild `chat_images` (it has no child tables, so the copy/drop/rename is safe — see `sqlite-table-rebuild` note): `source_url TEXT` and `image_url TEXT` become nullable, `origin` CHECK gains `'attachment'`, new `file_name TEXT`; indexes recreated. `repository.rs` maps the nullable columns; the startup sweep treats `request_id LIKE 'draft:%'` rows older than the app's previous start as unclaimed (the composer that staged them is gone).

### 10.5 The turn — `chat/turn/pictures.rs` (new)

```rust
pub(super) struct CarriedPictures { pub parts: Vec<ImagePart>, pub captions: Vec<String>, pub tokens: usize, pub seen: bool, pub shrunk: usize, pub dropped: usize }
pub(super) async fn carry_pictures(container: &dyn ChatRuntime, turn: &PreparedTurn, image_ids: &[String], room: usize) -> CarriedPictures;
```
`prepare.rs` loads `rows_for_ids` before `budget.plan` so the tokens are in `current_input`; `carry.rs` calls `carry_pictures` with the room the plan left for the fixed parts; `assemble.rs::plan_request` runs `attach_images_to_last_user` on both paths (`memory_turn.plan.messages` and the `from_context` result). The tool loop carries the `Multimodal` message unchanged through every round of the turn (the prefix within a turn is stable). `persist_user_message_pending` gets `attachment_image_ids` and calls `attach_ids_to_message`, stamps `metadata.images` and `metadata.picturesSeen`. Regenerate re-reads `metadata.images` ids, as it re-reads attachment ids. `plugin_impl.rs` (two call sites, lines 185 and 236) gains `attachment_image_ids: Option<Vec<String>>`.

### 10.6 Models that cannot see

`carry_pictures` with `turn.llm.vision() == None` returns no parts, one caption line per picture (`[Picture attached: name (W×H). This model cannot see pictures; say so if the answer depends on it.]`) appended to the message text, `seen: false`; a `status` event tells the window ("<model> cannot see pictures; it was told they are there"), and `metadata.picturesSeen = false` drives the chip.

## 11. History, memory, handoff, grounding

- Later turns: a user message replays as its text plus `[Picture attached: name]` per `metadata.images` entry, in `context_window_builder.rs` (string history), `context_assembler/render.rs` (`SourceMessage` gains `pictures: Vec<String>`; `render_recent` appends), `history_tools/read.rs`, `memory/summarize.rs`, `handoff.rs`; `shared/markdown_media.rs` gains `pub fn picture_captions(names: &[String]) -> String`. Pixels are not replayed: the prefix changes once (the turn after the picture) and is then stable, and no cache eviction can force a re-encode. The reader re-attaches explicitly when a later question needs the pixels (§10.1).
- Fork and tangents: `copy_for_messages` already copies rows per message id; it must include user messages' rows (agent verifies the caller passes every copied message).
- Export and backup: rows and files are generic; the Markdown exporter writes a user message's pictures beside it as it does an answer's.
- Grounding: the judge is text-only and stays so. A claim that could only be checked against a picture has no text source; the lexical pre-filter already strips picture lines from the answer. New: when the answer cites an attached picture (the source entry `chunk_id = image:<ref>` from PR 2's lifecycle, extended to `att-N`), the verifier marks the claim `unverifiable: picture` rather than unsupported, and the UI shows the existing "not checked" state. No vision in the judge in this build (§20).

## 12. Local sidecar vision

### 12.1 Catalog — `domain/models/curated/chat.rs`

```rust
pub const CHAT_PROJECTOR_FILE: &str = "mmproj.gguf";   // local name beside the weights, as OCR does
#[derive(Debug, Clone, Copy)]
pub struct CuratedVision {
    pub projector_remote_file: &'static str, pub projector_bytes: u64, pub projector_sha256: &'static str,
    pub geometry: VisionGeometry,
}
// CuratedChatModel gains: pub vision: Option<CuratedVision>,
pub fn chat_model_vision(model_path: &Path) -> Option<CuratedVision>;   // same directory+file match as chat_model_uses_mtp_draft
```
For each entry the agent reads the repository tree (`https://huggingface.co/api/models/<repo>/tree/main`) for `mmproj-F16.gguf` (or the repo's BF16 when no F16 exists) size and `lfs.oid`, and the projector's GGUF header for geometry: `clip.vision.patch_size` × `clip.vision.spatial_merge_size` (or `clip.has_qwen2vl_merger` ⇒ 2) gives `token_side`; `clip.vision.image_size` with no merger gives `Fixed { side, tokens = (side/patch)² / pooling }`; `max_pixels` for a `Grid` entry is the model card's recommended maximum (Qwen 3.x: 2048 squares). Entries whose repository ships no projector keep `vision: None` (expected for some Gemma 4 quantizations; verified per repo, not assumed). `metadata()` adds `VISION_CAPABILITY` to `capabilities`, sets `ModelMetadata.vision: Option<VisionGeometry>` and `ModelMetadata.vision_projector: Option<ModelFileMetadata>` (both `#[serde(default)]`; the `files` vector still lists the weights alone so a model download stays one file). `memory_estimate_gb` adds `projector_bytes` when a projector is declared, so "Est. memory" is honest for a seeing model.

### 12.2 Download and state

- `features/model_management/commands.rs`: `start_vision_projector_download(model_id)` builds a download for `vision_projector` into `<models>/<model id>/mmproj.gguf` through the existing download manager (one `ModelFileMetadata` with checksum; the manager hashes as it writes). `delete_downloaded_model` removes the projector with the weights.
- `DownloadedModel` (domain and `src/types/downloadedModels.ts`) gains `vision: Option<DownloadedVisionDto { projector_downloaded: bool, enabled: bool, geometry: VisionGeometry }>`, computed by the storage adapter from the catalog entry, the file's presence and `local_model_configs[id].vision`.
- `engine/vision.rs` (new): `local_vision(model_path, config: Option<&LocalModelConfigDto>) -> Option<(PathBuf, VisionGeometry)>`.

### 12.3 Launch — `sidecar_manager/config.rs`, `readiness.rs`, `engine/factory.rs`

`SidecarConfig.projector_path: Option<PathBuf>`; `build_server_args` appends `--mmproj <path>` when set, keeping `--spec-type draft-mtp` and `-np 1` exactly as today. `factory.rs::sidecar_config` sets it from `local_vision`; `create_local_llm_sidecar` passes `Some(geometry)` into `LlamaCppLlm::sidecar` (new parameter; `ServerConfig.vision`). Memory sizing (`from_capabilities`) counts the projector's bytes as resident weights.

Speculative decoding with a projector: upstream issue ggml-org/llama.cpp#19712 ("speculative decoding is not supported with multimodal", opened 2026-02-18) was closed on 2026-04-23 with the note that it is enabled by default since #19493. The pinned tag `v0.6.0` post-dates that by months, so the bundled binary accepts both. The launcher therefore never drops either flag. Belt and braces: `readiness.rs` recognises the old refusal line in the server's stderr (`speculative decoding is not supported with multimodal`) and, instead of retrying, fails the start with `AppError::InvalidConfig("This llama-server cannot draft and see at once. Turn off either Vision or Speculative decoding for <model> in Settings → Models.")`, which the configuration modal shows beside the two switches. Nothing is turned off for the user. The first live start on each platform (§19) confirms the flags coexist.

`ocr/launcher.rs:5` and `ocr_port.rs:34` are reworded: the OCR server is separate because it serves a different model for minutes at a time with its own window, not because a projector forbids drafting.

### 12.4 Not probed

`local_vision` reads the app's own catalog, its own download directory and its own settings. The remote llama.cpp adapter reads the declaration map. No `/props` modality field is read anywhere; the existing `/props` read in `scheduler/registry.rs` (slot count) is unchanged and not extended.

## 13. Security and bounds

- Attach paths are user-chosen files; bytes are validated by magic bytes before decoding; decode limits (§7) bound memory; the stored file is served only over `lattice-image://` to the main webview, as today.
- Nothing remote is loaded by the webview; staged thumbnails use `convertFileSrc` on the user's own file.
- Re-encoding strips EXIF (location, device) before any server sees the picture.
- Drafts: rows leased to `draft:*` are released when the composer discards them and swept at the next start.
- No server is asked what it supports; a mismatch is an explicit refusal with a Settings pointer.

## 14. Testing

Unit (lattice-core): `domain/models/vision/tests.rs` — `Grid` 28 and 32 (`1280×1600` → fit and tokens; never upscales `400×300`; `fit_within` halves area until under budget; stops at `min_pixels`), `Fixed` (constant tokens; `fit_within` is `None` below), `Tiles` (OpenAI `1024×1024` = 85 + 4·170 = 765; max tiles), `CONSERVATIVE_DEFAULT` ≥ each preset's tokens at its fit for three sizes, `cloud_vision_default` names, preset validation; `completion_input` tests — `attach_images_to_last_user` on both paths, `strip_images`, `text()` excludes base64; `markdown_media::picture_captions`.

Adapters: `llama_cpp/request_tests.rs` — `Multimodal` body shape; base64 not counted (`plain_prompt_tokens` of a 400 KB image ≈ its text + declared tokens); `measured_body` adds image tokens and the refusal names pictures; a `ServerConfig` without `vision` refuses image parts; `prefill_allowance` ignores base64; non-user role refused. `cloud/tests.rs` — OpenAI `input_image`, Anthropic `image` block, Google `image_url`; refusal without `vision`. `engine/ollama_client/tests.rs` — `images` field. `scheduler/tests.rs` — `ensure_fits` counts images in estimate and exact paths; message text. `scheduled`/`pinned`/`patient_model` delegate `vision()`.

Web images: `prepare` (decode → orient → fit → encode for JPEG/PNG/GIF/WebP fixtures generated in the test; alpha flattened; limits refuse a crafted 20k×20k header; EXIF orientation 6 rotates), cache path and hit, `gc` removes `prepared/<id>-*`, `store_attachment` (origin, refs `att-1..`, nullable columns), `rows_for_ids` ignores other conversations, `attach_ids_to_message` idempotent, `release_images`, `restage`, the migration applied on the sqlx test pool.

Conversation (through `test_runtime.rs` fakes and a scripted `LLMPort` with `with_vision`): pick keeps `img-2` and the result/stream carry only it; pick failure keeps all; no vision → no pick; `carry_pictures` builds a `Multimodal` last user message with images first; tokens charged in `current_input`; shrink then drop order; non-vision caption line and `picturesSeen=false`; `persist_user_message_pending` attaches draft rows; history builders caption user pictures; research transcript saved without images; `copy_for_messages` covers user rows.

Integration (`src-tauri/tests/vision_request_body.rs`, run once at the end): a stubbed llama-server on loopback (wiremock; the LLM adapter's `reqwest::Client` is not the guarded web client, so loopback is reachable) records the Chat Completions body for a turn with one attached PNG on a `LlamaCppLlm` declared `Grid 28`: asserts `messages[last].content` has `image_url` then `text`, the data URI decodes to a JPEG of the fitted size, `max_tokens` equals window − (text estimate + image tokens), `tools` present, and that the same turn on a client without `vision` sends a text-only body containing the caption line.

Frontend (vitest): `VisionDeclarationEditor.test.tsx` (preset fills fields; Custom edits; cost preview agrees with `visionGeometry.test.ts` vectors copied from the Rust tests), `LocalModelConfigModal.test.tsx` (vision section states; toggle saves `vision`), `LocalModelRow.test.tsx` (badge), `useAttachmentImport.test.tsx` (images split from documents; discard releases), `Message.test.tsx` (user figures, no citation chip, "Not shown" chip), `ModelPickerPopover` ("sees images"). `npm run type-check`, `npm run lint`, `export_bindings --check`.

## 15. File-by-file change list

lattice-core
- `domain/models/vision.rs`, `vision/tests.rs`, `vision/cloud_defaults.rs` (new); `domain/models/mod.rs` (`pub mod vision;`).
- `domain/models/curated/chat.rs`: `CuratedVision`, `CHAT_PROJECTOR_FILE`, `vision` on every entry, `chat_model_vision`, `metadata()` fields, memory estimate.
- `domain/models/selection/model_metadata.rs`: `vision`, `vision_projector`. `domain/models/downloaded.rs`: `vision`.
- `application/ports/llm_port.rs`: `Multimodal`, `ContentPart`, `ImagePart`, helpers, `LLMPort::vision`, `CompletionRequest::{image_tokens,has_images}`; its tests.
- `application/services/completion_input.rs`: `attach_images_to_last_user`, `strip_images`.
- `application/services/context_assembler/{plan,anchor,render}.rs`: counters; `SourceMessage.pictures`. `context_window_builder.rs`: user captions.
- `application/contracts/settings/llm.rs` (`vision_declarations`, `VisionDeclarationDto`), `generation.rs` (`LocalModelConfigDto.vision`), validation.
- `application/contracts/images.rs`: `ImageOrigin::Attachment`, nullable `source_url`, `file_name`, `PreparedImageDto`.
- `application/ports/chat_images.rs`: six methods (§10.3).
- `shared/markdown_media.rs`: `picture_captions`.

features/llm
- `llama_cpp/mod.rs` (`ServerConfig.vision`, `sidecar` parameter, `chat_messages`, refusal), `llama_cpp/request.rs` (typed counting), `llama_cpp/retry.rs`, `llama_cpp/request_tests.rs`.
- `scheduler/rejection.rs` (`RequestSize.image_tokens`, `describe`, `request_texts`), `scheduler/scheduled.rs` (`ensure_fits`, counters, delegate), `scheduler/tests.rs`.
- `cloud.rs` → `cloud/messages.rs` (new) + `vision` field; `cloud/tests.rs`.
- `engine/ollama_client.rs`, `engine/ollama_client/tests.rs`.
- `engine/vision.rs` (new); `engine/factory.rs` (`sidecar_config`, `create_local_llm_sidecar`, `create_ollama_llm`, mock `with_vision`); `engine/sidecar_manager/{config,readiness,tests}.rs`; `di/roles.rs` (declarations → adapters); `instances/pinned.rs`.
- `learning/patient_model.rs` (delegate).

features/model_management, download: `commands.rs` (`start_vision_projector_download`, delete), storage adapter (`DownloadedModel.vision`).

features/ocr: `launcher.rs` and lattice-core `ocr_port.rs` comments.

features/web/images: `prepare.rs` (new), `service.rs` (six methods, prepared cache), `repository.rs`, `store.rs` (`prepared_path`), `gc.rs`, `tests.rs`; `src-tauri/migrations/20261018090000_chat_image_attachments.sql`; `src-tauri/Cargo.toml` (`image`).

features/conversation: `commands/images.rs` (new) + `plugin.rs` registration + `plugin_impl.rs` (`attachment_image_ids`); `chat/turn/pictures.rs` (new), `turn/{prepare,carry,assemble,budget,research}.rs`; `tool_loop/image_pick.rs` (new), `tool_loop/images.rs`, `tool_loop.rs` (`chars_in_flight`); `chat/persistence.rs`; `chat/dto.rs` (`status` text only); `turn_record.rs` (`TurnModelDto.sees_images`); `history_tools/read.rs`, `memory/summarize.rs`, `handoff.rs`; `verification/{lexical,judge}.rs` (`unverifiable: picture`); `chat/test_runtime.rs`, `chat/tests.rs`, `tool_loop/tests.rs`.

bindings: `src/lib/bindings.ts` regenerated.

frontend: `src/utils/visionGeometry.ts` (+test), `src/features/settings/components/AITab/VisionDeclarationEditor.tsx` (+test), `AITab/{ModelsTab,LlamaCppMetaRow→modelRoles/LlamaCppMetaRow,OllamaMetaRow}.tsx`, `modelRoles/{LocalModelConfigModal,LocalModelRow}.tsx` (+tests), `src/features/model/api/client.ts`, `src/types/downloadedModels.ts`, `src/types/conversation.ts`, `src/features/chat/api/client.ts`, `hooks/{useAttachmentImport,useComposer}.ts`, `controller/useConversationTurnLifecycle.ts`, `components/{Message,ModelPickerPopover}.tsx`, `components/composer/Composer.tsx` (thumbnails), `src/utils/chatModelSelection.ts`, tests beside each.

## 16. PR split and build tracks

PR 1 `feat: vision capability, multimodal requests and image token accounting` — §5, §6, §8 (adapters, counters, settings DTOs, declaration UI, model picker badge). No behaviour change for anyone until a declaration exists.
PR 2 `feat: attached pictures the model sees` — §7, §10, §11 (prepare + cache, attachment rows + migration, turn carry, history captions, composer and bubble). Depends on PR 1.
PR 3 `feat: find_image keeps the picture that shows the subject` — §9. Depends on PR 2's `prepared`.
PR 4 `feat: local sidecar vision (projector download, --mmproj)` — §12. Depends on PR 1; independent of 2 and 3.

All branch from `feat/chat-vision`; PR 2 and PR 4 may open together once PR 1 is merged or its contract files are final. At most three agents at once, each owning the files listed and touching no other. Rust work runs `cargo check` and targeted `--lib` tests only, the integration test once at the end; changed files are formatted with `rustfmt <file>` (not `cargo fmt`, which rewrites files other tracks own). Frontend commands run from the repo root.

Track A — core contract and the model layer (PR 1, then PR 4)
- Owns: `lattice-core/src/domain/models/vision*`, `domain/models/mod.rs`, `domain/models/curated/chat.rs`, `domain/models/selection/model_metadata.rs`, `domain/models/downloaded.rs`, `application/ports/llm_port.rs` (+tests), `application/services/completion_input.rs`, `application/services/context_assembler/{plan,anchor}.rs` (counters only), `application/contracts/settings/*`; all of `src-tauri/src/features/llm/**`, `features/learning/patient_model.rs`, `features/model_management/**`, `features/download/**`, `features/ocr/launcher.rs`, `lattice-core/.../ocr_port.rs` (comments), `src/lib/bindings.ts` regeneration.
- Writes `vision.rs`, the `llm_port.rs` additions, `completion_input.rs` helpers and the settings DTOs **first**, verbatim from §5–§6, so B and C compile against them within A's first step.
- Verify: `… cargo check -p lattice-core` · `… cargo check` · `… cargo test -p lattice-core --lib domain::models::vision application::ports::llm_port application::services::completion_input` · `… cargo test --lib features::llm` · `… cargo test --lib features::model_management` · `… cargo run --features bindings-export --bin export_bindings` then `… -- --check` · `bash scripts/check-rust-layer-boundaries.sh` (no file over 800 lines — `cloud.rs` split; `too_many_arguments` 0; `global-state` 0; `raw-spawn` 0).

Track B — pictures and the turn (PR 2, then PR 3)
- Owns: `lattice-core/src/application/contracts/images.rs`, `application/ports/chat_images.rs`, `shared/markdown_media.rs`, `application/services/context_assembler/render.rs`, `application/services/context_window_builder.rs`; all of `features/web/images/**`, the migration, `src-tauri/Cargo.toml` (`image` line only); all of `features/conversation/**` listed in §15; `interfaces/di/**` if a container accessor changes.
- Depends on A's contract files (compiles against them once A's first step lands; until then, against this document with a local stub that is deleted before the PR).
- Verify: `… cargo check` · `… cargo test -p lattice-core --lib shared::markdown_media application::services::context_assembler application::services::context_window_builder` · `… cargo test --lib features::web::images` · `… cargo test --lib features::conversation::chat::turn features::conversation::chat::tool_loop features::conversation::chat::persistence features::conversation::memory features::conversation::handoff` · `python3 scripts/check-tauri-command-inventory.py` · `bash scripts/check-rust-layer-boundaries.sh` (`feature-edge features::conversation features::web` stays 2; files ≤ 800 lines — `attachments.rs` is 549 and `persistence.rs` is near 470, so the new logic goes in the new files).

Track C — frontend (PR 1 settings + badge; PR 2 composer + bubble; PR 4 projector controls)
- Owns: everything under `src/` except `src/lib/bindings.ts` (reads it after A regenerates; codes against §5–§6 field names before that), plus rig fixtures.
- Verify: `npx vitest run src/features/settings src/features/chat src/utils src/components/TiptapEditor` · `npm run type-check` · `npm run lint` · UI rig shots `both` for Settings → Models and a chat with an attached picture.

Integration (owner or the last agent, once): `… cargo test --test vision_request_body`, `npm run type-check && npm run lint && npx vitest run`, `bash scripts/check-rust-layer-boundaries.sh`, then the PRs in order.

## 17. Decisions and rejected alternatives

- A new `CompletionInput` variant rather than a field on `Message`: five builders and four counters change instead of 126 sites; every `if let`/`matches!` site stays valid.
- Bytes travel as base64 in the request, not as references the adapter resolves: adapters stay inside `features::llm` and never reach the image store; research transcripts are stripped before saving, so size never reaches the database.
- Attached pictures are `chat_images` rows, not library documents: images are not importable documents today; the picture store already has serving, leasing, sweep, fork copy, export and backup; adding OCR-to-library for images is a separate feature.
- Side request for the pick, not tool-role or follow-up messages (§9.1).
- Captions after the sending turn, not replayed pixels (§11): one prefix change then stability, no re-encoding on cache eviction, no per-turn picture cost; the reader can re-attach.
- Geometry presets plus raw fields, not a free "tokens per image" number: the same declaration drives downscaling and cost, and a preview shows what it means.
- A conservative default that over-charges rather than under-charges: a refusal names the pictures and points to Settings; an overrun fails the request after the work.
- The projector is a separate download, not part of the model's file list: a reader who never attaches pictures does not fetch 0.5–1.5 GB.

## 18. Open decisions (each with a recommendation)

1. Pick budget rule `max_tokens / candidates` (§9.2). Alternative: always `min_tokens` (cheapest). Recommend the ratio: recognition at ≈ 500 px is reliable; at a 4-square minimum it is not.
2. Cloud rows in Settings: one declaration per configured cloud model name (recommended; mirrors `ModelRef`) vs one per provider. Per model keeps a user who runs a text-only `o3` and `gpt-4o` side by side honest.
3. Default for the local vision toggle once the projector is downloaded: on (recommended) vs off. Downloading it is the opt-in.
4. Shrink order: last-attached first (recommended, the reader's newest addition is the most dispensable) vs proportional shrink of all. Proportional is fairer but makes every picture worse at once.
5. `Fixed` geometry cannot shrink; a Gemma-class model with six pictures in an 8k window will drop rather than shrink. Accept (recommended) or letterbox two pictures into one tile (never: it changes what the model sees).
6. Whether the pick request should bypass bounded conversation memory entirely — it does (its own `input`); confirm nobody expects it in the turn record. Recommend a `TurnStepDto` "Looked at 3 candidates, kept 1" so the reader sees why pictures vanished.
7. Mainline `v0.6.0` + `--mmproj` + `--spec-type draft-mtp`: settled from upstream (#19712 → #19493) but not yet observed on the bundled binary; the §12.3 refusal path makes a surprise visible rather than silent. If the first live start refuses, the open decision becomes which switch the configuration modal defaults to; recommend vision (the user asked for it by downloading the projector).

## 19. Live checklist

19.1 Bundled sidecar (the default user). Download Qwen 3.5 9B, then its projector from the model row; start a chat. Logs: sidecar args line contains `--mmproj …/mmproj.gguf` and, with speculative decoding on for the MTP build, `--spec-type draft-mtp` too; server stderr has no `speculative decoding is not supported with multimodal`; `llm.vision_declared { kind: "grid", max_tokens }`. Attach a photo and ask what it shows: `chat.pictures_carried { count: 1, tokens, shrunk: 0, dropped: 0 }`; the answer describes the picture; the user bubble shows the figure without a "Not shown" chip. Ask a follow-up without re-attaching: the model answers from the caption and says it cannot see the picture now. Turn vision off in the modal, regenerate: the chip appears and the answer says it was told a picture exists. Set the window to 4,096 and attach three photos: shrink then drop in the log, and a refusal naming picture tokens if even that does not fit.

19.2 A remote llama.cpp server (OpenAI-compatible) with a vision model loaded: declare Vision = Qwen 3.x VL on the connection row; the preview shows the cost of a 1280×960 photo; the same attach test passes; `find_image` on "Saturn V launch" logs `chat.image_pick { candidates: 3, kept: 1 }` and the answer shows one picture; a query with no good candidate ("photograph of the number seven") logs `kept: 0` and the answer has none. Observe on that server whether the pick's swap-out/swap-in of the conversation's cache appears as a re-prefill (it should not with checkpoints); note the result in §21.

19.3 Ollama with a vision tag declared; one cloud provider on its default: attach test passes; the refusal path when the declaration is turned off; the prefill guard does not fire while a 2 MP picture is encoded on the sidecar — if it does, record the measured encode time per image token for the follow-up.

19.4 Lifecycle: discard a staged picture (row released); restart with a staged-but-unsent picture (swept); fork a chat with a user picture (figure works in the branch after the original is deleted); export (file beside the Markdown); backup and restore.

## 20. Follow-ups, not in this build

- Vision for uncurated GGUFs: choose a projector file in the model's configuration modal; HF browser stops filtering `mmproj-*` when a weights file is selected.
- Image OCR into the library (an attached screenshot becomes searchable text); a vision judge for picture-cited claims; an eval set for the pick (`scripts/vision_pick_eval.py`, run history in `docs/evals`).
- Replaying pixels for the most recent picture when the budget allows, if live use shows the re-attach step is too much friction.
- Keep the lead-image URL in the page cache (PR 2 follow-up) so `from_page_url` picks need no refetch.

## 21. As built

Filled in by the build agents: departures from §5–§16 and why; the geometry each catalog entry ended with and the header fields it was read from; the live results from §19.

### 21.1 PR 1: capability contract, multimodal requests, token accounting

Departures from §5, §6 and §8:

- Declarations are resolved where the adapters are built, `features/llm/loading.rs::load_ref`, rather than in `di/roles.rs`: that is the one place a `ModelRef` becomes a client. `engine/vision.rs::declared_vision` holds the rule. A changed declaration counts as a client change in `instances/invalidation.rs`, so the model reloads.
- A `Grid` picture smaller than `min_pixels` is charged as `min_pixels`, because the encoder enlarges it to that area itself. Without that, the count reads low. The TypeScript port uses the same rule, and both suites check it against the same rows.
- `CONSERVATIVE_DEFAULT` does not cover Anthropic (about 1,089 tokens at 896×896) or Gemini (1,032), as §5.1 said it did. It does cover the local families and OpenAI. This does not matter in practice: every `claude-*` and `gemini-*` model gets its provider default. The doc comment and test say so.
- `CompletionInput::Multimodal`'s parts, `ContentPart` and `ImagePart`, and the `text`/`image_tokens`/`without_images` helpers live in `llm_port/multimodal.rs`, which keeps `llm_port.rs` under 800 lines. `ImagePart`'s `Debug` prints the byte count, never the base64. `CompletionRequest` also gained `image_count`, so a refusal can name how many pictures it carried.
- `attach_images_to_last_user` takes `&mut [CompletionInput]`.
- `RequestSize` gained `image_tokens` and `image_count`. A refusal reads "text counted exactly; N tokens for K pictures estimated from the model's declared geometry".
- `CloudLlm::with_endpoint` points a cloud client at another URL that speaks the same format. The request-body integration test needs it.
- Research jobs strip pictures in the job journal (`research_job.rs`), for both the saved request and every round, not only in the round transcript.
- The settings validator rejects a degenerate geometry through `LLMSettingsDto::vision_problems`, in both settings validators.

### 21.2 PR 4: local sidecar vision

Departures from §12:

- `ModelMetadata` gained no `vision` or `vision_projector` fields. A catalog entry with a projector carries the `vision` capability tag. The projector itself is reached through `curated::chat_model_vision(model_path)`, which matches the download layout as `chat_model_uses_mtp_draft` does and returns the projector's path, pinned download and geometry. Adding the two fields would have touched every `ModelMetadata` literal for no reader that needs them.
- "Est. memory" stays the weights-only figure. Adding the projector would push the 2B past the first-run tier and move every recommendation for a download most readers never make. The sidecar's own sizing counts a downloaded projector as resident weights whether or not a start loads it, so switching Vision never changes the window a model is sized to.
- The download command is `start_vision_projector_download(id)` in the download plugin, taking the downloaded model's id as `delete_model` does. The download feature owns the manager. It returns `{ downloadId }`, which is null when the whole projector is already on disk. The download is not a model of its own: no model record waits on it. A projector of the pinned size is a finished one, because the manager deletes a file whose digest does not match.
- Projector state is reported on the downloaded-model response (`DownloadedVisionDto`: downloaded, enabled, geometry, bytes), not on the stored `DownloadedModel`.
- `engine/vision.rs::local_vision` takes the switch as a flag. The switch is `LocalServerOptions.vision`, set from `LocalModelConfigDto.vision` (on unless `Some(false)`). Changing it is a settings change, so it reloads the model's server, with or without `--mmproj`.
- A projector that finishes downloading while a text-only server runs changes the server's identity (`LocalServerOptions.projector_ready`, set when a role pins the model, and `SidecarConfig.projector_path`). The next request starts a server that sees. Invalidation ignores that flag, because it is not a setting.
- An older server's refusal to draft and see at once is recognised in `readiness.rs::refused_combination` and reported as `InvalidConfig` naming both switches. A new attempt kind, `Refused`, stops the fallback ladder without marking the file unloadable.
- `MockLLMPort` moved to `engine/factory/mock.rs` to keep `factory.rs` under 800 lines.

Catalog geometries. Every catalog repository publishes `mmproj-F16.gguf`. Its size and digest come from the Hugging Face tree listing (`size`, `lfs.oid`), and its encoder fields from the projector's GGUF header. Token limits and picture markers come from the pinned llama.cpp source (`tools/mtmd/clip.cpp`, `tools/mtmd/mtmd.cpp`).

- Qwen 3.5 2B, 4B, 9B and 9B MTP, Qwen 3.8 27B and Qwen 3.6 35B A3B: `Grid { token_side: 32, min_pixels: 8·32², max_pixels: 2048·32², overhead_tokens: 2 }`. The header gives `clip.projector_type = qwen3vl_merger`, `clip.vision.patch_size = 16` and `clip.vision.spatial_merge_size = 2`. The minimum of 8 squares is the server's own Qwen-VL limit (`set_limit_image_tokens(8, 4096)`). The maximum of 2,048 squares is the Qwen3-VL model card's recommendation and sits under the server's 4,096, so the server never resizes again. The overhead is `<|vision_start|>` and `<|vision_end|>`. The MTP build's projector is its own file with its own digest.
- Gemma 4 E4B (`gemma4v`), 12B (`gemma4uv`) and 26B A4B (`gemma4v`): `Grid { token_side: 48, min_pixels: 70·48², max_pixels: 1120·48², overhead_tokens: 2 }`. The header gives `clip.vision.patch_size = 16`. The server pools 3×3 (`n_merge = 3`; the unified 12B uses 48 px patches without pooling), takes the dynamic-size preprocessor, and keeps a picture between 70 and 1,120 tokens (`set_limit_image_tokens(70, 1120)`), the model card's smallest and largest budgets. The overhead is `<|image>` and `<image|>`. Gemma 4 is a resolution grid, not the fixed 896 px SigLIP input §5.1 assumed for Gemma 3.

Live results (§19) are still to be recorded. That includes the first start of the pinned server with `--mmproj` and MTP drafting together.
