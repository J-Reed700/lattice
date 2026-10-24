import { useEffect, useId, useMemo, useRef, useState } from 'react';

import { ArrowLeft, LoaderCircle } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { AcceptSourceDialog } from '@/features/learning/lessons/review/AcceptSourceDialog';
import { AddSourceSheet, type AddSourceTarget } from '@/features/learning/lessons/review/AddSourceSheet';
import { ClaimInFocus } from '@/features/learning/lessons/review/ClaimInFocus';
import { ClaimQueue, queueRowId } from '@/features/learning/lessons/review/ClaimQueue';
import { ALL_RESOLVED, tallyLine, teachingNotesForYou } from '@/features/learning/lessons/review/claimReviewCopy';
import { AUDIT_STOP_PREFIX, NOTHING_CHANGED, currentPreparationJobs, reviewHold, type ClaimGroup } from '@/features/learning/lessons/review/claimReviewPresentation';
import { LessonAuditLine } from '@/features/learning/lessons/review/LessonAudit';
import { AuditHold, FocusMessage, StopReason } from '@/features/learning/lessons/review/ReviewHolds';
import { ReviewSourceReader, type ReaderRequest } from '@/features/learning/lessons/review/ReviewSourceReader';
import { isTeachingWaiting, TeachingFindings } from '@/features/learning/lessons/review/TeachingFindings';
import { useClaimReviewSession } from '@/features/learning/lessons/review/useClaimReviewSession';
import { useResumeLearningLessonReview } from '@/features/learning/lessons/review/useLessonClaimReview';
import { useReviewDrop, type DroppedSource } from '@/features/learning/lessons/review/useReviewDrop';
import { useEvidenceWaitsForIndex } from '@/features/learning/sources/useCourseIndexStatus';
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts';
import type { ClaimSupportDto } from '@/lib/bindings';

const NARROW = '(max-width: 767px)';

function useNarrow(): boolean {
  const [narrow, setNarrow] = useState(() => typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia(NARROW).matches);
  useEffect(() => {
    if (typeof window.matchMedia !== 'function') return;
    const media = window.matchMedia(NARROW);
    const update = () => setNarrow(media.matches);
    update();
    media.addEventListener?.('change', update);
    return () => media.removeEventListener?.('change', update);
  }, []);
  return narrow;
}

const ALL_KEYS: Array<[string, string]> = [
  ['A', 'Accept'], ['Shift A', 'Accept the source for all'], ['E', 'Edit the sentence'], ['R', 'Remove it'], ['S', 'Add a source'],
  ['G', 'General knowledge'], ['J / ↓', 'Next claim'], ['K / ↑', 'Previous claim'], ['U', 'Undo'], ['Enter', 'Open a queue row'], ['Esc', 'Close a form, then the screen'],
];

/**
 * The review screen (§3): the lesson's waiting claims as a queue beside one
 * claim in focus, the honest tally, Resume when nothing is left, and the
 * keyboard throughout. Drops and pastes anywhere open Add a source.
 */
