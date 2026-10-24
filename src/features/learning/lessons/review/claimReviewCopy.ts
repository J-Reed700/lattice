import type {
  AttachFailureKindDto,
  ClaimAttachReportDto,
  ClaimCountsDto,
  ClaimReviewItemDto,
  ClaimSupportDto,
  LearningClaimTally,
  LearningSufficiencyDto,
  LearningTrustLevel,
  ReopenCauseDto,
  ResearchSummaryDto,
  SourceKindDto,
  WaitingClaimsRecheckDto,
} from '@/lib/bindings';

const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
export const claimsCount = (count: number) => plural(count, 'claim', 'claims');

const KIND_WORDS: Record<SourceKindDto, string> = {
  online_book: 'online book',
  documentation: 'documentation',
  reference_work: 'reference work',
  peer_reviewed_paper: 'peer-reviewed paper',
  report: 'report',
  article: 'article',
  guide: 'guide',
  forum: 'forum',
  wiki: 'wiki',
  marketing_page: 'marketing page',
  your_file: 'your file',
  pasted_text: 'pasted text',
  page: 'page',
};

export const kindWord = (kind: SourceKindDto) => KIND_WORDS[kind];

export function kindWithArticle(kind: SourceKindDto): string {
  const word = KIND_WORDS[kind];
  if (kind === 'documentation' || kind === 'your_file' || kind === 'pasted_text') return word;
  return `${/^[aeiou]/.test(word) ? 'an' : 'a'} ${word}`;
}

const STANDING_WORDS: Record<LearningTrustLevel, string> = {
  primary: 'the subject’s own account',
  authoritative: 'an authority on this subject',
  established: 'a reliable identified source',
  community: 'a community page (forum, wiki, Q&A)',
  unknown: 'an unidentified author’s work',
  distrusted: 'a source you distrust',
};

export const standingWords = (level: LearningTrustLevel) => STANDING_WORDS[level];

export const ACCEPT_LEVELS: LearningTrustLevel[] = ['established', 'authoritative', 'primary'];

export function countsAs(support: ClaimSupportDto, sufficiency: LearningSufficiencyDto | null): string {
  if (support.role === 'contradicts') return support.standing ? standingWords(support.standing) : 'a source in this course';
  if (support.source.addedBy === 'user' && support.counts.outcome !== 'not_counted') return 'a source you added · enough, it states this';
  if (sufficiency?.sufficient && sufficiency.label === 'best_available') return 'the course’s strongest source on this subject · enough';
  const who = support.standing ? standingWords(support.standing) : `${kindWithArticle(support.kind)} not yet weighed`;
  switch (support.counts.outcome) {
    case 'enough': return `${who} · enough`;
    case 'not_enough': return `${who} · not enough on its own for this claim`;
    case 'not_counted': return `${who} · not counted: ${support.counts.reason}`;
  }
}

export function verifiedLabel(sufficiency: LearningSufficiencyDto | null): string | null {
  switch (sufficiency?.label) {
    case 'practice': return 'A recommendation';
    case 'attributed': return 'Attributed';
    case 'community_consensus': return 'Several community sources agree';
    case 'reported_by': return 'As reported by its source';
    case 'secondary': return 'From a secondary source';
    case 'mirror': return 'From a copy of the original';
    case 'best_available': return 'The course’s strongest source on this';
    default: return null;
  }
}

export function sourceLine(support: Pick<ClaimSupportDto, 'source' | 'site' | 'kind'>): string {
  return [support.source.title, support.site, kindWord(support.kind)].filter(Boolean).join(' · ');
}

export type FixAction = 'accept' | 'accept_source' | 'edit' | 'remove' | 'add_source' | 'general_knowledge';

export type WhyItWaits = {
  chip: string;
  reason: string;
  detail: string | null;
  fixes: FixAction[];
};

const REOPENED: Record<ReopenCauseDto['cause'], string> = {
  sentence_changed: 'The sentence changed.',
  source_deleted: 'Its source was deleted.',
  source_replaced: 'Its source was replaced.',
  unconfirmed: 'You took back your confirmation.',
  verifier_dispute: 'An independent check disagrees with the first one.',
  trust_changed: 'How far one of its sources is trusted changed.',
};

const supporting = (item: ClaimReviewItemDto) => (item.support?.role === 'supports' ? item.support : null);

