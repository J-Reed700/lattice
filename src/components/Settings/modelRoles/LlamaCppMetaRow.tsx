/**
 * LlamaCppMetaRow
 *
 * One read-only row previewing the llama.cpp server connection, mirroring
 * {@link OllamaMetaRow}. Unlike the Ollama row there is no synthetic model
 * backing it — the llama.cpp connection (URL + model) lives entirely in LLM
 * settings and is edited on the Chat settings page, so this row only shows
 * what is configured and links back there.
 *
 * {@link AIModelsTab} renders this in place of {@link OllamaMetaRow} whenever
 * the llama.cpp connection is the one in use for chat (see `activeRemoteRow`),
 * so the Downloaded tab previews the connection the user actually set up
 * instead of always showing Ollama's.
 */

import { useSettingsQuery } from '../../../hooks/queries/useSettingsQuery';

export function LlamaCppMetaRow() {
  const { data: settings } = useSettingsQuery();

  const connection = settings?.llm.llamaCpp;
  const url = connection?.url.trim() ?? '';
  const model = connection?.model.trim() ?? '';

  const goToChatSettings = () => {
    window.dispatchEvent(new CustomEvent('settings:navigate-tab', { detail: { tab: 'chat' } }));
  };

  const meta = [
    'llama.cpp',
    url || 'no server URL',
    model ? `model ${model}` : 'model not set',
  ].join(' · ');

  return (
    <div className="group flex items-center gap-4 border-b border-border-subtle py-3">
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium text-text-primary">llama.cpp connection</div>
        {url ? (
          <div className="truncate font-mono text-xs text-text-muted">{url}</div>
        ) : (
          <button
            type="button"
            onClick={goToChatSettings}
            className="text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
          >
            Set a server URL
          </button>
        )}
        <p className="mt-1 wrap-break-word text-xs text-text-muted">{meta}</p>
      </div>

      {/* The model is typed on the Chat settings page, not here. */}
      {!model ? (
        <button
          type="button"
          onClick={goToChatSettings}
          className="shrink-0 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
        >
          Set model
        </button>
      ) : null}
    </div>
  );
}
