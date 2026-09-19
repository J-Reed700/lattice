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
        className="pointer-events-none absolute left-4 top-1/2 h-[17px] w-[17px] -translate-y-1/2 text-text-tertiary"
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
        className="h-12 w-full rounded-xl border border-transparent bg-surface pl-11 pr-10 text-[16px] tracking-[-0.01em] text-text-primary shadow-sheet placeholder:text-text-muted outline-none transition-shadow duration-base focus:shadow-[var(--shadow-sheet),0_0_0_3px_hsl(var(--accent)/0.18)]"
      />
      {isSearching && (
        <Loader2
          className="absolute right-4 top-1/2 h-4 w-4 -translate-y-1/2 animate-spin text-text-muted"
          aria-hidden="true"
        />
      )}
    </div>
  );
});

SearchInput.displayName = 'SearchInput';
