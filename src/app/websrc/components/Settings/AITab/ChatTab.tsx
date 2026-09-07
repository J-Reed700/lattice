/**
 * Chat settings — provider connection, Ollama config, and the models
 * currently doing the work.
 */

import { useEffect, useState } from 'react';

import { cn } from '@/lib/utils';

import { GHOST_BUTTON_CLASS, INPUT_CLASS, SECONDARY_BUTTON_CLASS } from './shared';
import { useLlmSettings } from './useLlmSettings';
import { useDownloadedModels } from '../../../hooks/useDownloadedModels';
import { VaultAPI } from '../../../lib/api';
import { toast } from '../../../stores/toastStore';
import { PageHeader, SettingsRow, SettingsSection } from '../../ui';

import type { LLMSettings as ApiLLMSettings } from '../../../types/api/settings';

const PROVIDERS: Array<{ value: ApiLLMSettings['provider']; label: string }> = [
  { value: 'auto', label: 'Auto — local, then Ollama' },
  { value: 'local', label: 'Local only' },
  { value: 'ollama', label: 'Ollama' },
];

export function ChatTab() {
  const { llmSettings, isLoading, saveLlmUpdates, reload } = useLlmSettings();

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

  const {
    getActiveModel,
    getActiveEmbeddingModel,
    activeModel: activeChatModel,
    activeEmbeddingModel,
  } = useDownloadedModels();

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

  const trimmedHeaderName = ollamaHeaderNameDraft.trim();
  const trimmedHeaderValue = ollamaHeaderValueDraft.trim();
  const isHeaderPartialPair =
    (trimmedHeaderName !== '' && trimmedHeaderValue === '') ||
    (trimmedHeaderName === '' && trimmedHeaderValue !== '');
  const headerDraftDiffersFromSaved =
    trimmedHeaderName !== (llmSettings?.ollamaAuthHeaderName ?? '').trim() ||
    trimmedHeaderValue !== (llmSettings?.ollamaAuthHeaderValue ?? '').trim();
  const handleHeaderBlur = () => {
    if (isHeaderPartialPair) return;
    if (!headerDraftDiffersFromSaved) return;
    saveLlmUpdates({
      ollamaAuthHeaderName: trimmedHeaderName,
      ollamaAuthHeaderValue: trimmedHeaderValue,
    });
  };

  const handleApplyBasicAuth = () => {
    if (!ollamaBasicUserDraft.trim()) {
      toast.error('Enter a username for basic auth');
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
      toast.error('Enter an Ollama server URL first');
      return;
    }

    if ((authHeaderName && !authHeaderValue) || (!authHeaderName && authHeaderValue)) {
      toast.error('Set both the header name and value, or clear both');
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
      toast.error("Couldn't reach the Ollama server", {
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

  const routerModel = llmSettings?.router?.model?.trim();

  return (
    <>
      <PageHeader title="Chat" />

      <SettingsSection title="Provider">
        {isLoading ? (
          <div className="border-b border-border-subtle py-3 text-sm text-text-muted">Loading…</div>
        ) : !llmSettings ? (
          <div className="flex items-center gap-2 border-b border-border-subtle py-3">
            <p className="text-sm text-text-muted">Couldn&apos;t read chat settings.</p>
            <button
              type="button"
              onClick={reload}
              className="text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
            >
              Retry
            </button>
          </div>
        ) : (
          <SettingsRow label="Chat provider" htmlFor="chatProvider">
            <select
              id="chatProvider"
              value={provider}
              onChange={(event) =>
                saveLlmUpdates({ provider: event.target.value as ApiLLMSettings['provider'] })
              }
              className={INPUT_CLASS}
            >
              {PROVIDERS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </SettingsRow>
        )}
      </SettingsSection>

      {llmSettings && showOllamaSettings ? (
        <SettingsSection title="Ollama server">
          <SettingsRow label="Server URL" htmlFor="ollamaUrl">
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
          </SettingsRow>

          <SettingsRow label="Model" htmlFor="ollamaModel">
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
                {ollamaAvailableModels.length === 0 ? 'Test the connection first' : 'Select a model'}
              </option>
              {ollamaAvailableModels.map((modelName) => (
                <option key={modelName} value={modelName}>
                  {modelName}
                </option>
              ))}
            </select>
          </SettingsRow>

          <SettingsRow
            label="Utility model"
            hint="Used for routing and short calls. Blank reuses the chat model."
            htmlFor="ollamaUtilityModel"
          >
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
                  ? 'Test the connection first'
                  : 'Falls back to the chat model'}
              </option>
              {ollamaAvailableModels.map((modelName) => (
                <option key={modelName} value={modelName}>
                  {modelName}
                </option>
              ))}
            </select>
          </SettingsRow>

          <SettingsRow
            label="Connection"
            hint={
              ollamaModelsEndpoint
                ? `${ollamaAvailableModels.length} model(s) from ${ollamaModelsEndpoint}`
                : undefined
            }
          >
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={handleTestOllamaConnection}
                disabled={isTestingOllamaConnection}
                className={SECONDARY_BUTTON_CLASS}
              >
                {isTestingOllamaConnection ? 'Testing…' : 'Test connection'}
              </button>
              <button
                type="button"
                onClick={() => setShowOllamaAuth(!showOllamaAuth)}
                aria-expanded={showOllamaAuth}
                className={GHOST_BUTTON_CLASS}
              >
                {showOllamaAuth ? 'Hide auth' : 'Auth'}
              </button>
            </div>
          </SettingsRow>

          {showOllamaAuth ? (
            <>
              <SettingsRow
                label="Auth header"
                hint={isHeaderPartialPair ? 'Fill in both name and value, or clear both.' : undefined}
                stacked
              >
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={ollamaHeaderNameDraft}
                    onChange={(e) => setOllamaHeaderNameDraft(e.target.value)}
                    onBlur={handleHeaderBlur}
                    placeholder="Header name (e.g. Authorization)"
                    aria-label="Auth header name"
                    className={INPUT_CLASS}
                  />
                  <input
                    type="text"
                    value={ollamaHeaderValueDraft}
                    onChange={(e) => setOllamaHeaderValueDraft(e.target.value)}
                    onBlur={handleHeaderBlur}
                    placeholder="Header value (e.g. Bearer …)"
                    aria-label="Auth header value"
                    className={INPUT_CLASS}
                  />
                </div>
              </SettingsRow>

              <SettingsRow
                label="Basic auth"
                hint="Builds an Authorization header. The password itself is not stored."
                stacked
              >
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={ollamaBasicUserDraft}
                    onChange={(e) => setOllamaBasicUserDraft(e.target.value)}
                    placeholder="Username"
                    aria-label="Basic auth username"
                    className={INPUT_CLASS}
                  />
                  <input
                    type="password"
                    value={ollamaBasicPassDraft}
                    onChange={(e) => setOllamaBasicPassDraft(e.target.value)}
                    placeholder="Password"
                    aria-label="Basic auth password"
                    className={INPUT_CLASS}
                  />
                  <button
                    type="button"
                    onClick={handleApplyBasicAuth}
                    className={SECONDARY_BUTTON_CLASS}
                  >
                    Set header
                  </button>
                </div>
              </SettingsRow>
            </>
          ) : null}
        </SettingsSection>
      ) : null}

      <SettingsSection title="Active models">
        <SettingsRow label="Chat">
          <ModelValue
            name={
              provider === 'ollama'
                ? llmSettings?.model || null
                : activeChatModel?.model_name ?? null
            }
            id={provider === 'ollama' ? 'Ollama' : activeChatModel?.model_id ?? null}
          />
        </SettingsRow>
        <SettingsRow label="Router">
          <ModelValue name={routerModel || null} id={null} />
        </SettingsRow>
        <SettingsRow label="Embedding">
          <ModelValue
            name={activeEmbeddingModel?.model_name ?? null}
            id={activeEmbeddingModel?.model_id ?? null}
          />
        </SettingsRow>
      </SettingsSection>
    </>
  );
}

function ModelValue({ name, id }: { name: string | null; id: string | null }) {
  if (!name) {
    return <span className="text-sm text-text-muted">Not set</span>;
  }
  return (
    <div className="min-w-0 text-right">
      <div className="truncate text-sm text-text-primary">{name}</div>
      {id ? (
        <div className={cn('truncate font-mono text-xs text-text-muted')}>{id}</div>
      ) : null}
    </div>
  );
}
