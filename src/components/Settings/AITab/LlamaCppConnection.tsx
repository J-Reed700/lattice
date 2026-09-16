import { useEffect, useState } from 'react';

import { INPUT_CLASS, SECONDARY_BUTTON_CLASS } from './shared';
import { useLlmSettings } from './useLlmSettings';
import { VaultAPI } from '../../../lib/api';
import { toast } from '../../../stores/toastStore';
import { SettingsRow, SettingsSection } from '../../ui';


import type { LlamaCppSettingsDto } from '../../../lib/bindings';

const DEFAULT_CONNECTION: LlamaCppSettingsDto = {
  url: 'http://localhost:8080', model: '', authHeaderName: '', authHeaderValue: '',
};

export function LlamaCppConnection() {
  const { llmSettings, saveLlmUpdates } = useLlmSettings();
  const saved = llmSettings?.llamaCpp;
  const [draft, setDraft] = useState<LlamaCppSettingsDto>(saved ?? DEFAULT_CONNECTION);
  const [models, setModels] = useState<string[]>([]);
  const [testing, setTesting] = useState(false);
  useEffect(() => { setDraft(saved ?? DEFAULT_CONNECTION); }, [saved]);

  const update = (field: keyof LlamaCppSettingsDto, value: string) => {
    setDraft(current => ({ ...current, [field]: value }));
    setModels([]);
  };
  const save = async () => {
    const connection = { ...draft, url: draft.url.trim(), model: draft.model.trim(),
      authHeaderName: draft.authHeaderName.trim(), authHeaderValue: draft.authHeaderValue.trim() };
    if (Boolean(connection.authHeaderName) !== Boolean(connection.authHeaderValue)) {
      toast.error('Set both authentication header fields, or clear both');
      return;
    }
    if (await saveLlmUpdates({ provider: llmSettings?.provider === 'auto' ? 'auto' : 'llamacpp', llamaCpp: connection })) toast.success('llama.cpp settings saved');
  };
  const test = async () => {
    setTesting(true);
    try {
      const result = await VaultAPI.testLlamaCppConnection(draft);
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
        value={draft.authHeaderValue} placeholder="Basic … or Bearer …"
        onChange={event => update('authHeaderValue', event.target.value)} disabled={testing} />
    </SettingsRow>
    <div className="flex justify-end gap-2 py-3">
      <button type="button" aria-label="Test llama.cpp connection" className={SECONDARY_BUTTON_CLASS} disabled={testing} onClick={() => void test()}>
        {testing ? 'Testing…' : 'Test connection'}
      </button>
      <button type="button" className={SECONDARY_BUTTON_CLASS} disabled={testing} onClick={() => void save()}>Save connection</button>
    </div>
  </SettingsSection>;
}
