/**
 * LlamaCppMetaRow
 *
 * The llama.cpp connection lives in LLM settings rather than the downloaded
 * models table. Selecting Chat must update the provider, not a model-row flag.
 * Connection details remain editable on the Chat settings page.
 */

import { Check, Network } from 'lucide-react';

import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import { cn } from '@/lib/utils';
import { toast } from '@/stores/toastStore';

export function LlamaCppMetaRow() {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings, isPending } = useUpdateSettingsMutation();

  const connection = settings?.llm.llamaCpp;
  const url = connection?.url.trim() ?? '';
  const model = connection?.model.trim() ?? '';
  const isActive = settings?.llm.provider === 'llamacpp';
  const isConfigured = Boolean(url && model);

  const selectForChat = async () => {
    if (!isConfigured || isActive || isPending) return;
    try {
      await updateSettings({ category: 'llm', updates: { provider: 'llamacpp' } });
    } catch (error) {
      toast.error('Failed to set chat role', {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const goToChatSettings = () => {
    window.dispatchEvent(new CustomEvent('settings:navigate-tab', { detail: { tab: 'chat' } }));
  };

  const meta = model ? `Model · ${model}` : 'No model selected';

  return (
    <div className="connected-model-row group flex items-center gap-3 border-b border-border-subtle py-4">
      <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-raised text-accent"><Network className="h-4 w-4" aria-hidden="true" /></span>
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium text-text-primary">llama.cpp connection</div>
        {url ? <div className="truncate font-mono text-xs text-text-muted">{url}</div> : null}
        <p className="mt-1 wrap-break-word text-xs text-text-muted">{meta}</p>
        {settings?.llm.provider === 'auto' && isConfigured ? (
          <p className="mt-1 text-xs text-text-muted">
            Auto tries local models first. Select Chat to always use this connection.
          </p>
        ) : null}
      </div>

      <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
        <button
          type="button"
          onClick={() => void selectForChat()}
          disabled={!isConfigured || isActive || isPending}
          aria-pressed={isActive && isConfigured}
          title={!isConfigured ? 'Set a server URL and model in connection settings first'
            : isActive ? 'Selected for chat' : 'Use this llama.cpp connection for chat'}
          className={cn(
            'inline-flex h-7 items-center gap-1 rounded-md border px-2 text-xs transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
            isActive && isConfigured
              ? 'border-accent/25 bg-accent-muted font-medium text-accent'
              : 'border-border-default text-text-secondary hover:bg-surface-raised hover:text-text-primary',
            (!isConfigured || isPending) && 'cursor-not-allowed opacity-60',
          )}
        >
          {isActive && isConfigured ? <Check className="h-3 w-3" aria-hidden="true" /> : null}
          {isPending ? 'Setting…' : 'Chat'}
        </button>
        <button
          type="button"
          onClick={goToChatSettings}
          className="shrink-0 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
        >
          Edit connection
        </button>
      </div>
    </div>
  );
}
