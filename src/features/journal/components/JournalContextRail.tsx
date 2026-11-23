import { type ReactNode, useState } from 'react';

import { PanelRightClose } from 'lucide-react';

import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui';
import { IconButton } from '@/components/ui/IconButton';

export type ContextTab = 'conversation' | 'highlights' | 'sources';

interface JournalContextRailProps {
  /** Id of the entry picked in the index; a new pick brings its conversation forward. */
  selectedEntryId: string | null;
  highlightCount: number;
  sourceCount: number;
  conversation: ReactNode;
  highlights: ReactNode;
  sources: ReactNode;
  /** Optional footer control pinned to the foot of the rail. */
  footer?: ReactNode;
  onClose: () => void;
}

/**
 * Context inspector beside the page: the picked conversation, saved highlights,
 * and sources & citations. Defaults closed so writing gets full measure, and
 * toggles with ⌘.
 */
export function JournalContextRail({
  selectedEntryId,
  highlightCount,
  sourceCount,
  conversation,
  highlights,
  sources,
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
    { id: 'sources', label: 'Sources', count: sourceCount },
  ];

  return (
    <aside
      aria-label="Beside this page"
      className="flex h-full w-[320px] shrink-0 flex-col border-l border-border-subtle bg-bg max-lg:absolute max-lg:inset-y-0 max-lg:right-0 max-lg:z-20 max-lg:shadow-xl 2xl:w-[360px]"
    >
      <Tabs
        variant="pill"
        value={tab}
        onValueChange={(next) => setTab(next as ContextTab)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border-subtle px-3">
          <TabsList className="flex-1">
            {tabs.map((option) => (
              <TabsTrigger
                key={option.id}
                value={option.id}
                onClick={() => setTab(option.id)}
                className="flex-1 px-2 text-xs"
              >
                <span>{option.label}</span>
                {option.count ? (
                  <span className="tabular-nums text-text-muted">{option.count}</span>
                ) : null}
              </TabsTrigger>
            ))}
          </TabsList>
          <IconButton label="Hide panel" shortcut="⌘." onClick={onClose}>
            <PanelRightClose />
          </IconButton>
        </div>

        {/* All panels stay mounted so the highlights panel retains selection handlers. */}
        <TabsContent
          value="conversation"
          forceMount
          className="mt-0 min-h-0 flex-1 overflow-y-auto px-4 pb-6 pt-3 data-[state=inactive]:hidden"
        >
          {conversation}
        </TabsContent>
        <TabsContent
          value="highlights"
          forceMount
          className="mt-0 min-h-0 flex-1 overflow-y-auto px-4 pb-6 pt-3 data-[state=inactive]:hidden"
        >
          {highlights}
        </TabsContent>
        <TabsContent
          value="sources"
          forceMount
          aria-labelledby={undefined}
          aria-label="Sources and citations"
          className="mt-0 min-h-0 flex-1 overflow-y-auto px-4 pb-6 pt-3 data-[state=inactive]:hidden"
        >
          {sources}
        </TabsContent>
      </Tabs>

      {footer ? (
        <div className="shrink-0 border-t border-border-subtle px-3 py-2.5">{footer}</div>
      ) : null}
    </aside>
  );
}
