import { useState } from 'react';

import { MoreHorizontal, Plus, RefreshCw } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { useIndexedFoldersQuery } from '@/features/files/hooks/useIndexedFoldersQuery';
import { formatFolderDisplayName } from '@/features/files/model/documentMetadata';
import { type ClusterDto, type ClusterProgressPayload } from '@/types';
import {
  type CustomCollection,
  type LibraryScope,
  type SavedSearchPreset,
} from '@/types/fileBrowser';

interface LibraryRailProps {
  scope: LibraryScope;
  onScopeChange: (_scope: LibraryScope) => void;
  collections: CustomCollection[];
  onCreateCollection: (_name: string) => void;
  onRenameCollection?: (_collection: CustomCollection) => void;
  onDeleteCollection?: (_collection: CustomCollection) => void;
  collectionCounts?: Map<string, number>;
  savedSearches: SavedSearchPreset[];
  activeSavedSearchId: string | null;
  onApplySavedSearch: (_searchId: string) => void;
  onRenameSavedSearch: (_searchId: string, _name: string) => void;
  onCreateSavedSearch: (_name: string) => void;
  /** The automatic themes from the most recent clustering run. */
  themes: ClusterDto[];
  /** False below `THEMES_MIN_DOCUMENTS` — a flat list beats themes on a small vault. */
  themesEnabled: boolean;
  hasRunThemes: boolean;
  isFindingThemes: boolean;
  themeProgress: ClusterProgressPayload | null;
  onFindThemes: () => void;
}

/** What a run is doing right now, in the rail's one muted line. */
export function rebuildLabel(progress: ClusterProgressPayload | null): string {
  if (!progress) return 'Finding themes…';
  switch (progress.phase) {
    case 'loading':
      return 'Reading embeddings…';
    case 'clustering':
      return 'Grouping documents…';
    case 'labeling':
      return `Naming ${progress.current} of ${progress.total}…`;
    case 'saving':
      return 'Saving…';
    default:
      return 'Finding themes…';
  }
}

/**
 * A theme the model could not name gets its contents instead of a generic
 * title — never ship a label that says nothing.
 */
export function isFallbackLabel(theme: ClusterDto): boolean {
  return theme.labelSource === 'fallback' || /^Theme of \d+/.test(theme.label);
}

/**
 * The Library rail: plain lists of the things a corpus is organised by.
 * Sentence-case headings, `+` to create inline, no explanatory copy.
 */
