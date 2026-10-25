import { useRef, useState } from 'react';

import { Archive, CheckCircle2, FolderOpen, TriangleAlert } from 'lucide-react';

import { useApi } from '@/contexts/ApiContext';
import { MOVE_EXPORT_DEFAULTS, PackExportOptions, packOptionsReady, packOptionsRequest } from '@/features/learning/portability/PackExportOptions';
import { failureMessage, idempotentEntry, type RetryEntry } from '@/features/learning/portability/requestRetry';
import { RetryAlert } from '@/features/learning/portability/RetryAlert';
import { useExportAllLearningPacks } from '@/features/learning/portability/useLearningPortability';
import type { ExportAllLearningPacksRequestDto, ExportAllLearningPacksResultDto } from '@/lib/bindings';

type Retries = { exportAll?: RetryEntry<ExportAllLearningPacksRequestDto> };
type Exported = Extract<ExportAllLearningPacksResultDto, { outcome: 'exported' }>;

/** Every course, drafts included, one file each, into a folder Rust asks for. */
export function ExportAllCourses() {
  const api = useApi();
  const exportAll = useExportAllLearningPacks();
  const retry = useRef<Retries>({});
  const [options, setOptions] = useState(MOVE_EXPORT_DEFAULTS);
  const [result, setResult] = useState<Exported | null>(null);
  const [error, setError] = useState('');

  const send = async (entry: RetryEntry<ExportAllLearningPacksRequestDto>) => {
    setError('');
    try {
      const response = await exportAll.mutateAsync(entry.request);
      delete retry.current.exportAll;
      // Closing the folder dialog writes and records nothing.
      if (response.outcome === 'exported') setResult(response);
    } catch (cause) { setError(failureMessage(cause, 'The courses could not be exported.')); }
  };
  const run = () => send(idempotentEntry(retry, 'exportAll', { options: packOptionsRequest(options) }, (id) => ({ operationId: id() })));
  const reveal = async (path: string) => {
    const opened = await api.showInFolder(path);
    if (!opened.ok) setError(opened.error);
  };

  const exported = result?.exported ?? [];
  const failed = result?.failed ?? [];
  const total = exported.length + failed.length;
  const kept = retry.current.exportAll;

  return <section aria-labelledby="export-all-heading" data-testid="export-all-courses" className="min-w-0 rounded-2xl border border-border bg-surface p-4 sm:p-6">
    <div className="flex items-start gap-3"><span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent/10 text-accent"><Archive size={18} /></span><div><p className="text-[10px] font-semibold uppercase tracking-[.16em] text-accent">Backup</p><h2 id="export-all-heading" className="mt-1 font-serif text-2xl text-text-primary">Export all courses</h2><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">Saves every course, drafts included, as its own file in a folder you choose. Keep the folder as a backup, or import the files on another computer to carry on there.</p><p className="mt-2 max-w-2xl text-xs leading-5 text-text-muted">Lesson notes and unsaved lab drafts stay on this computer. They are not part of the files.</p></div></div>
    <PackExportOptions value={options} onChange={setOptions} />
    <button type="button" disabled={exportAll.isPending || !packOptionsReady(options)} onClick={() => void run()} className="mt-4 inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-50"><FolderOpen size={14} />{exportAll.isPending ? 'Exporting courses…' : 'Choose folder and export'}</button>

    {error && <RetryAlert className="mt-4" message={error} disabled={exportAll.isPending} retry={kept ? { label: 'Retry same export', run: () => void send(kept) } : undefined} />}

    {result && (total === 0
      ? <p role="status" className="mt-5 rounded-xl border border-dashed border-border p-4 text-sm text-text-muted">There are no courses to export yet.</p>
      : <div className="mt-5 space-y-3">
        <p role="status" className="text-sm text-text-secondary">Exported {exported.length} of {total} {total === 1 ? 'course' : 'courses'} to <span className="break-all font-medium text-text-primary">{result.destinationDir}</span>.</p>
        {exported.length > 0 && <ul aria-label="Exported courses" className="space-y-2">{exported.map((item) => <li key={item.programId} className="flex min-w-0 items-start justify-between gap-3 rounded-xl border border-border bg-background p-3"><div className="min-w-0"><p className="flex items-center gap-1.5 text-xs font-medium text-text-primary"><CheckCircle2 size={13} className="shrink-0 text-emerald-700" />{item.title}<span className="sr-only">: saved</span></p><p className="mt-1 break-all text-[10px] text-text-muted">{item.path}</p></div><button type="button" onClick={() => void reveal(item.path)} aria-label={`Reveal ${item.title} in folder`} className="shrink-0 rounded-full border border-border p-2 text-text-muted hover:text-text-primary"><FolderOpen size={13} /></button></li>)}</ul>}
        {failed.length > 0 && <ul aria-label="Courses that could not be exported" className="space-y-2">{failed.map((item) => <li key={item.programId} className="rounded-xl border border-amber-500/25 bg-amber-500/5 p-3"><p className="flex items-center gap-1.5 text-xs font-medium text-text-primary"><TriangleAlert size={13} className="shrink-0 text-amber-700" />{item.title}<span className="sr-only">: not exported</span></p><p className="mt-1 wrap-break-word text-[10px] leading-4 text-amber-800">{item.error}</p></li>)}</ul>}
      </div>)}
  </section>;
}
