/**
 * Chat Tab - Chat provider selection, Ollama config, active models, and runtime options.
 */

import { useEffect, useState } from 'react';

import { Brain, MessageSquare, Server, Settings2 } from 'lucide-react';

import { useLlmSettings } from './useLlmSettings';
import { INPUT_CLASS } from './shared';
import { useDownloadedModels } from '../../../hooks/useDownloadedModels';
import { VaultAPI } from '../../../lib/api';
import { useDownloadedModelsStore } from '../../../stores/downloadedModelsStore';
import { useSettingsStore } from '../../../stores/settingsStore';
import { toast } from '../../../stores/toastStore';

import type { LLMSettings as ApiLLMSettings } from '../../../types/api/settings';

export function ChatTab() {
  const { llmSettings, isLoading, saveLlmUpdates } = useLlmSettings();
  const aiSettings = useSettingsStore((state) => state.settings.ai);
  const updateAI = useSettingsStore((state) => state.updateAI);

  const [ollamaUrlDraft, setOllamaUrlDraft] = useState('');
  const [ollamaModelDraft, setOllamaModelDraft] = useState('');
  const [ollamaUtilityModelDraft, setOllamaUtilityModelDraft] = useState('');
  const [ollamaHeaderNameDraft, setOllamaHeaderNameDraft] = useState('');
  const [ollamaHeaderValueDraft, setOllamaHeaderValueDraft] = useState('');
  const [ollamaBasicUserDraft, setOllamaBasicUserDraft] = useState('');
  const [ollamaBasicPassDraft, setOllamaBasicPassDraft] = useState('');
  const [ollamaAvailableModels, setOllamaAvailableModels] = useState<string[]>([]);
  const [ollamaModelsEndpoint, setOllamaModelsEndpoint] = useState('');
  const [isTestingOllamaConnection, setIsTestingOllamaConnection] = useState(false);
  const [showOllamaAuth, setShowOllamaAuth] = useState(false);

  const { getActiveModel, getActiveEmbeddingModel } = useDownloadedModels();
  const activeChatModel = useDownloadedModelsStore((state) => state.activeModel);
  const activeEmbeddingModel = useDownloadedModelsStore((state) => state.activeEmbeddingModel);

  useEffect(() => {
    if (!llmSettings) return;
    setOllamaUrlDraft(llmSettings.ollamaUrl);
    setOllamaModelDraft(llmSettings.model);
    setOllamaUtilityModelDraft(llmSettings.ollamaUtilityModel || '');
    setOllamaHeaderNameDraft(llmSettings.ollamaAuthHeaderName || '');
    setOllamaHeaderValueDraft(llmSettings.ollamaAuthHeaderValue || '');
    if (llmSettings.ollamaAuthHeaderName || llmSettings.ollamaAuthHeaderValue) {
      setShowOllamaAuth(true);
    }
  }, [llmSettings]);

  useEffect(() => {
    void getActiveModel();
    void getActiveEmbeddingModel();
  }, [getActiveModel, getActiveEmbeddingModel]);

  const provider = llmSettings?.provider ?? 'auto';
  const showOllamaSettings = provider === 'ollama' || provider === 'auto';

  const handleApplyBasicAuth = () => {
    if (!ollamaBasicUserDraft.trim()) {
      toast.error('Basic auth username is required');
      return;
    }
    const token = btoa(`${ollamaBasicUserDraft}:${ollamaBasicPassDraft}`);
    saveLlmUpdates({
      ollamaAuthHeaderName: 'Authorization',
      ollamaAuthHeaderValue: `Basic ${token}`,
    });
  };

  const handleTestOllamaConnection = async () => {
    const ollamaUrl = ollamaUrlDraft.trim();
    const authHeaderName = ollamaHeaderNameDraft.trim();
    const authHeaderValue = ollamaHeaderValueDraft.trim();

    if (!ollamaUrl) {
      toast.error('Ollama URL is required');
      return;
    }

    if ((authHeaderName && !authHeaderValue) || (!authHeaderName && authHeaderValue)) {
      toast.error('Header name and value must both be set');
      return;
    }

    setIsTestingOllamaConnection(true);
    const result = await VaultAPI.testOllamaConnection({
      ollamaUrl,
      authHeaderName,
      authHeaderValue,
    });
    setIsTestingOllamaConnection(false);

    if (!result.ok) {
      setOllamaAvailableModels([]);
      setOllamaModelsEndpoint('');
      toast.error('Connection test failed', {
        message: result.error,
      });
      return;
    }

    setOllamaAvailableModels(result.data.models);
    setOllamaModelsEndpoint(result.data.endpoint);

    if (!result.data.models.includes(ollamaModelDraft)) {
      setOllamaModelDraft('');
    }

    toast.success(
      `Connected. Loaded ${result.data.models.length} model(s) from ${result.data.endpoint}.`
    );
  };

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex items-center gap-3 pb-4 border-b border-[hsl(var(--border-subtle))]">
        <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
          <MessageSquare className="w-5 h-5 text-[hsl(var(--accent))]" />
        </div>
        <div>
          <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))]">Chat</h2>
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Provider connection, active models, and runtime options
          </p>
        </div>
      </div>

      {/* Chat Provider */}
      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <Server className="w-4 h-4 text-[hsl(var(--accent))]" />
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Chat Provider</h3>
        </div>

        {isLoading ? (
          <div className="text-xs text-[hsl(var(--text-tertiary))]">Loading chat settings...</div>
        ) : !llmSettings ? (
          <div className="text-xs text-[hsl(var(--text-tertiary))]">
            Unable to load chat settings right now.
          </div>
        ) : (
          <div className="space-y-4">
            {/* Provider radio cards */}
            <div className="grid grid-cols-3 gap-3">
              {[
                { value: 'auto', label: 'Auto', desc: 'Local + Ollama fallback' },
                { value: 'local', label: 'Local Only', desc: 'Downloaded models' },
                { value: 'ollama', label: 'Ollama', desc: 'Remote server' },
              ].map((option) => (
                <label
                  key={option.value}
                  className={`p-3 rounded-lg border-2 transition-colors duration-fast text-center cursor-pointer ${
                    provider === option.value
                      ? 'border-[hsl(var(--accent))] bg-[hsl(var(--accent-muted))]'
                      : 'border-[hsl(var(--border-subtle))] hover:border-[hsl(var(--border-default))] bg-[hsl(var(--surface-raised))]'
                  }`}
                >
                  <input
                    type="radio"
                    name="chatProvider"
                    value={option.value}
                    checked={provider === option.value}
                    onChange={() =>
                      saveLlmUpdates({ provider: option.value as ApiLLMSettings['provider'] })
                    }
                    className="sr-only"
                  />
                  <div className="font-medium text-sm text-[hsl(var(--text-primary))]">
                    {option.label}
                  </div>
                  <div className="text-xs text-[hsl(var(--text-secondary))] mt-0.5">{option.desc}</div>
                </label>
              ))}
            </div>

            {/* Ollama configuration */}
            {showOllamaSettings && (
              <div className="p-4 bg-[hsl(var(--surface))] rounded-lg space-y-4">
                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1.5">
                    <label
                      htmlFor="ollamaUrl"
                      className="block text-xs font-medium text-[hsl(var(--text-secondary))]"
                    >
                      Server URL
                    </label>
                    <input
                      id="ollamaUrl"
                      type="text"
                      value={ollamaUrlDraft}
                      onChange={(e) => setOllamaUrlDraft(e.target.value)}
                      onBlur={() => {
                        if (ollamaUrlDraft !== llmSettings.ollamaUrl) {
                          saveLlmUpdates({ ollamaUrl: ollamaUrlDraft.trim() });
                        }
                      }}
                      placeholder="http://localhost:11434"
                      className={INPUT_CLASS}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <label
                      htmlFor="ollamaModel"
                      className="block text-xs font-medium text-[hsl(var(--text-secondary))]"
                    >
                      Model
                    </label>
                    <select
                      id="ollamaModel"
                      value={ollamaModelDraft}
                      onChange={(e) => {
                        const selectedModel = e.target.value;
                        setOllamaModelDraft(selectedModel);
                        if (selectedModel && selectedModel !== llmSettings.model) {
                          saveLlmUpdates({ model: selectedModel });
                        }
                      }}
                      disabled={ollamaAvailableModels.length === 0}
                      className={INPUT_CLASS}
                    >
                      <option value="">
                        {ollamaAvailableModels.length === 0
                          ? 'Test connection first'
                          : 'Select a model'}
                      </option>
                      {ollamaAvailableModels.map((modelName) => (
                        <option key={modelName} value={modelName}>
                          {modelName}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>

                <div className="space-y-1.5">
                  <label
                    htmlFor="ollamaUtilityModel"
                    className="block text-xs font-medium text-[hsl(var(--text-secondary))]"
                  >
                    Utility model (optional)
                  </label>
                  <select
                    id="ollamaUtilityModel"
                    value={ollamaUtilityModelDraft}
                    onChange={(e) => {
                      const selected = e.target.value;
                      setOllamaUtilityModelDraft(selected);
                      if (selected !== (llmSettings.ollamaUtilityModel || '')) {
                        saveLlmUpdates({ ollamaUtilityModel: selected });
                      }
                    }}
                    disabled={ollamaAvailableModels.length === 0}
                    className={INPUT_CLASS}
                  >
                    <option value="">
                      {ollamaAvailableModels.length === 0
                        ? 'Test connection first'
                        : 'Falls back to chat model'}
                    </option>
                    {ollamaAvailableModels.map((modelName) => (
                      <option key={modelName} value={modelName}>
                        {modelName}
                      </option>
                    ))}
                  </select>
                  <p className="text-[10px] text-[hsl(var(--text-tertiary))] leading-snug">
                    Used for routing and short utility calls. Leave blank to reuse the chat model.
                  </p>
                </div>

                <div className="flex items-center gap-3">
                  <button
                    type="button"
                    onClick={handleTestOllamaConnection}
                    disabled={isTestingOllamaConnection}
                    className="px-3 py-1.5 text-xs font-medium rounded-lg bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] hover:opacity-90 disabled:opacity-60 disabled:cursor-not-allowed transition-opacity"
                  >
                    {isTestingOllamaConnection ? 'Testing...' : 'Test Connection'}
                  </button>
                  {ollamaModelsEndpoint && (
                    <span className="text-xs text-[hsl(var(--text-tertiary))]">
                      {ollamaAvailableModels.length} model(s) from {ollamaModelsEndpoint}
                    </span>
                  )}
                  <button
                    type="button"
                    onClick={() => setShowOllamaAuth(!showOllamaAuth)}
                    className="ml-auto text-xs text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-secondary))] transition-colors"
                  >
                    {showOllamaAuth ? 'Hide auth' : 'Authentication...'}
                  </button>
                </div>

                {showOllamaAuth && (
                  <div className="pt-3 border-t border-[hsl(var(--border-subtle))] space-y-3">
                    <div className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                      Security Header
                    </div>
                    <div className="grid grid-cols-2 gap-3">
                      <input
                        type="text"
                        value={ollamaHeaderNameDraft}
                        onChange={(e) => setOllamaHeaderNameDraft(e.target.value)}
                        onBlur={() => {
                          if (ollamaHeaderNameDraft !== llmSettings.ollamaAuthHeaderName) {
                            saveLlmUpdates({
                              ollamaAuthHeaderName: ollamaHeaderNameDraft.trim(),
                            });
                          }
                        }}
                        placeholder="Header name (e.g. Authorization)"
                        className={INPUT_CLASS}
                      />
                      <input
                        type="text"
                        value={ollamaHeaderValueDraft}
                        onChange={(e) => setOllamaHeaderValueDraft(e.target.value)}
                        onBlur={() => {
                          if (ollamaHeaderValueDraft !== llmSettings.ollamaAuthHeaderValue) {
                            saveLlmUpdates({
                              ollamaAuthHeaderValue: ollamaHeaderValueDraft.trim(),
                            });
                          }
                        }}
                        placeholder="Header value (e.g. Bearer ...)"
                        className={INPUT_CLASS}
                      />
                    </div>

                    <div className="text-xs font-medium text-[hsl(var(--text-secondary))] pt-1">
                      Basic Auth Helper
                    </div>
                    <div className="flex items-end gap-3">
                      <div className="flex-1">
                        <input
                          type="text"
                          value={ollamaBasicUserDraft}
                          onChange={(e) => setOllamaBasicUserDraft(e.target.value)}
                          placeholder="Username"
                          className={INPUT_CLASS}
                        />
                      </div>
                      <div className="flex-1">
                        <input
                          type="password"
                          value={ollamaBasicPassDraft}
                          onChange={(e) => setOllamaBasicPassDraft(e.target.value)}
                          placeholder="Password"
                          className={INPUT_CLASS}
                        />
                      </div>
                      <button
                        type="button"
                        onClick={handleApplyBasicAuth}
                        className="px-3 py-2 text-xs font-medium rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-primary))] hover:border-[hsl(var(--border-default))] whitespace-nowrap"
                      >
                        Set Header
                      </button>
                    </div>
                    <p className="text-xs text-[hsl(var(--text-tertiary))]">
                      Generates an Authorization header from your credentials. Your password is not
                      stored separately.
                    </p>
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </section>

      {/* Active Models */}
      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <Brain className="w-4 h-4 text-[hsl(var(--accent))]" />
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Active Models</h3>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          <div className="p-3 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))]">
            <div className="text-xs font-medium text-[hsl(var(--text-tertiary))]">Chat LLM</div>
            <div className="text-sm text-[hsl(var(--text-primary))] mt-1">
              {provider === 'ollama'
                ? (llmSettings?.model ? `${llmSettings.model} (Ollama)` : 'Not set')
                : (activeChatModel?.model_name ?? 'Not set')}
            </div>
            {provider !== 'ollama' && activeChatModel?.model_id && (
              <div className="text-xs text-[hsl(var(--text-tertiary))] mt-1">
                {activeChatModel.model_id}
              </div>
            )}
          </div>

          <div className="p-3 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))]">
            <div className="text-xs font-medium text-[hsl(var(--text-tertiary))]">Router Model</div>
            <div className="text-sm text-[hsl(var(--text-primary))] mt-1">
              {llmSettings?.router?.model ?? 'Not set'}
            </div>
            <div className="text-xs text-[hsl(var(--text-tertiary))] mt-1">
              Required for follow-up routing
            </div>
          </div>

          <div className="p-3 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))]">
            <div className="text-xs font-medium text-[hsl(var(--text-tertiary))]">Embedding Model</div>
            <div className="text-sm text-[hsl(var(--text-primary))] mt-1">
              {activeEmbeddingModel?.model_name ?? 'Not set'}
            </div>
            {activeEmbeddingModel?.model_id && (
              <div className="text-xs text-[hsl(var(--text-tertiary))] mt-1">
                {activeEmbeddingModel.model_id}
              </div>
            )}
          </div>
        </div>
      </section>

      {/* Options */}
      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <Settings2 className="w-4 h-4 text-[hsl(var(--accent))]" />
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Options</h3>
        </div>

        <div className="p-4 bg-[hsl(var(--surface))] rounded-lg space-y-4">
          <div className="flex items-start gap-3">
            <input
              id="useQuantization"
              type="checkbox"
              checked={aiSettings.useQuantization}
              onChange={(e) => updateAI({ useQuantization: e.target.checked })}
              className="mt-0.5 w-4 h-4 text-[hsl(var(--accent))] bg-[hsl(var(--surface-raised))] border-[hsl(var(--border-subtle))] rounded focus:ring-2 focus:ring-[hsl(var(--accent))]"
            />
            <label htmlFor="useQuantization" className="flex-1 cursor-pointer">
              <div className="text-sm font-medium text-[hsl(var(--text-primary))]">Use Quantization</div>
              <p className="text-xs text-[hsl(var(--text-secondary))] mt-0.5">
                Reduce model size and improve speed with minimal quality loss.
              </p>
            </label>
          </div>
        </div>
      </section>
    </div>
  );
}
