import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { LoaderCircle } from 'lucide-react';

import { learningClaimReviewKey } from '@/features/learning/api/learningQueryKeys';
import { CheckerAccuracy } from '@/features/learning/lessons/review/CheckerAccuracy';
import { ClaimReviewCard, type ClaimActionHandler } from '@/features/learning/lessons/review/ClaimReviewCard';
import {
  AUDIT_STOP_PREFIX,
  CLAIM_FILTERS,
  FILTER_LABELS,
  NOTHING_CHANGED,
  announcement,
  applyFilter,
  claimsNoun,
  countsLine,
  filterCounts,
  groupBySection,
  initialFilter,
  isWaitingForYou,
  reviewHold,
  type ClaimFilter,
  type ReviewHold,
} from '@/features/learning/lessons/review/claimReviewPresentation';
import { LessonAuditLine } from '@/features/learning/lessons/review/LessonAudit';
import { isTeachingWaiting, TeachingFindings, teachingNotesNoun } from '@/features/learning/lessons/review/TeachingFindings';
import { useLessonClaimReview, useResolveLearningClaim, useResumeLearningLessonReview } from '@/features/learning/lessons/review/useLessonClaimReview';
import { useEvidenceWaitsForIndex } from '@/features/learning/sources/useCourseIndexStatus';
import { FilePreviewModal } from '@/features/reading/components/FilePreviewModal';
import { fromLearningEvidencePassage, type ResolvedReaderTarget } from '@/features/reading/model/sourceAdapter';
import type { ClaimActionRequestDto, ClaimReviewItemDto, LearningEvidencePassageDto, LessonClaimReviewDto, StopReasonDto } from '@/lib/bindings';

const NAVIGATION_KEYS = new Set(['ArrowDown', 'ArrowUp', 'Home', 'End']);

/** The bound of §6.3, stated wherever passes are counted. */
export const PASS_BOUND = 'Each pass fixes or hands over at least one claim; the machine gives up on a claim after two tries on the same evidence.';

const waitingIn = (review: LessonClaimReviewDto | null | undefined) => review?.claims.filter(isWaitingForYou).length ?? 0;

function StopReason({ reason }: { reason: StopReasonDto }) {
  const facts = reason.request;
  return (
    <div className="mt-2 text-sm leading-6 text-text-secondary">
      <p><span className="font-semibold text-text-primary">Stopped: </span>{reason.message}</p>
      <p><span className="font-semibold text-text-primary">Fix: </span>{reason.fix}</p>
      {facts && <details className="mt-1 text-xs">
        <summary className="cursor-pointer text-text-muted">Request details</summary>
        <dl className="mt-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 tabular-nums">
          <dt>Step</dt><dd>{facts.step}</dd>
          <dt>Prompt</dt><dd>{facts.promptTokensExact ?? facts.promptTokensEstimated ?? '—'} tokens{facts.promptTokensExact == null ? ' (estimated)' : ''}</dd>
          <dt>Window</dt><dd>{facts.windowTokens} tokens · {facts.outputReserved} reserved for the answer</dd>
          <dt>Server</dt><dd>{facts.serverKind}{facts.serverMessageSanitized ? ` · ${facts.serverMessageSanitized}` : ''}</dd>
          <dt>Attempts</dt><dd>{facts.attempts}</dd>
        </dl>
      </details>}
    </div>
  );
}

function AuditStop({ reason }: { reason: StopReasonDto }) {
  return (
    <div className="mt-2 text-sm leading-6 text-text-secondary">
      <p><span className="font-semibold text-text-primary">The Verifier audit couldn’t finish: </span>{reason.message}</p>
      <p><span className="font-semibold text-text-primary">Fix: </span>{reason.fix}</p>
    </div>
  );
}

