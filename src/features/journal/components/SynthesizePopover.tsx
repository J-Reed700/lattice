import { useEffect, useRef, useState } from 'react';

import { Combine, Loader2 } from 'lucide-react';

import { Button, Popover, PopoverContent, PopoverTrigger } from '@/components/ui';
import { useActiveSynthesis, useSynthesisPanel } from '@/features/journal/synthesis/synthesisPanel';

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
  const submitting = useRef(false);
  const isSynthesizing = useActiveSynthesis() !== null;
  const busy = isSubmitting || isSynthesizing;

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
    if (submitting.current || isSynthesizing) return;
    submitting.current = true;
    setIsSubmitting(true);
    setError(null);
    try {
      const ok = await onSynthesize(scope);
      if (ok) setIsOpen(false);
      else setError('Synthesis failed. Try again.');
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : 'Synthesis failed. Try again.');
    } finally {
      submitting.current = false;
      setIsSubmitting(false);
    }
  };

  return (
    <Popover
      open={isOpen}
      onOpenChange={(next) => {
        if (next && isSynthesizing) {
          useSynthesisPanel.getState().setMinimized(false);
          return;
        }
        setIsOpen(next);
        if (!next) {
          setError(null);
        }
      }}
    >
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled && !isSynthesizing}
          className="h-7 gap-1.5 px-2 text-xs text-text-secondary hover:text-text-primary"
        >
          {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin motion-reduce:animate-none" aria-hidden="true" /> : <Combine className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />}
          {busy ? 'Synthesizing…' : 'Synthesize…'}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-80 rounded-xl p-4">
        <p className="text-sm font-medium text-text-primary">Synthesize</p>
        <p className="mt-0.5 text-xs text-text-tertiary">
          {conversationId
            ? 'Writes a summary onto a journal page.'
            : 'Writes a summary of the chosen entries onto this page.'}
        </p>
        <fieldset disabled={busy} className="mt-3 border-t border-border-subtle">
          <legend className="sr-only">Synthesis scope</legend>
          {availableScopes.map((option) => (
            <label
              key={option.id}
              className={`flex cursor-pointer items-start gap-2.5 border-b border-border-subtle px-1.5 py-2.5 transition-colors duration-fast hover:bg-surface ${
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
                className="mt-1 accent-accent"
              />
              <span className="flex-1">
                <span className={`block text-ui ${scope === option.id ? 'font-medium text-text-primary' : 'text-text-secondary'}`}>
                  {option.label}
                </span>
                <span className="block text-xs text-text-tertiary">
                  {option.helper}
                </span>
              </span>
            </label>
          ))}
        </fieldset>
        {error && (
          <p role="alert" className="mt-2 text-xs text-danger-fg">{error}</p>
        )}
        {busy && <p role="status" className="mt-3 text-xs text-text-secondary">Synthesis is running. This can take a few minutes.</p>}
        <div className="mt-4 flex items-center justify-end gap-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setIsOpen(false)}
          >
            {busy ? 'Close' : 'Cancel'}
          </Button>
          <Button
            type="button"
            size="sm"
            onClick={() => void handleSubmit()}
            disabled={
              busy ||
              availableScopes.find((o) => o.id === scope)?.disabled
            }
          >
            {busy ? 'Synthesizing…' : 'Synthesize'}
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
