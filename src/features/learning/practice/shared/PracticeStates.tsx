import type React from 'react';

import { Inbox, Loader2 } from 'lucide-react';

export function PracticeLoadingState({ label }: { label: string }) {
  return (
    <div
      role="status"
      aria-live="polite"
      className="flex min-h-[160px] flex-col items-center justify-center gap-3 p-6 text-center text-xs text-text-secondary"
    >
      <Loader2 className="h-6 w-6 animate-spin text-accent" aria-hidden="true" />
      <span>{label}</span>
    </div>
  );
}

export function PracticeEmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex min-h-[160px] flex-col items-center justify-center gap-2 rounded-xl border border-border-subtle bg-surface p-6 text-center">
      <Inbox className="h-7 w-7 text-text-muted" aria-hidden="true" />
      <h4 className="text-sm font-semibold text-text-primary">{title}</h4>
      {description && <p className="text-xs text-text-secondary">{description}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}
