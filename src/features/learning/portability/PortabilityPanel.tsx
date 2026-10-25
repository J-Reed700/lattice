import { useRef, useState } from 'react';

import { Archive, Download, FolderOpen, LockKeyhole, Save, ShieldCheck } from 'lucide-react';

import { useApi } from '@/contexts/ApiContext';
import { CONFLICT_KINDS } from '@/features/learning/portability/conflictPolicies';
import { ImportCourseFlow } from '@/features/learning/portability/ImportCourseFlow';
import { MOVE_EXPORT_DEFAULTS, PackExportOptions, packOptionsReady, packOptionsRequest } from '@/features/learning/portability/PackExportOptions';
import { failureMessage, idempotentEntry, type RetryEntry } from '@/features/learning/portability/requestRetry';
import { RetryAlert } from '@/features/learning/portability/RetryAlert';
import {
  useExportLearningPack,
  useLearningPortability,
  useSaveLearningPackAs,
} from '@/features/learning/portability/useLearningPortability';
import type { ExportLearningPackRequestDto, LearningPackConflictKind, LearningPackExportNoticeDto, LearningPackStaysBehindKind } from '@/lib/bindings';


type ExportAction = 'export' | 'saveAs';
const RETRY_LABELS: Record<ExportAction, string> = { export: 'Retry same export', saveAs: 'Retry same save' };
function dateLabel(value: number) { const date = new Date(value); return Number.isNaN(date.getTime()) ? 'Date unavailable' : date.toLocaleString(); }
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

// The export notice's sentences: what stays behind, and why an export must wait.
const STAYS_BEHIND: Record<LearningPackStaysBehindKind, [one: string, many: string]> = {
  notebook_notes: ['lesson note', 'lesson notes'],
  practical_drafts: ['lab draft', 'lab drafts'],
  unfinished_lesson_reviews: ['unfinished lesson review', 'unfinished lesson reviews'],
  tangent_chats: ['tangent chat', 'tangent chats'],
  verifier_consent: ['Verifier audit consent', 'Verifier audit consents'],
};
const blockerSentence = (kind: LearningPackConflictKind) => CONFLICT_KINDS[kind].blocksExport ?? CONFLICT_KINDS[kind].reason(null);

/** "2 lesson notes, 1 lab draft and 1 tangent chat stay on this computer.", or null when nothing stays behind. */
function staysBehindLabel(notice: LearningPackExportNoticeDto | undefined) {
  if (!notice?.staysBehind.length) return null;
  const total = notice.staysBehind.reduce((sum, { count }) => sum + count, 0);
  const items = notice.staysBehind.map(({ kind, count }) => plural(count, ...STAYS_BEHIND[kind]));
  const listed = items.length > 1 ? `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}` : items[0];
  return `${listed} ${total === 1 ? 'stays' : 'stay'} on this computer.`;
}

