import { type ReactNode, useMemo, useState } from 'react';

import * as Popover from '@radix-ui/react-popover';
import { Check } from 'lucide-react';

import { useDownloadedModels } from '@/hooks/useDownloadedModels';

import type { DownloadedModel } from '../../types/downloadedModels';

/**
 * The chat-model picker, used both by "Try with…" on an answer and by the
 * active-model label above the composer.
 *
 * `useDownloadedModels()` is the only source: there is no `list_ollama_models`
 * command, and an Ollama server already appears in `downloaded_models` as a
 * synthetic row, so nothing else is needed to list both backends.
 */

export interface ModelPickerPopoverProps {
  activeModelId?: string | null;
  onSelect: (_modelId: string, _modelLabel: string) => void | Promise<void>;
  children: ReactNode;
  align?: 'start' | 'end';
}

const labelFor = (model: DownloadedModel): string => model.model_name || model.model_id;

export function ModelPickerPopover({
  activeModelId,
  onSelect,
  children,
  align = 'end',
}: ModelPickerPopoverProps) {
  const [open, setOpen] = useState(false);
  const { downloadedModels } = useDownloadedModels();

  const chatModels = useMemo(
    () => downloadedModels.filter((model) => model.model_type === 'language_model'),
    [downloadedModels]
  );

  const groups = useMemo(() => {
    const local = chatModels.filter((model) => model.backend !== 'ollama');
    const ollama = chatModels.filter((model) => model.backend === 'ollama');
    const result: Array<{ label: string; models: DownloadedModel[] }> = [];
    if (local.length > 0) result.push({ label: 'Local', models: local });
    if (ollama.length > 0) result.push({ label: 'Ollama', models: ollama });
    return result;
  }, [chatModels]);

  // Group labels only earn their space when there is more than one group.
  const showGroupLabels = groups.length > 1;

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>{children}</Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          sideOffset={6}
          align={align}
          className="z-50 w-[280px] rounded-md border border-subtle bg-surface-raised p-2 shadow-md outline-none"
        >
          {chatModels.length === 0 ? (
            <p className="px-2 py-1.5 text-sm text-[hsl(var(--text-muted))]">
              No other models installed.
            </p>
          ) : (
            groups.map((group) => (
              <div key={group.label}>
                {showGroupLabels && (
                  <p className="px-2 py-1 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
                    {group.label}
                  </p>
                )}
                {group.models.map((model) => {
                  const isActive = model.model_id === activeModelId;
                  return (
                    <button
                      key={model.id}
                      type="button"
                      onClick={() => {
                        setOpen(false);
                        void onSelect(model.model_id, labelFor(model));
                      }}
                      className="flex w-full items-center justify-between rounded-sm px-2 py-1.5 text-left text-sm text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface"
                    >
                      <span className="min-w-0 truncate">{labelFor(model)}</span>
                      {isActive && (
                        <Check className="ml-2 h-3.5 w-3.5 shrink-0 text-[hsl(var(--accent))]" />
                      )}
                    </button>
                  );
                })}
              </div>
            ))
          )}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
