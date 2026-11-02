/** Suggested prompts. Picking one fills the composer rather than sending. */
export function StarterList({ starters, onPick, className }: {
  starters: string[];
  onPick: (_text: string) => void;
  className?: string;
}) {
  return (
    <ul className={`border-t border-subtle ${className ?? ''}`}>
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
  );
}
