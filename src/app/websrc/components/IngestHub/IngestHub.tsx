import { type FC, useState, useEffect } from 'react';

import * as Tabs from '@radix-ui/react-tabs';

import { BatchFileImport } from '@/components/Ingest/BatchFileImport';
import { BatchUrlImport } from '@/components/Ingest/BatchUrlImport';
import { ImportHistory } from '@/components/Ingest/ImportHistory';
import { UrlImport } from '@/components/Ingest/UrlImport';
import { useToast } from '@/hooks/useToast';
import { cn } from '@/lib/utils';

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

export const IngestHub: FC<IngestHubProps> = ({
  defaultTab = 'single-url',
  onImportComplete
}) => {
  const { toast } = useToast();
  const [activeTab, setActiveTab] = useState<TabId>(() => {
    const saved = localStorage.getItem('ingestHub.lastTab');
    return (saved as TabId) || defaultTab;
  });

  useEffect(() => {
    localStorage.setItem('ingestHub.lastTab', activeTab);
  }, [activeTab]);

  const handleImportComplete = (type: TabId, success: boolean, count: number, message?: string) => {
    const result: ImportResult = { type, success, count, message };

    if (success) {
      toast.success(`Imported ${count} ${type === 'single-url' ? 'URL' : 'items'}`);
    } else {
      toast.error(message || 'An error occurred during import');
    }

    onImportComplete?.(result);
  };

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden p-6">
      <div className="mb-6">
        <h1 className="text-3xl font-bold">Import Content</h1>
        <p className="text-muted-foreground mt-2">
          Import URLs, bulk content, or upload files to your knowledge base
        </p>
      </div>

      <Tabs.Root
        value={activeTab}
        onValueChange={(value) => setActiveTab(value as TabId)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <Tabs.List className="flex gap-2 border-b border-border mb-6">
          <Tabs.Trigger
            value="single-url"
            className={cn(
              "px-4 py-2 font-medium transition-colors",
              "border-b-2 border-transparent",
              "hover:text-foreground hover:border-muted-foreground/50",
              "data-[state=active]:border-primary data-[state=active]:text-foreground",
              "data-[state=inactive]:text-muted-foreground"
            )}
          >
            Single URL
          </Tabs.Trigger>
          <Tabs.Trigger
            value="bulk-url"
            className={cn(
              "px-4 py-2 font-medium transition-colors",
              "border-b-2 border-transparent",
              "hover:text-foreground hover:border-muted-foreground/50",
              "data-[state=active]:border-primary data-[state=active]:text-foreground",
              "data-[state=inactive]:text-muted-foreground"
            )}
          >
            Bulk URLs
          </Tabs.Trigger>
          <Tabs.Trigger
            value="files"
            className={cn(
              "px-4 py-2 font-medium transition-colors",
              "border-b-2 border-transparent",
              "hover:text-foreground hover:border-muted-foreground/50",
              "data-[state=active]:border-primary data-[state=active]:text-foreground",
              "data-[state=inactive]:text-muted-foreground"
            )}
          >
            Files
          </Tabs.Trigger>
          <Tabs.Trigger
            value="history"
            className={cn(
              "px-4 py-2 font-medium transition-colors",
              "border-b-2 border-transparent",
              "hover:text-foreground hover:border-muted-foreground/50",
              "data-[state=active]:border-primary data-[state=active]:text-foreground",
              "data-[state=inactive]:text-muted-foreground"
            )}
          >
            History
          </Tabs.Trigger>
        </Tabs.List>

        <Tabs.Content value="single-url" className="flex-1 min-h-0 overflow-y-auto outline-none">
          <UrlImport
            onImport={() => {}}
            onImportComplete={(success: boolean, url: string) =>
              handleImportComplete('single-url', success, 1, url)
            }
          />
        </Tabs.Content>

        <Tabs.Content value="bulk-url" className="flex-1 min-h-0 overflow-y-auto outline-none">
          <BatchUrlImport
            onImport={() => {}}
          />
        </Tabs.Content>

        <Tabs.Content value="files" className="flex-1 min-h-0 overflow-y-auto outline-none">
          <BatchFileImport
            onImportComplete={(results: { successful: number; failed: number }) =>
              handleImportComplete(
                'files',
                results.failed === 0,
                results.successful,
                results.failed > 0 ? `${results.failed} files failed` : undefined
              )
            }
          />
        </Tabs.Content>

        <Tabs.Content value="history" className="flex-1 min-h-0 overflow-y-auto outline-none">
          <ImportHistory />
        </Tabs.Content>
      </Tabs.Root>
    </div>
  );
};
