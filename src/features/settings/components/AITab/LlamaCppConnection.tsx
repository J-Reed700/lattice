import { useEffect, useState } from 'react';

import { SettingsRow, SettingsSection } from '@/components/ui';
import { useApi } from '@/contexts/ApiContext';
import { probeLlamaCppServer } from '@/features/settings/hooks/connectionProbes';
import type { LlamaCppSettingsDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';

import { INPUT_CLASS, SECONDARY_BUTTON_CLASS } from './shared';
import { useLlmSettings } from './useLlmSettings';


const DEFAULT_CONNECTION: LlamaCppSettingsDto = {
  url: 'http://localhost:8080', model: '', authHeaderName: '',
};

export function LlamaCppConnection() {
  const api = useApi();
  const { llmSettings, authValueSet, saveLlmUpdates, saveAuthHeaderValue } = useLlmSettings();
  const saved = llmSettings?.llamaCpp;
  const [draft, setDraft] = useState<LlamaCppSettingsDto>(saved ?? DEFAULT_CONNECTION);
  // The value is write-only: the saved one never comes back, only whether one is set (`authValueSet`).
  const [valueDraft, setValueDraft] = useState('');
  const [models, setModels] = useState<string[]>([]);
  const [testing, setTesting] = useState(false);
  useEffect(() => { setDraft(saved ?? DEFAULT_CONNECTION); }, [saved]);

  const update = (field: 'url' | 'model' | 'authHeaderName', value: string) => {
    setDraft(current => ({ ...current, [field]: value }));
    setModels([]);
  };
  const stored = authValueSet.llamaCpp;
  const typedValue = valueDraft.trim();
  const connection = { url: draft.url.trim(), model: draft.model.trim(),
    authHeaderName: draft.authHeaderName.trim() };
  const save = async () => {
    if (Boolean(connection.authHeaderName) !== Boolean(typedValue || stored)) {
      toast.error('Set both authentication header fields, or clear both');
      return;
    }
    // The value goes first: the backend refuses a name with no value behind it.
    if (typedValue) {
      if (!(await saveAuthHeaderValue('llamaCpp', typedValue))) return;
      setValueDraft('');
    }
    if (await saveLlmUpdates({ llamaCpp: connection as LlamaCppSettingsDto })) toast.success('llama.cpp settings saved');
  };
  const clearHeader = async () => {
    if (!(await saveLlmUpdates({ llamaCpp: { ...connection, authHeaderName: '' } as LlamaCppSettingsDto }))) return;
    if (await saveAuthHeaderValue('llamaCpp', null)) {
      setDraft(current => ({ ...current, authHeaderName: '' }));
      setValueDraft('');
    }
  };
  const test = async () => {
    setTesting(true);
    try {
      const result = await probeLlamaCppServer({ ...connection, authHeaderValue: typedValue }, api);
      if (!result.ok) {
        toast.error('Could not connect to llama.cpp', { message: typeof result.details?.details === 'string' ? result.details.details : result.error });
        setModels([]);
        return;
      }
      setModels(result.data.models);
      if (!draft.model) setDraft(current => ({ ...current, model: result.data.models[0] ?? '' }));
      toast.success('llama.cpp models and chat connection verified');
    } catch (error) {
      toast.error('Could not connect to llama.cpp', { message: String(error) });
    } finally { setTesting(false); }
  };
  return <SettingsSection title="llama.cpp server">
    <SettingsRow label="llama.cpp URL" htmlFor="llamaCppUrl">
      <input id="llamaCppUrl" className={INPUT_CLASS} value={draft.url}
        placeholder="http://localhost:8080" onChange={event => update('url', event.target.value)} disabled={testing} />
    </SettingsRow>
    <SettingsRow label="llama.cpp model" htmlFor="llamaCppModel">
      <input id="llamaCppModel" className={INPUT_CLASS} value={draft.model} list="llamaCppModels"
        placeholder="Model ID from the server" onChange={event => update('model', event.target.value)} disabled={testing} />
      <datalist id="llamaCppModels">{models.map(model => <option key={model} value={model} />)}</datalist>
    </SettingsRow>
    <SettingsRow label="llama.cpp auth header name" htmlFor="llamaCppHeaderName">
      <input id="llamaCppHeaderName" className={INPUT_CLASS} value={draft.authHeaderName}
        placeholder="Authorization" onChange={event => update('authHeaderName', event.target.value)} disabled={testing} />
    </SettingsRow>
    <SettingsRow label="llama.cpp auth header value" htmlFor="llamaCppHeaderValue">
      <input id="llamaCppHeaderValue" type="password" autoComplete="off" className={INPUT_CLASS}
        value={valueDraft} placeholder={stored ? 'Value is set. Type to replace it' : 'Basic … or Bearer …'}
        onChange={event => { setValueDraft(event.target.value); setModels([]); }} disabled={testing} />
    </SettingsRow>
    <div className="flex justify-end gap-2 py-3">
      <button type="button" aria-label="Test llama.cpp connection" className={SECONDARY_BUTTON_CLASS} disabled={testing} onClick={() => void test()}>
        {testing ? 'Testing…' : 'Test connection'}
      </button>
      {stored ? <button type="button" className={SECONDARY_BUTTON_CLASS} disabled={testing} onClick={() => void clearHeader()}>Clear header</button> : null}
      <button type="button" className={SECONDARY_BUTTON_CLASS} disabled={testing} onClick={() => void save()}>Save connection</button>
    </div>
  </SettingsSection>;
}
