import { useState } from 'react';

import { Sparkles } from 'lucide-react';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';

export type SynthesisScope = 'current' | 'pinned' | 'deck';

interface SynthesizePopoverProps {
  selectedEntryId: string | null;
  pinnedCount: number;
  deckCount: number;
  onSynthesize: (scope: SynthesisScope) => Promise<boolean>;
  disabled?: boolean;
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
}: SynthesizePopoverProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [scope, setScope] = useState<SynthesisScope>('current');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
      label: `Recent deck (${deckCount})`,
      helper: 'Synthesize the most recent entries in this journal',
      disabled: deckCount === 0,
    },
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
          <Sparkles className="h-3.5 w-3.5" strokeWidth={1.75} />
          Synthesize…
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-80 p-4">
        <p className="text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
          Synthesize
        </p>
        <p className="mt-1 text-sm text-[hsl(var(--text-secondary))]">
          Fold one or more entries into an appended block on this page.
        </p>
        <fieldset className="mt-3 space-y-2">
          <legend className="sr-only">Synthesis scope</legend>
          {availableScopes.map((option) => (
            <label
              key={option.id}
              className={`flex cursor-pointer items-start gap-2 rounded-sm border border-[hsl(var(--border-subtle))] px-3 py-2 transition-colors duration-fast ${
                scope === option.id
                  ? 'bg-[hsl(var(--accent-muted))] border-[hsl(var(--accent))]'
                  : 'hover:bg-[hsl(var(--surface))]'
              } ${option.disabled ? 'pointer-events-none opacity-50' : ''}`}
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
                <span className="block text-sm font-medium text-[hsl(var(--text-primary))]">
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
