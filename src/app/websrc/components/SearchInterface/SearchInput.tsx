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
      className="w-full px-5 py-3.5 pl-12 bg-[var(--surface-elevated)] text-[var(--text-primary)] text-[15px] border border-[var(--border-color)] rounded-xl focus:outline-none focus:ring-2 focus:ring-[var(--accent-primary)]/50 focus:border-[var(--accent-primary)]/50 focus:shadow-lg focus:shadow-[var(--accent-primary)]/5 placeholder-[var(--text-tertiary)] transition-all duration-200"
    />
    <Search className="absolute left-4 top-4 w-5 h-5 text-[var(--text-tertiary)]" />
    {isSearching && (
      <div className="absolute right-4 top-4">
        <Loader2 className="h-5 w-5 text-[var(--accent-primary)] animate-spin" />
      </div>
    )}
  </div>
));

SearchInput.displayName = 'SearchInput';
