import type {
  AttachLearningClaimEvidenceRequestDto,
  ClaimAttachProgressDto,
  CheckDto,
  ClaimAttachReportDto,
  ClaimReviewItemDto,
  ClaimSupportDto,
  LearningSourceTrustEntryDto,
  LearningTrustChangeResultDto,
  LearningTrustOverrideInputDto,
  LearningTrustOverrideScopeDto,
  LessonClaimReviewDto,
  ResearchSummaryDto,
  WaitingClaimsRecheckDto,
} from "../../src/lib/bindings";

/**
 * The claim review side of the Learning Studio fixture (claim review UI
 * design §5.4): the review fields a seeded claim leaves out, attaching
 * evidence with its scope and progress, the Sources tab's re-check of the
 * waiting claims, and source trust overrides that re-decide claims.
 */

/** Sends one message on a Channel the renderer passed. */
export type SendOnChannel = (channel: unknown, message: ClaimAttachProgressDto) => void;

type Reviews = Record<string, LessonClaimReviewDto>;

const WAITING_STATES = new Set(["queued", "open", "reopened", "unchecked"]);

/** A statement as the fixture compares it with pasted text. */
function normalized(text: string): string {
  return text.trim().replace(/[.!?]+$/, "").replace(/\s+/g, " ").toLocaleLowerCase();
}

/** Whether `text` states `statement`: the fixture's stand-in for the judge. */
export function states(text: string, statement: string): boolean {
  const wanted = normalized(statement);
  return wanted.length > 0 && normalized(text).includes(wanted);
}

function hostOf(url: string | null): string | null {
  if (!url) return null;
  try {
    return new URL(url).host;
  } catch {
    return null;
  }
}

function placeOf(claim: ClaimReviewItemDto): string {
  const { unitKind, unitIndex, unitTitle } = claim.location;
  if (unitKind === "block") return unitTitle ? `Section ${unitIndex + 1} · ${unitTitle}` : `Section ${unitIndex + 1}`;
  return unitKind === "answer_key" ? `Question ${unitIndex + 1} · answer key` : `Question ${unitIndex + 1}`;
}

/** The support a seeded claim's first compared passage gives it. */
function supportOf(claim: ClaimReviewItemDto): ClaimSupportDto | null {
  const shown = claim.evidence[0];
  if (!shown) return null;
  const { passage } = shown;
  const contradicts = claim.verdictKind?.kind === "contradicted";
  const quote = claim.supportingQuote ?? "";
  const at = quote ? passage.text.indexOf(quote) : -1;
  return {
    role: contradicts ? "contradicts" : "supports",
    source: { sourceId: passage.sourceVersionId, sourceVersionId: passage.sourceVersionId, title: passage.title, url: passage.url, addedBy: shown.addedBy ?? "research" },
    site: hostOf(passage.url),
    kind: shown.addedBy === "user" && !passage.url ? "pasted_text" : "page",
    standing: null,
    counts: contradicts
      ? { outcome: "not_counted", reason: "it contradicts the claim" }
      : claim.state.state === "resolved_by_evidence" ? { outcome: "enough" } : { outcome: "not_enough" },
    quote,
    quoteSpan: at >= 0 ? [at, at + quote.length] : null,
    passage,
  };
}

function triedOf(claim: ClaimReviewItemDto): ResearchSummaryDto {
  const saved = new Set(claim.research.flatMap((attempt) => attempt.sourcesAdded.map((source) => source.sourceVersionId)));
  const compared = new Set(claim.evidence.map((shown) => shown.passage.sourceVersionId));
  const relevant = claim.research.flatMap((attempt) => attempt.sourcesAdded).filter((source) => compared.has(source.sourceVersionId));
  return {
    searches: claim.research.filter((attempt) => attempt.queries.length > 0).length,
    pagesRead: saved.size,
    pagesMentioning: relevant.length,
    stated: false,
    relevant,
  };
}

