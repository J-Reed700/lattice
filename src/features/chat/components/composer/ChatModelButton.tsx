import type { RefObject } from 'react';

import { ChevronDown, Cloud, HardDrive, Network, TriangleAlert } from 'lucide-react';

import { ModelPickerPopover } from '@/features/chat/components/ModelPickerPopover';
import type { ChatModelChoice } from '@/lib/bindings';
import {
  type ChatModelOption,
  type EffectiveChatModel,
  isUnavailableChatModel,
  runningOnCpuSentence,
} from '@/utils/chatModelSelection';

export interface ChatModelButtonProps {
  /** What this chat asked for. */
  choice: ChatModelChoice;
  /** What answers its next turn; `null` on the default with no default set. */
  effective: EffectiveChatModel | null;
  /** The Settings default, for the picker's "Use default" row. */
  defaultOption: ChatModelOption | null;
  /** Why the model answering this chat runs on the CPU, while it does. */
  degraded: string | null;
  /** There is a model to pick. Without one, and with none set, nothing is drawn. */
  canChoose: boolean;
  onSelect: (_modelId: string, _modelLabel: string) => void | Promise<void>;
  onUseDefault: () => void | Promise<void>;
  /** The palette opens the picker by clicking this. */
  triggerRef: RefObject<HTMLButtonElement | null>;
}

const ICON_CLASS = 'h-3 w-3 shrink-0 opacity-70';

/**
 * The model button beside the composer: which model answers this chat, and
 * whether that is the app default or this chat's own choice. It says so in
 * words, not colour: "Default · X", "X for this chat", "X unavailable" and
 * "· on CPU", each with the full sentence as its tooltip and accessible name.
 */
export function ChatModelButton({
  choice,
  effective,
  defaultOption,
  degraded,
  canChoose,
  onSelect,
  onUseDefault,
  triggerRef,
}: ChatModelButtonProps) {
  if (!effective && !canChoose) return null;

  const isDefault = choice.kind === 'default';
  const unavailable = isUnavailableChatModel(effective) ? effective : null;
  const option = effective && !isUnavailableChatModel(effective) ? effective : null;
  const name = unavailable?.label ?? option?.label ?? null;
  const cpuSentence = name && degraded && !unavailable ? runningOnCpuSentence(name, degraded) : null;

  const origin = isDefault ? 'app default' : unavailable ? 'chosen for this chat, unavailable' : 'chosen for this chat';
  const accessibleName = name
    ? [`Model for this chat: ${name} (${origin}).`, unavailable?.reason, cpuSentence].filter(Boolean).join(' ')
    : 'Choose a model for this chat';
  const title = unavailable?.reason ?? cpuSentence ?? (isDefault ? 'The chat model set in Settings' : 'This chat\'s own model');

  return (
    <ModelPickerPopover
      choice={choice}
      defaultOption={defaultOption}
      onUseDefault={onUseDefault}
      onSelect={onSelect}
      side="top"
      align="start"
    >
      <button
        ref={triggerRef}
        type="button"
        title={title}
        aria-label={accessibleName}
        className={`inline-flex h-7 min-w-0 items-center gap-1 rounded-md px-2 text-xs transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))] ${
          unavailable ? 'text-[hsl(var(--warning-fg))]' : 'text-[hsl(var(--text-tertiary))]'
        }`}
      >
        {unavailable ? (
          <TriangleAlert className={ICON_CLASS} strokeWidth={1.75} aria-hidden="true" />
        ) : option?.backend === 'llamacpp' || option?.backend === 'ollama' ? (
          <Network className={ICON_CLASS} strokeWidth={1.75} aria-hidden="true" />
        ) : option?.backend === 'cloud' ? (
          <Cloud className={ICON_CLASS} strokeWidth={1.75} aria-hidden="true" />
        ) : (
          <HardDrive className={ICON_CLASS} strokeWidth={1.75} aria-hidden="true" />
        )}
        {name ? (
          <>
            <span className="truncate">{isDefault ? `Default · ${name}` : name}</span>
            {unavailable ? (
              <span className="shrink-0">unavailable</span>
            ) : !isDefault ? (
              <span className="shrink-0 text-[hsl(var(--text-muted))]">for this chat</span>
            ) : null}
            {cpuSentence && <span className="shrink-0">· on CPU</span>}
          </>
        ) : (
          <span className="truncate">Choose model</span>
        )}
        <ChevronDown className={ICON_CLASS} strokeWidth={1.75} aria-hidden="true" />
      </button>
    </ModelPickerPopover>
  );
}
