import { type ReactNode, useMemo, useState } from 'react';

import * as Popover from '@radix-ui/react-popover';
import { Check, Cloud, HardDrive, Network } from 'lucide-react';

import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import {
  buildChatModelOptions,
  type ChatModelOption,
} from '@/utils/chatModelSelection';

/**
 * The chat-model picker, used both by "Try with…" on an answer and by the
 * active-model label above the composer.
 */

export interface ModelPickerPopoverProps {
  activeModelId?: string | null;
  onSelect: (_modelId: string, _modelLabel: string) => void | Promise<void>;
  children: ReactNode;
  align?: 'start' | 'end';
  side?: 'top' | 'bottom';
}

const GROUP_ORDER: Array<ChatModelOption['group']> = ['Local', 'llama.cpp', 'Ollama', 'Cloud'];

export function ModelPickerPopover({
  activeModelId,
  onSelect,
  children,
  align = 'end',
  side = 'top',
}: ModelPickerPopoverProps) {
  const [open, setOpen] = useState(false);
  const { downloadedModels } = useDownloadedModels();
  const { data: settings } = useSettingsQuery();

  const options = useMemo(
    () => buildChatModelOptions(settings?.llm, downloadedModels, activeModelId),
    [settings?.llm, downloadedModels, activeModelId]
  );

  const groups = useMemo(() => {
    const grouped = new Map<ChatModelOption['group'], ChatModelOption[]>();
    for (const option of options) {
      const list = grouped.get(option.group);
      if (list) {
        list.push(option);
      } else {
        grouped.set(option.group, [option]);
      }
    }
    return GROUP_ORDER.map((group) => ({
      group,
      options: grouped.get(group) ?? [],
    })).filter((entry) => entry.options.length > 0);
  }, [options]);

  // Group labels only earn their space when there is more than one group.
  const showGroupLabels = groups.length > 1;

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>{children}</Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side={side}
          sideOffset={6}
          align={align}
          className="surface-pop z-50 w-[300px] rounded-xl bg-surface-overlay p-2 shadow-lg outline-hidden"
        >
          {options.length === 0 ? (
            <p className="px-2 py-1.5 text-sm text-[hsl(var(--text-muted))]">
              No other models installed.
            </p>
          ) : (
            groups.map((group) => (
              <div key={group.group}>
                {showGroupLabels && (
                  <p className="px-2 pb-1 pt-1.5 text-xxs font-medium uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
                    {group.group}
                  </p>
                )}
                {group.options.map((option) => {
                  const isActive = settings?.llm
                    ? option.isActive
                    : option.isActive || (activeModelId != null && option.id === activeModelId);
                  return (
                    <button
                      key={option.id}
                      type="button"
                      onClick={() => {
                        setOpen(false);
                        void onSelect(option.id, option.triggerLabel);
                      }}
                      className="flex w-full items-start gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] focus-visible:bg-[hsl(var(--text-primary)/0.06)] focus-visible:outline-hidden"
                    >
                      <span className="flex h-5 w-5 shrink-0 items-center justify-center text-[hsl(var(--text-tertiary))]">
                        {option.backend === 'local' ? (
                          <HardDrive className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
                        ) : option.backend === 'llamacpp' || option.backend === 'ollama' ? (
                          <Network className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
                        ) : (
                          <Cloud className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
                        )}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm text-[hsl(var(--text-primary))]">
                          {option.label}
                        </span>
                        {option.subtitle && (
                          <span className="block truncate font-mono text-xs leading-snug text-[hsl(var(--text-muted))]">
                            {option.subtitle}
                          </span>
                        )}
                      </span>
                      {isActive && (
                        <Check
                          className="mt-1 h-3.5 w-3.5 shrink-0 text-[hsl(var(--accent))]"
                          aria-hidden="true"
                        />
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