export function PortabilityPanel({ programId, onImported }: { programId: string; onImported?: (programId: string) => void }) {
  const api = useApi();
  const workspace = useLearningPortability(programId);
  const exportPack = useExportLearningPack(programId);
  const saveAs = useSaveLearningPackAs(programId);
  const retry = useRef<Partial<Record<ExportAction, RetryEntry<ExportLearningPackRequestDto>>>>({});
  const [options, setOptions] = useState(MOVE_EXPORT_DEFAULTS);
  const [error, setError] = useState<{ message: string; retry?: ExportAction } | null>(null);
  const [success, setSuccess] = useState('');
  const [lastDestination, setLastDestination] = useState('');

  // Rust names the file from the course title; the request carries no name.
  const entryFor = (action: ExportAction) => idempotentEntry(
    retry, action,
    { programId, options: packOptionsRequest(options) },
    (id) => ({ operationId: id() }),
  );
  const saved = (path: string | undefined) => {
    if (path) setLastDestination(path);
    setSuccess(path ? `Course file saved to ${path}` : 'Course file saved.');
  };

  const exportRequest = async () => {
    const entry = entryFor('export');
    setError(null); setSuccess('');
    try {
      const updated = await exportPack.mutateAsync(entry.request);
      delete retry.current.export;
      saved(updated.exports.at(0)?.destinationPath);
    } catch (cause) { setError({ message: failureMessage(cause, 'The course file could not be created.'), retry: 'export' }); }
  };
  const saveAsRequest = async () => {
    const entry = entryFor('saveAs');
    setError(null); setSuccess('');
    try {
      const result = await saveAs.mutateAsync(entry.request);
      delete retry.current.saveAs;
      // Closing the save dialog writes and records nothing.
      if (result.outcome === 'saved') saved(result.export.destinationPath);
    } catch (cause) { setError({ message: failureMessage(cause, 'The course file could not be saved.'), retry: 'saveAs' }); }
  };

  const busy = exportPack.isPending || saveAs.isPending;
  const showInFolder = async () => {
    const path = lastDestination || workspace.data?.exports.at(0)?.destinationPath;
    if (!path) return;
    const result = await api.showInFolder(path);
    if (!result.ok) setError({ message: result.error });
  };

  if (workspace.isLoading) return <div role="status" className="rounded-2xl border border-border bg-surface p-6 text-sm text-text-muted">Loading bundle history…</div>;
  if (workspace.error) return <section className="rounded-2xl border border-border bg-surface p-6"><h3 className="font-serif text-xl text-text-primary">Portability history is unavailable</h3><p role="alert" className="mt-2 text-sm text-rose-700">{workspace.error.message}</p><button type="button" onClick={() => void workspace.refetch()} className="mt-3 rounded-full border border-border px-4 py-2 text-sm">Retry</button></section>;
  const notice = workspace.data?.exportNotice;
  const blockers = notice?.blockers ?? [];
  const blocked = blockers.length > 0;
  const staysBehind = staysBehindLabel(notice);
  const exportDisabled = busy || blocked || !packOptionsReady(options);
  const failedAction = error?.retry && retry.current[error.retry] ? error.retry : undefined;
  const runAction: Record<ExportAction, () => Promise<void>> = { export: exportRequest, saveAs: saveAsRequest };

  return <div className="space-y-5" data-testid="learning-portability-workspace">
    <header className="rounded-2xl border border-[#d7c9b8] bg-[#eee8df] p-5 dark:border-white/10 dark:bg-[#28251f] sm:p-7">
      <div className="flex items-start gap-3"><span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent/10 text-accent"><Archive size={18} /></span><div><p className="text-[10px] font-semibold uppercase tracking-[.16em] text-accent">Portable learning</p><h3 className="mt-1 font-serif text-2xl text-text-primary sm:text-3xl">Move this course to another computer</h3><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">Save the course and your progress to a file, copy it across, and import it on the other computer. It opens at the lesson you left off, with your flashcard schedule and quiz history. Every import is previewed first; nothing is written until you apply it.</p></div></div>
      <div className="mt-5 grid gap-3 sm:grid-cols-3"><div className="rounded-xl border border-white/70 bg-white/55 p-3 dark:border-white/10 dark:bg-black/10"><ShieldCheck size={15} className="text-emerald-700" /><p className="mt-2 text-xs font-medium text-text-primary">Private by default</p><p className="mt-1 text-[10px] leading-4 text-text-muted">Credentials and Lattice chat outside this program are never included.</p></div><div className="rounded-xl border border-white/70 bg-white/55 p-3 dark:border-white/10 dark:bg-black/10"><LockKeyhole size={15} className="text-accent" /><p className="mt-2 text-xs font-medium text-text-primary">Exact source versions</p><p className="mt-1 text-[10px] leading-4 text-text-muted">The manifest records content checksums and source version IDs.</p></div><div className="rounded-xl border border-white/70 bg-white/55 p-3 dark:border-white/10 dark:bg-black/10"><Archive size={15} className="text-text-muted" /><p className="mt-2 text-xs font-medium text-text-primary">Review before writing</p><p className="mt-1 text-[10px] leading-4 text-text-muted">Conflicts and actions are listed before you choose Apply.</p></div></div>
    </header>

    {error && <RetryAlert message={error.message} disabled={busy || blocked} retry={failedAction && { label: RETRY_LABELS[failedAction], run: () => void runAction[failedAction]() }} />}
    {success && <div role="status" className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-emerald-600/20 bg-emerald-500/5 px-4 py-3 text-sm text-emerald-800"><span className="min-w-0 wrap-break-word">{success}</span>{lastDestination || workspace.data?.exports.length ? <button type="button" onClick={() => void showInFolder()} className="inline-flex shrink-0 items-center gap-1.5 text-xs font-semibold underline"><FolderOpen size={13} /> Reveal file</button> : null}</div>}

    <div className="grid min-w-0 gap-5 xl:grid-cols-2">
      <section className="min-w-0 rounded-2xl border border-border bg-surface p-4 sm:p-6"><div className="flex items-center gap-2"><Download size={16} className="text-accent" /><div><p className="text-[10px] font-semibold uppercase tracking-[.14em] text-accent">Export</p><h4 className="font-serif text-xl text-text-primary">Save this course to a file</h4></div></div><p className="mt-2 text-xs leading-5 text-text-secondary">Save to… asks where to put the file, such as a USB drive or a synced folder. Export to app folder keeps a copy in Lattice&apos;s own exports folder; you can reveal it after it is created.</p>
        {blockers.map((kind) => <p key={kind} role="note" className="mt-3 rounded-lg border border-amber-500/25 bg-amber-500/10 p-3 text-xs leading-5 text-amber-800">{blockerSentence(kind)}</p>)}
        {staysBehind && <p className="mt-3 rounded-lg border border-border bg-background p-3 text-xs leading-5 text-text-secondary">{staysBehind} <span className="text-text-muted">They are not part of the file.</span></p>}
        <PackExportOptions value={options} onChange={setOptions} />
        <div className="mt-4 flex flex-wrap gap-2">
          <button type="button" disabled={exportDisabled} onClick={() => void saveAsRequest()} className="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-50"><Save size={14} />{saveAs.isPending ? 'Saving…' : 'Save to…'}</button>
          <button type="button" disabled={exportDisabled} onClick={() => void exportRequest()} className="inline-flex items-center gap-2 rounded-full border border-accent/35 px-4 py-2.5 text-xs font-semibold text-accent disabled:opacity-50">{exportPack.isPending ? 'Creating course file…' : 'Export to app folder'}</button>
        </div>
      </section>

      <ImportCourseFlow headingLevel={4} onImported={(importedId) => onImported?.(importedId)} />
    </div>

    <section className="rounded-2xl border border-border bg-surface p-4 sm:p-6"><div className="flex items-center justify-between gap-3"><div><p className="text-[10px] font-semibold uppercase tracking-[.14em] text-text-muted">History</p><h4 className="font-serif text-xl text-text-primary">Recent bundles</h4></div><span className="rounded-full bg-background px-3 py-1 text-[10px] text-text-muted">{workspace.data?.exports.length ?? 0} exports · {workspace.data?.imports.length ?? 0} imports</span></div>
      {(workspace.data?.exports.length ?? 0) === 0 && (workspace.data?.imports.length ?? 0) === 0 ? <p className="mt-3 rounded-xl border border-dashed border-border p-4 text-xs text-text-muted">Export and import records will appear here after they are created.</p> : <div className="mt-4 grid gap-3 lg:grid-cols-2">{workspace.data?.exports.slice().reverse().map((item) => <article key={item.id} className="min-w-0 rounded-xl border border-border bg-background p-4"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><p className="text-xs font-medium text-text-primary">{item.manifest.title}</p><p className="mt-1 break-all text-[10px] text-text-muted">{item.destinationPath}</p><p className="mt-2 text-[10px] text-text-muted">Created {dateLabel(item.createdAt)} · v{item.manifest.version} · {item.manifest.entries.length} entries</p></div><button type="button" onClick={() => void api.showInFolder(item.destinationPath)} className="shrink-0 rounded-full border border-border p-2 text-text-muted" aria-label={`Reveal ${item.manifest.title} in folder`}><FolderOpen size={13} /></button></div></article>)}{workspace.data?.imports.slice().reverse().map((item) => <article key={item.id} className="min-w-0 rounded-xl border border-border bg-background p-4"><p className="text-xs font-medium text-text-primary">Imported program</p><p className="mt-1 break-all text-[10px] text-text-muted">{item.importedProgramId}</p><p className="mt-2 text-[10px] text-text-muted">{dateLabel(item.importedAt)} · {item.appliedChanges.length} applied changes{item.backupId ? ` · backup ${item.backupId}` : ''}</p></article>)}</div>}</section>
  </div>;
}
