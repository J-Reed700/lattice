import { memo } from 'react';

interface SearchModeButtonsProps {
  mode: 'semantic' | 'keyword' | 'hybrid';
  onModeChange: (mode: 'semantic' | 'keyword' | 'hybrid') => void;
}

export const SearchModeButtons = memo(({ mode, onModeChange }: SearchModeButtonsProps) => (
  <div className="inline-flex p-1 gap-0.5 bg-[var(--bg-tertiary)]/50 rounded-lg border border-[var(--border-color)]">
    <button
      onClick={() => onModeChange('semantic')}
      className={`px-3 py-1.5 text-sm rounded-md transition-all duration-200 ${
        mode === 'semantic'
          ? 'bg-[var(--surface-elevated)] text-[var(--text-primary)] shadow-sm'
          : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
      }`}
    >
      Semantic
    </button>
    <button
      onClick={() => onModeChange('keyword')}
      className={`px-3 py-1.5 text-sm rounded-md transition-all duration-200 ${
        mode === 'keyword'
          ? 'bg-[var(--surface-elevated)] text-[var(--text-primary)] shadow-sm'
          : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
      }`}
    >
      Keyword
    </button>
    <button
      onClick={() => onModeChange('hybrid')}
      className={`px-3 py-1.5 text-sm rounded-md transition-all duration-200 ${
        mode === 'hybrid'
          ? 'bg-[var(--surface-elevated)] text-[var(--text-primary)] shadow-sm'
          : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
      }`}
    >
      Hybrid
    </button>
  </div>
));

SearchModeButtons.displayName = 'SearchModeButtons';
