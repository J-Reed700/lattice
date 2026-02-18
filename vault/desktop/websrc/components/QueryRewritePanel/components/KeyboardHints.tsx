export function KeyboardHints() {
  return (
    <div className="flex items-center gap-4 text-xs text-[var(--text-secondary)] pt-2 border-t border-[var(--border-color)]">
      <div className="flex items-center gap-2">
        <kbd className="px-2 py-1 bg-[var(--surface-elevated)] border border-[var(--border-color)] rounded text-xs">
          0-3
        </kbd>
        <span>Select query</span>
      </div>
      <div className="flex items-center gap-2">
        <kbd className="px-2 py-1 bg-[var(--surface-elevated)] border border-[var(--border-color)] rounded text-xs">
          Esc
        </kbd>
        <span>Close</span>
      </div>
    </div>
  );
}
