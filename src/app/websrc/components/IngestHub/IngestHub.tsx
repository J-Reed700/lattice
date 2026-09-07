import { type FC, useState, useEffect, useRef } from 'react';

import * as Tabs from '@radix-ui/react-tabs';

import { BatchFileImport } from '@/components/Ingest/BatchFileImport';
import { BatchUrlImport } from '@/components/Ingest/BatchUrlImport';
import { ImportHistory } from '@/components/Ingest/ImportHistory';
import { UrlImport } from '@/components/Ingest/UrlImport';
import { PageHeader } from '@/components/ui';
import { useToast } from '@/hooks/useToast';
import VaultAPI from '@/lib/api';
import { cn } from '@/lib/utils';
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

const TABS: ReadonlyArray<{ id: TabId; label: string }> = [
  { id: 'single-url', label: 'URL' },
  { id: 'bulk-url', label: 'URLs' },
  { id: 'files', label: 'Files' },
  { id: 'history', label: 'History' },
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
  window.history.pushState({}, '', '/files');
  window.dispatchEvent(new PopStateEvent('popstate'));
};

const triggerClass = cn(
  'border-b-2 pb-1 text-sm transition-colors duration-fast',
  'data-[state=active]:border-[hsl(var(--accent))] data-[state=active]:text-text-primary',
  'data-[state=inactive]:border-transparent data-[state=inactive]:text-text-tertiary',
  'hover:text-text-primary'
);

/**
 * IngestHub
 *
 * The onramp. Reading-column anatomy A: one PageHeader, one row of text
 * tabs, then the surface for the chosen source.
 * See `.design/UX-OVERHAUL-BRIEF.md` §2A.
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
        <PageHeader title="Import" />

        <Tabs.Root
          value={activeTab}
          onValueChange={(value) => setActiveTab(value as TabId)}
        >
          <Tabs.List className="flex items-center gap-4">
            {TABS.map((tab) => (
              <Tabs.Trigger key={tab.id} value={tab.id} className={triggerClass}>
                {tab.label}
              </Tabs.Trigger>
            ))}
          </Tabs.List>

          <Tabs.Content value="single-url" className="mt-6 outline-none">
            <UrlImport
              onImport={() => {}}
              onImportComplete={(success: boolean, url: string) =>
                handleImportComplete('single-url', success, 1, url)
              }
            />
          </Tabs.Content>

          <Tabs.Content value="bulk-url" className="mt-6 outline-none">
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

          <Tabs.Content value="files" className="mt-6 outline-none">
            <BatchFileImport
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

          <Tabs.Content value="history" className="mt-6 outline-none">
            <ImportHistory />
          </Tabs.Content>
        </Tabs.Root>
      </div>
    </main>
  );
};
