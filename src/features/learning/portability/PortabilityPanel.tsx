import { useQueryClient } from '@tanstack/react-query';
import { Archive, Download, FolderOpen, LockKeyhole, Save, ShieldCheck } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useApi } from '@/contexts/ApiContext';
import { CONFLICT_KINDS } from '@/features/learning/portability/conflictPolicies';
import { ImportCourseFlow } from '@/features/learning/portability/ImportCourseFlow';
import { PackExportOptions, packOptionsReady } from '@/features/learning/portability/PackExportOptions';
import {
  EMPTY_PROGRAM_PORTABILITY_RUN,
  type ExportAction,
  usePortabilityRunStore,
} from '@/features/learning/portability/portabilityRunStore';
import { RetryAlert } from '@/features/learning/portability/RetryAlert';
import { useLearningPortability } from '@/features/learning/portability/useLearningPortability';
import type { LearningPackConflictKind, LearningPackExportNoticeDto, LearningPackStaysBehindKind } from '@/lib/bindings';

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
  const client = useQueryClient();
  const workspace = useLearningPortability(programId);
  const runState = usePortabilityRunStore((store) => store.programRuns[programId] ?? EMPTY_PROGRAM_PORTABILITY_RUN);
  const setProgramOptions = usePortabilityRunStore((store) => store.setProgramOptions);
  const setProgramError = usePortabilityRunStore((store) => store.setProgramError);
  const runProgramExport = usePortabilityRunStore((store) => store.runProgramExport);

  const { options, pendingAction, error, success, lastDestination, retryExport, retrySaveAs } = runState;
  const busy = pendingAction !== null;

  const showInFolder = async () => {
    const path = lastDestination || workspace.data?.exports.at(0)?.destinationPath;
    if (!path) return;
    const result = await api.showInFolder(path);
    if (!result.ok) setProgramError(programId, { message: result.error });
  };

  if (workspace.isLoading) return <div role="status" className="rounded-xl border border-border-subtle bg-surface p-6 text-sm text-text-muted shadow-sm">Loading bundle history…</div>;
  if (workspace.error) return <section className="rounded-xl border border-border-subtle bg-surface p-6 shadow-sm"><h3 className="font-sans text-lg font-semibold tracking-[-0.02em] text-text-primary">Portability history is unavailable</h3><p role="alert" className="mt-2 text-sm text-danger-fg">{workspace.error.message}</p><Button type="button" variant="outline" onClick={() => void workspace.refetch()} className="mt-3">Retry</Button></section>;
  const notice = workspace.data?.exportNotice;
  const blockers = notice?.blockers ?? [];
  const blocked = blockers.length > 0;
  const staysBehind = staysBehindLabel(notice);
  const exportDisabled = busy || blocked || !packOptionsReady(options);
  const hasKeptRetry = error?.retry === 'export' ? Boolean(retryExport) : error?.retry === 'saveAs' ? Boolean(retrySaveAs) : false;
  const failedAction = error?.retry && hasKeptRetry ? error.retry : undefined;

  return <div className="space-y-5" data-testid="learning-portability-workspace">
    <header className="rounded-xl border border-border-subtle bg-surface p-5 shadow-sm sm:p-6">
      <div className="flex items-start gap-3"><span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent-muted text-accent"><Archive size={18} /></span><div><p className="text-[10px] font-semibold uppercase tracking-[.16em] text-accent">Portable learning</p><h3 className="mt-1 font-sans text-xl font-semibold tracking-[-0.025em] text-text-primary sm:text-2xl">Move this course to another computer</h3><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">Save the course and your progress to a file, copy it across, and import it on the other computer. It opens at the lesson you left off, with your flashcard schedule and quiz history. Every import is previewed first; nothing is written until you apply it.</p></div></div>
      <div className="mt-5 grid gap-3 sm:grid-cols-3"><div className="rounded-lg border border-border-subtle bg-surface-sunken/60 p-3"><ShieldCheck size={15} className="text-success-fg" /><p className="mt-2 text-xs font-medium text-text-primary">Private by default</p><p className="mt-1 text-xs leading-4 text-text-muted">Credentials and Lattice chat outside this program are never included.</p></div><div className="rounded-lg border border-border-subtle bg-surface-sunken/60 p-3"><LockKeyhole size={15} className="text-accent" /><p className="mt-2 text-xs font-medium text-text-primary">Exact source versions</p><p className="mt-1 text-xs leading-4 text-text-muted">The manifest records content checksums and source version IDs.</p></div><div className="rounded-lg border border-border-subtle bg-surface-sunken/60 p-3"><Archive size={15} className="text-text-muted" /><p className="mt-2 text-xs font-medium text-text-primary">Review before writing</p><p className="mt-1 text-xs leading-4 text-text-muted">Conflicts and actions are listed before you choose Apply.</p></div></div>
    </header>

    {error && <RetryAlert message={error.message} disabled={busy || blocked} retry={failedAction && { label: RETRY_LABELS[failedAction], run: () => void runProgramExport(programId, failedAction, api, client) }} />}
    {success && <div role="status" className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-success/25 bg-success-muted/40 px-4 py-3 text-sm text-success-fg"><span className="min-w-0 wrap-break-word">{success}</span>{lastDestination || workspace.data?.exports.length ? <button type="button" onClick={() => void showInFolder()} className="inline-flex shrink-0 items-center gap-1.5 text-xs font-semibold underline"><FolderOpen size={13} /> Reveal file</button> : null}</div>}

    <div className="grid min-w-0 gap-5 xl:grid-cols-2">
      <section className="min-w-0 rounded-xl border border-border-subtle bg-surface p-4 shadow-sm sm:p-6"><div className="flex items-center gap-2"><Download size={16} className="text-accent" /><div><p className="text-[10px] font-semibold uppercase tracking-[.14em] text-accent">Export</p><h4 className="font-sans text-lg font-semibold tracking-[-0.02em] text-text-primary">Save this course to a file</h4></div></div><p className="mt-2 text-xs leading-5 text-text-secondary">Save to… asks where to put the file, such as a USB drive or a synced folder. Export to app folder keeps a copy in Lattice&apos;s own exports folder; you can reveal it after it is created.</p>
        {blockers.map((kind) => <p key={kind} role="note" className="mt-3 rounded-lg border border-warning/25 bg-warning-muted/40 p-3 text-xs leading-5 text-warning-fg">{blockerSentence(kind)}</p>)}
        {staysBehind && <p className="mt-3 rounded-lg border border-border-subtle bg-bg p-3 text-xs leading-5 text-text-secondary">{staysBehind} <span className="text-text-muted">They are not part of the file.</span></p>}
        <PackExportOptions value={options} onChange={(next) => setProgramOptions(programId, next)} />
        <div className="mt-4 flex flex-wrap gap-2">
          <Button type="button" disabled={exportDisabled} onClick={() => void runProgramExport(programId, 'saveAs', api, client)}><Save size={14} />{pendingAction === 'saveAs' ? 'Saving…' : 'Save to…'}</Button>
          <Button type="button" variant="outline" disabled={exportDisabled} onClick={() => void runProgramExport(programId, 'export', api, client)}>{pendingAction === 'export' ? 'Creating course file…' : 'Export to app folder'}</Button>
        </div>
      </section>

      <ImportCourseFlow headingLevel={4} programId={programId} onImported={(importedId) => onImported?.(importedId)} />
    </div>

    <section className="rounded-xl border border-border-subtle bg-surface p-4 shadow-sm sm:p-6"><div className="flex items-center justify-between gap-3"><div><p className="text-[10px] font-semibold uppercase tracking-[.14em] text-text-muted">History</p><h4 className="font-sans text-lg font-semibold tracking-[-0.02em] text-text-primary">Recent bundles</h4></div><span className="rounded-full bg-bg px-3 py-1 text-xs text-text-muted">{workspace.data?.exports.length ?? 0} exports · {workspace.data?.imports.length ?? 0} imports</span></div>
      {(workspace.data?.exports.length ?? 0) === 0 && (workspace.data?.imports.length ?? 0) === 0 ? <p className="mt-3 rounded-xl border border-dashed border-border-default p-4 text-xs text-text-muted">Export and import records will appear here after they are created.</p> : <div className="mt-4 grid gap-3 lg:grid-cols-2">{workspace.data?.exports.slice().reverse().map((item) => <article key={item.id} className="min-w-0 rounded-xl border border-border-subtle bg-bg p-4"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><p className="text-xs font-medium text-text-primary">{item.manifest.title}</p><p className="mt-1 break-all text-xs text-text-muted">{item.destinationPath}</p><p className="mt-2 text-xs text-text-muted">Created {dateLabel(item.createdAt)} · v{item.manifest.version} · {item.manifest.entries.length} entries</p></div><Button type="button" variant="outline" size="icon" onClick={() => void api.showInFolder(item.destinationPath)} aria-label={`Reveal ${item.manifest.title} in folder`}><FolderOpen size={13} /></Button></div></article>)}{workspace.data?.imports.slice().reverse().map((item) => <article key={item.id} className="min-w-0 rounded-xl border border-border-subtle bg-bg p-4"><p className="text-xs font-medium text-text-primary">Imported program</p><p className="mt-1 break-all text-xs text-text-muted">{item.importedProgramId}</p><p className="mt-2 text-xs text-text-muted">{dateLabel(item.importedAt)} · {item.appliedChanges.length} applied changes{item.backupId ? ` · backup ${item.backupId}` : ''}</p></article>)}</div>}</section>
  </div>;
}
