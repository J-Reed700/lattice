import { learnerError } from '@/lib/errorHandling';

/** What went wrong in one plain sentence from the error's code, with the backend's own text under Details. */
export function PlainError({ error, className = 'mt-4 rounded-xl bg-rose-500/10 p-4 text-sm text-rose-700' }: { error: unknown; className?: string }) {
  const { headline, raw } = learnerError(error);
  return <div role="alert" className={className}>
    <p>{headline}</p>
    {raw && <details className="mt-2 text-xs">
      <summary className="cursor-pointer font-medium">Details</summary>
      <p className="mt-1 wrap-break-word">{raw}</p>
    </details>}
  </div>;
}
