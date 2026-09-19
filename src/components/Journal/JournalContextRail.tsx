import { type ReactNode, useState } from 'react';

import { PanelRightClose } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { cn } from '@/lib/utils';

export type ContextTab = 'conversation' | 'highlights';

interface JournalContextRailProps {
  /** Id of the entry picked in the index; a new pick brings its conversation forward. */
  selectedEntryId: string | null;
  highlightCount: number;
  conversation: ReactNode;
  highlights: ReactNode;
  /** The synthesize control, pinned to the foot of the rail. */
  footer: ReactNode;
  onClose: () => void;
}

/**
 * What sits beside the page while you write: the conversation you picked, and
 * the lines you kept. It used to trail the page as collapsed rows, below the
 * fold — so picking an entry looked like it did nothing.
 */
export function JournalContextRail({
  selectedEntryId,
  highlightCount,
  conversation,
  highlights,
  footer,
  onClose,
}: JournalContextRailProps) {
  const [tab, setTab] = useState<ContextTab>('conversation');

  // Picking another entry is a request to read it. Adjusted during render, not
  // in an effect, so there is no frame where the old tab shows the new entry.
  const [seenEntryId, setSeenEntryId] = useState(selectedEntryId);
  if (seenEntryId !== selectedEntryId) {
    setSeenEntryId(selectedEntryId);
    if (selectedEntryId) setTab('conversation');
  }

  const tabs: Array<{ id: ContextTab; label: string; count?: number }> = [
    { id: 'conversation', label: 'Conversation' },
    { id: 'highlights', label: 'Highlights', count: highlightCount },
  ];

  return (
    <aside
      aria-label="Beside this page"
      className="flex h-full w-[320px] shrink-0 flex-col border-l border-border-subtle bg-bg 2xl:w-[360px]"
    >
      <div className="flex h-12 shrink-0 items-center gap-2 px-3">
        <div role="tablist" className="flex h-8 flex-1 items-center gap-0.5 rounded-lg bg-[hsl(var(--text-primary)/0.06)] p-[3px]">
          {tabs.map((option) => {
            const isActive = option.id === tab;
            return (
              <button
                key={option.id}
                type="button"
                role="tab"
                aria-selected={isActive}
                onClick={() => setTab(option.id)}
                className={cn(
                  'flex h-full flex-1 items-center justify-center gap-1.5 rounded-[5px] text-xs font-medium transition-[background-color,color,box-shadow] duration-fast',
                  isActive
                    ? 'bg-surface-overlay text-text-primary shadow-control'
                    : 'text-text-tertiary hover:text-text-primary',
                )}
              >
                {option.label}
                {option.count ? <span className="tabular-nums text-text-muted">{option.count}</span> : null}
              </button>
            );
          })}
        </div>
        <IconButton label="Hide panel" shortcut="⌘." onClick={onClose}>
          <PanelRightClose />
        </IconButton>
      </div>

      {/* Both stay mounted: the highlights panel owns the selection toolbar. */}
      <div role="tabpanel" className={cn('min-h-0 flex-1 overflow-y-auto px-4 pb-6 pt-2', tab !== 'conversation' && 'hidden')}>
        {conversation}
      </div>
      <div role="tabpanel" className={cn('min-h-0 flex-1 overflow-y-auto px-4 pb-6 pt-2', tab !== 'highlights' && 'hidden')}>
        {highlights}
      </div>

      <div className="shrink-0 border-t border-border-subtle px-3 py-2.5">{footer}</div>
    </aside>
  );
}