function Header({ review, waiting, teachingWaiting, hold }: { review: LessonClaimReviewDto; waiting: number; teachingWaiting: number; hold: ReviewHold }) {
  if (review.state === 'stopped' && review.stopReason) {
    return <><h3 className="text-base font-semibold text-text-primary">Lesson preparation stopped</h3><StopReason reason={review.stopReason} /></>;
  }
  const auditStop = review.stopReason?.code.startsWith(AUDIT_STOP_PREFIX) ? review.stopReason : null;
  if (review.state === 'needs_review' && hold === 'audit_stop' && auditStop) {
    return <>
      <h3 className="text-base font-semibold text-text-primary">Waiting for the Verifier audit</h3>
      <AuditStop reason={auditStop} />
      <p className="mt-2 text-sm leading-6 text-text-secondary">No claim needs you. The lesson publishes once the audit completes, or once the audit is turned off for this course; after the fix, choose <span className="font-semibold">Resume verification</span>.</p>
    </>;
  }
  if (review.state === 'needs_review' && hold === 'audit') {
    return <>
      <h3 className="text-base font-semibold text-text-primary">Waiting for the Verifier audit</h3>
      <p className="mt-2 text-sm leading-6 text-text-secondary">No claim needs you, but the Verifier audit has not finished, so the lesson is held. The audit line below shows how far it got. Choose <span className="font-semibold">Resume verification</span> to run it again, or turn the audit off for this course on the Plan tab.</p>
    </>;
  }
  if (review.state === 'needs_review' && hold === 'resolved') {
    return <><h3 className="text-base font-semibold text-text-primary">Ready to resume</h3><p className="mt-2 text-sm leading-6 text-text-secondary">Every claim that needed you is resolved. Choose <span className="font-semibold">Resume verification</span> to check your changes and publish the lesson.</p></>;
  }
  if (waiting > 0) {
    return <>
      <h3 className="text-base font-semibold text-text-primary">Needs your review · {waiting === 1 ? '1 claim needs your review' : `${claimsNoun(waiting)}`}</h3>
      <p className="mt-2 text-sm leading-6 text-text-secondary">Everything the lesson says was checked against your references. {waiting === 1 ? '1 claim' : `${waiting} claims`} could not be verified or {waiting === 1 ? 'was' : 'were'} contradicted; {waiting === 1 ? 'it is' : 'they are'} listed below with what was compared. Resolve each one, then choose <span className="font-semibold">Resume verification</span>. The lesson becomes ready only when every claim is verified, confirmed by you, removed, or edited and re-verified.</p>
      {auditStop && <AuditStop reason={auditStop} />}
    </>;
  }
  if (review.state === 'ready' && teachingWaiting > 0) {
    return <>
      <h3 className="text-base font-semibold text-text-primary">Needs your review · {teachingNotesNoun(teachingWaiting)}</h3>
      <p className="mt-2 text-sm leading-6 text-text-secondary">The lesson is published. The teaching review raised {teachingWaiting === 1 ? 'a point' : `${teachingWaiting} points`} its repairs did not settle; {teachingWaiting === 1 ? 'it is' : 'they are'} listed below with the reviewer’s note and the part of the lesson {teachingWaiting === 1 ? 'it is' : 'they are'} about. Choose <span className="font-semibold">Keep as written</span> when the lesson is right as it is.</p>
    </>;
  }
  if (review.state === 'verifying') {
    return <><h3 className="text-base font-semibold text-text-primary">Claims being checked</h3><p className="mt-2 text-sm leading-6 text-text-secondary">Claims that need you appear here while preparation runs, and you can act on them now.</p></>;
  }
  return <><h3 className="text-base font-semibold text-text-primary">Claim review</h3><p className="mt-2 text-sm leading-6 text-text-secondary">Every claim is verified, confirmed by you, removed, or general knowledge.</p></>;
}

/**
 * The lesson's claims: what failed, why, against which passage, and the
 * user's fixes, then Resume (§10). Reads well at 1 claim and at 43.
 */
