import { type ChangeEvent, memo, useEffect, useRef } from 'react';

import { Search, Loader2 } from 'lucide-react';

interface SearchInputProps {
  value: string;
  onChange: (e: ChangeEvent<HTMLInputElement>) => void;
  isSearching: boolean;
}

export const SearchInput = memo(({ value, onChange, isSearching }: SearchInputProps) => {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  return (
    <div className="relative">
      <Search
        className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-text-muted"
        strokeWidth={1.75}
        aria-hidden="true"
      />
      <input
        ref={inputRef}
        type="text"
        placeholder="Search your documents"
        aria-label="Search your documents"
        value={value}
        onChange={onChange}
        className="h-10 w-full rounded-md border border-border-default bg-surface pl-9 pr-9 text-base text-text-primary placeholder:text-text-muted outline-none transition-colors duration-fast focus:border-accent"
      />
      {isSearching && (
        <Loader2
          className="absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 animate-spin text-text-muted"
          aria-hidden="true"
        />
      )}
    </div>
  );
});

SearchInput.displayName = 'SearchInput';
