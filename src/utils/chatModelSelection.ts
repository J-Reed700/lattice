import { configuredConnections } from './configuredConnections';

import type { ChatModelChoice, CloudProvider, ModelRef } from '../lib/bindings';
import type { LLMSettings } from '../types/api/settings';
import type { DownloadedModel } from '../types/downloadedModels';

export const OLLAMA_SERVER_MODEL_ID = '__ollama_server__';
export const LLAMACPP_SERVER_MODEL_ID = '__llamacpp_server__';
export const CLOUD_PROVIDER_MODEL_ID = '__cloud_provider__';

export type ChatModelBackend = 'local' | 'llamacpp' | 'ollama' | 'cloud';

/** One row of the chat model picker. */
export interface ChatModelOption {
  id: string;
  /** The model this row names, exactly as a chat or a single request stores it. */
  ref: ModelRef;
  label: string;
  triggerLabel: string;
  subtitle: string | null;
  group: 'Local' | 'llama.cpp' | 'Ollama' | 'Cloud';
  backend: ChatModelBackend;
  /** The window a downloaded model runs with: "8,704 ctx · set by you", or "32K ctx" on Auto. */
  contextLabel: string | null;
  /** `false` only when the GGUF declares no tool calling; `null` when unknown. */
  supportsTools: boolean | null;
  /** The chat model set in Settings, which every chat on the default answers with. */
  isDefault: boolean;
}

/** A chat names a model it cannot use right now: deleted, or its connection removed. */
export interface UnavailableChatModel {
  unavailable: true;
  label: string;
  /** What is wrong, in the words the chat shows. */
  reason: string;
}

export type EffectiveChatModel = ChatModelOption | UnavailableChatModel;

export function isUnavailableChatModel(model: EffectiveChatModel | null): model is UnavailableChatModel {
  return model !== null && 'unavailable' in model;
}

const CLOUD_PROVIDER_NAMES: Record<CloudProvider, string> = {
  openai: 'OpenAI',
  anthropic: 'Anthropic',
  google: 'Google',
};

function isCloudProvider(provider: LLMSettings['provider']): provider is CloudProvider {
  return provider === 'openai' || provider === 'anthropic' || provider === 'google';
}