/** Fills the review fields a seeded review leaves out (UI design §5.1, §5.2). */
export function withReviewFields(review: LessonClaimReviewDto): LessonClaimReviewDto {
  const counts = review.counts as LessonClaimReviewDto["counts"] & { removed?: number };
  if (counts.removed !== undefined) {
    const { removed, ...rest } = counts;
    review.counts = { ...rest, removedByYou: removed, editedByYou: 0, rewritten: 0 };
  }
  review.counts.removedByYou ??= 0;
  review.counts.editedByYou ??= 0;
  review.counts.rewritten ??= 0;
  for (const claim of review.claims) {
    claim.sufficiency ??= null;
    claim.presentation ??= { statement: claim.statement, sentence: claim.location.quote, contextBefore: "", contextAfter: "", place: placeOf(claim) };
    if (claim.support === undefined) claim.support = supportOf(claim);
    claim.alsoSupported ??= [];
    claim.tried ??= triedOf(claim);
  }
  return review;
}

function recount(review: LessonClaimReviewDto) {
  const count = (state: string) => review.claims.filter((item) => item.state.state === state).length;
  review.counts = { ...review.counts, verified: count("resolved_by_evidence"), confirmed: count("confirmed"), queued: count("queued") + count("reopened") };
}

/** The check that verified a claim against `support`. */
function supportedCheck(support: ClaimSupportDto): CheckDto {
  return {
    checker: { role: "local", model: "e2e" },
    result: { verdict: "supported", sourceVersionId: support.source.sourceVersionId, supportingQuote: support.quote },
    reason: "The source states it.",
    fingerprint: "e2e",
    evidence: [{ sourceVersionId: support.source.sourceVersionId, startByte: support.passage.startByte, endByte: support.passage.endByte, textSha256: "e2e" }],
    windowTokens: 0,
  };
}

/** Marks a claim verified against `support`: by the user's attach, or by a machine check against a source the user added. */
function verify(claim: ClaimReviewItemDto, support: ClaimSupportDto, at: number, actor: "user" | "machine") {
  claim.state = { state: "resolved_by_evidence" };
  claim.availableActions = ["attach_evidence"];
  claim.supportingQuote = support.quote;
  claim.support = support;
  const check = supportedCheck(support);
  claim.history.push({
    ordinal: claim.history.length, actor, at, jobId: null, appVersion: "e2e", checkerModel: "e2e",
    event: actor === "user"
      ? { kind: "evidence_attached", sourceVersionId: support.source.sourceVersionId, provenance: { input: "pasted_text" }, result: { outcome: "checked", check } }
      : { kind: "checked", check },
  } as ClaimReviewItemDto["history"][number]);
}

function addedSupport(sourceId: string, versionId: string, title: string, url: string | null, text: string, statement: string): ClaimSupportDto {
  const at = text.toLocaleLowerCase().indexOf(normalized(statement));
  const quote = at >= 0 ? text.slice(at, at + normalized(statement).length) : text;
  return {
    role: "supports",
    source: { sourceId, sourceVersionId: versionId, title, url, addedBy: "user" },
    site: hostOf(url),
    kind: url ? "page" : "pasted_text",
    standing: null,
    counts: { outcome: "enough" },
    quote,
    quoteSpan: at >= 0 ? [at, at + quote.length] : null,
    passage: { sourceVersionId: versionId, title, url, text, startByte: 0, endByte: text.length, retrievalKind: "lexical", trust: null },
  };
}

/**
 * `attach_learning_claim_evidence`: a link whose address holds "403" is
 * refused; pasted text verifies each claim in scope whose statement it
 * contains; a link, file or saved passage verifies the claim it is attached
 * to. Each claim checked is reported on the progress channel.
 */
