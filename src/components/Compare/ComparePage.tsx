import { useCallback, useMemo, useState } from 'react';

import { Columns3 } from 'lucide-react';
import { useNavigate, useSearchParams } from 'react-router';

import { FilePreviewModal } from '@/components/Chat/FilePreviewModal';
import { EmptyState } from '@/components/EmptyState';
import { PageHeader } from '@/components/ui';
import { useCompareQuery } from '@/hooks/queries/useCompareQuery';
import VaultAPI from '@/lib/api';
import { toast } from '@/stores/toastStore';
import type { CompareCellDto, CompareRowDto } from '@/types/api/compare';
import type { PassageLocator, SourceWithMetadata } from '@/types/conversation';
import { mimeTypeForPath } from '@/utils/mimeTypes';

import { ColumnEditor, parseColumns } from './ColumnEditor';
import { compareTableToMarkdown } from './compareMarkdown';
import { CompareTable } from './CompareTable';

function parseIds(value: string | null): string[] {
  if (!value) return [];
  return value
    .split(',')
    .map((id) => id.trim())
    .filter(Boolean);
}

/**
 * Compare — a table across a handful of documents, with a citation under every
 * answer that has one and "not stated" wherever a document does not answer.
 */
export function ComparePage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const documentIds = useMemo(() => parseIds(searchParams.get('ids')), [searchParams]);
  const [columnDraft, setColumnDraft] = useState(() => searchParams.get('columns') ?? '');
  const [columns, setColumns] = useState<string[]>(() =>
    parseColumns(searchParams.get('columns') ?? '').slice(0, 6),
  );
  const [hasRun, setHasRun] = useState(() => columns.length > 0);
  const [previewSource, setPreviewSource] = useState<SourceWithMetadata | null>(null);
  const [previewLocator, setPreviewLocator] = useState<PassageLocator | null>(null);

  const query = useCompareQuery(documentIds, columns, hasRun);
  const table = query.data ?? null;

  const refetchCompare = query.refetch;

  const runCompare = useCallback(
    (next: string[]) => {
      // Same columns, same query key, `staleTime: Infinity` — React Query would
      // serve the cached table and the button would look broken. Ask again.
      const unchanged =
        hasRun &&
        next.length === columns.length &&
        next.every((column, index) => column === columns[index]);

      setColumns(next);
      setHasRun(true);
      const params = new URLSearchParams(searchParams);
      params.set('columns', next.join(', '));
      setSearchParams(params, { replace: true });

      if (unchanged) {
        void refetchCompare();
      }
    },
    [columns, hasRun, refetchCompare, searchParams, setSearchParams],
  );

  const openCitation = useCallback((row: CompareRowDto, cell: CompareCellDto) => {
    if (!cell.citation) return;
    // The cited sentence is what the reader should land on, not the top of the
    // file. `chunkId` identifies the passage; `text` is what locates it.
    setPreviewLocator({
      text: cell.citation.excerpt,
      chunkId: cell.citation.chunkId,
    });
    setPreviewSource({
      documentId: row.documentId,
      chunkId: cell.citation.chunkId,
      fileName: row.title,
      filePath: row.filePath,
      mimeType: mimeTypeForPath(row.filePath),
      category: '',
      content: cell.citation.excerpt,
      excerpt: cell.citation.excerpt,
      highlights: [],
      score: 1,
      fileSizeBytes: 0,
      // No mtime is known here; the comparison's own timestamp is not one.
      modifiedAt: '',
    });
  }, []);

  const saveToJournal = useCallback(async () => {
    if (!table) return;
    const result = await VaultAPI.quickCapture(compareTableToMarkdown(table));
    if (!result.ok) {
      toast.error('Could not write to the journal', { message: result.error });
      return;
    }
    const { noteId, noteTitle } = result.data;
    toast.success(`Comparison saved to "${noteTitle}".`, {
      action: {
        label: 'Open',
        onClick: () =>
          navigate(`/journals?${new URLSearchParams({ noteId }).toString()}`),
      },
    });
  }, [navigate, table]);

  const metaLine =
    columns.length > 0
      ? `${documentIds.length} ${documentIds.length === 1 ? 'document' : 'documents'} · ${
          columns.length
        } ${columns.length === 1 ? 'column' : 'columns'}`
      : `${documentIds.length} ${documentIds.length === 1 ? 'document' : 'documents'}`;

  const body = (() => {
    if (documentIds.length < 2) {
      return (
        <EmptyState
          className="py-24"
          icon={<Columns3 />}
          title="Nothing to compare yet"
          description="Select two or more documents in the Library to compare them."
          action={{ label: 'Open Library', onClick: () => navigate('/files'), shortcut: '⌘2' }}
        />
      );
    }
    if (!hasRun || columns.length === 0) return null;
    if (query.isPending || query.isFetching) {
      return (
        <p className="text-sm text-text-tertiary">
          Reading {documentIds.length} {documentIds.length === 1 ? 'document' : 'documents'}…
        </p>
      );
    }
    if (query.error) {
      return (
        <div>
          <p className="text-sm text-[hsl(var(--danger-fg))]">
            Couldn’t build the table. {query.error.message}
          </p>
          <button
            type="button"
            onClick={() => void query.refetch()}
            className="mt-3 text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
          >
            Try again
          </button>
        </div>
      );
    }
    if (!table || table.rows.length === 0) {
      return <p className="text-sm text-text-tertiary">Nothing to compare.</p>;
    }
    return (
      <>
        <CompareTable table={table} onOpenCitation={openCitation} />
        <p className="mt-6 text-xs text-text-muted">
          Filled by {table.modelName}. Cells say “not stated” when the document doesn’t
          answer.
        </p>
      </>
    );
  })();

  return (
    <main className="h-full overflow-y-auto bg-bg">
      <div className="mx-auto w-full max-w-[1100px] px-6 pt-10 pb-16">
        <PageHeader
          title="Compare"
          meta={metaLine}
          actions={
            table ? (
              <button
                type="button"
                onClick={() => void saveToJournal()}
                className="text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
              >
                Save to journal
              </button>
            ) : undefined
          }
        />

        {documentIds.length >= 2 ? (
          <ColumnEditor
            value={columnDraft}
            onChange={setColumnDraft}
            onSubmit={runCompare}
            isRunning={query.isFetching}
          />
        ) : null}

        {body}
      </div>

      <FilePreviewModal
        isOpen={previewSource !== null}
        onClose={() => {
          setPreviewSource(null);
          setPreviewLocator(null);
        }}
        source={previewSource}
        initialLocator={previewLocator}
      />
    </main>
  );
}