function insufficient(item: ClaimReviewItemDto): Omit<WhyItWaits, 'chip'> {
  const support = supporting(item);
  const kind = support ? kindWithArticle(support.kind) : null;
  switch (item.sufficiency?.missing) {
    case 'no_primary_for_subject': return {
      reason: 'This needs the subject’s own account, and the course has none that states it.',
      detail: support ? `It is stated by ${kind}${support.site ? ` from ${support.site}` : ''}.` : null,
      fixes: ['add_source', 'accept_source'],
    };
    case 'stronger_source_silent': return {
      reason: 'The course has a stronger source on this subject, and it does not state this.',
      detail: support ? `${support.source.title} states it; the stronger source was read and does not say it.` : null,
      fixes: ['accept_source', 'edit'],
    };
    case 'no_independent_corroboration': return {
      reason: 'Only one source states this, and this kind of statement needs a second one.',
      detail: support ? `${support.source.title} states it; a second, independent source would settle it.` : null,
      fixes: ['add_source', 'accept_source'],
    };
    case 'only_testimonial': return {
      reason: 'The sources that state this are reporting what someone else said.',
      detail: support ? `${support.source.title} attributes it to someone else; the original would settle it.` : null,
      fixes: ['add_source', 'accept_source'],
    };
    case 'only_stale': return {
      reason: 'The sources that state this are older than the newest one on the subject.',
      detail: null,
      fixes: ['add_source', 'accept'],
    };
    case 'no_measurement_origin': return {
      reason: 'This number needs the source that measured it.',
      detail: support ? `${support.source.title} reports it without saying who measured it.` : null,
      fixes: ['add_source', 'accept_source'],
    };
    default: return {
      reason: 'No source the course trusts states this.',
      detail: support ? `The one passage that does is from ${kind} whose author is not identified.` : null,
      fixes: ['accept_source', 'add_source'],
    };
  }
}

function verdictCopy(item: ClaimReviewItemDto): WhyItWaits {
  const kind = item.verdictKind;
  switch (kind?.kind) {
    case 'insufficient_standing': return { chip: 'Waiting for you', ...insufficient(item) };
    case 'unsupported': {
      const mentioning = item.tried.pagesMentioning || item.evidence.length;
      return {
        chip: 'Waiting for you', reason: 'No saved source states this.',
        detail: mentioning > 0 ? `${plural(mentioning, 'passage mentions', 'passages mention')} the subject; none state it.` : null,
        fixes: ['edit', 'remove', 'add_source'],
      };
    }
    case 'no_source': return { chip: 'Waiting for you', reason: 'No saved source mentions this.', detail: null, fixes: ['add_source', 'remove'] };
    case 'contradicted': {
      const against = item.support?.role === 'contradicts' ? item.support : null;
      return {
        chip: 'Says otherwise', reason: 'A source says otherwise.',
        detail: against ? `${against.source.title}: “${against.quote}”` : null,
        fixes: ['edit', 'accept'],
      };
    }
    case 'general_knowledge_proposed': return {
      chip: 'Common knowledge?', reason: 'No source states this, and it looks like common knowledge.',
      detail: 'Accept it as such, or add a source.', fixes: ['general_knowledge', 'add_source'],
    };
    case 'could_not_judge': return { chip: 'Could not check', reason: 'The checker could not give an answer for this one.', detail: null, fixes: ['accept', 'edit'] };
    case 'disputed': return {
      chip: 'Disputed', reason: 'An independent check disagrees with the first one.',
      detail: item.verifierCheck?.reason || null, fixes: ['accept', 'edit'],
    };
    default: return { chip: 'Waiting for you', reason: 'This claim is waiting for you.', detail: null, fixes: ['accept', 'edit'] };
  }
}

export function whyItWaits(item: ClaimReviewItemDto): WhyItWaits {
  const copy = verdictCopy(item);
  if (item.state.state !== 'reopened') return copy;
  return { ...copy, chip: 'Reopened', detail: [REOPENED[item.state.cause.cause], copy.detail].filter(Boolean).join(' ') || null };
}

export function resolvedChip(item: ClaimReviewItemDto): string {
  const state = item.state;
  switch (state.state) {
    case 'resolved_by_evidence': return 'Verified';
    case 'confirmed': return 'Confirmed by you';
    case 'removed': return state.by === 'user' ? 'Removed by you' : 'Removed';
    case 'edited': return state.successor ? 'Edited' : 'Removed by an edit';
    case 'general_knowledge': return 'Common knowledge';
    case 'unchecked':
    case 'open': return 'Being checked';
    case 'queued':
    case 'reopened': return whyItWaits(item).chip;
  }
}

export type FixGroup = 'accept_source' | 'needs_source' | 'says_otherwise' | 'common_knowledge' | 'could_not_check';

