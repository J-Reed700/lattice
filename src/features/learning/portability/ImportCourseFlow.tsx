import { useId, useRef, useState } from 'react';

import { open } from '@tauri-apps/plugin-dialog';
import { Check, RefreshCw, Upload } from 'lucide-react';

import { awaitsPreparation, blockedConflicts, blockedReason, CONFLICT_KINDS, conflictPolicyOption, DEFAULT_CONFLICT_POLICY, existingCourseConflict, isBlocked, proceedingPolicies } from '@/features/learning/portability/conflictPolicies';
import { failureMessage, idempotentEntry, type RetryEntry } from '@/features/learning/portability/requestRetry';
import { RetryAlert } from '@/features/learning/portability/RetryAlert';
import { useImportCourse } from '@/features/learning/portability/useLearningPortability';
import type {
  ApplyLearningPackImportRequestDto,
  CancelLearningPackImportPreviewRequestDto,
  LearningPackConflictPolicy,
  LearningPackImportPreviewDto,
  PreviewLearningPackImportRequestDto,
} from '@/lib/bindings';

type Retries = {
  preview?: RetryEntry<PreviewLearningPackImportRequestDto>;
  apply?: RetryEntry<ApplyLearningPackImportRequestDto>;
  cancel?: RetryEntry<CancelLearningPackImportPreviewRequestDto>;
};
type Failure = { message: string; retry?: 'preview' | 'apply' };

// The flow's own heading, its preview's title, and the preview's parts. On its
// own screen it is the main section (h2); in a course's Import & export panel
// it sits under the panel's h3 beside the export section's h4.
const HEADINGS = { 2: ['h2', 'h3', 'h4'], 4: ['h4', 'h5', 'h6'] } as const;

/**
 * Bring a course file onto this computer, with or without a course open:
 * pick the file, preview it as a move (`DEFAULT_CONFLICT_POLICY`), and offer
 * the choices the backend says clear what blocks it. Nothing is written before Apply.
 */
