import type { LLMSettings } from '@/types/api/settings';

/** Default server addresses are suggestions, not saved connections. */
export function configuredConnections(settings: LLMSettings | null | undefined) {
  if (!settings) return { ollama: false, llamaCpp: false };

  const url = settings.ollamaUrl.trim().replace(/\/+$/, '');
  // Server-specific values establish configuration too. Never use `model`
  // alone: it has a built-in default and also holds cloud provider models.
  const ollamaSaved = settings.ollamaConfigured
    || settings.provider === 'ollama'
    || (url !== '' && url !== 'http://localhost:11434')
    || Boolean(settings.ollamaUtilityModel.trim())
    || Boolean(settings.ollamaAuthHeaderName.trim() && settings.ollamaAuthHeaderValue.trim());

  return {
    ollama: Boolean(ollamaSaved && url && settings.model.trim()),
    llamaCpp: Boolean(settings.llamaCpp.url.trim() && settings.llamaCpp.model.trim()),
  };
}