export const FIX_GROUP_LABELS: Record<FixGroup, string> = {
  accept_source: 'Accept a source',
  needs_source: 'Needs a source',
  says_otherwise: 'Says otherwise',
  common_knowledge: 'Common knowledge?',
  could_not_check: 'Could not check',
};

export const FIX_GROUP_ORDER: FixGroup[] = ['accept_source', 'needs_source', 'says_otherwise', 'common_knowledge', 'could_not_check'];

export function fixGroupOf(item: ClaimReviewItemDto): FixGroup {
  switch (item.verdictKind?.kind) {
    case 'insufficient_standing': return item.sufficiency?.missing === 'no_measurement_origin' ? 'needs_source' : 'accept_source';
    case 'contradicted':
    case 'disputed': return 'says_otherwise';
    case 'general_knowledge_proposed': return 'common_knowledge';
    case 'could_not_judge': return 'could_not_check';
    default: return 'needs_source';
  }
}

export function triedLine(tried: ResearchSummaryDto): string | null {
  if (tried.searches === 0 && tried.pagesRead === 0) return null;
  const searched = tried.searches > 0
    ? `Searched the web ${tried.searches === 1 ? 'once' : `${tried.searches} times`} and read ${plural(tried.pagesRead, 'page', 'pages')}`
    : `Read ${plural(tried.pagesRead, 'page', 'pages')}`;
  if (tried.stated) return `${searched}; one of them states it.`;
  if (tried.pagesMentioning > 0) return `${searched}; ${plural(tried.pagesMentioning, 'page mentions', 'pages mention')} it but ${tried.pagesMentioning === 1 ? 'does' : 'do'} not state it.`;
  return `${searched}; none stated it.`;
}

export function tallyLine(counts: ClaimCountsDto): string {
  const parts: Array<[number, string]> = [
    [counts.queued, counts.queued === 1 ? 'claim needs you' : 'claims need you'],
    [counts.verified, 'verified'],
    [counts.confirmed, 'confirmed by you'],
    [counts.generalKnowledge, 'common knowledge'],
    [counts.rewritten, 'rewritten by the machine'],
    [counts.removedByYou, 'removed by you'],
    [counts.editedByYou, 'edited by you'],
    [counts.open, 'being checked'],
  ];
  return parts.filter(([count]) => count > 0).map(([count, words]) => `${count} ${words}`).join(' · ');
}

export function jobTallyLine(tally: LearningClaimTally): string {
  const checking = tally.unchecked + tally.searching + tally.sentBack + tally.repairing;
  const parts: Array<[number, string]> = [
    [tally.waiting, tally.waiting === 1 ? 'claim needs you' : 'claims need you'],
    [tally.verified, 'verified'],
    [tally.resolvedByYou, 'resolved by you'],
    [tally.rewritten ?? 0, 'rewritten by the machine'],
    [checking, 'being checked'],
  ];
  return parts.filter(([count]) => count > 0).map(([count, words]) => `${count} ${words}`).join(' · ');
}

export function preparingLine(tally: LearningClaimTally | null | undefined): string {
  if (!tally) return 'Preparing';
  const total = tally.unchecked + tally.searching + tally.sentBack + tally.repairing + tally.waiting + tally.verified + tally.resolvedByYou;
  return total > 0 ? `Preparing · ${total - tally.unchecked} of ${total} checked` : 'Preparing';
}

export const claimsNeedYou = (count: number) => `${claimsCount(count)} need${count === 1 ? 's' : ''} you`;
export const teachingNotesForYou = (count: number) => `${plural(count, 'teaching note', 'teaching notes')} for you`;

export const ALL_RESOLVED = 'Everything is resolved. Resume verification to publish the lesson.';
const left = (remaining: number) => (remaining > 0 ? `${remaining} left.` : ALL_RESOLVED);

export type AnnouncedAction = 'confirm' | 'edit' | 'remove' | 'general_knowledge' | 'accept_all_general_knowledge' | 'research_with_hint' | 'unconfirm' | 'undo';

export function announcement(action: AnnouncedAction, remaining: number): string {
  switch (action) {
    case 'confirm': return `Accepted. ${left(remaining)}`;
    case 'edit': return 'Sentence edited. It is checked when you resume.';
    case 'remove': return `Removed. ${left(remaining)}`;
    case 'general_knowledge': return `Accepted as common knowledge. ${left(remaining)}`;
    case 'accept_all_general_knowledge': return `Common-knowledge proposals accepted. ${left(remaining)}`;
    case 'research_with_hint': return `Searched with your hint. ${left(remaining)}`;
    case 'unconfirm': return `Acceptance withdrawn. ${left(remaining)}`;
    case 'undo': return `Undone. ${left(remaining)}`;
  }
}

