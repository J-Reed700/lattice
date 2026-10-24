import { RotateCcw } from 'lucide-react';

/** A failed request's message, with a retry of the same request when one is safe. */
export function RetryAlert({ message, retry, disabled = false, className = '' }: {
  message: string;
  retry?: { label: string; run: () => void };
  disabled?: boolean;
  className?: string;
}) {
  return <div role="alert" className={`flex flex-wrap items-center justify-between gap-3 rounded-xl border border-rose-500/20 bg-rose-500/5 px-4 py-3 text-sm text-rose-700 ${className}`}>
    <span className="min-w-0 wrap-break-word">{message}</span>
    {retry && <button type="button" disabled={disabled} onClick={retry.run} className="inline-flex items-center gap-1 text-xs font-semibold underline"><RotateCcw size={12} /> {retry.label}</button>}
  </div>;
}
