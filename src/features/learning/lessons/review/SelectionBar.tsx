import type { ChangeEvent } from 'react';

import { Button } from '@/components/ui/button';

export type SelectionAction = {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  /** Shown under the bar while the action is disabled: why it cannot run. */
  hint?: string;
  tone?: 'default' | 'danger';
};

/**
 * One bar of actions for the selected review items, sticky at the bottom of
 * their list. Renders nothing while nothing is selected.
 */
export function SelectionBar({ count, noun, actions, onClear, busy = false }: {
  count: number;
  /** The item's name in the singular and the plural. */
  noun: [string, string];
  actions: SelectionAction[];
  onClear: () => void;
  busy?: boolean;
}) {
  if (count === 0) return null;
  const line = `${count} ${count === 1 ? noun[0] : noun[1]} selected`;
  const hints = [...new Set(actions.flatMap((action) => (action.disabled && action.hint ? [action.hint] : [])))];
  return (
    <div role="toolbar" aria-label={line} className="sticky bottom-0 z-10 rounded-xl border border-border bg-surface px-3 py-2 shadow-control">
      <div className="flex flex-wrap items-center gap-2">
        <span className="mr-auto text-sm font-medium tabular-nums text-text-primary">{line}</span>
        {actions.map((action) => (
          <Button key={action.label} type="button" size="sm" variant={action.tone === 'danger' ? 'destructive' : 'outline'}
            disabled={busy || action.disabled} onClick={action.onClick}>
            {action.label}
          </Button>
        ))}
        <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={onClear}>Clear</Button>
      </div>
      {hints.map((hint) => <p key={hint} className="mt-1 text-xs leading-5 text-text-secondary">{hint}</p>)}
    </div>
  );
}

/** A review item's checkbox; `onToggle` says whether shift was held, for ranges. */
export function ReviewCheckbox({ checked, label, onToggle, disabled = false }: {
  checked: boolean;
  label: string;
  onToggle: (shiftKey: boolean) => void;
  disabled?: boolean;
}) {
  const changed = (event: ChangeEvent<HTMLInputElement>) => {
    const native = event.nativeEvent as Partial<MouseEvent>;
    onToggle(native.shiftKey === true);
  };
  return (
    <input type="checkbox" aria-label={label} checked={checked} disabled={disabled} onChange={changed}
      className="mt-1 size-4 shrink-0 cursor-pointer accent-[hsl(var(--accent))] disabled:cursor-default disabled:opacity-45" />
  );
}
