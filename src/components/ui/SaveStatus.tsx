import { AlertCircle, Check, Circle, LoaderCircle } from 'lucide-react';

import { cn } from '@/lib/utils';

import './interactions.css';

export function SaveStatus({ state, error, savedLabel = 'Saved', unsavedLabel = 'Unsaved changes', className }: {
  state: 'saving' | 'saved' | 'unsaved' | 'error';
  error?: string;
  savedLabel?: string;
  unsavedLabel?: string;
  className?: string;
}) {
  const Icon = state === 'saving' ? LoaderCircle : state === 'saved' ? Check : state === 'error' ? AlertCircle : Circle;
  const text = state === 'saving' ? 'Saving…' : state === 'saved' ? savedLabel : state === 'error' ? `Could not save${error ? `: ${error}` : ''}` : unsavedLabel;
  return <span role={state === 'error' ? 'alert' : 'status'} data-state={state} className={cn('ui-save-status', className)}><Icon size={12} aria-hidden="true" className={state === 'saving' ? 'animate-spin' : undefined} />{text}</span>;
}
