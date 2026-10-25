import { useEffect, useState } from 'react';

import { RefreshCw } from 'lucide-react';

import { SearchableSelect, SettingsRow, SettingsSection } from '@/components/ui';
import { useCloudModels } from '@/features/model/hooks/useCloudModels';
import { useCloudApiKeyMutation } from '@/features/settings/hooks/useSettingsQuery';
import type { CloudProvider } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';
import { CLOUD_PROVIDER_NAMES } from '@/utils/chatModelSelection';
import { handleAsyncEvent, voidAsync } from '@/utils/promiseHandlers';

import { INPUT_CLASS, SECONDARY_BUTTON_CLASS } from './shared';

export function CloudConnection({ provider, model, onSaveModel }: {
  provider: CloudProvider;
  model: string;
  onSaveModel: (model: string) => Promise<boolean>;
}) {
  const catalog = useCloudModels(provider);
  const { setCloudApiKey } = useCloudApiKeyMutation();
  const [key, setKey] = useState('');
  const [savingKey, setSavingKey] = useState(false);
  const [manual, setManual] = useState(false);
  const [draft, setDraft] = useState(model);
  useEffect(() => setDraft(model), [model]);

  const models = catalog.data?.models ?? [];
  const savedModelMissing = Boolean(model.trim() && catalog.data?.hasApiKey && !models.some((entry) => entry.id === model));
  const options = models.map((entry) => ({ value: entry.id, label: entry.name, description: entry.id !== entry.name ? entry.id : undefined }));
  // A refresh never silently changes the user's saved model. Keep naming it
  // while the request runs or if a restricted key cannot list it.
  if (model.trim() && !options.some((entry) => entry.value === model)) {
    options.unshift({ value: model, label: model, description: 'Saved model' });
  }

  const saveKey = async () => {
    setSavingKey(true);
    try {
      const result = await setCloudApiKey({ provider, key: key.trim() });
      if (result.ok) {
        setKey('');
        toast.success('API key saved');
      } else {
        toast.error('Could not save API key', { message: result.error });
      }
    } finally {
      setSavingKey(false);
    }
  };

  return <SettingsSection title="Cloud connection">
    <p className="py-2 text-sm text-text-muted">
      Chat prompts and retrieved passages are sent to the selected provider. API billing is separate from subscriptions.
    </p>
    <SettingsRow label="API key" htmlFor="cloudKey">
      <div className="flex gap-2">
        <input id="cloudKey" type="password" autoComplete="off" className={INPUT_CLASS} value={key}
          placeholder={catalog.data?.hasApiKey ? 'API key saved — enter a replacement' : 'Stored in your system keychain'}
          onChange={(event) => setKey(event.target.value)} />
        <button type="button" className={SECONDARY_BUTTON_CLASS} disabled={savingKey || !key.trim()} onClick={handleAsyncEvent(saveKey)}>
          {savingKey ? 'Saving…' : 'Save key'}
        </button>
      </div>
    </SettingsRow>
    <SettingsRow label="Model" htmlFor="cloudModel" stacked>
      <div className="flex items-center gap-2">
        {manual ? <input id="cloudModel" className={INPUT_CLASS} value={draft} onChange={(event) => setDraft(event.target.value)}
          placeholder="Enter the provider's model ID"
          onBlur={() => { if (draft.trim() && draft.trim() !== model) voidAsync(() => onSaveModel(draft.trim()))(); }} />
          : <SearchableSelect id="cloudModel" label="Model" value={model} options={options}
            onValueChange={(id) => voidAsync(() => onSaveModel(id))()}
            disabled={options.length === 0 || savingKey}
            placeholder={catalog.isLoading ? 'Loading models…' : 'Choose a model'}
            searchPlaceholder="Search available models…" className="min-w-0 flex-1" />}
        <button type="button" className={SECONDARY_BUTTON_CLASS} disabled={catalog.isFetching || savingKey}
          onClick={() => { void catalog.refetch(); }} aria-label="Refresh models">
          <RefreshCw className={`h-4 w-4 ${catalog.isFetching ? 'animate-spin' : ''}`} aria-hidden="true" />
          <span className="ml-2">Refresh</span>
        </button>
      </div>
      <div className="mt-2 space-y-2 text-sm text-text-muted" aria-live="polite">
        {catalog.isLoading ? <p>Loading available models…</p>
          : catalog.error ? <p role="alert">{catalog.error.message}</p>
            : catalog.data?.hasApiKey === false ? <p>Save an API key to load available {CLOUD_PROVIDER_NAMES[provider]} models.</p>
              : catalog.data ? <p>{models.length} available {CLOUD_PROVIDER_NAMES[provider]} {models.length === 1 ? 'model' : 'models'}.</p> : null}
        {savedModelMissing && <p>The saved model was not returned by the provider. Choose an available model or enter its ID manually.</p>}
        <button type="button" className="text-text-secondary hover:text-text-primary" onClick={() => setManual((value) => !value)}>
          {manual ? 'Choose from available models' : 'Enter a model ID manually'}
        </button>
      </div>
    </SettingsRow>
  </SettingsSection>;
}
