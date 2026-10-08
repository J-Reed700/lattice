import { type FC, useState, useEffect, useRef } from 'react';

import * as Tabs from '@radix-ui/react-tabs';
import { Files, History, Link, List, type LucideIcon } from 'lucide-react';

import { BatchFileImport } from '@/components/Ingest/BatchFileImport';
import { BatchUrlImport } from '@/components/Ingest/BatchUrlImport';
import { ImportHistory } from '@/components/Ingest/ImportHistory';
import { UrlImport } from '@/components/Ingest/UrlImport';
import { PageHeader } from '@/components/ui';
import { useToast } from '@/hooks/useToast';
import VaultAPI from '@/lib/api';
import { cn } from '@/lib/utils';
import { useFileBrowserStore } from '@/stores/fileBrowserStore';
import type { CorpusShapeDto } from '@/types';

import { composeIngestSummary, type IngestNoun } from './ingestSummary';


type TabId = 'single-url' | 'bulk-url' | 'files' | 'history';

interface ImportResult {
  type: TabId;
  success: boolean;
  count: number;
  message?: string;
}

interface IngestHubProps {
  defaultTab?: TabId;
  onImportComplete?: (result: ImportResult) => void;
}

const TABS: ReadonlyArray<{ id: TabId; label: string; icon: LucideIcon }> = [
  { id: 'single-url', label: 'One link', icon: Link },
  { id: 'bulk-url', label: 'Multiple links', icon: List },
  { id: 'files', label: 'Files', icon: Files },
  { id: 'history', label: 'History', icon: History },
];

/** The corpus shape, or null on any failure — the sentence is optional, the toast is not. */
const loadCorpusShapeSafe = async (): Promise<CorpusShapeDto | null> => {
  try {
    const result = await VaultAPI.getCorpusShape();
    return result.ok ? result.data : null;
  } catch {
    return null;
  }
};

/**
 * Navigate without a router hook.
 *
 * The hub is rendered bare in its own tests, so it must not require a router
 * context to mount. Pushing the URL and replaying it as a popstate is what the
 * browser does on Back, and the router handles it the same way — no import of
 * the route table, no cycle.
 */
const openLibrary = (): void => {
  const library = useFileBrowserStore.getState();
  library.setScope({ kind: 'all' });
  library.setSearchQuery('');
  library.setFilterByType(null);
  library.setFilterBySource('all');
  library.clearContentSearch();
  window.history.pushState({}, '', '/files');
  window.dispatchEvent(new PopStateEvent('popstate'));
};

const triggerClass = cn(
  'inline-flex items-center justify-center gap-2 rounded-md px-4 py-2 text-sm font-medium transition-colors duration-fast',
  'data-[state=active]:bg-surface data-[state=active]:text-accent data-[state=active]:shadow-sm',
  'data-[state=inactive]:text-text-tertiary hover:text-text-primary',
  'focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring'
);

/**
 * IngestHub
 *
 * The onramp. Reading-column anatomy A: one PageHeader, one row of text
 * tabs, then the surface for the chosen source.
 */
export const IngestHub: FC<IngestHubProps> = ({
  defaultTab = 'single-url',
  onImportComplete
}) => {
  const { toast } = useToast();
  // `useToast()` hands back a fresh object every render; the ref keeps the
  // async continuation below from closing over a stale one.
  const toastRef = useRef(toast);
  toastRef.current = toast;
  const [activeTab, setActiveTab] = useState<TabId>(() => {
    const requested = new URLSearchParams(window.location.search).get('tab');
    if (TABS.some(tab => tab.id === requested)) return requested as TabId;
    const saved = localStorage.getItem('ingestHub.lastTab');
    return (saved as TabId) || defaultTab;
  });

  useEffect(() => {
    localStorage.setItem('ingestHub.lastTab', activeTab);
  }, [activeTab]);

  const handleImportComplete = (type: TabId, success: boolean, count: number, message?: string) => {
    const result: ImportResult = { type, success, count, message };
    // Fires first, synchronously — callers must not wait on the shape lookup.
    onImportComplete?.(result);

    if (!success) {
      toastRef.current.error(message ? `Couldn't import ${message}` : "Couldn't import");
      return;
    }

    const noun: IngestNoun =
      type === 'single-url' || type === 'bulk-url' ? 'URL' : type === 'files' ? 'file' : 'item';

    void (async () => {
      const shape = await loadCorpusShapeSafe();
      const summary = composeIngestSummary(noun, count, shape);
      toastRef.current.success(summary.title, {
        ...(summary.message ? { message: summary.message } : {}),
        action: { label: 'Open Library', onClick: openLibrary },
      });
    })();
  };

  return (
    <main className="h-full overflow-y-auto bg-bg">
      <div className="mx-auto w-full max-w-[760px] px-6 pt-10 pb-16">
        <PageHeader title="Import" description="Bring articles, documents, and files into your library." />

        <Tabs.Root
          value={activeTab}
          onValueChange={(value) => setActiveTab(value as TabId)}
        >
          <Tabs.List aria-label="Import source" className="flex flex-wrap items-center gap-1 rounded-lg border border-border-subtle bg-surface-sunken p-1">
            {TABS.map(({ icon: Icon, ...tab }) => (
              <Tabs.Trigger key={tab.id} value={tab.id} className={triggerClass}>
                <Icon className="h-4 w-4" aria-hidden="true" />
                {tab.label}
              </Tabs.Trigger>
            ))}
          </Tabs.List>

          <Tabs.Content value="single-url" className="mt-6 outline-hidden">
            <UrlImport
              onImport={() => {}}
              onImportComplete={(success: boolean, url: string) =>
                handleImportComplete('single-url', success, 1, url)
              }
            />
          </Tabs.Content>

          <Tabs.Content value="bulk-url" className="mt-6 outline-hidden">
            <BatchUrlImport
              onImport={() => {}}
              onImportComplete={(results: { successful: number; failed: number }) =>
                handleImportComplete(
                  'bulk-url',
                  results.failed === 0,
                  results.successful,
                  results.failed > 0 ? `${results.failed} URLs` : undefined
                )
              }
            />
          </Tabs.Content>

          <Tabs.Content value="files" className="mt-6 outline-hidden">
            <BatchFileImport
              onReviewFailures={() => setActiveTab('history')}
              onImportComplete={(results: { successful: number; failed: number }) =>
                handleImportComplete(
                  'files',
                  results.failed === 0,
                  results.successful,
                  results.failed > 0 ? `${results.failed} files` : undefined
                )
              }
            />
          </Tabs.Content>

          <Tabs.Content value="history" className="mt-6 outline-hidden">
            <ImportHistory />
          </Tabs.Content>
        </Tabs.Root>
      </div>
    </main>
  );
};
