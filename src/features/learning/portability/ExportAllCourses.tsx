import { useEffect } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { Archive, CheckCircle2, FolderOpen, TriangleAlert } from 'lucide-react';

import { useApi } from '@/contexts/ApiContext';
import { PackExportOptions, packOptionsReady } from '@/features/learning/portability/PackExportOptions';
import { usePortabilityRunStore } from '@/features/learning/portability/portabilityRunStore';
import { RetryAlert } from '@/features/learning/portability/RetryAlert';

/** Every course, drafts included, one file each, into a folder Rust asks for. */
export function ExportAllCourses() {
  const api = useApi();
  const client = useQueryClient();
  const state = usePortabilityRunStore((store) => store.exportAll);
  const setOptions = usePortabilityRunStore((store) => store.setExportAllOptions);
  const setError = usePortabilityRunStore((store) => store.setExportAllError);
  const markMounted = usePortabilityRunStore((store) => store.markExportAllMounted);
  const runExportAll = usePortabilityRunStore((store) => store.runExportAll);

  useEffect(() => {
    markMounted(true);
    return () => markMounted(false);
  }, [markMounted]);

  const reveal = async (path: string) => {
    const opened = await api.showInFolder(path);
    if (!opened.ok) setError(opened.error);
  };

  const { options, pending, result, error, retryEntry } = state;
  const exported = result?.exported ?? [];
  const failed = result?.failed ?? [];
  const total = exported.length + failed.length;

  return <section aria-labelledby="export-all-heading" data-testid="export-all-courses" className="min-w-0 rounded-xl border border-border-subtle bg-surface p-4 shadow-sm sm:p-6">
    <div className="flex items-start gap-3"><span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent-muted text-accent"><Archive size={18} /></span><div><p className="text-[10px] font-semibold uppercase tracking-[.16em] text-accent">Backup</p><h2 id="export-all-heading" className="mt-1 font-sans text-xl font-semibold tracking-[-0.025em] text-text-primary">Export all courses</h2><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">Saves every course, drafts included, as its own file in a folder you choose. Keep the folder as a backup, or import the files on another computer to carry on there.</p><p className="mt-2 max-w-2xl text-xs leading-5 text-text-muted">Lesson notes and unsaved lab drafts stay on this computer. They are not part of the files.</p></div></div>
    <PackExportOptions value={options} onChange={setOptions} />
    <button type="button" disabled={pending || !packOptionsReady(options)} onClick={() => void runExportAll(api, client)} className="mt-4 inline-flex h-9 items-center gap-2 rounded-lg bg-action px-4 text-xs font-medium text-action-fg shadow-action hover:bg-action-hover disabled:opacity-50"><FolderOpen size={14} />{pending ? 'Exporting courses…' : 'Choose folder and export'}</button>

    {error && <RetryAlert className="mt-4" message={error} disabled={pending} retry={retryEntry ? { label: 'Retry same export', run: () => void runExportAll(api, client, retryEntry) } : undefined} />}

    {result && (total === 0
      ? <p role="status" className="mt-5 rounded-xl border border-dashed border-border-default p-4 text-sm text-text-muted">There are no courses to export yet.</p>
      : <div className="mt-5 space-y-3">
        <p role="status" className="text-sm text-text-secondary">Exported {exported.length} of {total} {total === 1 ? 'course' : 'courses'} to <span className="break-all font-medium text-text-primary">{result.destinationDir}</span>.</p>
        {exported.length > 0 && <ul aria-label="Exported courses" className="space-y-2">{exported.map((item) => <li key={item.programId} className="flex min-w-0 items-start justify-between gap-3 rounded-xl border border-border-subtle bg-bg p-3"><div className="min-w-0"><p className="flex items-center gap-1.5 text-xs font-medium text-text-primary"><CheckCircle2 size={13} className="shrink-0 text-success-fg" />{item.title}<span className="sr-only">: saved</span></p><p className="mt-1 break-all text-[10px] text-text-muted">{item.path}</p></div><button type="button" onClick={() => void reveal(item.path)} aria-label={`Reveal ${item.title} in folder`} className="shrink-0 rounded-lg border border-border-default bg-surface p-2 text-text-muted shadow-2xs hover:text-text-primary"><FolderOpen size={13} /></button></li>)}</ul>}
        {failed.length > 0 && <ul aria-label="Courses that could not be exported" className="space-y-2">{failed.map((item) => <li key={item.programId} className="rounded-xl border border-warning/25 bg-warning-muted/40 p-3"><p className="flex items-center gap-1.5 text-xs font-medium text-text-primary"><TriangleAlert size={13} className="shrink-0 text-warning-fg" />{item.title}<span className="sr-only">: not exported</span></p><p className="mt-1 wrap-break-word text-[10px] leading-4 text-warning-fg">{item.error}</p></li>)}</ul>}
      </div>)}
  </section>;
}
