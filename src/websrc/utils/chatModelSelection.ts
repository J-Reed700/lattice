import type { LLMSettings } from '../types/api/settings';

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
    case 'anthropic': return settings?.model.trim() || null;
    case 'local': return downloadedModel;
    case 'auto': return downloadedModel || llamaCppModel || ollamaModel;
  }
}