export function cleanModelDisplayName(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return '';

  const isPath =
    trimmed.startsWith('/') ||
    trimmed.startsWith('\\') ||
    trimmed.startsWith('./') ||
    trimmed.startsWith('../') ||
    trimmed.startsWith('~/') ||
    /^[A-Za-z]:[\\/]/.test(trimmed) ||
    (trimmed.match(/\//g) || []).length > 1;

  if (isPath) {
    const segments = trimmed.split(/[/\\]+/).filter(Boolean);
    return segments[segments.length - 1] ?? trimmed;
  }

  return trimmed;
}

export function formatServerEndpoint(url: string): string | null {
  const trimmed = url.trim().replace(/^https?:\/\//, '').replace(/\/+$/, '');
  return trimmed || null;
}

export function formatModelSize(bytes: number): string | null {
  if (!bytes || bytes < 1024 * 1024) return null;
  const mb = bytes / (1024 * 1024);
  return mb < 1024 ? `${mb.toFixed(0)} MB` : `${(mb / 1024).toFixed(1)} GB`;
}

/**
 * The window a downloaded model runs with. A window saved in Settings is the
 * user's instruction and is shown exactly; otherwise the model sizes itself up
 * to its trained length, shown rounded.
 */
export function formatContextLabel(model: Pick<DownloadedModel, 'configured_context_length' | 'trained_context_length'>): string | null {
  const configured = model.configured_context_length;
  if (configured) return `${configured.toLocaleString('en-US')} ctx · set by you`;
  const trained = model.trained_context_length;
  if (!trained) return null;
  return trained >= 1024 ? `${Math.round(trained / 1024)}K ctx` : `${trained.toLocaleString('en-US')} ctx`;
}

/**
 * The full sentence behind an "on CPU" suffix, for its tooltip and its
 * accessible name: the suffix alone does not say why or what it costs.
 */
export function runningOnCpuSentence(name: string, reason: string): string {
  return `${name} is running on the CPU: ${reason}. Answers will be slow.`;
}

/** Stable, log-friendly form of a model reference, matching the backend's. */
export function modelRefKey(ref: ModelRef): string {
  switch (ref.kind) {
    case 'local': return `local:${ref.model_id}`;
    case 'ollama': return `ollama:${ref.model}`;
    case 'llama_cpp': return `llamacpp:${ref.model}`;
    case 'cloud': return `cloud:${ref.provider}/${ref.model}`;
  }
}

export function sameModelRef(a: ModelRef | null | undefined, b: ModelRef | null | undefined): boolean {
  if (!a || !b) return false;
  return modelRefKey(a) === modelRefKey(b);
}

/**
 * The model a picker row names. The three connection rows name whatever
 * Settings serves on that connection now; a downloaded row names itself, and
 * an explicit Ollama row names its own tag rather than the Settings tag.
 */
export function modelRefForOption(
  option: Pick<ChatModelOption, 'id' | 'backend'>,
  settings: LLMSettings,
): ModelRef {
  switch (option.id) {
    case LLAMACPP_SERVER_MODEL_ID:
      return { kind: 'llama_cpp', model: settings.llamaCpp.model.trim() };
    case OLLAMA_SERVER_MODEL_ID:
      return { kind: 'ollama', model: settings.model.trim() };
    case CLOUD_PROVIDER_MODEL_ID:
      if (!isCloudProvider(settings.provider)) {
        throw new Error('The cloud row names the cloud provider set in Settings, and none is set.');
      }
      return { kind: 'cloud', provider: settings.provider, model: settings.model.trim() };
    default:
      return option.backend === 'ollama'
        ? { kind: 'ollama', model: option.id }
        : { kind: 'local', model_id: option.id };
  }
}

/**
 * The model a row in Settings → Models names. The synthetic Ollama row stands
 * for the connection, so it names the chat tag saved in Settings, and names
 * nothing until that tag is known.
 */
export function modelRefForRow(
  model: Pick<DownloadedModel, 'model_id' | 'backend'>,
  settings: LLMSettings | null | undefined,
): ModelRef | null {
  if (model.model_id === OLLAMA_SERVER_MODEL_ID) {
    const tag = settings?.model.trim();
    return tag ? { kind: 'ollama', model: tag } : null;
  }
  return model.backend === 'ollama'
    ? { kind: 'ollama', model: model.model_id }
    : { kind: 'local', model_id: model.model_id };
}

/** A human name for a model reference, from the rows and settings that know it. */
export function modelRefLabel(ref: ModelRef, downloadedModels: DownloadedModel[]): string {
  switch (ref.kind) {
    case 'local':
      return downloadedModels.find((m) => m.model_id === ref.model_id)?.model_name || ref.model_id;
    case 'ollama':
    case 'llama_cpp':
      return cleanModelDisplayName(ref.model) || ref.model;
    case 'cloud':
      return ref.model;
  }
}

/** Which picker row is the chat model set in Settings. */
export function resolveDefaultChatOptionId(
  settings: LLMSettings | null | undefined,
  downloadedModels: DownloadedModel[],
): string | null {
  const activeLocal = downloadedModels.find(
    (m) =>
      m.is_active_for_chat &&
      m.backend !== 'ollama' &&
      m.model_id !== OLLAMA_SERVER_MODEL_ID &&
      (m.model_type === 'language_model' || m.model_type === 'chat')
  );

  if (!settings) {
    return (
      activeLocal?.model_id ??
      downloadedModels.find((m) => m.is_active_for_chat && m.model_id !== OLLAMA_SERVER_MODEL_ID)?.model_id ??
      null
    );
  }

  const connections = configuredConnections(settings);

  switch (settings.provider) {
    case 'llamacpp':
      return connections.llamaCpp ? LLAMACPP_SERVER_MODEL_ID : null;
    case 'ollama':
      return connections.ollama ? OLLAMA_SERVER_MODEL_ID : null;
    case 'openai':
    case 'anthropic':
    case 'google':
      return settings.model.trim() ? CLOUD_PROVIDER_MODEL_ID : null;
    case 'local':
      return activeLocal?.model_id ?? null;
    case 'auto':
      if (activeLocal) return activeLocal.model_id;
      if (connections.llamaCpp) return LLAMACPP_SERVER_MODEL_ID;
      if (connections.ollama) return OLLAMA_SERVER_MODEL_ID;
      return null;
    default:
      return null;
  }
}

export function buildChatModelOptions(
  settings: LLMSettings | null | undefined,
  downloadedModels: DownloadedModel[],
): ChatModelOption[] {
  const defaultId = resolveDefaultChatOptionId(settings, downloadedModels);
  const connections = configuredConnections(settings);
  const options: ChatModelOption[] = [];

  // 1. Local models
  const localModels = downloadedModels.filter(
    (m) =>
      (m.model_type === 'language_model' || m.model_type === 'chat') &&
      m.backend !== 'ollama' &&
      m.model_id !== OLLAMA_SERVER_MODEL_ID
  );
  for (const m of localModels) {
    const label = m.model_name || m.model_id;
    const sizeLabel = formatModelSize(m.file_size_bytes);
    const idDiffers = m.model_name && m.model_id !== m.model_name ? m.model_id : null;
    const subtitle = [sizeLabel, idDiffers].filter(Boolean).join(' · ') || null;
    options.push({
      id: m.model_id,
      ref: { kind: 'local', model_id: m.model_id },
      label,
      triggerLabel: label,
      subtitle,
      group: 'Local',
      backend: 'local',
      contextLabel: formatContextLabel(m),
      supportsTools: m.supports_tool_calling ?? null,
      isDefault: defaultId === m.model_id,
    });
  }

  // 2. llama.cpp connection
  if (connections.llamaCpp && settings) {
    const modelLabel = cleanModelDisplayName(settings.llamaCpp.model);
    options.push({
      id: LLAMACPP_SERVER_MODEL_ID,
      ref: modelRefForOption({ id: LLAMACPP_SERVER_MODEL_ID, backend: 'llamacpp' }, settings),
      label: modelLabel,
      triggerLabel: `llama.cpp · ${modelLabel}`,
      subtitle: formatServerEndpoint(settings.llamaCpp.url),
      group: 'llama.cpp',
      backend: 'llamacpp',
      contextLabel: null,
      supportsTools: null,
      isDefault: defaultId === LLAMACPP_SERVER_MODEL_ID,
    });
  }

  // 3. Ollama connection, then the Ollama tags downloaded as rows of their own
  if (connections.ollama && settings) {
    const modelLabel = cleanModelDisplayName(settings.model);
    options.push({
      id: OLLAMA_SERVER_MODEL_ID,
      ref: modelRefForOption({ id: OLLAMA_SERVER_MODEL_ID, backend: 'ollama' }, settings),
      label: modelLabel,
      triggerLabel: `Ollama · ${modelLabel}`,
      subtitle: formatServerEndpoint(settings.ollamaUrl),
      group: 'Ollama',
      backend: 'ollama',
      contextLabel: null,
      supportsTools: null,
      isDefault: defaultId === OLLAMA_SERVER_MODEL_ID,
    });
  }

  const explicitOllamaModels = downloadedModels.filter(
    (m) =>
      m.backend === 'ollama' &&
      m.model_id !== OLLAMA_SERVER_MODEL_ID &&
      (m.model_type === 'language_model' || m.model_type === 'chat')
  );
  for (const m of explicitOllamaModels) {
    const label = m.model_name || m.model_id;
    options.push({
      id: m.model_id,
      ref: { kind: 'ollama', model: m.model_id },
      label,
      triggerLabel: `Ollama · ${label}`,
      subtitle: null,
      group: 'Ollama',
      backend: 'ollama',
      contextLabel: null,
      supportsTools: null,
      isDefault: defaultId === m.model_id,
    });
  }

  // 4. Cloud provider
  if (settings && isCloudProvider(settings.provider) && settings.model.trim()) {
    const providerName = CLOUD_PROVIDER_NAMES[settings.provider];
    const modelLabel = settings.model.trim();
    options.push({
      id: CLOUD_PROVIDER_MODEL_ID,
      ref: modelRefForOption({ id: CLOUD_PROVIDER_MODEL_ID, backend: 'cloud' }, settings),
      label: modelLabel,
      triggerLabel: `${providerName} · ${modelLabel}`,
      subtitle: providerName,
      group: 'Cloud',
      backend: 'cloud',
      contextLabel: null,
      supportsTools: null,
      isDefault: defaultId === CLOUD_PROVIDER_MODEL_ID,
    });
  }

  return options;
}

/** The chat model set in Settings, as a picker row; `null` when none is set. */
export function resolveDefaultChatOption(
  settings: LLMSettings | null | undefined,
  downloadedModels: DownloadedModel[],
): ChatModelOption | null {
  return buildChatModelOptions(settings, downloadedModels).find((o) => o.isDefault) ?? null;
}

function unavailable(label: string, reason: string): UnavailableChatModel {
  return { unavailable: true, label, reason };
}

/**
 * A row for a remote model the picker does not list: a tag or a cloud model
 * this chat chose before Settings moved on. The chat still names it, and the
 * connection still serves it, so it is still what answers.
 */
function remoteOption(ref: Exclude<ModelRef, { kind: 'local' }>, settings: LLMSettings | null | undefined): ChatModelOption {
  const label = cleanModelDisplayName(ref.model) || ref.model;
  const base = { id: modelRefKey(ref), ref, label, contextLabel: null, supportsTools: null, isDefault: false };
  switch (ref.kind) {
    case 'ollama':
      return { ...base, triggerLabel: `Ollama · ${label}`, subtitle: formatServerEndpoint(settings?.ollamaUrl ?? ''), group: 'Ollama', backend: 'ollama' };
    case 'llama_cpp':
      return { ...base, triggerLabel: `llama.cpp · ${label}`, subtitle: formatServerEndpoint(settings?.llamaCpp.url ?? ''), group: 'llama.cpp', backend: 'llamacpp' };
    case 'cloud': {
      const providerName = CLOUD_PROVIDER_NAMES[ref.provider];
      return { ...base, triggerLabel: `${providerName} · ${label}`, subtitle: providerName, group: 'Cloud', backend: 'cloud' };
    }
  }
}

/**
 * What answers this chat's next turn: the Settings default for a chat on the
 * default, otherwise the model the chat chose. A chosen model that cannot be
 * used now is reported as unavailable with the reason, never swapped for
 * another. `null` means a chat on the default with no default set.
 *
 * Remote checks mirror the backend's set-time ones (an endpoint is saved);
 * whether a server or a key actually answers is for the turn to report.
 */
export function resolveEffectiveChatOption(
  choice: ChatModelChoice,
  settings: LLMSettings | null | undefined,
  downloadedModels: DownloadedModel[],
): EffectiveChatModel | null {
  const options = buildChatModelOptions(settings, downloadedModels);
  if (choice.kind === 'default') return options.find((o) => o.isDefault) ?? null;

  const ref = choice.model;
  const listed = options.find((o) => sameModelRef(o.ref, ref));
  if (listed) return listed;

  const label = modelRefLabel(ref, downloadedModels);
  switch (ref.kind) {
    case 'local':
      return unavailable(label, `This chat is set to ${label}, which is no longer downloaded.`);
    case 'ollama':
      // Before settings load nothing is known to be missing.
      if (settings && !settings.ollamaUrl.trim()) {
        return unavailable(label, `This chat uses ${label} on Ollama, but no Ollama connection is configured.`);
      }
      return remoteOption(ref, settings);
    case 'llama_cpp':
      if (settings && !settings.llamaCpp.url.trim()) {
        return unavailable(label, `This chat uses ${label} on llama.cpp, but no llama.cpp connection is configured.`);
      }
      return remoteOption(ref, settings);
    case 'cloud':
      return remoteOption(ref, settings);
  }
}
