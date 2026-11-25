import { type FC, useState, useEffect } from 'react';

import { Files, History, Link, List, type LucideIcon } from 'lucide-react';

import { PageHeader, Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui';
import { useApi } from '@/contexts/ApiContext';
import { fetchCorpusShape } from '@/features/corpus-shape/hooks/useCorpusShapeQuery';
import { BatchFileImport } from '@/features/files/components/Ingest/BatchFileImport';
import { BatchUrlImport } from '@/features/files/components/Ingest/BatchUrlImport';
import { ImportHistory } from '@/features/files/components/Ingest/ImportHistory';
import { UrlImport } from '@/features/files/components/Ingest/UrlImport';
import { openLibraryRoot } from '@/features/files/hooks/batchUrlImportRunner';
import { useToast } from '@/hooks/useToast';
import type { ApiClient } from '@/lib/api';
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
const loadCorpusShapeSafe = async (api: ApiClient): Promise<CorpusShapeDto | null> => {
  try {
    return await fetchCorpusShape(api);
  } catch {
    return null;
  }
};

/**
 * IngestHub
 *
 * Centered Import Hub: PageHeader, unified pill Tabs, and a calm surface card
 * for each import workflow.
 */
export const IngestHub: FC<IngestHubProps> = ({
  defaultTab = 'single-url',
  onImportComplete,
}) => {
  const api = useApi();
  const { toast } = useToast();
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
      toast.error(message ? `Couldn't import ${message}` : "Couldn't import");
      return;
    }

    const noun: IngestNoun =
      type === 'single-url' || type === 'bulk-url' ? 'URL' : type === 'files' ? 'file' : 'item';

    void (async () => {
      const shape = await loadCorpusShapeSafe(api);
      const summary = composeIngestSummary(noun, count, shape);
      toast.success(summary.title, {
        ...(summary.message ? { message: summary.message } : {}),
        action: { label: 'Open Library', onClick: openLibraryRoot },
      });
    })();
  };

  return (
    <main className="h-full overflow-y-auto bg-bg">
      <div className="mx-auto w-full max-w-[760px] px-6 pt-10 pb-16">
        <PageHeader
          title="Import"
          description="Bring web articles, PDFs, notes, and local files into your searchable library."
        />

        <Tabs
          variant="pill"
          value={activeTab}
          onValueChange={(value) => setActiveTab(value as TabId)}
        >
          <TabsList aria-label="Import source" className="h-9 p-1">
            {TABS.map(({ icon: Icon, ...tab }) => (
              <TabsTrigger key={tab.id} value={tab.id} className="gap-1.5 px-3.5">
                <Icon className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
                {tab.label}
              </TabsTrigger>
            ))}
          </TabsList>

          <div className="mt-5 rounded-xl border border-border-subtle bg-surface p-6 shadow-sm">
            <TabsContent value="single-url" className="mt-0 outline-hidden">
              <UrlImport
                onImport={() => {}}
                onImportComplete={(success: boolean, url: string) =>
                  handleImportComplete('single-url', success, 1, url)
                }
              />
            </TabsContent>

            <TabsContent value="bulk-url" className="mt-0 outline-hidden">
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
            </TabsContent>

            <TabsContent value="files" className="mt-0 outline-hidden">
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
            </TabsContent>

            <TabsContent value="history" className="mt-0 outline-hidden">
              <ImportHistory />
            </TabsContent>
          </div>
        </Tabs>
      </div>
    </main>
  );
};
