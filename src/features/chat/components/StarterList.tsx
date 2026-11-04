import { cn } from '@/lib/utils';

/** Suggested prompts, under a line of their own when there is one. Picking one fills the composer rather than sending. */
export function StarterList({ starters, onPick, note, className }: {
  starters: string[];
  onPick: (_text: string) => void;
  /** A line above the prompts, or alone when there are none. */
  note?: string;
  className?: string;
}) {
  return (
    <>
      {note && <p className="text-sm text-[hsl(var(--text-muted))]">{note}</p>}
      {starters.length > 0 && (
        <ul className={cn('border-t border-subtle', note && 'mt-4', className)}>
          {starters.map((starter) => (
            <li key={starter} className="border-b border-subtle">
              <button
                type="button"
                onClick={() => onPick(starter)}
                className="w-full px-1 py-2.5 text-left font-serif text-sm text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
              >
                {starter}
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
