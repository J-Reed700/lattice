import { memo } from 'react';

interface SearchModeButtonsProps {
  mode: 'semantic' | 'keyword' | 'hybrid';
  onModeChange: (mode: 'semantic' | 'keyword' | 'hybrid') => void;
}

export const SearchModeButtons = memo(({ mode, onModeChange }: SearchModeButtonsProps) => (
  <div className="inline-flex p-1 gap-0.5 bg-[hsl(var(--surface-raised))]/50 rounded-lg border border-[hsl(var(--border-subtle))]">
    <button
      onClick={() => onModeChange('semantic')}
      className={`px-3 py-1.5 text-sm rounded-md transition-colors duration-fast ${
        mode === 'semantic'
          ? 'bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-primary))] shadow-sm'
          : 'text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
      }`}
    >
      Semantic
    </button>
    <button
      onClick={() => onModeChange('keyword')}
      className={`px-3 py-1.5 text-sm rounded-md transition-colors duration-fast ${
        mode === 'keyword'
          ? 'bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-primary))] shadow-sm'
          : 'text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
      }`}
    >
      Keyword
    </button>
    <button
      onClick={() => onModeChange('hybrid')}
      className={`px-3 py-1.5 text-sm rounded-md transition-colors duration-fast ${
        mode === 'hybrid'
          ? 'bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-primary))] shadow-sm'
          : 'text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
      }`}
    >
      Hybrid
    </button>
  </div>
));

SearchModeButtons.displayName = 'SearchModeButtons';
