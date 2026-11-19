import { useEffect } from 'react';

import { ArrowLeft, Plus } from 'lucide-react';

import { formatShortcut } from '@/config/shortcuts';
import { cn } from '@/lib/utils';

export type LibraryHubView = 'files' | 'search' | 'references' | 'compare' | 'ingest';

interface LibraryHubNavProps {
  activeView: LibraryHubView;
  onNavigate: (path: string) => void;
  className?: string;
}

const LENS_ITEMS: ReadonlyArray<{ id: Exclude<LibraryHubView, 'ingest'>; label: string; path: string }> = [
  { id: 'files', label: 'Documents', path: '/files' },
  { id: 'search', label: 'Search', path: '/search' },
  { id: 'references', label: 'Saved References', path: '/references' },
  { id: 'compare', label: 'Compare', path: '/compare' },
];

/**
 * Shared top navigation bar unifying the Knowledge Library Hub surfaces
 * (/files, /search, /references, /compare, /ingest).
 */
export function LibraryHubNav({ activeView, onNavigate, className }: LibraryHubNavProps) {
  const importShortcut = formatShortcut('Mod+I');

  useEffect(() => {
    if (activeView === 'ingest') return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const target = event.target as HTMLElement | null;
      const isEditable =
        target instanceof HTMLElement &&
        (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable);
      if (isEditable) return;
      if ((event.metaKey || event.ctrlKey) && !event.shiftKey && !event.altKey && event.key.toLowerCase() === 'i') {
        event.preventDefault();
        onNavigate('/ingest');
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [activeView, onNavigate]);

  return (
    <div
      className={cn(
        'flex h-11 shrink-0 items-center justify-between gap-4 border-b border-border-subtle bg-bg px-6',
        className,
      )}
    >
      <nav aria-label="Library views" className="flex min-w-0 items-center gap-1 overflow-x-auto">
        {LENS_ITEMS.map((item) => {
          const isActive = activeView === item.id;
          return (
            <a
              key={item.id}
              href={item.path}
              onClick={(event) => {
                event.preventDefault();
                onNavigate(item.path);
              }}
              aria-current={isActive ? 'page' : undefined}
              className={cn(
                'relative inline-flex h-11 shrink-0 items-center whitespace-nowrap border-b-2 px-3 text-ui transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                isActive
                  ? 'border-accent font-medium text-text-primary'
                  : 'border-transparent text-text-tertiary hover:text-text-primary',
              )}
            >
              {item.label}
            </a>
          );
        })}
      </nav>

      {activeView === 'ingest' ? (
        <button
          type="button"
          onClick={() => onNavigate('/files')}
          className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-border-subtle bg-surface px-2.5 text-xs font-medium text-text-secondary transition-colors duration-fast hover:bg-surface-raised hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          <ArrowLeft className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
          Back to Documents
        </button>
      ) : (
        <button
          type="button"
          onClick={() => onNavigate('/ingest')}
          title={`Import (${importShortcut})`}
          className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md bg-accent px-2.5 text-xs font-medium text-accent-fg shadow-xs transition-colors duration-fast hover:bg-accent-hover focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          <Plus className="h-3.5 w-3.5" strokeWidth={2} aria-hidden="true" />
          <span>Import</span>
          <kbd
            aria-hidden="true"
            className="hidden rounded-xs bg-black/15 px-1 py-0.5 font-sans text-[10px] font-normal leading-none sm:inline"
          >
            {importShortcut}
          </kbd>
        </button>
      )}
    </div>
  );
}