export function LessonClaimReviewPanel({ programId, lessonId, courseRevision, jobActive = false, lessonAnchor = null }: {
  programId: string;
  lessonId: string;
  /** The course revision, which Resume's job action checks. */
  courseRevision: number;
  /** A preparation attempt is running: act now, Resume is not needed. */
  jobActive?: boolean;
  /** `lesson-<id>-section-` when the lesson is published and its sections are on the page. */
  lessonAnchor?: string | null;
}) {
  const client = useQueryClient();
  const query = useLessonClaimReview(programId, lessonId);
  const resolve = useResolveLearningClaim(programId, lessonId);
  const resume = useResumeLearningLessonReview(programId, lessonId);
  // Resuming checks claims against the course's evidence.
  const waitsForIndex = useEvidenceWaitsForIndex(programId);
  const review = query.data ?? null;
  const claims = useMemo(() => review?.claims ?? [], [review]);
  const [filter, setFilter] = useState<ClaimFilter | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [active, setActive] = useState<string | null>(null);
  const [acting, setActing] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [message, setMessage] = useState('');
  const [undoable, setUndoable] = useState<string | null>(null);
  const [reader, setReader] = useState<ResolvedReaderTarget | null>(null);
  // The card to focus once the list re-renders after an action (§10).
  const [focusTarget, setFocusTarget] = useState<{ claimId: string | null } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const resumeHelp = useId();

  const currentFilter = filter ?? initialFilter(claims);
  const visible = useMemo(() => applyFilter(claims, currentFilter), [claims, currentFilter]);
  const sections = useMemo(() => groupBySection(visible), [visible]);
  const ordered = sections.flatMap((section) => section.claims);
  const counts = filterCounts(claims);
  const waiting = claims.filter(isWaitingForYou).length;
  const teachingWaiting = review?.teachingFindings.filter(isTeachingWaiting).length ?? 0;
  const proposals = claims.filter((claim) => claim.verdictKind?.kind === 'general_knowledge_proposed' && isWaitingForYou(claim));
  const single = claims.length === 1;
  const rovingId = ordered.some((claim) => claim.claimId === active) ? active : ordered[0]?.claimId ?? null;

  useEffect(() => {
    if (!focusTarget || !listRef.current) return;
    const headers = [...listRef.current.querySelectorAll<HTMLButtonElement>('[data-claim-header]')];
    (headers.find((header) => header.dataset.claimId === focusTarget.claimId) ?? headers[0])?.focus();
    setFocusTarget(null);
  }, [focusTarget, review]);

  const runAction: ClaimActionHandler = async (item, request, kind) => {
    if (!review) return null;
    const index = ordered.findIndex((claim) => claim.claimId === item.claimId);
    const next = ordered[index + 1] ?? ordered[index - 1] ?? null;
    setActing(item.claimId);
    setErrors(({ [item.claimId]: _dropped, ...rest }) => rest);
    try {
      const result = await resolve.mutateAsync({ claimId: item.claimId, action: request, expectedRevision: review.revision });
      // The mutation reloads the review before it settles: count from the fresh one.
      const remaining = waitingIn(client.getQueryData<LessonClaimReviewDto | null>(learningClaimReviewKey(programId, lessonId)));
      const lines = result.attach?.lines.join(' ');
      setMessage(lines ? `${lines} ${claimsNoun(remaining)} waiting for you.` : announcement(kind, remaining));
      setUndoable(kind === 'undo' || result.attach?.failure ? null : item.claimId);
      if (kind !== 'attach_evidence' && kind !== 'research_with_hint') setFocusTarget({ claimId: next?.claimId ?? null });
      return result.attach;
    } catch (error) {
      setErrors((current) => ({ ...current, [item.claimId]: error instanceof Error ? error.message : 'The action could not be saved.' }));
      throw error;
    } finally {
      setActing(null);
    }
  };

  const act = (item: ClaimReviewItemDto, request: ClaimActionRequestDto, kind: Parameters<ClaimActionHandler>[2]) =>
    runAction(item, request, kind).catch(() => null);

  const undoLast = () => {
    const item = claims.find((claim) => claim.claimId === undoable);
    if (item) void act(item, { action: 'undo' }, 'undo');
  };

  const acceptAll = async () => {
    if (!review || !proposals[0]) return;
    setActing('all');
    try {
      await resolve.mutateAsync({ claimId: proposals[0].claimId, action: { action: 'accept_all_general_knowledge' }, expectedRevision: review.revision });
      setMessage(announcement('accept_all_general_knowledge', waitingIn(client.getQueryData<LessonClaimReviewDto | null>(learningClaimReviewKey(programId, lessonId)))));
      setUndoable(null);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : 'The proposals could not be accepted.');
    } finally {
      setActing(null);
    }
  };

  const onListKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!NAVIGATION_KEYS.has(event.key) || !(event.target instanceof HTMLElement) || !event.target.matches('[data-claim-header]')) return;
    const headers = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[data-claim-header]')];
    const index = headers.indexOf(event.target as HTMLButtonElement);
    const target = event.key === 'Home' ? 0 : event.key === 'End' ? headers.length - 1
      : Math.min(headers.length - 1, Math.max(0, index + (event.key === 'ArrowDown' ? 1 : -1)));
    event.preventDefault();
    headers[target]?.focus();
  };

  if (query.isPending) return <p role="status" className="my-4 text-sm text-text-muted">Loading the claim review…</p>;
  if (query.isError) return <div role="alert" className="my-4 rounded-xl bg-rose-500/10 p-4 text-sm text-rose-700"><p>{query.error.message}</p><button type="button" onClick={() => void query.refetch()} className="mt-2 underline">Load the claim review again</button></div>;
  if (!review) return null;

  const hold = reviewHold(review, waiting);
  const resumable = review.state === 'needs_review' || review.state === 'stopped';
  // An audit hold is fixed outside the claims (Settings, the course switch), so
  // no claim event is needed before Resume (§6.2, §14.6); the backend agrees.
  const resumeBlocked = jobActive ? 'Verification is running; it takes your changes into account at its next step.'
    : review.state === 'needs_review' && hold === 'claims' && !review.changedSinceStop ? NOTHING_CHANGED
    : waitsForIndex ?? null;
  const resumeLabel = review.state === 'stopped' ? `Retry after fixing: ${review.stopReason?.fix ?? 'the cause above'}` : 'Resume verification';

  return (
    <section aria-label="Claim review" className="my-4 rounded-2xl border border-border bg-surface p-4 sm:p-5">
      <Header review={review} waiting={waiting} teachingWaiting={teachingWaiting} hold={hold} />
      <p className="mt-3 text-xs leading-5 text-text-muted"><span className="font-medium text-text-secondary">Pass {review.pass}</span> · {countsLine(review)}</p>
      <p className="mt-1 text-xs leading-5 text-text-muted">{PASS_BOUND}</p>
      <LessonAuditLine programId={programId} lessonId={lessonId} />

      {resumable && <div className="mt-4 flex flex-wrap items-center gap-3">
        <button type="button" disabled={Boolean(resumeBlocked) || resume.isPending} title={resumeBlocked ?? undefined} aria-describedby={resumeBlocked ? resumeHelp : undefined}
          onClick={() => { resume.reset(); resume.mutate({ courseRevision }); }}
          className="inline-flex min-h-10 items-center gap-2 rounded-full bg-accent px-5 text-sm font-semibold text-accent-fg disabled:opacity-50">
          {resume.isPending && <LoaderCircle size={15} className="animate-spin motion-reduce:animate-none" aria-hidden="true" />}
          {resume.isPending ? 'Resuming…' : resumeLabel}
        </button>
        {resumeBlocked && <p id={resumeHelp} className="text-xs text-text-secondary">{resumeBlocked}</p>}
        {resume.isSuccess && <p role="status" className="text-xs text-text-secondary">Verification resumed. Progress appears above.</p>}
        {resume.error && <p role="alert" className="w-full text-xs text-rose-700">{resume.error.message}</p>}
      </div>}

      {/* Always in the tree, so the first announcement is not lost. */}
      <div className={message ? 'mt-3 flex flex-wrap items-center gap-3 rounded-lg bg-background px-3 py-2 text-xs text-text-secondary' : 'sr-only'}>
        <p aria-live="polite" aria-atomic="true">{message}</p>
        {undoable && <button type="button" disabled={acting !== null} onClick={undoLast} className="min-h-8 rounded-full border border-border px-3 font-medium text-text-primary disabled:opacity-50">Undo</button>}
      </div>

      {!single && claims.length > 0 && <div role="group" aria-label="Show claims" className="mt-4 flex flex-wrap gap-2">
        {CLAIM_FILTERS.filter((key) => key === 'all' || counts[key] > 0 || key === currentFilter).map((key) => (
          <button key={key} type="button" aria-pressed={currentFilter === key} onClick={() => setFilter(key)}
            className={`min-h-9 rounded-full border px-3 text-xs font-medium ${currentFilter === key ? 'border-accent/50 bg-accent/10 text-accent' : 'border-border text-text-secondary'}`}>
            {FILTER_LABELS[key]} <span className="tabular-nums">({counts[key]})</span>
          </button>
        ))}
      </div>}

      {proposals.length > 0 && <button type="button" disabled={acting !== null} onClick={() => void acceptAll()}
        className="mt-3 min-h-9 rounded-full border border-border px-3 text-xs font-medium text-text-primary disabled:opacity-50">
        Accept all {proposals.length} general-knowledge {proposals.length === 1 ? 'proposal' : 'proposals'}
      </button>}

      <div ref={listRef} onKeyDown={onListKeyDown} className="mt-4 space-y-4">
        {claims.length === 0 && <p className="text-sm text-text-muted">No claims have been extracted from this lesson yet.</p>}
        {claims.length > 0 && visible.length === 0 && <p className="text-sm text-text-muted">No claims are {FILTER_LABELS[currentFilter].toLowerCase()} right now.</p>}
        {sections.map((section) => (
          <section key={section.key} aria-label={section.label}>
            {!single && <h4 className="sticky top-0 z-10 flex items-center justify-between gap-2 bg-surface/95 py-1.5 text-xs font-semibold text-text-secondary backdrop-blur">
              <span className="min-w-0 truncate">{section.label}</span>
              <span className="shrink-0 tabular-nums text-text-muted">{claimsNoun(section.claims.length)}</span>
            </h4>}
            <ul role="list" className="space-y-2">
              {section.claims.map((item) => (
                <li key={item.claimId}>
                  <ClaimReviewCard
                    item={item} programId={programId} alwaysOpen={single}
                    expanded={expanded.has(item.claimId)}
                    headerTabIndex={rovingId === item.claimId ? 0 : -1}
                    onHeaderFocus={() => setActive(item.claimId)}
                    onToggle={() => setExpanded((current) => {
                      const next = new Set(current);
                      if (next.has(item.claimId)) next.delete(item.claimId); else next.add(item.claimId);
                      return next;
                    })}
                    busy={acting === item.claimId || acting === 'all'}
                    error={errors[item.claimId] ?? null}
                    onAction={runAction}
                    onOpenPassage={(passage: LearningEvidencePassageDto) => setReader(fromLearningEvidencePassage(passage))}
                    lessonAnchor={lessonAnchor}
                  />
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>

      <TeachingFindings programId={programId} lessonId={lessonId} findings={review.teachingFindings} lessonAnchor={lessonAnchor}
        onKept={(remaining) => setMessage(`Kept as written. ${teachingNotesNoun(remaining)} waiting for you.`)} />

      <CheckerAccuracy programId={programId} />

      <FilePreviewModal isOpen={reader !== null} onClose={() => setReader(null)} source={reader?.source ?? null} initialLocator={reader?.locator ?? null} />
    </section>
  );
}