export function LibraryRail({
  scope,
  onScopeChange,
  collections,
  onCreateCollection,
  onRenameCollection,
  onDeleteCollection,
  collectionCounts,
  savedSearches,
  activeSavedSearchId,
  onApplySavedSearch,
  onRenameSavedSearch,
  onCreateSavedSearch,
  themes,
  themesEnabled,
  hasRunThemes,
  isFindingThemes,
  themeProgress,
  onFindThemes,
}: LibraryRailProps) {
  const foldersQuery = useIndexedFoldersQuery();
  const folders = foldersQuery.data ?? [];

  return (
    <aside className="flex h-full w-[236px] shrink-0 flex-col gap-6 overflow-y-auto border-r border-border-subtle bg-surface-sunken/70 px-3 py-5">
      <RailSection heading="Folders">
        <RailRow
          label="All documents"
          isActive={scope.kind === 'all'}
          onClick={() => onScopeChange({ kind: 'all' })}
        />
        {folders.map((folder) => (
          <RailRow
            key={folder.path}
            label={formatFolderDisplayName(folder.path)}
            title={folder.path}
            trailing={folder.documentCount.toLocaleString()}
            isActive={scope.kind === 'folder' && scope.path === folder.path}
            onClick={() => onScopeChange({ kind: 'folder', path: folder.path })}
          />
        ))}
      </RailSection>

      <RailSection
        heading="Collections"
        onCreate={onCreateCollection}
        createLabel="New collection"
        placeholder="Collection name"
        empty="Create a collection to organize your documents."
        isEmpty={collections.length === 0}
      >
        {collections.map((collection) => (
          <CollectionRow
            key={collection.id}
            collection={collection}
            count={collectionCounts?.get(collection.id) ?? collection.documentIds.length}
            isActive={scope.kind === 'collection' && scope.id === collection.id}
            onClick={() => onScopeChange({ kind: 'collection', id: collection.id })}
            onRename={onRenameCollection ? () => onRenameCollection(collection) : undefined}
            onDelete={onDeleteCollection ? () => onDeleteCollection(collection) : undefined}
          />
        ))}
      </RailSection>

      {themesEnabled ? (
        <RailSection
          heading="Themes"
          action={
            <IconButton
              label={hasRunThemes ? 'Refresh themes' : 'Find themes'}
              tooltipSide="right"
              onClick={onFindThemes}
              disabled={isFindingThemes}
            >
              <RefreshCw strokeWidth={1.75} />
            </IconButton>
          }
          empty="No themes yet. Find themes to group your vault by topic."
          isEmpty={themes.length === 0 && !isFindingThemes}
        >
          {isFindingThemes ? (
            <p className="px-2.5 py-1 text-xs tabular-nums text-text-muted">
              {rebuildLabel(themeProgress)}
            </p>
          ) : (
            themes.map((theme) => (
              <ThemeRow
                key={theme.id}
                theme={theme}
                isActive={scope.kind === 'theme' && scope.id === theme.id}
                onClick={() => onScopeChange({ kind: 'theme', id: theme.id })}
              />
            ))
          )}
        </RailSection>
      ) : null}

      <RailSection
        heading="Saved searches"
        onCreate={onCreateSavedSearch}
        createLabel="Save current search"
        placeholder="Search name"
        empty="No saved searches."
        isEmpty={savedSearches.length === 0}
      >
        {savedSearches.map((search) => (
          <RailRow
            key={search.id}
            label={search.name}
            isActive={activeSavedSearchId === search.id}
            onClick={() => onApplySavedSearch(search.id)}
            onDoubleClick={() => onRenameSavedSearch(search.id, search.name)}
          />
        ))}
      </RailSection>
    </aside>
  );
}

function CollectionRow({ collection, count, isActive, onClick, onRename, onDelete }: {
  collection: CustomCollection;
  count: number;
  isActive: boolean;
  onClick: () => void;
  onRename?: () => void;
  onDelete?: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div
      className={`group relative flex items-center rounded-lg transition-[background-color,box-shadow] duration-fast ${
        isActive
          ? 'bg-surface shadow-sm ring-1 ring-border-subtle'
          : 'hover:bg-surface/65'
      }`}
    >
      <button
        type="button"
        onClick={onClick}
        aria-current={isActive ? 'page' : undefined}
        className={`flex min-w-0 flex-1 items-center gap-2 px-2.5 py-1.5 text-left text-ui transition-colors duration-fast ${
          isActive
            ? 'font-semibold text-text-primary'
            : 'text-text-secondary hover:text-text-primary'
        }`}
      >
        <span className="min-w-0 flex-1 truncate" title={collection.name}>{collection.name}</span>
        {collection.kind === 'snapshot' ? <span className="text-xs font-normal text-text-muted">snapshot</span> : null}
        <span className="shrink-0 text-xs font-normal tabular-nums text-text-muted">{count.toLocaleString()}</span>
      </button>
      {onRename || onDelete ? (
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger asChild>
            <button type="button" aria-label={`Actions for ${collection.name}`} className="mr-1 rounded-md p-1.5 text-text-muted hover:bg-surface-sunken hover:text-text-primary">
              <MoreHorizontal className="h-4 w-4" />
            </button>
          </PopoverTrigger>
          <PopoverContent align="start" side="right" className="w-44 p-1">
            {onRename ? <button type="button" className="w-full rounded-md px-3 py-2 text-left text-sm hover:bg-[hsl(var(--text-primary)/0.06)]" onClick={() => { setOpen(false); onRename(); }}>Rename collection</button> : null}
            {onDelete ? <button type="button" className="w-full rounded-md px-3 py-2 text-left text-sm text-danger hover:bg-danger-muted" onClick={() => { setOpen(false); onDelete(); }}>Delete collection</button> : null}
          </PopoverContent>
        </Popover>
      ) : null}
    </div>
  );
}

interface RailSectionProps {
  heading: string;
  children?: React.ReactNode;
  onCreate?: (_name: string) => void;
  createLabel?: string;
  placeholder?: string;
  empty?: string;
  isEmpty?: boolean;
  /** Replaces the inline-create button in the heading row. */
  action?: React.ReactNode;
}

