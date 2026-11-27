import { useEffect, useState } from 'react';

import { Button, Input, SettingsRow, SettingsSection } from '@/components/ui';
import { useApi } from '@/contexts/ApiContext';
import { probeLlamaCppServer } from '@/features/settings/hooks/connectionProbes';
import type { LlamaCppSettingsDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';

import { useLlmSettings } from './useLlmSettings';

const DEFAULT_CONNECTION: LlamaCppSettingsDto = {
  url: 'http://localhost:8080',
  model: '',
  authHeaderName: '',
};

export function LlamaCppConnection() {
  const api = useApi();
  const {
    llmSettings,
    authValueSet,
    authValueStatus = 'ready',
    saveLlmUpdates,
    saveAuthHeaderValue,
  } = useLlmSettings();
  const saved = llmSettings?.llamaCpp;
  const [draft, setDraft] = useState<LlamaCppSettingsDto>(saved ?? DEFAULT_CONNECTION);
  // The value is write-only: the saved one never comes back, only whether one is set (`authValueSet`).
  const [valueDraft, setValueDraft] = useState('');
  const [models, setModels] = useState<string[]>([]);
  const [testing, setTesting] = useState(false);
  useEffect(() => {
    setDraft(saved ?? DEFAULT_CONNECTION);
  }, [saved]);

  const update = (field: 'url' | 'model' | 'authHeaderName', value: string) => {
    setDraft((current) => ({ ...current, [field]: value }));
    setModels([]);
  };
  const stored = authValueSet.llamaCpp;
  const savedHeaderName = saved?.authHeaderName ?? '';
  const hasSavedHeaderName = Boolean(savedHeaderName.trim());
  const authStatusKnown = authValueStatus === 'ready';
  const hasHalfConfiguredHeader = authStatusKnown && !stored && hasSavedHeaderName;
  const showClearHeader = authStatusKnown && (stored || hasSavedHeaderName);
  const typedValue = valueDraft.trim();
  const connection = {
    url: draft.url.trim(),
    model: draft.model.trim(),
    authHeaderName: draft.authHeaderName.trim(),
  };
  const keepsExistingSavedHeader =
    !authStatusKnown && hasSavedHeaderName && connection.authHeaderName === savedHeaderName.trim();
  const hasConfiguredValue = Boolean(typedValue) || stored || keepsExistingSavedHeader;
  const save = async () => {
    if (Boolean(connection.authHeaderName) !== hasConfiguredValue) {
      toast.error('Set both authentication header fields, or clear both');
      return;
    }
    if (typedValue) {
      if (!(await saveAuthHeaderValue('llamaCpp', typedValue))) return;
      setValueDraft('');
    }
    if (await saveLlmUpdates({ llamaCpp: connection })) toast.success('llama.cpp settings saved');
  };
  const clearHeader = async () => {
    if (!(await saveLlmUpdates({ llamaCpp: { ...connection, authHeaderName: '' } }))) return;
    if (await saveAuthHeaderValue('llamaCpp', null)) {
      setDraft((current) => ({ ...current, authHeaderName: '' }));
      setValueDraft('');
    }
  };
  const test = async () => {
    setTesting(true);
    try {
      const result = await probeLlamaCppServer({ ...connection, authHeaderValue: typedValue }, api);
      if (!result.ok) {
        toast.error('Could not connect to llama.cpp', {
          message:
            typeof result.details?.details === 'string' ? result.details.details : result.error,
        });
        setModels([]);
        return;
      }
      setModels(result.data.models);
      if (!draft.model) setDraft((current) => ({ ...current, model: result.data.models[0] ?? '' }));
      toast.success('llama.cpp models and chat connection verified');
    } catch (error) {
      toast.error('Could not connect to llama.cpp', { message: String(error) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <SettingsSection title="llama.cpp server">
      <SettingsRow label="llama.cpp URL" htmlFor="llamaCppUrl">
        <Input
          id="llamaCppUrl"
          value={draft.url}
          placeholder="http://localhost:8080"
          onChange={(event) => update('url', event.target.value)}
          disabled={testing}
        />
      </SettingsRow>
      <SettingsRow label="llama.cpp model" htmlFor="llamaCppModel">
        <Input
          id="llamaCppModel"
          value={draft.model}
          list="llamaCppModels"
          placeholder="Model ID from the server"
          onChange={(event) => update('model', event.target.value)}
          disabled={testing}
        />
        <datalist id="llamaCppModels">
          {models.map((model) => (
            <option key={model} value={model} />
          ))}
        </datalist>
      </SettingsRow>
      <SettingsRow label="llama.cpp auth header name" htmlFor="llamaCppHeaderName">
        <Input
          id="llamaCppHeaderName"
          value={draft.authHeaderName}
          placeholder="Authorization"
          onChange={(event) => update('authHeaderName', event.target.value)}
          disabled={testing}
        />
      </SettingsRow>
      <SettingsRow
        label="llama.cpp auth header value"
        htmlFor="llamaCppHeaderValue"
        hint={
          hasHalfConfiguredHeader
            ? 'Auth header name is saved without a value in Keychain. Enter a value or click Clear header.'
            : authValueStatus === 'loading' && hasSavedHeaderName
              ? 'Checking Keychain for the saved header value…'
              : authValueStatus === 'error' && hasSavedHeaderName
                ? 'Couldn’t check Keychain status. Saved header settings are preserved.'
                : undefined
        }
      >
        <Input
          id="llamaCppHeaderValue"
          type="password"
          autoComplete="off"
          value={valueDraft}
          placeholder={
            stored
              ? 'Value is set. Type to replace it'
              : authValueStatus === 'loading' && hasSavedHeaderName
                ? 'Checking Keychain…'
                : authValueStatus === 'error' && hasSavedHeaderName
                  ? 'Keychain status unavailable. Type to replace'
                  : 'Basic … or Bearer …'
          }
          onChange={(event) => {
            setValueDraft(event.target.value);
            setModels([]);
          }}
          disabled={testing}
        />
      </SettingsRow>
      <div className="flex justify-end gap-2 py-3">
        <Button
          type="button"
          variant="outline"
          aria-label="Test llama.cpp connection"
          disabled={testing}
          onClick={() => void test()}
        >
          {testing ? 'Testing…' : 'Test connection'}
        </Button>
        {showClearHeader ? (
          <Button
            type="button"
            variant="outline"
            disabled={testing}
            onClick={() => void clearHeader()}
          >
            Clear header
          </Button>
        ) : null}
        <Button type="button" variant="outline" disabled={testing} onClick={() => void save()}>
          Save connection
        </Button>
      </div>
    </SettingsSection>
  );
}
