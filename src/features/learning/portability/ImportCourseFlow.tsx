import { useEffect, useId, useRef } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { Check, RefreshCw, Upload } from 'lucide-react';

import { useApi } from '@/contexts/ApiContext';
import { awaitsPreparation, blockedConflicts, blockedReason, CONFLICT_KINDS, conflictPolicyOption, existingCourseConflict, isBlocked, proceedingPolicies } from '@/features/learning/portability/conflictPolicies';
import { usePortabilityRunStore } from '@/features/learning/portability/portabilityRunStore';
import { RetryAlert } from '@/features/learning/portability/RetryAlert';

// The flow's own heading, its preview's title, and the preview's parts. On its
// own screen it is the main section (h2); in a course's Import & export panel
// it sits under the panel's h3 beside the export section's h4.
const HEADINGS = { 2: ['h2', 'h3', 'h4'], 4: ['h4', 'h5', 'h6'] } as const;

/**
 * Bring a course file onto this computer, with or without a course open:
 * pick the file, preview it as a move (`DEFAULT_CONFLICT_POLICY`), and offer
 * the choices the backend says clear what blocks it. Nothing is written before Apply.
 */
export function ImportCourseFlow({ headingLevel, programId, onImported }: { headingLevel: keyof typeof HEADINGS; programId?: string; onImported(programId: string, title: string): void }) {
  const [Heading, PreviewHeading, PartHeading] = HEADINGS[headingLevel];
  const ids = useId();
  const api = useApi();
  const client = useQueryClient();
  const scope = programId ?? 'standalone';
  const onImportedRef = useRef(onImported);
  onImportedRef.current = onImported;

  const state = usePortabilityRunStore((store) => store.importFlow);
  const markMounted = usePortabilityRunStore((store) => store.markImportMounted);
  const dismissFeedback = usePortabilityRunStore((store) => store.dismissImportFeedback);
  const setApplyConfirm = usePortabilityRunStore((store) => store.setImportApplyConfirm);
  const chooseAndPreview = usePortabilityRunStore((store) => store.chooseAndPreviewImportFile);
  const retryPreview = usePortabilityRunStore((store) => store.retryImportPreview);
  const choosePolicy = usePortabilityRunStore((store) => store.chooseImportPolicy);
  const abandonPreview = usePortabilityRunStore((store) => store.abandonImportPreview);
  const applyImport = usePortabilityRunStore((store) => store.applyImport);

  useEffect(() => {
    markMounted(true);
    return () => {
      markMounted(false);
    };
  }, [markMounted]);

  const handledImportSeqRef = useRef(state.importSeq);
  useEffect(() => {
    if (state.importSeq <= handledImportSeqRef.current || !state.lastImported) return;
    handledImportSeqRef.current = state.importSeq;
    onImportedRef.current(state.lastImported.programId, state.lastImported.title);
  }, [state.importSeq, state.lastImported]);

  const sameScope = state.scope === null || state.scope === scope;
  const { pendingAction } = state;
  const preview = sameScope ? state.preview : null;
  const applyConfirm = sameScope ? state.applyConfirm : false;
  const failure = sameScope ? state.failure : null;
  const notice = sameScope ? state.notice : '';
  const keptPreview = sameScope ? state.retryPreview : null;
  const keptApply = sameScope ? state.retryApply : null;
  const busy = pendingAction !== null;
  const previewPending = sameScope && (pendingAction === 'preview' || pendingAction === 'choosing');
  const applyPending = sameScope && pendingAction === 'apply';

  const runApply = async () => {
    await applyImport(api, client);
  };

  const blocked = preview ? blockedConflicts(preview) : [];
  const existing = preview ? existingCourseConflict(preview) : undefined;
  const choices = preview ? proceedingPolicies(preview) : [];
  const policy = preview ? conflictPolicyOption(preview.conflictPolicy) : null;
  const failedRetry = failure?.retry === 'preview' && keptPreview
    ? { label: 'Retry same preview', run: () => void retryPreview(api, client) }
    : failure?.retry === 'apply' && keptApply
      ? { label: 'Retry apply', run: () => void runApply() }
      : undefined;

  return <section aria-labelledby={`${ids}-heading`} data-testid="import-course-flow" className="min-w-0 rounded-xl border border-border-subtle bg-surface p-4 shadow-sm sm:p-6">
    <div className="flex items-center gap-2"><Upload size={16} className="text-accent" /><div><p className="text-[10px] font-semibold uppercase tracking-[.14em] text-accent">Import</p><Heading id={`${ids}-heading`} className="font-sans text-lg font-semibold tracking-[-0.02em] text-text-primary">Bring a course to this computer</Heading></div></div>
    <p className="mt-2 text-xs leading-5 text-text-secondary">Choose a course file exported from Lattice. You see what it contains and every change it would make before anything is written, and the course opens where you left off.</p>
    <button type="button" disabled={busy} onClick={() => void chooseAndPreview(api, client, scope)} className="mt-4 inline-flex h-8 items-center gap-2 rounded-lg border border-border-default bg-surface px-3.5 text-xs font-medium text-text-primary shadow-control hover:border-border-strong hover:bg-surface-raised disabled:opacity-50">{previewPending ? 'Checking the file…' : 'Choose a course file…'}</button>

    {failure && <RetryAlert className="mt-4" message={failure.message} disabled={busy} retry={failedRetry} onDismiss={!busy ? dismissFeedback : undefined} />}
    {notice && (
      <div role="status" className="mt-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-success/25 bg-success-muted/40 px-4 py-3 text-sm text-success-fg">
        <span>{notice}</span>
        <div className="flex flex-wrap items-center gap-2">
          {sameScope && state.lastImported && !applyPending && (
            <button
              type="button"
              onClick={() => onImportedRef.current(state.lastImported!.programId, state.lastImported!.title)}
              className="shrink-0 rounded-lg bg-action px-3 py-1.5 text-xs font-medium text-action-fg shadow-action hover:bg-action-hover"
            >
              Open course
            </button>
          )}
          {!applyPending && (
            <button
              type="button"
              onClick={dismissFeedback}
              className="shrink-0 text-xs font-semibold underline"
            >
              Dismiss
            </button>
          )}
        </div>
      </div>
    )}

    {preview && policy && <article className="mt-5 min-w-0 rounded-xl border border-border-subtle bg-bg p-3 sm:p-4">
      <div className="flex flex-wrap items-start justify-between gap-2"><div className="min-w-0"><p className="text-[10px] font-semibold uppercase tracking-[.13em] text-accent">Dry run · {preview.status.replace(/_/g, ' ')}</p><PreviewHeading className="mt-1 wrap-break-word font-sans text-base font-semibold tracking-[-0.02em] text-text-primary">{preview.incomingProgramTitle}</PreviewHeading><p className="mt-1 break-all text-[10px] text-text-muted">{preview.sourcePath}</p></div><span className="rounded-md bg-accent-muted px-2 py-0.5 text-[10px] font-semibold text-accent">{policy.label}</span></div>
      <p className="mt-2 text-[10px] leading-4 text-text-muted">{policy.description}</p>
      <div className="mt-4 grid grid-cols-2 gap-2 text-xs"><div className="rounded-lg border border-border-subtle p-3"><span className="block text-[10px] text-text-muted">Checksum</span><code className="mt-1 block break-all text-[10px] text-text-secondary">{preview.manifest.rootSha256}</code></div><div className="rounded-lg border border-border-subtle p-3"><span className="block text-[10px] text-text-muted">Pack version</span><span className="mt-1 block text-text-secondary">{preview.manifest.format} · v{preview.manifest.version}</span></div></div>
      <section aria-label="Pack privacy manifest" className="mt-3 min-w-0 rounded-lg border border-border-subtle p-3"><PartHeading className="text-xs font-semibold text-text-primary">Pack privacy manifest</PartHeading><dl className="mt-3 grid min-w-0 gap-2 text-[10px] sm:grid-cols-2">{([['Answer keys included', preview.manifest.privacy.includesAnswerKeys], ['Hidden evaluators included', preview.manifest.privacy.includesHiddenEvaluators], ['Learner evidence included', preview.manifest.privacy.includesLearnerEvidence], ['Practical artifacts included', preview.manifest.privacy.includesPracticalArtifacts], ['Full source bodies included', preview.manifest.privacy.includesFullSourceBodies], ['Source redistribution confirmed', preview.manifest.privacy.sourceBodyRedistributionConfirmed], ['Private chat included', preview.manifest.privacy.includesPrivateChat], ['Credentials included', preview.manifest.privacy.includesCredentials]] as const).map(([label, value]) => <div key={label} className="flex min-w-0 items-center justify-between gap-2 rounded bg-surface px-2.5 py-2"><dt className="min-w-0 text-text-muted">{label}</dt><dd className={`shrink-0 ${value ? 'font-semibold text-warning-fg' : 'text-success-fg'}`}>{value ? 'Yes' : 'No'}</dd></div>)}</dl><div className="mt-3 rounded-lg bg-bg p-2.5"><p className="text-[10px] font-medium text-text-primary">Omitted items</p>{preview.manifest.privacy.omittedItems.length > 0 ? <ul className="mt-1 list-disc space-y-1 wrap-break-word pl-4 text-[10px] leading-4 text-text-muted">{preview.manifest.privacy.omittedItems.map((item, index) => <li key={`${item}:${index}`}>{item}</li>)}</ul> : <p className="mt-1 text-[10px] text-text-muted">None listed.</p>}</div></section>
      <div className="mt-4"><PartHeading className="text-xs font-semibold text-text-primary">Conflicts · {preview.conflicts.length}</PartHeading>{preview.conflicts.length ? <ul className="mt-2 space-y-2">{preview.conflicts.map((item, index) => <li key={`${item.kind}:${item.existingId ?? ''}:${index}`} className="rounded-lg border border-warning/25 bg-warning-muted/40 p-3 text-[10px]"><div className="font-medium text-text-primary">{CONFLICT_KINDS[item.kind].label}{item.existingTitle ? ` · ${item.existingTitle}` : ''}</div><div className="mt-1 text-text-muted">{isBlocked(item, preview.conflictPolicy) ? 'Blocked' : 'Cleared'} by “{policy.label}”</div></li>)}</ul> : <p className="mt-1 text-[10px] text-text-muted">No ID or content conflicts were found during preview.</p>}</div>
      <div className="mt-4"><PartHeading className="text-xs font-semibold text-text-primary">Proposed changes · {preview.changes.length}</PartHeading>{preview.changes.length ? <ul className="mt-2 max-h-40 space-y-2 overflow-auto">{preview.changes.map((item, index) => <li key={`${item.entityKind}:${item.entityId}:${index}`} className="rounded-lg border border-border-subtle p-2.5 text-[10px]"><span className="font-medium text-accent">{item.action.replace(/_/g, ' ')}</span><span className="ml-2 text-text-muted">{item.entityKind}</span><p className="mt-1 text-text-secondary">{item.description}</p></li>)}</ul> : <p className="mt-1 text-[10px] text-text-muted">No write actions are proposed.</p>}</div>
      {preview.warnings.length > 0 && <div className="mt-4 rounded-lg border border-warning/25 bg-warning-muted/40 p-3"><PartHeading className="text-xs font-semibold text-warning-fg">Review these warnings</PartHeading><ul className="mt-2 list-disc space-y-1 pl-4 text-[10px] leading-4 text-warning-fg">{preview.warnings.map((warning, index) => <li key={`${warning}:${index}`}>{warning}</li>)}</ul></div>}

      {blocked.length > 0 && preview.status !== 'applied' && <div role="group" aria-labelledby={`${ids}-blocked`} className="mt-4 rounded-xl border border-accent/25 bg-accent-muted/40 p-4">
        <PartHeading id={`${ids}-blocked`} className="text-sm font-semibold text-text-primary">{existing ? `This course is already on this computer${existing.existingTitle ? ` (${existing.existingTitle})` : ''}` : 'This file cannot be imported as it is'}</PartHeading>
        {blocked.some((item) => item !== existing) && <ul className="mt-2 list-disc space-y-1 pl-4 text-xs leading-5 text-text-secondary">{blocked.filter((item) => item !== existing).map((item, index) => <li key={`${item.kind}:${item.existingId ?? ''}:${index}`}>{blockedReason(item)}</li>)}</ul>}
        {choices.length > 0 && <><p className="mt-3 text-xs text-text-secondary">{existing ? 'Choose how to bring this version in. You review the result before anything is written.' : 'You can still bring it in this way:'}</p><div className="mt-2 grid gap-2 sm:grid-cols-2">{choices.map((value) => { const option = conflictPolicyOption(value); return <button key={value} type="button" disabled={busy} aria-label={option.label} aria-describedby={`${ids}-${value}`} onClick={() => void choosePolicy(api, client, value)} className={`rounded-xl border p-3 text-left disabled:opacity-50 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent ${option.suggested ? 'border-accent bg-surface shadow-sm' : 'border-border-subtle bg-bg hover:border-border-default'}`}><span className="block text-xs font-semibold text-text-primary">{option.label}</span><span id={`${ids}-${value}`} className="mt-1 block text-[10px] leading-4 text-text-muted">{option.description}</span></button>; })}</div></>}
        {/* A block that clears by itself can be checked again, beside any choice that clears it now. */}
        {awaitsPreparation(preview)
          ? <div className="mt-3 flex flex-wrap items-center gap-3"><p className="text-xs text-text-secondary">{choices.length > 0 ? 'Or check again' : 'Check again'} once it has finished preparing.</p><button type="button" disabled={busy} onClick={() => void choosePolicy(api, client, preview.conflictPolicy)} className="inline-flex items-center gap-1.5 rounded-lg border border-border-default bg-surface px-3 py-1.5 text-xs font-semibold text-accent shadow-control disabled:opacity-50"><RefreshCw size={12} /> {previewPending ? 'Checking…' : 'Check again'}</button></div>
          : choices.length === 0 && <p className="mt-3 text-xs text-text-secondary">None of the import choices can bring this file in.</p>}
      </div>}

      {preview.status === 'pending' && <div className="mt-4 flex flex-wrap gap-2">{preview.canApply && <button type="button" disabled={busy} onClick={() => setApplyConfirm(true)} className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-action px-3.5 text-xs font-medium text-action-fg shadow-action disabled:opacity-40"><Check size={13} />Apply these changes</button>}<button type="button" disabled={busy} onClick={() => void abandonPreview(api, client)} className="h-8 rounded-lg border border-border-default bg-surface px-3.5 text-xs text-text-secondary shadow-control disabled:opacity-50">Cancel preview</button>{!preview.canApply && blocked.length === 0 && <span className="self-center text-[10px] text-warning-fg">This preview is not eligible to apply. Review its warnings and conflicts.</span>}</div>}
      {applyConfirm && <div role="dialog" aria-modal="true" aria-labelledby={`${ids}-confirm`} className="mt-4 rounded-xl border border-danger/25 bg-danger-muted/35 p-4"><PartHeading id={`${ids}-confirm`} className="font-medium text-text-primary">Apply the reviewed import?</PartHeading><p className="mt-1 text-xs leading-5 text-text-secondary">This will write {preview.changes.length} listed change{preview.changes.length === 1 ? '' : 's'} as “{policy.label}”. The pack checksum will be checked again before writing.</p><div className="mt-3 flex flex-wrap gap-2"><button type="button" autoFocus disabled={busy} onClick={() => void runApply()} className="h-8 rounded-lg bg-danger px-3.5 text-xs font-semibold text-white disabled:opacity-50">{applyPending ? 'Applying…' : 'Confirm import'}</button><button type="button" disabled={busy} onClick={() => setApplyConfirm(false)} className="h-8 rounded-lg border border-border-default bg-surface px-3.5 text-xs text-text-secondary shadow-control">Keep reviewing</button></div></div>}
    </article>}
  </section>;
}
