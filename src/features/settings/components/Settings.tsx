/**
 * Settings — minimalist 6-domain shell with instant `/` search, editorial
 * underline sub-tabs, full legacy `?tab=` deep-link support, and Radix Dialog
 * confirmations.
 */

import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  Archive,
  ArrowRight,
  RotateCcw,
  Search,
  Upload,
  X,
} from 'lucide-react';
import { useSearchParams } from 'react-router';

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  SidebarHeader,
  Tabs,
  TabsList,
  TabsTrigger,
} from '@/components/ui';
import { describeFailure } from '@/lib/errorHandling';
import { cn } from '@/lib/utils';
import { toast } from '@/stores/toastStore';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import { LlmSettingsProvider } from './AITab';
import {
  DEFAULT_SETTINGS_TAB,
  getDomainForTab,
  getTabMeta,
  resolveSettingsTab,
  searchSettings,
  SETTINGS_DOMAINS,
  tabGroups,
  type SettingsDomainId,
  type SettingsTab,
} from './settingsRegistry';
import { useJsonFileDialogs } from '../hooks/useJsonFileDialogs';
import { useSettingsTransfer } from '../hooks/useSettingsQuery';
import './settings.css';

export { tabGroups };

const footerButtonClass =
  'flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-xs font-medium text-text-secondary transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-45';

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
}

