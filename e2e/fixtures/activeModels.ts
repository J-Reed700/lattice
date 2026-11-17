import type { Page } from '@playwright/test';

import type { DownloadedModelResponse, ModelRef, SettingsDto } from '../../src/lib/bindings';
import { mockCommands } from './tauri';

/**
 * Answers `get_active_models` from the settings and downloaded rows the test's
 * own fixtures serve, by the backend's rule (`chat_model_resolution.rs`), so
 * the answer moves when a flow changes the provider or a row.
 */
export async function mockActiveModels(page: Page) {
  await mockCommands(page, (_arg: undefined, ipc) => ({
    get_active_models: async () => {
      const { llm } = (await ipc.invoke('get_settings')) as SettingsDto;
      const rows = (await ipc.invoke('list_downloaded_models')) as DownloadedModelResponse[];
      const trim = (value: string | null | undefined) => (value ?? '').trim();
      const present = <T>(model: T | null): model is T => model !== null;

      const url = trim(llm.ollamaUrl).replace(/\/+$/, '');
      const ollamaSaved = url !== '' && (
        llm.ollamaConfigured
        || llm.provider === 'ollama'
        || url !== 'http://localhost:11434'
        || trim(llm.ollamaUtilityModel) !== ''
        || trim(llm.ollamaAuthHeaderName) !== ''
      );
      const tag = trim(llm.model);
      const llamaModel = trim(llm.llamaCpp?.model);
      const llamaCpp: ModelRef | null = trim(llm.llamaCpp?.url) && llamaModel ? { kind: 'llama_cpp', model: llamaModel } : null;
      const ollama: ModelRef | null = ollamaSaved && tag ? { kind: 'ollama', model: tag } : null;
      const utilityTag = trim(llm.ollamaUtilityModel) || tag || null;
      const chatRow = rows.find((row) => row.is_active_for_chat);
      const local: ModelRef | null = chatRow && chatRow.backend !== 'ollama' ? { kind: 'local', model_id: chatRow.model_id } : null;
      const providerNames = { openai: 'OpenAI', anthropic: 'Anthropic', google: 'Google' } as const;
      const unset = (what: string) => `No ${what} is set in Settings → Chat`;

      let chat: ModelRef[] = [];
      let chatUnset: string | null = null;
      switch (llm.provider) {
        case 'local':
          if (local) chat = [local];
          else chatUnset = 'No chat model is selected in Settings → Downloaded';
          break;
        case 'ollama':
          if (tag) chat = [{ kind: 'ollama', model: tag }];
          else chatUnset = unset('Ollama chat model');
          break;
        case 'llamacpp':
          if (llamaModel) chat = [{ kind: 'llama_cpp', model: llamaModel }];
          else chatUnset = unset('llama.cpp model');
          break;
        case 'openai':
        case 'anthropic':
        case 'google':
          if (tag) chat = [{ kind: 'cloud', provider: llm.provider, model: tag }];
          else chatUnset = unset(`${providerNames[llm.provider]} model`);
          break;
        case 'auto':
          chat = [local, llamaCpp, ollama].filter(present);
          if (chat.length === 0) {
            chatUnset = 'No chat model is selected in Settings → Downloaded, and no llama.cpp or Ollama server is configured in Settings → Chat';
          }
      }

      const utilityRow = llm.utilityModel ? undefined : rows.find((row) => row.is_active_for_utility);
      let utility: ModelRef[] = [];
      if (llm.utilityModel) {
        utility = [llm.utilityModel];
      } else if (utilityRow && utilityRow.backend !== 'ollama') {
        utility = [{ kind: 'local', model_id: utilityRow.model_id }];
      } else if (utilityRow && utilityTag) {
        switch (llm.provider) {
          case 'ollama':
            utility = [{ kind: 'ollama', model: utilityTag }];
            break;
          case 'llamacpp':
            utility = [{ kind: 'llama_cpp', model: utilityTag }];
            break;
          case 'openai':
          case 'anthropic':
          case 'google':
            utility = [{ kind: 'cloud', provider: llm.provider, model: utilityTag }];
            break;
          case 'auto': {
            const fromLlamaCpp: ModelRef | null = llamaCpp ? { kind: 'llama_cpp', model: utilityTag } : null;
            const fromOllama: ModelRef | null = ollamaSaved ? { kind: 'ollama', model: utilityTag } : null;
            utility = [fromLlamaCpp, fromOllama].filter(present);
          }
        }
      }

      return { chat, chat_unset: chatUnset, utility, llama_cpp: llamaCpp, ollama, ollama_utility_tag: utilityTag };
    },
  }), undefined);
}
