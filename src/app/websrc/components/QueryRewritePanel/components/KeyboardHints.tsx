export function KeyboardHints() {
  return (
    <div className="flex items-center gap-4 text-xs text-[hsl(var(--text-secondary))] pt-2 border-t border-[hsl(var(--border-subtle))]">
      <div className="flex items-center gap-2">
        <kbd className="px-2 py-1 bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded text-xs">
          0-3
        </kbd>
        <span>Select query</span>
      </div>
      <div className="flex items-center gap-2">
        <kbd className="px-2 py-1 bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded text-xs">
          Esc
        </kbd>
        <span>Close</span>
      </div>
    </div>
  );
}