function HighlightedText({ text, query }: { text: string; query: string }) {
  const trimmed = query.trim();
  if (!trimmed) return <>{text}</>;
  const tokens = trimmed
    .split(/\s+/)
    .filter(Boolean)
    .map((token) => token.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  if (tokens.length === 0) return <>{text}</>;
  const regex = new RegExp(`(${tokens.join('|')})`, 'gi');
  const parts = text.split(regex);
  return (
    <>
      {parts.map((part, index) =>
        index % 2 === 1 ? (
          <mark
            key={index}
            className="rounded-xs bg-accent/18 px-0.5 text-text-primary"
          >
            {part}
          </mark>
        ) : (
          <Fragment key={index}>{part}</Fragment>
        ),
      )}
    </>
  );
}

export function Settings() {
  const [searchParams, setSearchParams] = useSearchParams();
  const requestedTab = searchParams.get('tab');
  const [activeTab, setActiveTab] = useState<SettingsTab>(
    () => resolveSettingsTab(requestedTab) ?? DEFAULT_SETTINGS_TAB,
  );
  // Remember the last sub-tab visited in each domain so switching domains feels continuous.
  const [lastTabByDomain, setLastTabByDomain] = useState<Partial<Record<SettingsDomainId, SettingsTab>>>(() => {
    const initial = resolveSettingsTab(requestedTab) ?? DEFAULT_SETTINGS_TAB;
    return { [getDomainForTab(initial).id]: initial };
  });

  const [searchQuery, setSearchQuery] = useState('');
  const [pendingHighlightId, setPendingHighlightId] = useState<string | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const mainScrollRef = useRef<HTMLElement | null>(null);

  const selectTab = useCallback((nextTab: SettingsTab, highlightElementId?: string) => {
    const domain = getDomainForTab(nextTab);
    setActiveTab(nextTab);
    setLastTabByDomain((previous) => ({ ...previous, [domain.id]: nextTab }));
    setSearchQuery('');
    setPendingHighlightId(highlightElementId ?? null);
    if (!highlightElementId && mainScrollRef.current) {
      mainScrollRef.current.scrollTop = 0;
    }
  }, []);

  // Consume `?tab=` deep links while keeping `/settings` clean so repeated
  // deep links to the same tab always re-activate it.
  useEffect(() => {
    if (!requestedTab) return;
    const resolved = resolveSettingsTab(requestedTab);
    if (resolved) {
      selectTab(resolved);
    }
    const next = new URLSearchParams(searchParams);
    next.delete('tab');
    setSearchParams(next, { replace: true });
  }, [requestedTab, searchParams, setSearchParams, selectTab]);

  // Other panes ask to jump here (e.g. "Browse catalog" from Downloaded models).
  useEffect(() => {
    const handleNavigate = (event: Event) => {
      const detail = (event as CustomEvent<{ tab?: string }>).detail;
      const resolved = resolveSettingsTab(detail?.tab);
      if (resolved) {
        selectTab(resolved);
      }
    };
    window.addEventListener('settings:navigate-tab', handleNavigate);
    return () => window.removeEventListener('settings:navigate-tab', handleNavigate);
  }, [selectTab]);

  // Global `/` shortcut focuses Settings search when not already typing in a field.
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== '/' || event.metaKey || event.ctrlKey || event.altKey) return;
      if (isEditableTarget(event.target)) return;
      event.preventDefault();
      searchInputRef.current?.focus();
      searchInputRef.current?.select();
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // Scroll to and softly highlight the jumped-to setting once the target tab mounts.
  useEffect(() => {
    if (!pendingHighlightId) return;
    let highlightTimer: number | undefined;
    const frame = window.requestAnimationFrame(() => {
      const target = document.getElementById(pendingHighlightId);
      const row = target?.closest('.settings-row') ?? target;
      if (row instanceof HTMLElement) {
        if (typeof row.scrollIntoView === 'function') {
          row.scrollIntoView({ behavior: 'smooth', block: 'center' });
        }
        row.classList.remove('settings-search-highlight');
        void row.offsetWidth;
        row.classList.add('settings-search-highlight');
        highlightTimer = window.setTimeout(() => {
          row.classList.remove('settings-search-highlight');
        }, 1900);
      }
      setPendingHighlightId(null);
    });
    return () => {
      window.cancelAnimationFrame(frame);
      if (highlightTimer !== undefined) window.clearTimeout(highlightTimer);
    };
  }, [pendingHighlightId, activeTab]);

  const [showResetDialog, setShowResetDialog] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [isResetting, setIsResetting] = useState(false);

  const { resetSettings, exportSettings, importSettings } = useSettingsTransfer();
  const { saveJsonFile, openJsonFile } = useJsonFileDialogs();

  const handleReset = async () => {
    setIsResetting(true);
    const result = await resetSettings();
    setIsResetting(false);
    setShowResetDialog(false);

    if (result.ok) {
      toast.success('Settings reset to defaults', { duration: 3000 });
    } else {
      toast.error("Couldn't reset settings", { message: result.error, duration: 5000 });
    }
  };

  const handleExport = async () => {
    setIsExporting(true);
    try {
      const result = await exportSettings();
      if (!result.ok) {
        throw new Error(result.error);
      }

      const saved = await saveJsonFile('lattice-settings.json', result.data);
      if (!saved.ok) {
        toast.error("Couldn't export settings", {
          message: describeFailure(saved),
          duration: 5000,
        });
      } else if (saved.data) {
        toast.success('Settings exported successfully', { duration: 3000 });
      }
    } catch (error) {
      console.error("Couldn't export settings:", error);
      toast.error("Couldn't export settings", { message: String(error), duration: 5000 });
    } finally {
      setIsExporting(false);
    }
  };

  const handleImport = async () => {
    setIsImporting(true);
    try {
      const opened = await openJsonFile();
      if (!opened.ok) {
        toast.error("Couldn't import settings", {
          message: describeFailure(opened),
          duration: 5000,
        });
      } else if (opened.data !== null) {
        const result = await importSettings(opened.data);

        if (result.ok) {
          toast.success('Settings imported successfully', { duration: 3000 });
        } else {
          toast.error('Invalid settings file', { message: result.error, duration: 5000 });
        }
      }
    } catch (error) {
      console.error("Couldn't import settings:", error);
      toast.error("Couldn't import settings", { message: String(error), duration: 5000 });
    } finally {
      setIsImporting(false);
    }
  };

  const activeMeta = getTabMeta(activeTab);
  const activeDomain = getDomainForTab(activeTab);
  const ActiveTabComponent = activeMeta.component;
  const isWideContentTab = Boolean(activeMeta.wide);

  const trimmedQuery = searchQuery.trim();
  const isSearching = trimmedQuery.length > 0;
  const searchMatches = useMemo(() => searchSettings(trimmedQuery), [trimmedQuery]);

  const matchCountByDomain = useMemo(() => {
    const counts = new Map<SettingsDomainId, number>();
    for (const match of searchMatches) {
      counts.set(match.domain.id, (counts.get(match.domain.id) ?? 0) + 1);
    }
    return counts;
  }, [searchMatches]);

  const visibleDomains = useMemo(() => {
    if (!isSearching) return SETTINGS_DOMAINS;
    return SETTINGS_DOMAINS.filter((domain) => (matchCountByDomain.get(domain.id) ?? 0) > 0);
  }, [isSearching, matchCountByDomain]);

  const handleDomainClick = (domainId: SettingsDomainId) => {
    if (isSearching) {
      const firstInDomain = searchMatches.find((match) => match.domain.id === domainId);
      if (firstInDomain) {
        selectTab(firstInDomain.tab.id, firstInDomain.item.elementId);
        return;
      }
    }
    const domain = SETTINGS_DOMAINS.find((candidate) => candidate.id === domainId);
    if (!domain) return;
    const remembered = lastTabByDomain[domainId];
    const targetTab =
      remembered && domain.tabs.some((tab) => tab.id === remembered)
        ? remembered
        : domain.tabs[0].id;
    selectTab(targetTab);
  };

  return (
    <div className="settings-workspace flex h-full min-w-0 bg-bg">
      {/* Sidebar */}
      <aside className="settings-navigation flex w-[236px] shrink-0 flex-col border-r border-border-subtle bg-surface-sunken/70">
        <SidebarHeader title="Settings" />

        {/* Instant Settings search */}
        <div className="px-2.5 pb-2 pt-2.5">
          <div className="relative">
            <Search
              className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-text-muted"
              strokeWidth={1.75}
              aria-hidden="true"
            />
            <input
              ref={searchInputRef}
              type="search"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Escape') {
                  if (searchQuery) {
                    event.preventDefault();
                    setSearchQuery('');
                  } else {
                    searchInputRef.current?.blur();
                  }
                } else if (event.key === 'Enter' && searchMatches.length > 0) {
                  event.preventDefault();
                  const top = searchMatches[0];
                  selectTab(top.tab.id, top.item.elementId);
                }
              }}
              placeholder="Search settings…"
              aria-label="Search settings"
              className="h-8 w-full rounded-lg border border-border-default bg-surface pl-8 pr-8 text-ui text-text-primary shadow-control placeholder:text-text-muted outline-hidden transition-[border-color,box-shadow] duration-fast hover:border-border-strong focus:border-accent/60 focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-search-cancel-button]:hidden"
            />
            {isSearching ? (
              <button
                type="button"
                onClick={() => {
                  setSearchQuery('');
                  searchInputRef.current?.focus();
                }}
                aria-label="Clear settings search"
                className="absolute right-1.5 top-1/2 inline-flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-sm text-text-muted transition-colors duration-fast hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                <X className="h-3.5 w-3.5" aria-hidden="true" />
              </button>
            ) : (
              <kbd
                aria-hidden="true"
                className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 rounded border border-border-default bg-surface-sunken px-1.5 py-0.5 font-sans text-[10px] font-medium leading-none text-text-tertiary"
              >
                /
              </kbd>
            )}
          </div>
        </div>

        {/* 6 minimalist domain items */}
        <nav aria-label="Settings sections" className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
          {visibleDomains.length === 0 ? (
            <p className="px-2.5 py-3 text-xs text-text-muted">
              No sections match &ldquo;{trimmedQuery}&rdquo;.
            </p>
          ) : (
            <div className="space-y-0.5">
              {visibleDomains.map((domain) => {
                const isActive = !isSearching && activeDomain.id === domain.id;
                const Icon = domain.icon;
                const matchCount = matchCountByDomain.get(domain.id);
                return (
                  <button
                    key={domain.id}
                    type="button"
                    onClick={() => handleDomainClick(domain.id)}
                    aria-current={isActive ? 'page' : undefined}
                    className={cn(
                      'group relative flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-ui transition-[background-color,box-shadow,color] duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                      isActive
                        ? 'bg-surface font-semibold text-text-primary shadow-sm ring-1 ring-border-subtle'
                        : 'text-text-secondary hover:bg-[hsl(var(--text-primary)/0.045)] hover:text-text-primary',
                    )}
                  >
                    <Icon
                      className={cn(
                        'h-4 w-4 shrink-0 transition-colors duration-fast',
                        isActive ? 'text-accent' : 'text-text-muted group-hover:text-text-secondary',
                      )}
                      strokeWidth={1.75}
                      aria-hidden="true"
                    />
                    <span className="min-w-0 flex-1 truncate">{domain.label}</span>
                    {isSearching && matchCount ? (
                      <span className="shrink-0 rounded-full bg-accent-muted px-1.5 py-0.5 text-[11px] font-medium tabular-nums text-accent">
                        {matchCount}
                      </span>
                    ) : null}
                  </button>
                );
              })}
            </div>
          )}
        </nav>

        {/* Footer — Transfer & Reset actions */}
        <div className="shrink-0 space-y-0.5 border-t border-border-subtle p-2">
          <button
            type="button"
            onClick={handleAsyncEvent(handleExport)}
            disabled={isExporting}
            className={footerButtonClass}
          >
            <Archive className="h-3.5 w-3.5 text-text-muted" aria-hidden="true" />
            {isExporting ? 'Exporting…' : 'Export'}
          </button>
          <button
            type="button"
            onClick={handleAsyncEvent(handleImport)}
            disabled={isImporting}
            className={footerButtonClass}
          >
            <Upload className="h-3.5 w-3.5 text-text-muted" aria-hidden="true" />
            {isImporting ? 'Importing…' : 'Import'}
          </button>
          <button
            type="button"
            onClick={() => setShowResetDialog(true)}
            className={cn(
              footerButtonClass,
              'text-danger-fg hover:bg-danger-muted hover:text-danger-fg',
            )}
          >
            <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
            Reset all
          </button>
        </div>
      </aside>

      {/* Main content — editorial reading column */}
      <main
        ref={mainScrollRef}
        className="h-full min-h-0 min-w-0 flex-1 overflow-y-auto bg-bg"
        aria-label="Settings content"
      >
        <div
          className={cn(
            'settings-content mx-auto w-full px-8 pb-20 pt-5',
            !isSearching && isWideContentTab ? 'max-w-[1060px]' : 'max-w-[760px]',
          )}
        >
          {isSearching ? (
            <section aria-label="Settings search results">
              <div className="mb-6 flex items-baseline justify-between gap-4 border-b border-border-subtle pb-4">
                <div>
                  <h1 className="font-sans text-2xl font-semibold tracking-[-0.025em] text-text-primary">
                    Search results
                  </h1>
                  <p className="mt-1 text-sm text-text-tertiary">
                    {searchMatches.length === 0
                      ? `No settings match “${trimmedQuery}”.`
                      : `${searchMatches.length} ${searchMatches.length === 1 ? 'setting' : 'settings'} matching “${trimmedQuery}”`}
                  </p>
                </div>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    setSearchQuery('');
                    searchInputRef.current?.focus();
                  }}
                >
                  Clear search
                </Button>
              </div>

              {searchMatches.length === 0 ? (
                <div className="py-12 text-center">
                  <p className="text-sm text-text-secondary">
                    Try searching for a setting name like &ldquo;theme&rdquo;, &ldquo;reranker&rdquo;,
                    &ldquo;temperature&rdquo;, &ldquo;backup&rdquo;, or &ldquo;ollama&rdquo;.
                  </p>
                </div>
              ) : (
                <div className="divide-y divide-border-subtle border-b border-border-subtle">
                  {searchMatches.map((match) => (
                    <button
                      key={match.item.id}
                      type="button"
                      onClick={() => selectTab(match.tab.id, match.item.elementId)}
                      className="group flex w-full items-center justify-between gap-4 py-3.5 text-left transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.03)] focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-baseline gap-2">
                          <span className="text-sm font-medium text-text-primary">
                            <HighlightedText text={match.item.label} query={trimmedQuery} />
                          </span>
                          <span className="text-xs text-text-muted">
                            <HighlightedText text={match.domain.label} query={trimmedQuery} />
                            {' · '}
                            <HighlightedText text={match.tab.label} query={trimmedQuery} />
                          </span>
                        </div>
                        <p className="mt-0.5 text-xs leading-relaxed text-text-tertiary">
                          <HighlightedText text={match.item.description} query={trimmedQuery} />
                        </p>
                      </div>
                      <ArrowRight
                        className="h-4 w-4 shrink-0 text-text-muted transition-transform duration-fast group-hover:translate-x-0.5 group-hover:text-text-primary"
                        aria-hidden="true"
                      />
                    </button>
                  ))}
                </div>
              )}
            </section>
          ) : (
            <>
              {/* In-pane underline sub-tabs whenever a domain groups 2+ sub-sections */}
              {activeDomain.tabs.length > 1 ? (
                <div className="mb-4">
                  <Tabs
                    variant="underline"
                    value={activeTab}
                    onValueChange={(value) => selectTab(value as SettingsTab)}
                  >
                    <div className="overflow-x-auto">
                      <TabsList
                        aria-label={`${activeDomain.label} sections`}
                        className="flex w-max min-w-full justify-start"
                      >
                        {activeDomain.tabs.map((tab) => (
                          <TabsTrigger key={tab.id} value={tab.id}>
                            {tab.label}
                          </TabsTrigger>
                        ))}
                      </TabsList>
                    </div>
                  </Tabs>
                </div>
              ) : null}

              <LlmSettingsProvider>
                <ActiveTabComponent />
              </LlmSettingsProvider>
            </>
          )}
        </div>
      </main>

      {/* Reset confirmation — Radix Dialog */}
      <Dialog open={showResetDialog} onOpenChange={setShowResetDialog}>
        <DialogContent
          hideClose
          className="max-w-sm"
          onInteractOutside={(event) => event.preventDefault()}
        >
          <DialogHeader>
            <DialogTitle className="text-base font-medium text-text-primary">
              Reset all settings?
            </DialogTitle>
            <DialogDescription className="text-sm text-text-secondary">
              Every setting goes back to its default. Export first if you want a copy.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="mt-5">
            <Button
              type="button"
              variant="outline"
              onClick={() => setShowResetDialog(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              onClick={() => {
                void handleReset();
              }}
              disabled={isResetting}
            >
              {isResetting ? 'Resetting…' : 'Reset'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

Settings.displayName = 'Settings';