export function attachEvidence(
  reviews: Reviews,
  args: { request: AttachLearningClaimEvidenceRequestDto; onProgress?: unknown },
  send: SendOnChannel,
): ClaimAttachReportDto {
  const { request, onProgress } = args;
  const progress = (stage: ClaimAttachProgressDto) => onProgress !== undefined && send(onProgress, stage);
  const review = reviews[request.lessonId];
  if (!review) throw { code: "NOT_FOUND", message: "This lesson has no claim review yet." };
  if (review.revision !== request.expectedRevision) throw { code: "INVALID_STATE", message: "This claim changed; reload and act again." };
  const claim = review.claims.find((item) => item.claimId === request.claimId);
  if (!claim) throw { code: "NOT_FOUND", message: "No claim with that id in this lesson." };
  const { evidence } = request;
  const others = { tried: 0, verified: 0, notVerified: 0, noPassage: 0, stopped: null };
  if (evidence.kind === "attach_link") {
    progress({ stage: "fetching", host: hostOf(evidence.url) });
    if (evidence.url.includes("403")) {
      const failure = { kind: "refused" as const, message: `${hostOf(evidence.url)} refused the request (HTTP 403).`, nextSteps: [{ step: "paste_text" as const, origin: evidence.url }, { step: "add_file" as const }] };
      progress({ stage: "done" });
      return { revision: review.revision, source: null, alreadySaved: false, outcome: null, failure, snapshot: { state: "not_applicable" }, others, lines: [failure.message] };
    }
  }
  progress({ stage: "saving" });
  const sourceId = `added-source-${review.revision}`;
  const versionId = `added-version-${review.revision}`;
  const title = evidence.kind === "attach_text" ? evidence.title : evidence.kind === "attach_link" ? evidence.url : "Your file";
  const url = evidence.kind === "attach_link" ? evidence.url : null;
  const text = evidence.kind === "attach_text" ? evidence.text : "";
  const scoped = request.scope.scope === "claims" ? request.scope.claimIds.filter((id) => id !== claim.claimId) : [];
  const inScope = review.claims.filter((item) => scoped.includes(item.claimId) && WAITING_STATES.has(item.state.state));
  const total = 1 + inScope.length;
  const now = Date.now();
  progress({ stage: "checking", done: 0, total, statement: claim.statement });
  const stated = evidence.kind !== "attach_text" || states(text, claim.statement);
  if (stated) verify(claim, addedSupport(sourceId, versionId, title, url, text || claim.statement, claim.statement), now, "user");
  inScope.forEach((item, index) => {
    progress({ stage: "checking", done: index + 1, total, statement: item.statement });
    others.tried += 1;
    if (text && states(text, item.statement)) {
      verify(item, addedSupport(sourceId, versionId, title, url, text, item.statement), now, "machine");
      others.verified += 1;
    } else others.noPassage += 1;
  });
  review.revision += 1;
  review.changedSinceStop = true;
  recount(review);
  progress({ stage: "done" });
  const lines = [stated ? `Verified against ${title}.` : `Nothing in ${title} mentions this claim. It is saved in your sources.`];
  if (others.tried) lines.push(`Also checked ${others.tried} other open claim${others.tried === 1 ? "" : "s"} against this source: ${others.verified} verified, ${others.noPassage} no passage.`);
  return {
    revision: review.revision,
    source: { sourceId, sourceVersionId: versionId, title, url, addedBy: "user" },
    alreadySaved: false,
    outcome: stated && claim.support ? { outcome: "checked", check: supportedCheck(claim.support) } : { outcome: "no_passage" },
    failure: null,
    snapshot: { state: "not_applicable" },
    others,
    lines,
  };
}

/**
 * The Sources tab's re-check after an add (`checkWaiting`): every waiting
 * claim of the held lessons checked against the new text. `null` when no
 * lesson holds a waiting claim, or the user turned the check off.
 */
export function recheckWaiting(
  reviews: Reviews,
  args: { checkWaiting?: boolean; onProgress?: unknown },
  added: { sourceId: string; versionId: string; title: string; url: string | null; text: string },
  send: SendOnChannel,
): WaitingClaimsRecheckDto | null {
  if (!args.checkWaiting) return null;
  const held = Object.values(reviews).filter((review) =>
    (review.state === "needs_review" || review.state === "verifying") && review.claims.some((item) => WAITING_STATES.has(item.state.state)));
  if (held.length === 0) return null;
  const progress = (stage: ClaimAttachProgressDto) => args.onProgress !== undefined && send(args.onProgress, stage);
  const waiting = held.flatMap((review) => review.claims.filter((item) => WAITING_STATES.has(item.state.state)).map((item) => ({ review, item })));
  const recheck: WaitingClaimsRecheckDto = { lessons: held.length, tried: 0, verified: 0, notVerified: 0, noPassage: 0, stopped: null };
  const now = Date.now();
  waiting.forEach(({ review, item }, index) => {
    progress({ stage: "checking", done: index, total: waiting.length, statement: item.statement });
    recheck.tried += 1;
    if (states(added.text, item.statement)) {
      verify(item, addedSupport(added.sourceId, added.versionId, added.title, added.url, added.text, item.statement), now, "machine");
      recheck.verified += 1;
      review.revision += 1;
      review.changedSinceStop = true;
      recount(review);
    } else recheck.noPassage += 1;
  });
  progress({ stage: "done" });
  return recheck;
}