export function ImportCourseFlow({ headingLevel, onImported }: { headingLevel: keyof typeof HEADINGS; onImported(programId: string, title: string): void }) {
  const [Heading, PreviewHeading, PartHeading] = HEADINGS[headingLevel];
  const ids = useId();
  const { preview: previewPack, apply: applyPack, cancel: cancelPack } = useImportCourse();
  const retry = useRef<Retries>({});
  const [preview, setPreview] = useState<LearningPackImportPreviewDto | null>(null);
  const [applyConfirm, setApplyConfirm] = useState(false);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [notice, setNotice] = useState('');
  const busy = previewPack.isPending || applyPack.isPending || cancelPack.isPending;

  const reset = () => { setFailure(null); setNotice(''); setApplyConfirm(false); };

  const runPreview = async (entry: RetryEntry<PreviewLearningPackImportRequestDto>) => {
    reset();
    try {
      const updated = await previewPack.mutateAsync(entry.request);
      delete retry.current.preview;
      setPreview(updated.importPreviews.find((item) => item.id === entry.request.previewId) ?? updated.importPreviews[0] ?? null);
    } catch (cause) { setFailure({ message: failureMessage(cause, 'The selected file could not be previewed.'), retry: 'preview' }); }
  };

  const previewFile = (sourcePath: string, conflictPolicy: LearningPackConflictPolicy) =>
    runPreview(idempotentEntry(retry, 'preview', { sourcePath, conflictPolicy }, (id) => ({ operationId: id(), previewId: id() })));

  const chooseFile = async () => {
    reset();
    try {
      const chosen = await open({ multiple: false, directory: false, title: 'Choose a course file', filters: [{ name: 'Lattice course', extensions: ['lattice-learning'] }] });
      if (!chosen || Array.isArray(chosen)) return;
      await previewFile(chosen, DEFAULT_CONFLICT_POLICY);
    } catch (cause) { setFailure({ message: failureMessage(cause, 'The selected file could not be previewed.') }); }
  };

  /** Cancels a pending preview; returns its cancelled form, or undefined when the cancel failed. */
  const cancelPending = async (target: LearningPackImportPreviewDto) => {
    const entry = idempotentEntry(retry, 'cancel', { previewId: target.id }, (id) => ({ operationId: id() }));
    try {
      const updated = await cancelPack.mutateAsync(entry.request);
      delete retry.current.cancel;
      return updated.importPreviews.find((item) => item.id === target.id) ?? { ...target, status: 'cancelled' as const };
    } catch (cause) {
      setFailure({ message: failureMessage(cause, 'The preview could not be cancelled.') });
      return undefined;
    }
  };

  // Re-preview the same file under another choice, or under the same one to
  // check again; the blocked preview is cancelled first.
  const choosePolicy = async (policy: LearningPackConflictPolicy) => {
    if (!preview || busy) return;
    reset();
    if (preview.status === 'pending') {
      const cancelled = await cancelPending(preview);
      if (!cancelled) return;
      setPreview(cancelled);
    }
    await previewFile(preview.sourcePath, policy);
  };

  const abandonPreview = async () => {
    if (!preview || busy) return;
    reset();
    if (await cancelPending(preview)) {
      setPreview(null);
      setNotice('Preview cancelled. Nothing was imported.');
    }
  };

  const runApply = async () => {
    if (!preview?.canApply || busy) return;
    const entry = idempotentEntry(retry, 'apply', { previewId: preview.id, expectedRootSha256: preview.manifest.rootSha256 }, (id) => ({ operationId: id() }));
    // The confirmation stays open, showing "Applying…", until the import settles.
    setFailure(null); setNotice('');
    try {
      const updated = await applyPack.mutateAsync(entry.request);
      delete retry.current.apply;
      const title = preview.incomingProgramTitle;
      setPreview(updated.importPreviews.find((item) => item.id === preview.id) ?? { ...preview, status: 'applied' });
      setNotice(`“${title}” is ready on this computer.`);
      onImported(updated.programId, title);
    } catch (cause) {
      setFailure({ message: failureMessage(cause, 'The import could not be applied.'), retry: 'apply' });
    } finally {
      setApplyConfirm(false);
    }
  };

  const blocked = preview ? blockedConflicts(preview) : [];
  const existing = preview ? existingCourseConflict(preview) : undefined;
  const choices = preview ? proceedingPolicies(preview) : [];
  const policy = preview ? conflictPolicyOption(preview.conflictPolicy) : null;
  const failedPreview = failure?.retry === 'preview' ? retry.current.preview : undefined;
  const failedRetry = failedPreview ? { label: 'Retry same preview', run: () => void runPreview(failedPreview) }
    : failure?.retry === 'apply' && retry.current.apply ? { label: 'Retry apply', run: () => void runApply() } : undefined;

  return <section aria-labelledby={`${ids}-heading`} data-testid="import-course-flow" className="min-w-0 rounded-2xl border border-border bg-surface p-4 sm:p-6">
    <div className="flex items-center gap-2"><Upload size={16} className="text-accent" /><div><p className="text-[10px] font-semibold uppercase tracking-[.14em] text-accent">Import</p><Heading id={`${ids}-heading`} className="font-serif text-xl text-text-primary">Bring a course to this computer</Heading></div></div>
    <p className="mt-2 text-xs leading-5 text-text-secondary">Choose a course file exported from Lattice. You see what it contains and every change it would make before anything is written, and the course opens where you left off.</p>
    <button type="button" disabled={busy} onClick={() => void chooseFile()} className="mt-4 inline-flex items-center gap-2 rounded-full border border-accent/35 px-4 py-2.5 text-xs font-semibold text-accent disabled:opacity-50">{previewPack.isPending ? 'Checking the file…' : 'Choose a course file…'}</button>

    {failure && <RetryAlert className="mt-4" message={failure.message} disabled={busy} retry={failedRetry} />}
    {notice && <p role="status" className="mt-4 rounded-xl border border-emerald-600/20 bg-emerald-500/5 px-4 py-3 text-sm text-emerald-800">{notice}</p>}

    {preview && policy && <article className="mt-5 min-w-0 rounded-xl border border-border bg-background p-3 sm:p-4">
      <div className="flex flex-wrap items-start justify-between gap-2"><div className="min-w-0"><p className="text-[10px] font-semibold uppercase tracking-[.13em] text-accent">Dry run · {preview.status.replace(/_/g, ' ')}</p><PreviewHeading className="mt-1 wrap-break-word font-serif text-lg text-text-primary">{preview.incomingProgramTitle}</PreviewHeading><p className="mt-1 break-all text-[10px] text-text-muted">{preview.sourcePath}</p></div><span className="rounded-full bg-accent/10 px-2.5 py-1 text-[9px] font-medium text-accent">{policy.label}</span></div>
      <p className="mt-2 text-[10px] leading-4 text-text-muted">{policy.description}</p>
      <div className="mt-4 grid grid-cols-2 gap-2 text-xs"><div className="rounded-lg border border-border p-3"><span className="block text-[10px] text-text-muted">Checksum</span><code className="mt-1 block break-all text-[10px] text-text-secondary">{preview.manifest.rootSha256}</code></div><div className="rounded-lg border border-border p-3"><span className="block text-[10px] text-text-muted">Pack version</span><span className="mt-1 block text-text-secondary">{preview.manifest.format} · v{preview.manifest.version}</span></div></div>
      <section aria-label="Pack privacy manifest" className="mt-3 min-w-0 rounded-lg border border-border p-3"><PartHeading className="text-xs font-semibold text-text-primary">Pack privacy manifest</PartHeading><dl className="mt-3 grid min-w-0 gap-2 text-[10px] sm:grid-cols-2">{([['Answer keys included', preview.manifest.privacy.includesAnswerKeys], ['Hidden evaluators included', preview.manifest.privacy.includesHiddenEvaluators], ['Learner evidence included', preview.manifest.privacy.includesLearnerEvidence], ['Practical artifacts included', preview.manifest.privacy.includesPracticalArtifacts], ['Full source bodies included', preview.manifest.privacy.includesFullSourceBodies], ['Source redistribution confirmed', preview.manifest.privacy.sourceBodyRedistributionConfirmed], ['Private chat included', preview.manifest.privacy.includesPrivateChat], ['Credentials included', preview.manifest.privacy.includesCredentials]] as const).map(([label, value]) => <div key={label} className="flex min-w-0 items-center justify-between gap-2 rounded bg-surface px-2.5 py-2"><dt className="min-w-0 text-text-muted">{label}</dt><dd className={`shrink-0 ${value ? 'font-semibold text-amber-700' : 'text-emerald-700'}`}>{value ? 'Yes' : 'No'}</dd></div>)}</dl><div className="mt-3 rounded-lg bg-background p-2.5"><p className="text-[10px] font-medium text-text-primary">Omitted items</p>{preview.manifest.privacy.omittedItems.length > 0 ? <ul className="mt-1 list-disc space-y-1 wrap-break-word pl-4 text-[10px] leading-4 text-text-muted">{preview.manifest.privacy.omittedItems.map((item, index) => <li key={`${item}:${index}`}>{item}</li>)}</ul> : <p className="mt-1 text-[10px] text-text-muted">None listed.</p>}</div></section>
      <div className="mt-4"><PartHeading className="text-xs font-semibold text-text-primary">Conflicts · {preview.conflicts.length}</PartHeading>{preview.conflicts.length ? <ul className="mt-2 space-y-2">{preview.conflicts.map((item, index) => <li key={`${item.kind}:${item.existingId ?? ''}:${index}`} className="rounded-lg border border-amber-500/20 bg-amber-500/5 p-3 text-[10px]"><div className="font-medium text-text-primary">{CONFLICT_KINDS[item.kind].label}{item.existingTitle ? ` · ${item.existingTitle}` : ''}</div><div className="mt-1 text-text-muted">{isBlocked(item, preview.conflictPolicy) ? 'Blocked' : 'Cleared'} by “{policy.label}”</div></li>)}</ul> : <p className="mt-1 text-[10px] text-text-muted">No ID or content conflicts were found during preview.</p>}</div>
      <div className="mt-4"><PartHeading className="text-xs font-semibold text-text-primary">Proposed changes · {preview.changes.length}</PartHeading>{preview.changes.length ? <ul className="mt-2 max-h-40 space-y-2 overflow-auto">{preview.changes.map((item, index) => <li key={`${item.entityKind}:${item.entityId}:${index}`} className="rounded-lg border border-border p-2.5 text-[10px]"><span className="font-medium text-accent">{item.action.replace(/_/g, ' ')}</span><span className="ml-2 text-text-muted">{item.entityKind}</span><p className="mt-1 text-text-secondary">{item.description}</p></li>)}</ul> : <p className="mt-1 text-[10px] text-text-muted">No write actions are proposed.</p>}</div>
      {preview.warnings.length > 0 && <div className="mt-4 rounded-lg bg-amber-500/10 p-3"><PartHeading className="text-xs font-semibold text-amber-800">Review these warnings</PartHeading><ul className="mt-2 list-disc space-y-1 pl-4 text-[10px] leading-4 text-amber-800">{preview.warnings.map((warning, index) => <li key={`${warning}:${index}`}>{warning}</li>)}</ul></div>}

      {blocked.length > 0 && preview.status !== 'applied' && <div role="group" aria-labelledby={`${ids}-blocked`} className="mt-4 rounded-xl border border-accent/25 bg-accent/5 p-4">
        <PartHeading id={`${ids}-blocked`} className="text-sm font-semibold text-text-primary">{existing ? `This course is already on this computer${existing.existingTitle ? ` (${existing.existingTitle})` : ''}` : 'This file cannot be imported as it is'}</PartHeading>
        {blocked.some((item) => item !== existing) && <ul className="mt-2 list-disc space-y-1 pl-4 text-xs leading-5 text-text-secondary">{blocked.filter((item) => item !== existing).map((item, index) => <li key={`${item.kind}:${item.existingId ?? ''}:${index}`}>{blockedReason(item)}</li>)}</ul>}
        {choices.length > 0 && <><p className="mt-3 text-xs text-text-secondary">{existing ? 'Choose how to bring this version in. You review the result before anything is written.' : 'You can still bring it in this way:'}</p><div className="mt-2 grid gap-2 sm:grid-cols-2">{choices.map((value) => { const option = conflictPolicyOption(value); return <button key={value} type="button" disabled={busy} aria-label={option.label} aria-describedby={`${ids}-${value}`} onClick={() => void choosePolicy(value)} className={`rounded-xl border p-3 text-left disabled:opacity-50 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent ${option.suggested ? 'border-accent bg-surface shadow-sm' : 'border-border bg-background hover:border-accent/40'}`}><span className="block text-xs font-semibold text-text-primary">{option.label}</span><span id={`${ids}-${value}`} className="mt-1 block text-[10px] leading-4 text-text-muted">{option.description}</span></button>; })}</div></>}
        {/* A block that clears by itself can be checked again, beside any choice that clears it now. */}
        {awaitsPreparation(preview)
          ? <div className="mt-3 flex flex-wrap items-center gap-3"><p className="text-xs text-text-secondary">{choices.length > 0 ? 'Or check again' : 'Check again'} once it has finished preparing.</p><button type="button" disabled={busy} onClick={() => void choosePolicy(preview.conflictPolicy)} className="inline-flex items-center gap-1.5 rounded-full border border-accent/35 px-3 py-1.5 text-xs font-semibold text-accent disabled:opacity-50"><RefreshCw size={12} /> {previewPack.isPending ? 'Checking…' : 'Check again'}</button></div>
          : choices.length === 0 && <p className="mt-3 text-xs text-text-secondary">None of the import choices can bring this file in.</p>}
      </div>}

      {preview.status === 'pending' && <div className="mt-4 flex flex-wrap gap-2">{preview.canApply && <button type="button" disabled={busy} onClick={() => setApplyConfirm(true)} className="inline-flex items-center gap-1.5 rounded-full bg-accent px-4 py-2 text-xs font-semibold text-accent-fg disabled:opacity-40"><Check size={13} />Apply these changes</button>}<button type="button" disabled={busy} onClick={() => void abandonPreview()} className="rounded-full border border-border px-4 py-2 text-xs text-text-secondary disabled:opacity-50">Cancel preview</button>{!preview.canApply && blocked.length === 0 && <span className="self-center text-[10px] text-amber-700">This preview is not eligible to apply. Review its warnings and conflicts.</span>}</div>}
      {applyConfirm && <div role="dialog" aria-modal="true" aria-labelledby={`${ids}-confirm`} className="mt-4 rounded-xl border border-rose-500/25 bg-rose-500/5 p-4"><PartHeading id={`${ids}-confirm`} className="font-medium text-text-primary">Apply the reviewed import?</PartHeading><p className="mt-1 text-xs leading-5 text-text-secondary">This will write {preview.changes.length} listed change{preview.changes.length === 1 ? '' : 's'} as “{policy.label}”. The pack checksum will be checked again before writing.</p><div className="mt-3 flex flex-wrap gap-2"><button type="button" autoFocus disabled={busy} onClick={() => void runApply()} className="rounded-full bg-rose-700 px-4 py-2 text-xs font-semibold text-white disabled:opacity-50">{applyPack.isPending ? 'Applying…' : 'Confirm import'}</button><button type="button" disabled={busy} onClick={() => setApplyConfirm(false)} className="rounded-full border border-border px-4 py-2 text-xs text-text-secondary">Keep reviewing</button></div></div>}
    </article>}
  </section>;
}
