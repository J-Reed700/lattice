import { configuredConnections } from './configuredConnections';

import type { LLMSettings } from '../types/api/settings';
import type { DownloadedModel } from '../types/downloadedModels';

export const OLLAMA_SERVER_MODEL_ID = '__ollama_server__';
export const LLAMACPP_SERVER_MODEL_ID = '__llamacpp_server__';
export const CLOUD_PROVIDER_MODEL_ID = '__cloud_provider__';

export type ChatModelBackend = 'local' | 'llamacpp' | 'ollama' | 'cloud';

export interface ChatModelOption {
  id: string;
  label: string;
  triggerLabel: string;
  subtitle: string | null;
  group: 'Local' | 'llama.cpp' | 'Ollama' | 'Cloud';
  backend: ChatModelBackend;
  targetProvider: LLMSettings['provider'];
  downloadedModelId: string | null;
  isActive: boolean;
}

/** Match backend provider selection when enabling chat and naming a conversation. */
export function resolveChatModel(settings: LLMSettings | null | undefined, localModel: string | null): string | null {
  // The legacy server row is a provider shortcut, not a downloaded local model.
  const downloadedModel = localModel === '__ollama_server__' ? null : localModel;
  const ollamaModel = settings?.ollamaUrl.trim() ? settings.model.trim() || null : null;
  const llamaCppModel = settings?.llamaCpp?.url.trim() ? settings.llamaCpp.model.trim() || null : null;
  switch (settings?.provider ?? 'auto') {
    case 'llamacpp': return llamaCppModel;
    case 'ollama': return ollamaModel;
    case 'openai':
    case 'anthropic':
    case 'google': return settings?.model.trim() || null;
    case 'local': return downloadedModel;
    case 'auto': return downloadedModel || llamaCppModel || ollamaModel;
  }
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

export function resolveActiveChatOptionId(
  settings: LLMSettings | null | undefined,
  downloadedModels: DownloadedModel[],
  fallbackActiveId?: string | null
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
      fallbackActiveId ??
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
  fallbackActiveId?: string | null
): ChatModelOption[] {
  const activeId = resolveActiveChatOptionId(settings, downloadedModels, fallbackActiveId);
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
      label,
      triggerLabel: label,
      subtitle,
      group: 'Local',
      backend: 'local',
      targetProvider: 'local',
      downloadedModelId: m.model_id,
      isActive: activeId === m.model_id,
    });
  }

  // 2. llama.cpp connection
  if (connections.llamaCpp && settings) {
    const modelLabel = cleanModelDisplayName(settings.llamaCpp.model);
    const endpoint = formatServerEndpoint(settings.llamaCpp.url);
    options.push({
      id: LLAMACPP_SERVER_MODEL_ID,
      label: modelLabel,
      triggerLabel: `llama.cpp · ${modelLabel}`,
      subtitle: endpoint,
      group: 'llama.cpp',
      backend: 'llamacpp',
      targetProvider: 'llamacpp',
      downloadedModelId: null,
      isActive: activeId === LLAMACPP_SERVER_MODEL_ID,
    });
  }

  // 3. Ollama connection
  if (connections.ollama && settings) {
    const modelLabel = cleanModelDisplayName(settings.model);
    const endpoint = formatServerEndpoint(settings.ollamaUrl);
    const hasSyntheticRow = downloadedModels.some((m) => m.model_id === OLLAMA_SERVER_MODEL_ID);
    options.push({
      id: OLLAMA_SERVER_MODEL_ID,
      label: modelLabel,
      triggerLabel: `Ollama · ${modelLabel}`,
      subtitle: endpoint,
      group: 'Ollama',
      backend: 'ollama',
      targetProvider: 'ollama',
      downloadedModelId: hasSyntheticRow ? OLLAMA_SERVER_MODEL_ID : null,
      isActive: activeId === OLLAMA_SERVER_MODEL_ID,
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
      label,
      triggerLabel: `Ollama · ${label}`,
      subtitle: null,
      group: 'Ollama',
      backend: 'ollama',
      targetProvider: 'ollama',
      downloadedModelId: m.model_id,
      isActive: activeId === m.model_id,
    });
  }

  // 4. Cloud provider
  if (
    settings &&
    (settings.provider === 'openai' || settings.provider === 'anthropic' || settings.provider === 'google') &&
    settings.model.trim()
  ) {
    const providerNames: Record<'openai' | 'anthropic' | 'google', string> = {
      openai: 'OpenAI',
      anthropic: 'Anthropic',
      google: 'Google',
    };
    const pName = providerNames[settings.provider];
    const modelLabel = settings.model.trim();
    options.push({
      id: CLOUD_PROVIDER_MODEL_ID,
      label: modelLabel,
      triggerLabel: `${pName} · ${modelLabel}`,
      subtitle: pName,
      group: 'Cloud',
      backend: 'cloud',
      targetProvider: settings.provider,
      downloadedModelId: null,
      isActive: activeId === CLOUD_PROVIDER_MODEL_ID,
    });
  }

  return options;
}

export function resolveActiveChatOption(
  settings: LLMSettings | null | undefined,
  downloadedModels: DownloadedModel[],
  fallbackActiveId?: string | null
): ChatModelOption | null {
  return buildChatModelOptions(settings, downloadedModels, fallbackActiveId).find((o) => o.isActive) ?? null;
}
