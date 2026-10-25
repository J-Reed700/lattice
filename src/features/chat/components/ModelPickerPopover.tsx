import { type KeyboardEvent, type ReactNode, useId, useMemo, useRef, useState } from 'react';

import * as Popover from '@radix-ui/react-popover';
import { Check, Cloud, HardDrive, Network, Settings2 } from 'lucide-react';

import { useCloudModelCatalogs } from '@/features/model/hooks/useCloudModels';
import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { degradedReasonFor, useLoadedModels } from '@/features/model/hooks/useLoadedModels';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import type { ChatModelChoice } from '@/lib/bindings';
import {
  buildChatModelOptions,
  CLOUD_PROVIDER_NAMES,
  type ChatModelOption,
  runningOnCpuSentence,
  sameModelRef,
} from '@/utils/chatModelSelection';

/**
 * The model picker for one chat, used by the model button above the composer
 * and by "Try with another model" on an answer. Picking a row changes that
 * chat, or that one answer, and nothing else: the app-wide default is set in
 * Settings.
 */

export interface ModelPickerPopoverProps {
  /** What this chat asked for; the checked row follows it. */
  choice: ChatModelChoice;
  /** The Settings default, named on the "Use default" row. */
  defaultOption?: ChatModelOption | null;
  /**
   * Puts this chat back on the Settings default. Without it there is no
   * "Use default" row, and the row of the model that answers is checked.
   */
  onUseDefault?: () => void | Promise<void>;
  onSelect: (_modelId: string, _modelLabel: string) => void | Promise<void>;
  children: ReactNode;
  /** Names the menu for a screen reader. */
  label?: string;
  align?: 'start' | 'end';
  side?: 'top' | 'bottom';
  /** Controlled open state, for a picker opened from somewhere other than its child. */
  open?: boolean;
  onOpenChange?: (_open: boolean) => void;
  /** The child only positions the picker; it does not toggle it (an overflow menu opens it). */
  anchorOnly?: boolean;
}

const GROUP_ORDER: Array<ChatModelOption['group']> = ['Local', 'llama.cpp', 'Ollama', 'Cloud'];

const ROW_CLASS =
  'flex w-full items-start gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] focus-visible:bg-[hsl(var(--text-primary)/0.06)] focus-visible:outline-hidden';
const ICON_SLOT_CLASS = 'flex h-5 w-5 shrink-0 items-center justify-center text-[hsl(var(--text-tertiary))]';
const BADGE_CLASS =
  'shrink-0 rounded-sm border border-border-default px-1 py-px text-xxs leading-4 text-[hsl(var(--text-secondary))]';
const SEES_IMAGES_BADGE_CLASS =
  'shrink-0 rounded-sm border border-accent/25 bg-accent-muted px-1 py-px text-xxs leading-4 text-accent';

