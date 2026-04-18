import { type ChangeEvent, memo } from 'react';

import { Search, Loader2 } from 'lucide-react';

interface SearchInputProps {
  value: string;
  onChange: (e: ChangeEvent<HTMLInputElement>) => void;
  isSearching: boolean;
}

export const SearchInput = memo(({ value, onChange, isSearching }: SearchInputProps) => (
  <div className="relative mb-4">
    <input
      type="text"
      placeholder="Search your files..."
      value={value}
      onChange={onChange}
      className="w-full px-5 py-3.5 pl-12 bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-primary))] text-base border border-[hsl(var(--border-subtle))] rounded-md focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))] focus:border-[hsl(var(--accent))] placeholder-[hsl(var(--text-tertiary))] transition-colors duration-fast"
    />
    <Search className="absolute left-4 top-4 w-4 h-4 text-[hsl(var(--text-tertiary))]" strokeWidth={1.75} />
    {isSearching && (
      <div className="absolute right-4 top-4">
        <Loader2 className="h-4 w-4 text-[hsl(var(--accent))] animate-spin" />
      </div>
    )}
  </div>
));

SearchInput.displayName = 'SearchInput';