/** The claims whose support an override scope covers. */
function covered(reviews: Reviews, scope: LearningTrustOverrideScopeDto | LearningTrustOverrideInputDto) {
  return Object.values(reviews).flatMap((review) =>
    review.claims
      .filter((item) => item.support && (scope.scope === "source" ? item.support.source.sourceId === scope.sourceId : item.support.site === scope.registrableDomain))
      .map((item) => ({ review, item })));
}

function scopeOf(input: LearningTrustOverrideInputDto): LearningTrustOverrideScopeDto {
  if (input.scope === "source") return { scope: "source", programId: input.programId, sourceId: input.sourceId };
  if (input.scope === "site") return { scope: "site", registrableDomain: input.registrableDomain };
  return { scope: "site_subject", registrableDomain: input.registrableDomain, class: { label: input.classLabel, key: input.classLabel.toLocaleLowerCase() } };
}

function changeResult(scope: LearningTrustOverrideScopeDto, touched: ReturnType<typeof covered>, verified: number): LearningTrustChangeResultDto {
  const versions = [...new Set(touched.map(({ item }) => item.support?.source.sourceVersionId ?? ""))].filter(Boolean);
  return {
    change: { scope, sourceVersionIds: versions },
    impact: { versions: versions.length, claims: touched.length, lessons: new Set(touched.map(({ review }) => review.lessonId)).size },
    redecided: { verified, reopened: 0, stillFailing: touched.length - verified, notLevelled: 0 },
  } as LearningTrustChangeResultDto;
}

/** `set_source_trust_override`: the claims it covers that wait verify. */
export function setTrustOverride(reviews: Reviews, overrides: LearningTrustOverrideInputDto[], input: LearningTrustOverrideInputDto): LearningTrustChangeResultDto {
  overrides.push(input);
  const touched = covered(reviews, input);
  const now = Date.now();
  let verified = 0;
  for (const { review, item } of touched) {
    if (!WAITING_STATES.has(item.state.state) || !item.support) continue;
    verify(item, { ...item.support, counts: { outcome: "enough" } }, now, "machine");
    verified += 1;
    review.revision += 1;
    review.changedSinceStop = true;
    recount(review);
  }
  return changeResult(scopeOf(input), touched, verified);
}

/** `clear_source_trust_override`: `null` when nothing was set there. */
export function clearTrustOverride(reviews: Reviews, overrides: LearningTrustOverrideInputDto[], scope: LearningTrustOverrideScopeDto): LearningTrustChangeResultDto | null {
  const key = (target: LearningTrustOverrideScopeDto) => (target.scope === "source" ? `source:${target.sourceId}` : `${target.scope}:${target.registrableDomain}`);
  const at = overrides.findIndex((input) => key(scopeOf(input)) === key(scope));
  if (at < 0) return null;
  overrides.splice(at, 1);
  return changeResult(scope, covered(reviews, scope), 0);
}

/** `get_learning_source_trust` with `includeImpact`: one entry per source, with what each scope would reach. */
export function trustImpacts(reviews: Reviews, sources: Array<{ sourceId: string; sourceVersionId: string; url: string | null }>): LearningSourceTrustEntryDto[] {
  return sources.map(({ sourceId, sourceVersionId, url }) => {
    const reach = (touched: ReturnType<typeof covered>) => ({
      versions: new Set(touched.map(({ item }) => item.support?.source.sourceVersionId)).size || 1,
      claims: touched.length,
      lessons: new Set(touched.map(({ review }) => review.lessonId)).size,
    });
    const site = hostOf(url);
    return {
      sourceId, sourceVersionId,
      trust: { sourceVersionId, level: "unknown", subject: null, reasons: [], overrideScope: null },
      standing: null, standingReasons: [], provenance: null, trackRecord: { right: 0, wrong: 0 }, overrides: [],
      impact: {
        source: reach(covered(reviews, { scope: "source", programId: "", sourceId })),
        site: site ? reach(covered(reviews, { scope: "site", registrableDomain: site })) : null,
      },
    };
  });
}
