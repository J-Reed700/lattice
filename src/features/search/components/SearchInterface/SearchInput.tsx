import { type ChangeEvent, memo, useEffect, useRef } from 'react';

import { Loader2, Search, X } from 'lucide-react';

interface SearchInputProps {
  value: string;
  onChange: (e: ChangeEvent<HTMLInputElement>) => void;
  onClear?: () => void;
  isSearching: boolean;
}

export const SearchInput = memo(({ value, onChange, onClear, isSearching }: SearchInputProps) => {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const showClear = !isSearching && value.length > 0 && Boolean(onClear);

  return (
    <div className="relative">
      <Search
        className="pointer-events-none absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-text-tertiary"
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
        className="h-11 w-full rounded-xl border border-border-default bg-surface pl-10 pr-10 text-[15px] tracking-[-0.01em] text-text-primary shadow-control placeholder:text-text-muted outline-hidden transition-[border-color,box-shadow] duration-fast hover:border-border-strong focus-visible:border-accent/60 focus-visible:ring-2 focus-visible:ring-ring"
      />
      {isSearching && (
        <Loader2
          className="absolute right-3.5 top-1/2 h-4 w-4 -translate-y-1/2 animate-spin text-text-muted"
          aria-hidden="true"
        />
      )}
      {showClear && (
        <button
          type="button"
          onClick={() => {
            onClear?.();
            inputRef.current?.focus();
          }}
          aria-label="Clear search"
          className="absolute right-2.5 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-md text-text-tertiary transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          <X className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
        </button>
      )}
    </div>
  );
});

SearchInput.displayName = 'SearchInput';