function RailSection({
  heading,
  children,
  onCreate,
  createLabel,
  placeholder,
  empty,
  isEmpty = false,
  action,
}: RailSectionProps) {
  const [draft, setDraft] = useState<string | null>(null);

  const submit = () => {
    const name = (draft ?? '').trim();
    if (name && onCreate) {
      onCreate(name);
    }
    setDraft(null);
  };

  return (
    <section>
      <div className="flex h-7 items-center justify-between pl-2.5">
        <h3 className="text-xs font-medium text-text-muted">{heading}</h3>
        {action ?? null}
        {!action && onCreate ? (
          <IconButton
            label={createLabel ?? `Add ${heading}`}
            tooltipSide="right"
            onClick={() => setDraft((current) => (current === null ? '' : null))}
          >
            <Plus strokeWidth={1.75} />
          </IconButton>
        ) : null}
      </div>

      {draft !== null ? (
        <input
          autoFocus
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={submit}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              submit();
            } else if (event.key === 'Escape') {
              event.preventDefault();
              setDraft(null);
            }
          }}
          placeholder={placeholder}
          aria-label={createLabel ?? heading}
          className="mt-1 h-8 w-full rounded-lg border border-border-default bg-surface px-2.5 text-sm text-text-primary shadow-control outline-hidden transition-colors duration-fast placeholder:text-text-muted focus:border-accent focus-visible:ring-2 focus-visible:ring-ring"
        />
      ) : null}

      <div className="mt-1 space-y-0.5">
        {isEmpty && !children ? null : children}
        {isEmpty && empty ? <p className="px-2.5 py-1 text-xs leading-relaxed text-text-muted">{empty}</p> : null}
      </div>
    </section>
  );
}

interface ThemeRowProps {
  theme: ClusterDto;
  isActive: boolean;
  onClick: () => void;
}

function ThemeRow({ theme, isActive, onClick }: ThemeRowProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`relative flex w-full flex-col items-start gap-0.5 rounded-lg px-2.5 py-1.5 text-left transition-[background-color,color,box-shadow] duration-fast ${
        isActive
          ? 'bg-surface font-semibold text-text-primary shadow-sm ring-1 ring-border-subtle'
          : 'text-text-secondary hover:bg-surface/65 hover:text-text-primary'
      }`}
    >
      <span className="flex w-full items-baseline justify-between gap-2">
        <span className="truncate text-ui">{theme.label}</span>
        <span className="shrink-0 text-xs font-normal tabular-nums text-text-muted">
          {theme.memberCount.toLocaleString()}
        </span>
      </span>
      {isFallbackLabel(theme) && theme.sampleTitles.length > 0 ? (
        <span className="w-full truncate text-xs font-normal text-text-muted">
          {theme.sampleTitles.slice(0, 3).join(' · ')}
        </span>
      ) : theme.description ? (
        <span className="w-full truncate text-xs font-normal text-text-muted">{theme.description}</span>
      ) : null}
    </button>
  );
}

interface RailRowProps {
  label: string;
  title?: string;
  trailing?: string;
  isActive?: boolean;
  onClick?: () => void;
  onDoubleClick?: () => void;
}

function RailRow({ label, title, trailing, isActive = false, onClick, onDoubleClick }: RailRowProps) {
  const content = (
    <>
      <span className="min-w-0 flex-1 truncate" title={title ?? label}>{label}</span>
      {trailing ? (
        <span className="shrink-0 text-xs font-normal tabular-nums text-text-muted">{trailing}</span>
      ) : null}
    </>
  );

  const className = `relative flex h-8 w-full items-center gap-2 rounded-lg px-2.5 text-left text-ui transition-[background-color,color,box-shadow] duration-fast ${
    isActive
      ? 'bg-surface font-semibold text-text-primary shadow-sm ring-1 ring-border-subtle'
      : 'text-text-secondary'
  } ${onClick && !isActive ? 'hover:bg-surface/65 hover:text-text-primary' : ''}`;

  if (!onClick) {
    return <div className={className}>{content}</div>;
  }

  return (
    <button type="button" onClick={onClick} onDoubleClick={onDoubleClick} className={className}>
      {content}
    </button>
  );
}
