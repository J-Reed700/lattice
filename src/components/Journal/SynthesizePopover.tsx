import { useEffect, useState } from 'react';

import { Combine } from 'lucide-react';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';

export type SynthesisScope = 'current' | 'pinned' | 'deck' | 'week' | 'conversation';

export interface WeekCandidateCounts {
  conversations: number;
  references: number;
  notes: number;
  total: number;
}

interface SynthesizePopoverProps {
  selectedEntryId: string | null;
  pinnedCount: number;
  deckCount: number;
  onSynthesize: (scope: SynthesisScope) => Promise<boolean>;
  disabled?: boolean;
  /** When set, a "This conversation" scope is offered. */
  conversationId?: string | null;
  /** Title shown in that option's helper line. */
  conversationTitle?: string | null;
  /** Real counts behind "Past week"; the option is disabled when total is 0. */
  weekCandidates?: WeekCandidateCounts;
}

/**
 * "6 conversations · 4 saved passages · 2 journal pages" — real counts only,
 * zero parts omitted, singulars correct.
 */
export function describeWeekCandidates(counts: WeekCandidateCounts): string {
  const parts: string[] = [];
  if (counts.conversations > 0) {
    parts.push(
      `${counts.conversations} conversation${counts.conversations === 1 ? '' : 's'}`,
    );
  }
  if (counts.references > 0) {
    parts.push(
      `${counts.references} saved passage${counts.references === 1 ? '' : 's'}`,
    );
  }
  if (counts.notes > 0) {
    parts.push(`${counts.notes} journal page${counts.notes === 1 ? '' : 's'}`);
  }
  return parts.join(' · ');
}

/**
 * Quiet Synthesize affordance: ghost button in the bottom rail opens a
 * Popover with three radio-style options, routes to the existing synthesis
 * flow.
 * Spec §5.5.
 */
export function SynthesizePopover({
  selectedEntryId,
  pinnedCount,
  deckCount,
  onSynthesize,
  disabled = false,
  conversationId = null,
  conversationTitle = null,
  weekCandidates,
}: SynthesizePopoverProps) {
  const defaultScope: SynthesisScope = conversationId ? 'conversation' : 'current';
  const [isOpen, setIsOpen] = useState(false);
  const [scope, setScope] = useState<SynthesisScope>(defaultScope);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setScope(defaultScope);
  }, [defaultScope]);

  const weekHelper = weekCandidates
    ? weekCandidates.total > 0
      ? describeWeekCandidates(weekCandidates)
      : 'Nothing from the past week'
    : 'Conversations, saved passages and journal pages';

  const availableScopes: Array<{
    id: SynthesisScope;
    label: string;
    helper: string;
    disabled?: boolean;
  }> = [
    {
      id: 'current',
      label: 'This entry only',
      helper: selectedEntryId ? 'Synthesize the selected entry' : 'Select an entry first',
      disabled: !selectedEntryId,
    },
    {
      id: 'pinned',
      label: `All pinned entries (${pinnedCount})`,
      helper:
        pinnedCount > 0
          ? 'Synthesize every entry you have pinned'
          : 'You have no pinned entries',
      disabled: pinnedCount === 0,
    },
    {
      id: 'deck',
      label: `Recent entries (${deckCount})`,
      helper: 'Synthesize the most recent entries in this journal',
      disabled: deckCount === 0,
    },
    {
      id: 'week',
      label: 'Past week',
      helper: weekHelper,
      disabled: weekCandidates ? weekCandidates.total === 0 : false,
    },
    ...(conversationId
      ? [
          {
            id: 'conversation' as const,
            label: 'This conversation',
            helper: conversationTitle?.trim() || 'The open conversation',
          },
        ]
      : []),
  ];

  const handleSubmit = async () => {
    setIsSubmitting(true);
    setError(null);
    const ok = await onSynthesize(scope);
    setIsSubmitting(false);
    if (ok) {
      setIsOpen(false);
    } else {
      setError('Synthesis failed. Try again.');
    }
  };

  return (
    <Popover
      open={isOpen}
      onOpenChange={(next) => {
        setIsOpen(next);
        if (!next) {
          setError(null);
        }
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={disabled}
          className="inline-flex items-center gap-1.5 rounded-sm px-2 py-1 text-sm text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast disabled:cursor-not-allowed disabled:opacity-40"
        >
          <Combine className="h-3.5 w-3.5" strokeWidth={1.75} />
          Synthesize…
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-80 p-4">
        <p className="text-sm font-medium text-[hsl(var(--text-primary))]">Synthesize</p>
        <p className="mt-0.5 text-xs text-[hsl(var(--text-tertiary))]">
          {conversationId
            ? 'Writes a summary onto a journal page.'
            : 'Writes a summary of the chosen entries onto this page.'}
        </p>
        <fieldset className="mt-3 border-t border-[hsl(var(--border-subtle))]">
          <legend className="sr-only">Synthesis scope</legend>
          {availableScopes.map((option) => (
            <label
              key={option.id}
              className={`flex cursor-pointer items-start gap-2.5 border-b border-[hsl(var(--border-subtle))] px-1 py-2.5 transition-colors duration-fast hover:bg-[hsl(var(--surface))] ${
                option.disabled ? 'pointer-events-none opacity-50' : ''
              }`}
            >
              <input
                type="radio"
                name="synthesis-scope"
                checked={scope === option.id}
                onChange={() => {
                  if (!option.disabled) setScope(option.id);
                }}
                disabled={option.disabled}
                className="mt-1 accent-[hsl(var(--accent))]"
              />
              <span className="flex-1">
                <span className={`block text-sm ${scope === option.id ? 'text-[hsl(var(--text-primary))]' : 'text-[hsl(var(--text-secondary))]'}`}>
                  {option.label}
                </span>
                <span className="block text-xs text-[hsl(var(--text-tertiary))]">
                  {option.helper}
                </span>
              </span>
            </label>
          ))}
        </fieldset>
        {error && (
          <p className="mt-2 text-xs text-[hsl(var(--danger-fg))]">{error}</p>
        )}
        <div className="mt-4 flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={() => setIsOpen(false)}
            className="rounded-sm px-3 py-1.5 text-xs text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void handleSubmit()}
            disabled={
              isSubmitting ||
              availableScopes.find((o) => o.id === scope)?.disabled
            }
            className="inline-flex items-center gap-1.5 rounded-sm bg-[hsl(var(--accent))] px-3 py-1.5 text-xs font-medium text-[hsl(var(--accent-fg))] hover:bg-[hsl(var(--accent-hover))] transition-colors duration-fast disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isSubmitting ? 'Synthesizing…' : 'Synthesize'}
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
