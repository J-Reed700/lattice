import { useEffect, useMemo, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { learningClaimReviewKey } from '@/features/learning/api/learningQueryKeys';
import type { ActionForm } from '@/features/learning/lessons/review/ClaimActions';
import { announcement, trustedAnnouncement, type AnnouncedAction, type AttachSummary } from '@/features/learning/lessons/review/claimReviewCopy';
import { defaultGroupMode, groupClaims, isWaitingForYou, type ClaimGroup, type GroupMode } from '@/features/learning/lessons/review/claimReviewPresentation';
import { useLessonClaimReview, useResolveLearningClaim } from '@/features/learning/lessons/review/useLessonClaimReview';
import { useClearSourceTrustOverride } from '@/features/learning/sources/trust/useSourceTrust';
import type {
  ClaimActionRequestDto,
  ClaimAttachReportDto,
  ClaimReviewItemDto,
  ClaimSupportDto,
  LearningTrustChangeResultDto,
  LearningTrustOverrideScopeDto,
  LessonClaimReviewDto,
} from '@/lib/bindings';

/** How long a resolved claim stays in the queue with its ✓ before it drops out. */
const SETTLE_MS = 1200;

type Undo = { kind: 'claim'; claimId: string } | { kind: 'trust'; scope: LearningTrustOverrideScopeDto } | null;

const waitingIn = (review: LessonClaimReviewDto | null | undefined) => review?.claims.filter(isWaitingForYou).length ?? 0;

const hostOf = (url: string | null) => { try { return url ? new URL(url).host : null; } catch { return null; } };

/** A passage of what the user added, shown when it does not state the claim. */
function closestOf(item: ClaimReviewItemDto | undefined, versionId: string, report: ClaimAttachReportDto | null): ClaimSupportDto | null {
  const passage = item?.evidence.find((entry) => entry.passage.sourceVersionId === versionId)?.passage;
  if (!item || !passage || !report?.source) return null;
  const first = /^[\s\S]{1,320}?[.!?](?=\s|$)/.exec(passage.text.trim())?.[0] ?? passage.text.trim().slice(0, 320);
  return {
    role: 'supports', source: report.source, site: hostOf(passage.url),
    kind: passage.url ? 'page' : 'pasted_text', standing: null, counts: { outcome: 'not_enough' },
    quote: first, quoteSpan: null, passage,
  };
}

/**
 * The review screen's state (§3): the queue's order and grouping, the claim
 * in focus, the forms and sheet open on it, what the last action announced
 * and how to undo it. Resolving moves to the next claim in queue order.
 */
export function useClaimReviewSession(programId: string, lessonId: string) {
  const client = useQueryClient();
  const query = useLessonClaimReview(programId, lessonId);
  const resolve = useResolveLearningClaim(programId, lessonId);
  const clearTrust = useClearSourceTrustOverride();
  const review = query.data ?? null;
  const claims = useMemo(() => review?.claims ?? [], [review]);

  const [mode, setMode] = useState<GroupMode | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [teachingActive, setTeachingActive] = useState(false);
  const [opened, setOpened] = useState<Set<string>>(new Set());
  const [settling, setSettling] = useState<Set<string>>(new Set());
  const [form, setForm] = useState<ActionForm | null>(null);
  const [acting, setActing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState('');
  const [undo, setUndo] = useState<Undo>(null);
  const [closest, setClosest] = useState<{ claimId: string; versionId: string; report: ClaimAttachReportDto } | null>(null);
  const [focusTick, setFocusTick] = useState(0);
  const timers = useRef<number[]>([]);
  useEffect(() => () => timers.current.forEach((timer) => window.clearTimeout(timer)), []);

  const waiting = useMemo(() => claims.filter(isWaitingForYou), [claims]);
  const currentMode = mode ?? defaultGroupMode(waiting);
  const queueClaims = useMemo(() => claims.filter((claim) => isWaitingForYou(claim) || settling.has(claim.claimId)), [claims, settling]);
  const groups: ClaimGroup[] = useMemo(() => groupClaims(queueClaims, currentMode), [queueClaims, currentMode]);
  const ordered = useMemo(() => groups.flatMap((group) => group.claims), [groups]);
  const resolved = useMemo(() => claims.filter((claim) => !isWaitingForYou(claim) && !settling.has(claim.claimId)), [claims, settling]);

  // The claim in focus: the chosen one while it exists, else the first in queue order.
  const active = claims.find((claim) => claim.claimId === activeId) ?? (teachingActive ? undefined : ordered[0]);
  const activeIndex = active ? ordered.findIndex((claim) => claim.claimId === active.claimId) : -1;
  useEffect(() => {
    if (!active || opened.has(active.claimId)) return;
    setOpened((current) => new Set(current).add(active.claimId));
  }, [active, opened]);

  const select = (claimId: string) => {
    setTeachingActive(false);
    setActiveId(claimId);
    setForm(null);
    setError(null);
  };
  const step = (by: 1 | -1) => {
    const next = ordered[activeIndex + by];
    if (next) { select(next.claimId); setFocusTick((tick) => tick + 1); }
  };
  const openTeaching = () => { setTeachingActive(true); setActiveId(null); setForm(null); };

  const fresh = () => client.getQueryData<LessonClaimReviewDto | null>(learningClaimReviewKey(programId, lessonId));

  /** The claim leaves the queue after a short settle, and the next one comes into focus. */
  const settle = (item: ClaimReviewItemDto, next: ClaimReviewItemDto | null) => {
    setSettling((current) => new Set(current).add(item.claimId));
    timers.current.push(window.setTimeout(() => setSettling((current) => {
      const after = new Set(current);
      after.delete(item.claimId);
      return after;
    }), SETTLE_MS));
    if (next) select(next.claimId); else setActiveId(null);
    setFocusTick((tick) => tick + 1);
  };

  const run = async (item: ClaimReviewItemDto, request: ClaimActionRequestDto, kind: AnnouncedAction): Promise<boolean> => {
    if (!review) return false;
    const index = ordered.findIndex((claim) => claim.claimId === item.claimId);
    const next = ordered.slice(index + 1).find((claim) => claim.claimId !== item.claimId) ?? ordered.slice(0, Math.max(index, 0)).find((claim) => claim.claimId !== item.claimId) ?? null;
    setActing(true);
    setError(null);
    try {
      await resolve.mutateAsync({ claimId: item.claimId, action: request, expectedRevision: review.revision });
      // The mutation reloads the review before it settles: count from the fresh one.
      const after = fresh();
      setMessage(announcement(kind, waitingIn(after)));
      setUndo(kind === 'undo' ? null : { kind: 'claim', claimId: item.claimId });
      const stillWaiting = after?.claims.find((claim) => claim.claimId === item.claimId);
      if (kind !== 'undo' && (!stillWaiting || !isWaitingForYou(stillWaiting))) settle(item, next);
      if (kind === 'undo') {
        // It waits again: no longer settling, so it counts among its group's others at once.
        setSettling((current) => { const after = new Set(current); after.delete(item.claimId); return after; });
        select(item.claimId);
        setFocusTick((tick) => tick + 1);
      }
      return true;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'The action could not be saved.');
      return false;
    } finally {
      setActing(false);
    }
  };

  const acceptProposals = async (proposals: ClaimReviewItemDto[]) => {
    if (proposals[0]) await run(proposals[0], { action: 'accept_all_general_knowledge' }, 'accept_all_general_knowledge');
  };

  const trusted = (result: LearningTrustChangeResultDto, support: ClaimSupportDto) => {
    setMessage(trustedAnnouncement(support.source.title, result.redecided.verified, result.redecided.stillFailing));
    setUndo({ kind: 'trust', scope: { scope: 'source', programId, sourceId: support.source.sourceId } });
  };

  const added = (report: ClaimAttachReportDto, summary: AttachSummary, scope: string[]) => {
    setMessage(summary.line);
    setUndo(null);
    const anchor = claims.find((claim) => claim.claimId === scope[0]);
    if (summary.notStated && scope.length === 1 && report.source) setClosest({ claimId: scope[0], versionId: report.source.sourceVersionId, report });
    else setClosest(null);
    // Claims in scope that the source settled show their ✓, then drop out; focus moves past them.
    const after = fresh();
    const done = new Set(scope.filter((id) => { const claim = after?.claims.find((entry) => entry.claimId === id); return !claim || !isWaitingForYou(claim); }));
    if (done.size === 0) return;
    setSettling((current) => new Set([...current, ...done]));
    timers.current.push(window.setTimeout(() => setSettling((current) => new Set([...current].filter((id) => !done.has(id)))), SETTLE_MS));
    if (anchor && done.has(anchor.claimId)) {
      const index = ordered.findIndex((claim) => claim.claimId === anchor.claimId);
      const next = ordered.slice(index + 1).find((claim) => !done.has(claim.claimId)) ?? ordered.find((claim) => !done.has(claim.claimId)) ?? null;
      if (next) select(next.claimId); else setActiveId(null);
      setFocusTick((tick) => tick + 1);
    }
  };

  const undoLast = async () => {
    if (!undo) return;
    if (undo.kind === 'claim') {
      const item = claims.find((claim) => claim.claimId === undo.claimId);
      if (item) await run(item, { action: 'undo' }, 'undo');
      return;
    }
    setActing(true);
    try {
      await clearTrust.mutateAsync(undo.scope);
      await client.invalidateQueries({ queryKey: learningClaimReviewKey(programId, lessonId) });
      setMessage(announcement('undo', waitingIn(fresh())));
      setUndo(null);
    } catch (cause) {
      setMessage(cause instanceof Error ? cause.message : 'The change could not be undone.');
    } finally {
      setActing(false);
    }
  };

  const closestSupport = closest && active?.claimId === closest.claimId ? closestOf(active, closest.versionId, closest.report) : null;

  return {
    query, review, claims, waiting, groups, ordered, resolved, settling, opened,
    mode: currentMode, setMode, active, activeIndex, teachingActive, select, step, openTeaching,
    form, setForm, acting, error, message, setMessage, undo, undoLast, run, acceptProposals, trusted, added,
    closestSupport, focusTick,
  };
}
