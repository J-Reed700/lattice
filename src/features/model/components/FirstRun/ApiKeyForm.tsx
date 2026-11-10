import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { DialogFooter } from '@/components/ui/dialog';
import { useCloudModels } from '@/features/model/hooks/useCloudModels';
import { useCloudApiKeyMutation, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import type { CloudProvider } from '@/lib/bindings';
import { getErrorMessage } from '@/lib/errorUtils';
import { toast } from '@/stores/toastStore';
import { CLOUD_PROVIDER_NAMES } from '@/utils/chatModelSelection';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

const PROVIDERS = Object.keys(CLOUD_PROVIDER_NAMES) as CloudProvider[];
const FIELD_CLASS = 'w-full rounded-md border border-border-subtle bg-surface px-3 py-2 text-sm text-text-primary disabled:opacity-60';

interface ApiKeyFormProps {
  onBack: () => void;
  /** Called once the chosen model is saved for chats and courses. */
  onDone: () => void;
}

export function ApiKeyForm({ onBack, onDone }: ApiKeyFormProps) {
  const [provider, setProvider] = useState<CloudProvider>('anthropic');
  const [key, setKey] = useState('');
  const [model, setModel] = useState('');
  const [saving, setSaving] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);
  const [useError, setUseError] = useState<string | null>(null);
  const catalog = useCloudModels(provider);
  const { setCloudApiKey } = useCloudApiKeyMutation();
  const updateSettings = useUpdateSettingsMutation();
  const name = CLOUD_PROVIDER_NAMES[provider];
  const models = catalog.data?.models ?? [];

  const saveKey = async () => {
    setSaving(true);
    setKeyError(null);
    try {
      const result = await setCloudApiKey({ provider, key: key.trim() });
      if (result.ok) setKey('');
      else setKeyError(result.error);
    } finally {
      setSaving(false);
    }
  };

  const useModel = async () => {
    setUseError(null);
    try {
      await updateSettings.mutateAsync({
        category: 'llm',
        updates: { provider, model, maxTokens: 8192, learningModel: { kind: 'cloud', provider, model } },
      });
    } catch (error) {
      setUseError(getErrorMessage(error));
      return;
    }
    const label = models.find((entry) => entry.id === model)?.name ?? model;
    onDone();
    toast.success(`Using ${label}`, { message: `Chats and courses now use ${name}.` });
  };

  return (
    <>
      <div className="space-y-3">
        <div className="space-y-1.5">
          <label htmlFor="firstRunProvider" className="text-sm text-text-secondary">Provider</label>
          <select
            id="firstRunProvider"
            className={FIELD_CLASS}
            value={provider}
            onChange={(event) => {
              setProvider(event.target.value as CloudProvider);
              setModel('');
              setKeyError(null);
            }}
          >
            {PROVIDERS.map((value) => <option key={value} value={value}>{CLOUD_PROVIDER_NAMES[value]}</option>)}
          </select>
        </div>
        <div className="space-y-1.5">
          <label htmlFor="firstRunApiKey" className="text-sm text-text-secondary">API key</label>
          <div className="flex gap-2">
            <input
              id="firstRunApiKey"
              type="password"
              autoComplete="off"
              className={FIELD_CLASS}
              value={key}
              placeholder={catalog.data?.hasApiKey ? 'API key saved — enter a replacement' : 'Stored in your system keychain'}
              onChange={(event) => setKey(event.target.value)}
            />
            <Button variant="outline" disabled={saving || !key.trim()} onClick={handleAsyncEvent(saveKey)}>
              {saving ? 'Saving…' : 'Save key'}
            </Button>
          </div>
          {keyError && <p role="alert" className="text-sm text-danger-fg">{keyError}</p>}
        </div>
        <div className="space-y-1.5">
          <label htmlFor="firstRunModel" className="text-sm text-text-secondary">Model</label>
          <select
            id="firstRunModel"
            className={FIELD_CLASS}
            value={model}
            disabled={!catalog.data?.hasApiKey || saving}
            onChange={(event) => setModel(event.target.value)}
          >
            <option value="">Choose a model</option>
            {models.map((entry) => <option key={entry.id} value={entry.id}>{entry.name}</option>)}
          </select>
          {catalog.isLoading && <p className="text-sm text-text-muted">Loading models…</p>}
          {catalog.error && <p role="alert" className="text-sm text-danger-fg">{catalog.error.message}</p>}
          {useError && <p role="alert" className="text-sm text-danger-fg">{useError}</p>}
        </div>
        <p className="text-sm text-text-muted">
          Chats and courses send their text to {name}. API billing is separate from subscriptions.
        </p>
      </div>
      <DialogFooter className="flex-col gap-2 sm:flex-row sm:items-center">
        <Button variant="ghost" onClick={onBack} className="sm:mr-auto">Download a model instead</Button>
        <Button disabled={!model || updateSettings.isPending} onClick={handleAsyncEvent(useModel)}>
          Use this model
        </Button>
      </DialogFooter>
    </>
  );
}