export function ModelPickerPopover({
  choice,
  defaultOption = null,
  onUseDefault,
  onSelect,
  children,
  label = 'Model for this chat',
  align = 'end',
  side = 'top',
  open: openProp,
  onOpenChange,
  anchorOnly = false,
}: ModelPickerPopoverProps) {
  const [uncontrolledOpen, setUncontrolledOpen] = useState(false);
  const open = openProp ?? uncontrolledOpen;
  const setOpen = (next: boolean) => {
    if (openProp === undefined) setUncontrolledOpen(next);
    onOpenChange?.(next);
  };
  const { downloadedModels } = useDownloadedModels();
  const { data: settings } = useSettingsQuery();
  const cloud = useCloudModelCatalogs(open);
  const loadedModels = useLoadedModels();
  const menuRef = useRef<HTMLDivElement>(null);
  const groupIdPrefix = useId();

  const options = useMemo(
    () => buildChatModelOptions(settings?.llm, downloadedModels, cloud.catalogs),
    [settings?.llm, downloadedModels, cloud.catalogs]
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
  const offersDefault = onUseDefault !== undefined;
  const isChecked = (option: ChatModelOption) =>
    choice.kind === 'explicit'
      ? sameModelRef(option.ref, choice.model)
      : !offersDefault && option.isDefault;

  const items = () =>
    Array.from(menuRef.current?.querySelectorAll<HTMLElement>('[role="menuitemradio"]') ?? []);

  // A menu moves with the arrow keys and starts on the checked row.
  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const rows = items();
    if (rows.length === 0) return;
    const index = rows.indexOf(document.activeElement as HTMLElement);
    const target =
      event.key === 'ArrowDown' ? rows[(index + 1) % rows.length]
        : event.key === 'ArrowUp' ? rows[(index - 1 + rows.length) % rows.length]
          : event.key === 'Home' ? rows[0]
            : event.key === 'End' ? rows[rows.length - 1]
              : null;
    if (!target) return;
    event.preventDefault();
    target.focus();
  };

  const choose = (run: () => void | Promise<void>) => {
    setOpen(false);
    void run();
  };

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      {anchorOnly ? (
        <Popover.Anchor asChild>{children}</Popover.Anchor>
      ) : (
        <Popover.Trigger asChild>{children}</Popover.Trigger>
      )}
      <Popover.Portal>
        <Popover.Content
          side={side}
          sideOffset={6}
          align={align}
          onOpenAutoFocus={(event) => {
            const rows = items();
            const start = rows.find((row) => row.getAttribute('aria-checked') === 'true') ?? rows[0];
            if (!start) return;
            event.preventDefault();
            start.focus();
          }}
          className="surface-pop z-50 max-h-[min(70vh,600px)] w-[320px] overflow-y-auto rounded-xl bg-surface-overlay p-2 shadow-lg outline-hidden"
        >
          <div className="flex items-center justify-between gap-2 px-2 py-1.5 text-xs text-text-muted">
            <span aria-live="polite">{cloud.isLoading ? 'Loading cloud models…' : 'Available models'}</span>
            <button type="button" className="shrink-0 text-text-secondary hover:text-text-primary disabled:opacity-50"
              disabled={cloud.isFetching} onClick={() => { void cloud.refresh(); }}>Refresh models</button>
          </div>
          {cloud.errors.map((error) => <p key={error.provider} role="alert" className="px-2 py-1.5 text-xs text-text-muted">
            {CLOUD_PROVIDER_NAMES[error.provider]}: {error.message}
          </p>)}
          <div ref={menuRef} role="menu" aria-label={label} onKeyDown={handleKeyDown}>
            {offersDefault && (
              <button
                type="button"
                role="menuitemradio"
                aria-checked={choice.kind === 'default'}
                onClick={() => choose(onUseDefault)}
                className={ROW_CLASS}
              >
                <span className={ICON_SLOT_CLASS}>
                  <Settings2 className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm text-[hsl(var(--text-primary))]">
                    {defaultOption ? `Use default (${defaultOption.label})` : 'Use default'}
                  </span>
                  <span className="block truncate text-xs leading-snug text-[hsl(var(--text-muted))]">
                    {defaultOption
                      ? 'Follows the chat model set in Settings'
                      : 'No chat model is selected in Settings → Models.'}
                  </span>
                </span>
                {choice.kind === 'default' && <CheckMark />}
              </button>
            )}
            {options.length === 0 ? (
              <p className="px-2 py-1.5 text-sm text-[hsl(var(--text-muted))]">
                No models available. Add a provider API key in Settings → Chat, or install a local model.
              </p>
            ) : (
              groups.map((group) => {
                const groupLabelId = `${groupIdPrefix}-${group.group}`;
                return (
                  <div
                    key={group.group}
                    role={showGroupLabels ? 'group' : undefined}
                    aria-labelledby={showGroupLabels ? groupLabelId : undefined}
                  >
                    {showGroupLabels && (
                      <p
                        id={groupLabelId}
                        className="px-2 pb-1 pt-1.5 text-xxs font-medium uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]"
                      >
                        {group.group}
                      </p>
                    )}
                    {group.options.map((option) => (
                      <OptionRow
                        key={option.id}
                        option={option}
                        checked={isChecked(option)}
                        degraded={degradedReasonFor(option.ref, loadedModels)}
                        onChoose={() => choose(() => onSelect(option.id, option.triggerLabel))}
                      />
                    ))}
                  </div>
                );
              })
            )}
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function OptionRow({
  option,
  checked,
  degraded,
  onChoose,
}: {
  option: ChatModelOption;
  checked: boolean;
  degraded: string | null;
  onChoose: () => void;
}) {
  const detail = [option.subtitle, option.contextLabel].filter(Boolean).join(' · ');
  return (
    <button type="button" role="menuitemradio" aria-checked={checked} onClick={onChoose} className={ROW_CLASS}>
      <span className={ICON_SLOT_CLASS}>
        {option.backend === 'local' ? (
          <HardDrive className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
        ) : option.backend === 'llamacpp' || option.backend === 'ollama' ? (
          <Network className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
        ) : (
          <Cloud className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
        )}
      </span>
      <span className="min-w-0 flex-1">
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="truncate text-sm text-[hsl(var(--text-primary))]">{option.label}</span>
          {option.supportsTools === false && (
            <span className={BADGE_CLASS} title="This model cannot call tools, so it answers without searching.">
              no tools
            </span>
          )}
          {option.seesImages === true && (
            <span className={SEES_IMAGES_BADGE_CLASS} title="Pictures you attach are shown to this model.">
              sees images
            </span>
          )}
          {degraded && (
            <span className={BADGE_CLASS} title={runningOnCpuSentence(option.label, degraded)}>
              on CPU
            </span>
          )}
        </span>
        {detail && (
          <span className="block truncate font-mono text-xs leading-snug text-[hsl(var(--text-muted))]">
            {detail}
          </span>
        )}
      </span>
      {checked && <CheckMark />}
    </button>
  );
}

function CheckMark() {
  return <Check className="mt-1 h-3.5 w-3.5 shrink-0 text-[hsl(var(--accent))]" aria-hidden="true" />;
}