export function ClaimReviewScreen({ programId, lessonId, lessonTitle, courseRevision, onBack }: {
  programId: string;
  lessonId: string;
  lessonTitle: string;
  /** The course revision, which Resume's job action checks. */
  courseRevision: number;
  /** Back to the course page's Lessons tab. */
  onBack: () => void;
}) {
  const session = useClaimReviewSession(programId, lessonId);
  const resume = useResumeLearningLessonReview(programId, lessonId);
  const plan = useLearningPlan(programId);
  const waitsForIndex = useEvidenceWaitsForIndex(programId);
  const narrow = useNarrow();
  const resumeHelp = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const statementRef = useRef<HTMLParagraphElement>(null);
  const [sheet, setSheet] = useState<{ target: AddSourceTarget; initial: DroppedSource | null } | null>(null);
  const [trusting, setTrusting] = useState<{ support: ClaimSupportDto; sharing: number } | null>(null);
  const [reader, setReader] = useState<ReaderRequest | null>(null);
  const [legend, setLegend] = useState(false);
  const [narrowView, setNarrowView] = useState<'queue' | 'claim'>('queue');

  const { review, active, ordered, waiting, groups } = session;
  // A preparation attempt for this lesson is running: act now, Resume is not needed.
  const jobActive = currentPreparationJobs(plan.data?.jobs ?? []).some((job) => (job.status === 'pending' || job.status === 'running') && job.activity?.lessonTitle === lessonTitle);
  const teachingWaiting = review?.teachingFindings.filter(isTeachingWaiting).length ?? 0;

  // After an action, focus moves to the statement of the claim now in focus.
  useEffect(() => {
    if (session.focusTick > 0) requestAnimationFrame(() => statementRef.current?.focus());
  }, [session.focusTick]);

  const groupOf = (claimId: string) => groups.find((group) => group.claims.some((claim) => claim.claimId === claimId)) ?? null;
  const sharingOf = (support: ClaimSupportDto | null) => (support?.role === 'supports'
    ? waiting.filter((claim) => claim.support?.role === 'supports' && claim.support.source.sourceId === support.source.sourceId).length : 0);
  const claimTarget = (): AddSourceTarget | null => {
    if (!active) return null;
    const others = (groupOf(active.claimId)?.claims ?? []).filter((claim) => claim.claimId !== active.claimId && !session.settling.has(claim.claimId)).map((claim) => claim.claimId);
    return { level: 'claim', claimIds: [active.claimId], others };
  };
  const openSheet = (target: AddSourceTarget | null, initial: DroppedSource | null = null) => { if (target?.claimIds.length) setSheet({ target, initial }); };
  const openGroupSheet = (group: ClaimGroup) => openSheet({ level: group.key === 'queue' ? 'queue' : 'group', claimIds: group.claims.map((claim) => claim.claimId) }, null);
  const acceptSource = (support: ClaimSupportDto | null) => { if (support?.role === 'supports') setTrusting({ support, sharing: sharingOf(support) }); };

  const drop = useReviewDrop(rootRef, (source) => openSheet(sheet?.target ?? claimTarget() ?? (ordered.length ? { level: 'queue', claimIds: ordered.map((claim) => claim.claimId) } : null), source));

  const anyOpen = Boolean(session.form || sheet || trusting || reader);
  const click = (keys: string) => rootRef.current?.querySelector<HTMLButtonElement>(`[aria-keyshortcuts="${keys}"]:not(:disabled)`)?.click();
  const shortcuts = useMemo(() => [
    { id: 'review-next', keys: 'J', description: 'Next claim', handler: () => session.step(1) },
    { id: 'review-next-arrow', keys: 'ArrowDown', description: 'Next claim', handler: () => session.step(1) },
    { id: 'review-previous', keys: 'K', description: 'Previous claim', handler: () => session.step(-1) },
    { id: 'review-previous-arrow', keys: 'ArrowUp', description: 'Previous claim', handler: () => session.step(-1) },
    { id: 'review-accept', keys: 'A', description: 'Accept', handler: () => click('A') },
    { id: 'review-accept-source', keys: 'Shift+A', description: 'Accept the source for all', handler: () => click('Shift+A') },
    { id: 'review-edit', keys: 'E', description: 'Edit the sentence', handler: () => click('E') },
    { id: 'review-remove', keys: 'R', description: 'Remove it', handler: () => click('R') },
    { id: 'review-add-source', keys: 'S', description: 'Add a source', handler: () => click('S') },
    { id: 'review-general', keys: 'G', description: 'General knowledge', handler: () => { if (active?.availableActions.includes('general_knowledge')) void session.run(active, { action: 'general_knowledge' }, 'general_knowledge'); } },
    { id: 'review-undo', keys: 'U', description: 'Undo', handler: () => void session.undoLast() },
    { id: 'review-legend', keys: 'Shift+?', description: 'Keyboard keys', handler: () => setLegend((open) => !open) },
  ], [session, active]);  
  useKeyboardShortcuts(shortcuts, { context: 'review', enabled: !anyOpen && !session.acting });
  const escape = useMemo(() => [{ id: 'review-escape', keys: 'Escape', description: 'Close a form, then the screen', handler: () => {
    if (reader || trusting) return; // the dialog closes itself
    if (session.form) session.setForm(null);
    else if (sheet) setSheet(null);
    else if (legend) setLegend(false);
    else if (narrow && narrowView === 'claim') setNarrowView('queue');
    else onBack();
  } }], [reader, trusting, session, sheet, legend, narrow, narrowView, onBack]);
  useKeyboardShortcuts(escape, { context: 'review' });

  const header = (
    <header className="border-b border-border px-4 py-3 sm:px-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <button type="button" onClick={onBack} className="inline-flex min-h-10 min-w-0 items-center gap-2 text-left font-serif text-lg text-text-primary hover:text-accent">
          <ArrowLeft size={16} aria-hidden="true" className="shrink-0" /><span className="truncate">{lessonTitle}</span>
        </button>
        {review && (review.state === 'needs_review' || review.state === 'stopped') && (() => {
          const hold = reviewHold(review, waiting.length);
          const blocked = jobActive ? 'Verification is running; it takes your changes into account at its next step.'
            : review.state === 'needs_review' && hold === 'claims' && !review.changedSinceStop ? NOTHING_CHANGED
            : waitsForIndex ?? null;
          const label = review.state === 'stopped' ? `Retry after fixing: ${review.stopReason?.fix ?? 'the cause above'}` : 'Resume verification';
          return <div className="flex flex-wrap items-center gap-2">
            {blocked && <p id={resumeHelp} className="max-w-xs text-xs text-text-secondary">{blocked}</p>}
            <Button type="button" size="lg" disabled={Boolean(blocked) || resume.isPending} aria-describedby={blocked ? resumeHelp : undefined}
              onClick={() => { resume.reset(); resume.mutate({ courseRevision }, { onSuccess: onBack }); }}>
              {resume.isPending && <LoaderCircle className="animate-spin motion-reduce:animate-none" aria-hidden="true" />}
              {resume.isPending ? 'Resuming…' : label}
            </Button>
          </div>;
        })()}
      </div>
      {review && <p data-testid="review-tally" className="mt-1 text-sm text-text-secondary">
        {jobActive && <span className="font-medium text-text-primary">Checking your changes · </span>}
        {tallyLine(review.counts) || (teachingWaiting ? teachingNotesForYou(teachingWaiting) : 'Nothing to check yet')}
      </p>}
      {review?.state === 'needs_review' && waiting.length === 0 && !review.changedSinceStop && <LessonAuditLine programId={programId} lessonId={lessonId} />}
      {resume.error && <p role="alert" className="mt-1 text-xs text-danger-fg">{resume.error.message}</p>}
    </header>
  );

  if (session.query.isPending) {
    return <div className="flex h-full min-h-0 flex-col bg-background">{header}
      <div role="status" aria-label="Loading the review" className="grid flex-1 gap-4 p-4 md:grid-cols-[20rem_1fr] sm:p-6">
        <div className="h-64 animate-pulse rounded-2xl bg-surface" /><div className="h-96 animate-pulse rounded-2xl bg-surface" />
      </div></div>;
  }
  if (session.query.isError) {
    return <div className="flex h-full min-h-0 flex-col bg-background">{header}
      <div role="alert" className="m-4 rounded-xl bg-danger-muted p-4 text-sm text-text-primary sm:m-6">
        <p>{session.query.error.message}</p>
        <Button type="button" variant="outline" className="mt-3" onClick={() => void session.query.refetch()}>Load again</Button>
      </div></div>;
  }

  const hold = review ? reviewHold(review, waiting.length) : 'claims';
  const auditStop = review?.stopReason?.code.startsWith(AUDIT_STOP_PREFIX) ? review.stopReason : null;
  const position = session.activeIndex >= 0 ? session.activeIndex + 1 : 0;
  const sheetNode = sheet && review && <AddSourceSheet key={sheet.target.claimIds.join(',')} programId={programId} lessonId={lessonId} revision={review.revision}
    target={sheet.target} initial={sheet.initial} remaining={waiting.length - 1} onClose={() => setSheet(null)}
    onAdded={(report, summary, scope) => { setSheet(null); session.added(report, summary, scope); }} />;

  const focus = (() => {
    if (!review) return <FocusMessage title="Not checked yet"><p>This lesson’s claims have not been checked yet. They appear here once preparation checks them.</p></FocusMessage>;
    if (review.state === 'stopped' && review.stopReason) return <FocusMessage title="Lesson preparation stopped"><StopReason reason={review.stopReason} /></FocusMessage>;
    if (session.teachingActive) return <TeachingFindings programId={programId} lessonId={lessonId} findings={review.teachingFindings}
      onKept={(remaining) => session.setMessage(`Kept as written. ${remaining > 0 ? teachingNotesForYou(remaining) : 'No teaching notes left.'}`)} />;
    if (active) return <ClaimInFocus ref={statementRef} programId={programId} item={active} position={position || 1} total={Math.max(ordered.length, 1)}
      onPrevious={session.activeIndex > 0 ? () => session.step(-1) : null} onNext={session.activeIndex >= 0 && session.activeIndex < ordered.length - 1 ? () => session.step(1) : null}
      sharing={sharingOf(active.support)} busy={session.acting} error={session.error} form={session.form} onFormChange={session.setForm}
      onAct={(request, kind) => session.run(active, request, kind)} onAcceptSource={() => acceptSource(active.support)} onAddSource={() => openSheet(claimTarget())}
      closest={session.closestSupport} onOpenSource={(support) => setReader({ source: support.source, passage: support.passage, quote: support.quote })}
      onOpenRelevant={(source) => setReader({ source, passage: null, quote: null })} sheet={sheetNode}
      back={narrow ? <Button type="button" size="icon" variant="ghost" aria-label="Back to the queue" onClick={() => setNarrowView('queue')}><ArrowLeft aria-hidden="true" /></Button> : undefined}
      stickyActions={narrow} />;
    if (review.state === 'needs_review' && (hold === 'audit' || hold === 'audit_stop')) return <AuditHold reason={auditStop} />;
    if (sheetNode) return sheetNode;
    return <FocusMessage title="Everything is resolved">
      <p>{review.state === 'ready' ? 'Every claim is verified, confirmed by you, removed or edited.' : `Every claim is verified, confirmed by you, removed or edited. ${ALL_RESOLVED.replace('Everything is resolved. ', '')}`}</p>
    </FocusMessage>;
  })();

  const queue = <ClaimQueue groups={groups} mode={session.mode} onModeChange={session.setMode} activeId={active?.claimId ?? null}
    openedIds={session.opened} settledIds={session.settling}
    onSelect={(claimId) => { session.select(claimId); if (narrow) setNarrowView('claim'); requestAnimationFrame(() => document.getElementById(queueRowId(claimId))?.focus()); }}
    onAcceptSource={(group) => acceptSource(group.claims[0]?.support ?? null)} onAddSource={openGroupSheet}
    onAcceptProposals={(proposals) => void session.acceptProposals(proposals)} resolved={session.resolved}
    teachingWaiting={teachingWaiting} teachingActive={session.teachingActive} onOpenTeaching={() => { session.openTeaching(); if (narrow) setNarrowView('claim'); }} busy={session.acting} />;

  return (
    <div ref={rootRef} {...drop.handlers} className={`relative flex h-full min-h-0 flex-col bg-background ${drop.dragging ? 'ring-2 ring-inset ring-accent' : ''}`}>
      {header}
      {/* Always in the tree, so the first announcement is not lost. */}
      <div className={session.message ? 'flex flex-wrap items-center gap-3 border-b border-border bg-surface px-4 py-2 text-sm text-text-secondary sm:px-6' : 'sr-only'}>
        <p aria-live="polite" aria-atomic="true">{session.message}</p>
        {session.undo && <Button type="button" size="sm" variant="outline" disabled={session.acting} onClick={() => void session.undoLast()} aria-keyshortcuts="U">Undo</Button>}
      </div>
      {drop.importing && <p role="status" className="px-4 py-2 text-xs text-text-secondary sm:px-6">Importing {drop.importing}…</p>}
      {drop.error && <p role="alert" className="px-4 py-2 text-xs text-danger-fg sm:px-6">{drop.error}</p>}
      {legend && <section aria-label="Keyboard keys" className="border-b border-border bg-surface px-4 py-3 sm:px-6">
        <ul className="grid gap-1 text-xs text-text-secondary sm:grid-cols-2 lg:grid-cols-3">
          {ALL_KEYS.map(([key, what]) => <li key={key}><kbd className="kbd">{key}</kbd> {what}</li>)}
        </ul>
        <p className="mt-2 text-xs text-text-muted">Keys do nothing while you type in a field.</p>
      </section>}

      {narrow ? (
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          {narrowView === 'queue' ? queue : <div className="rounded-2xl bg-surface p-4 shadow-sheet">{focus}</div>}
        </div>
      ) : (
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(15rem,20rem)_minmax(0,1fr)] gap-4 p-4 lg:grid-cols-[minmax(17rem,22rem)_minmax(0,1fr)] sm:p-6">
          <div className="flex min-h-0 flex-col rounded-2xl bg-surface-raised p-2">{queue}</div>
          <div className="min-h-0 overflow-y-auto rounded-2xl bg-surface p-5 shadow-sheet sm:p-6">{focus}</div>
        </div>
      )}

      <AcceptSourceDialog programId={programId} support={trusting?.support ?? null} sharing={trusting?.sharing ?? 0}
        onClose={() => setTrusting(null)} onTrusted={(result, support) => { setTrusting(null); session.trusted(result, support); }} />
      <ReviewSourceReader programId={programId} request={reader} onClose={() => setReader(null)} />
    </div>
  );
}