export function trustedAnnouncement(title: string, verified: number, stillWaiting: number): string {
  return `Trusted ${title} for this course: ${plural(verified, 'claim', 'claims')} verified, ${stillWaiting} still need${stillWaiting === 1 ? 's' : ''} you.`;
}

export function addSourceTitle(scope: 'claim' | 'group' | 'queue', count: number): string {
  if (scope === 'claim' || count <= 1) return 'Add a source for this claim';
  return scope === 'queue' ? `Add a source for all ${count} claims` : `Add a source for these ${count} claims`;
}

export const ADD_SOURCE_HINT = 'Drop a file or a link here, paste an address or text, or choose a file. A page you add is read as it is; if it says this, that settles it.';

export function progressLine(stage: { stage: 'fetching'; host: string | null } | { stage: 'saving' } | { stage: 'checking'; done: number; total: number; statement: string } | { stage: 'done' }): string {
  switch (stage.stage) {
    case 'fetching': return stage.host ? `Reading ${stage.host}…` : 'Reading it…';
    case 'saving': return 'Saving it to your sources';
    case 'checking': {
      const statement = stage.statement.length > 80 ? `${stage.statement.slice(0, 79)}…` : stage.statement;
      return `Checking claim ${Math.min(stage.done + 1, stage.total)} of ${stage.total}: ${statement}`;
    }
    case 'done': return 'Done';
  }
}

const FAILURES: Record<AttachFailureKindDto, (host: string) => string> = {
  refused: (host) => `${host} refused the request.`,
  paywall: (host) => `${host} shows a sign-in or subscription wall.`,
  thin: () => 'The page has almost no text to check.',
  browser_unavailable: () => 'The page needs a browser to read and none is available.',
  blocked: () => 'That address is private or local and cannot be fetched.',
  unreachable: (host) => `${host} could not be reached.`,
};

export function failureLine(kind: AttachFailureKindDto, host: string | null, message: string): string {
  const line = FAILURES[kind](host ?? 'The site');
  const status = kind === 'refused' ? /HTTP \d{3}/.exec(message)?.[0] : null;
  return status ? line.replace(/\.$/, ` (${status}).`) : line;
}

export type AttachSummary = {
  stated: boolean;
  notStated: boolean;
  line: string;
};

export function attachSummary(report: ClaimAttachReportDto, scopeSize: number, remaining: number): AttachSummary {
  const title = report.source?.title ?? 'the source';
  const outcome = report.outcome;
  const checked = outcome?.outcome === 'checked' ? outcome.check.result : null;
  const stated = Boolean(checked && checked.verdict !== 'failed');
  const notStated = checked?.verdict === 'failed';
  const noPassage = outcome?.outcome === 'no_passage';
  const others = report.others;
  if (scopeSize > 1) {
    const verified = (stated ? 1 : 0) + others.verified;
    const mismatched = (notStated ? 1 : 0) + others.notVerified;
    const unmentioned = (noPassage ? 1 : 0) + others.noPassage;
    const total = Math.max(scopeSize, 1 + others.tried);
    const done = verified + mismatched + unmentioned;
    const parts = [
      `Verified ${verified} of ${total} claims`,
      mismatched ? `${mismatched} ${mismatched === 1 ? 'does' : 'do'} not match what it says` : null,
      unmentioned ? `${unmentioned} ${unmentioned === 1 ? 'is' : 'are'} not mentioned by it` : null,
    ].filter(Boolean).join('; ');
    const head = report.alreadySaved ? `${title} was already one of your sources; the claims were checked against it.` : `Added ${title}.`;
    const stopped = others.stopped ? ` Checked ${done} of ${total}; the rest were not checked: ${others.stopped}. Add and check again to continue.` : '';
    return { stated, notStated, line: `${head} ${parts}.${stopped}` };
  }
  if (stated) return { stated, notStated, line: `Added ${title}; it states this. ${left(remaining)}` };
  if (notStated) return { stated, notStated, line: `Added ${title}; it does not say this. The closest passage is shown: edit the sentence to match it, or add another source.` };
  return { stated, notStated, line: `Nothing in ${title} mentions this claim. It is saved in your sources.` };
}

export function recheckLine(recheck: WaitingClaimsRecheckDto | null | undefined): string | null {
  if (!recheck || recheck.tried === 0) return null;
  const stopped = recheck.stopped ? ` The rest were not checked: ${recheck.stopped}.` : '';
  return `Checked ${recheck.tried} waiting ${recheck.tried === 1 ? 'claim' : 'claims'} against it: ${recheck.verified} verified.${stopped}`;
}
