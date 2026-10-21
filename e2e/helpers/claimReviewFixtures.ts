import type { Page } from '@playwright/test';

/*
 * Claims for the review screen's specs, in the shapes of
 * `ClaimReviewItemDto` (UI design §5.1): the statement in its sentence, the
 * support on a named source, and what research tried. Built here and handed
 * to the mock's state, which `attach_learning_claim_evidence` and the trust
 * override commands then re-decide (§5.4).
 */

export type SourceFixture = { sourceId: string; title: string; site: string | null; kind: string; url: string | null };

type ClaimOptions = {
  id: string;
  statement: string;
  unitIndex: number;
  verdict: 'contradicted' | 'unsupported' | 'insufficient_standing' | 'no_source';
  source?: SourceFixture;
  role?: 'supports' | 'contradicts';
  /** The sentence of the source that states (or contradicts) the claim. */
  quote?: string;
  context?: string;
};

export function claimFixture(options: ClaimOptions) {
  const { id, statement, unitIndex, verdict, source, role = 'supports', quote = '', context = '' } = options;
  const text = context ? `${context} ${quote}` : quote;
  const passage = source ? { sourceVersionId: `${source.sourceId}-v1`, title: source.title, url: source.url, text, startByte: 0, endByte: text.length, retrievalKind: 'hybrid', trust: null } : null;
  return {
    claimId: id,
    location: { unitIndex, unitKind: 'block', unitTitle: `Section ${unitIndex + 1} title`, field: 'body', quote: statement, span: null },
    statement,
    state: { state: 'queued' },
    verdictKind: verdict === 'insufficient_standing' ? { kind: verdict, bar: 'established', have: [] } : { kind: verdict },
    reason: null,
    evidence: passage ? [{ passage, addedBy: 'research' }] : [],
    supportingQuote: null, research: [], repairs: [], executions: [], human: null,
    availableActions: ['confirm', 'edit', 'remove', 'general_knowledge', 'attach_evidence', 'research_with_hint'],
    history: [], localCheck: null, verifierCheck: null,
    sufficiency: verdict === 'insufficient_standing'
      ? { claimKind: 'general', subject: null, sufficient: false, bar: 'established', label: null, missing: 'no_identified_source', ceiling: null, reasons: [] }
      : null,
    presentation: { statement, sentence: statement, contextBefore: '', contextAfter: '', place: `Section ${unitIndex + 1} · Section ${unitIndex + 1} title` },
    support: source && passage ? {
      role,
      source: { sourceId: source.sourceId, sourceVersionId: passage.sourceVersionId, title: source.title, url: source.url, addedBy: 'research' },
      site: source.site, kind: source.kind, standing: 'unknown', counts: { outcome: 'not_enough' },
      quote, quoteSpan: null, passage,
    } : null,
    alsoSupported: [],
    tried: { searches: 3, pagesRead: 12, pagesMentioning: 0, stated: false, relevant: [] },
  };
}

type LearningState = {
  program: { modules: Array<{ lessons: Array<{ id: string; title: string; preparation: string; blocks: unknown[]; questions: unknown[] }> }> };
  planWorkspace: { programId: string; programRevision: number; jobs: unknown[] };
  claimReviews: Record<string, unknown>;
};

/** Holds the first lesson for review with these claims; returns its id and title. */
export async function holdLessonForReview(page: Page, claims: ReturnType<typeof claimFixture>[], verified = 9) {
  return page.evaluate(({ claims: held, verified: alreadyVerified }) => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: LearningState }).__LATTICE_LEARNING_STATE__;
    const lesson = state.program.modules[0].lessons[0];
    lesson.preparation = 'outline';
    lesson.blocks = [];
    lesson.questions = [];
    const finishedAt = Date.now() - 60_000;
    state.planWorkspace.jobs = [{
      id: 'held-job', programId: state.planWorkspace.programId, operationId: 'operation', kind: 'lesson_preparation', payloadSha256: 'fixture',
      baseRevisionNumber: state.planWorkspace.programRevision, status: 'completed', progressCompleted: 1, progressTotal: 1,
      progressMessage: `Needs your review: ${held.length} claims`, activity: null, resultId: `needs_review:${lesson.id}`, error: null, retryOfJobId: null,
      createdAt: finishedAt - 3_600_000, startedAt: finishedAt - 3_600_000, finishedAt,
    }];
    state.claimReviews[lesson.id] = {
      lessonId: lesson.id, programId: state.planWorkspace.programId, state: 'needs_review', revision: 3, pass: 2, stopReason: null, changedSinceStop: false,
      counts: { verified: alreadyVerified, confirmed: 0, generalKnowledge: 0, queued: held.length, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 0 },
      teachingFindings: [],
      claims: held,
    };
    return { id: lesson.id, title: lesson.title };
  }, { claims, verified });
}

export async function unsupportedCommands(page: Page) {
  return page.evaluate(() => (window as unknown as { __LATTICE_LEARNING_STATE__: { unsupportedCommands: string[] } }).__LATTICE_LEARNING_STATE__.unsupportedCommands);
}
