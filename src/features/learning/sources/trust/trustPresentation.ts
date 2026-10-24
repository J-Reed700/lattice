import type {
  LearningClaimEvidenceDto,
  LearningClaimTrustDto,
  LearningKindBarDto,
  LearningSiteStanding,
  LearningSourceRelationDto,
  LearningSourceTrustDto,
  LearningSufficiencyDto,
  LearningTrustImpactDto,
  LearningTrustLevel,
  LearningTrustRedecidedDto,
  LearningTrustOverrideDto,
} from '@/lib/bindings';

export const TRUST_LEVELS: readonly LearningTrustLevel[] = ['primary', 'authoritative', 'established', 'community', 'unknown', 'distrusted'];

export const SITE_STANDINGS: readonly LearningSiteStanding[] = ['institutional', 'identified', 'community', 'distrusted'];

export const LEVEL_WORD: Record<LearningTrustLevel, string> = {
  primary: 'Primary',
  authoritative: 'Authoritative',
  established: 'Established',
  community: 'Community',
  unknown: 'Unknown standing',
  distrusted: 'Distrusted',
};

/** One line each, for the level picker and the reasons popover. */
export const LEVEL_MEANING: Record<LearningTrustLevel, string> = {
  primary: 'The owner of the subject, or the record itself.',
  authoritative: 'An institution with an editorial or peer-review process.',
  established: 'An identified author or publisher.',
  community: 'A forum, wiki, Q&A site or other community page.',
  unknown: 'The publisher could not be identified.',
  distrusted: 'Excluded from every bar.',
};

export const STANDING_WORD: Record<LearningSiteStanding | 'unknown', string> = {
  institutional: 'Institutional',
  identified: 'Identified',
  community: 'Community',
  distrusted: 'Distrusted',
  unknown: 'Unknown',
};

export const STANDING_MEANING: Record<LearningSiteStanding, string> = {
  institutional: 'An organisation publishing on its own behalf.',
  identified: 'An identified person or small publisher.',
  community: 'A community platform: forum, wiki, Q&A.',
  distrusted: 'Never counted as support.',
};

const KIND_WORD: Record<string, string> = {
  specification: 'Specification',
  measurement: 'Measurement',
  version_bound: 'Version-bound',
  definition: 'Definition',
  practice: 'Practice',
  opinion: 'Opinion',
  historical: 'Historical',
  general: 'General',
};

/** What a claim of each kind asserts, in plain words. */
const KIND_PHRASE: Record<string, string> = {
  specification: 'how an API, standard or rule is defined',
  measurement: 'a number or measurement',
  version_bound: 'a claim tied to a version or date',
  definition: 'what a term means',
  practice: 'a recommendation',
  opinion: 'an evaluation',
  historical: 'a historical fact',
  general: 'a general statement',
};

export const levelRank = (level: LearningTrustLevel) => TRUST_LEVELS.length - TRUST_LEVELS.indexOf(level);

export const kindWord = (kind: string) => KIND_WORD[kind] ?? kind.replace(/_/g, ' ');

const plural = (count: number, one: string, many = `${one}s`) => `${count} ${count === 1 ? one : many}`;

function hasReason(trust: LearningSourceTrustDto, code: string) {
  return trust.reasons.some((reason) => reason.code === code);
}

export function badgeText(trust: LearningSourceTrustDto): string {
  if (trust.level === 'unknown' && hasReason(trust, 'added_by_you')) return 'Added by you · origin not stated';
  const relational = trust.level === 'primary' && trust.subject ? ` for ${trust.subject}` : '';
  const yours = trust.overrideScope || hasReason(trust, 'distrusted_by_you') ? ' (you)' : '';
  return `${LEVEL_WORD[trust.level]}${relational}${yours}`;
}

/** The copy relation a level was inherited through, verbatim, when one was. */
export function inheritedNote(trust: LearningSourceTrustDto): string | null {
  return trust.reasons.find((reason) => reason.code === 'mirror')?.text ?? null;
}

const RELATION_WORD: Record<LearningSourceRelationDto['kind'], string> = {
  copy_of: 'Copy of',
  contains_copy_of: 'Contains a copy of',
  archive_of: 'Archived copy of',
  canonical_of: 'Republished from',
  cites: 'Cites',
  cited_for_claim: 'Cited for a claim',
};

/** "Mirror of …", "Cites …", "Cited by …", read from this version's side. */
export function relationLine(relation: LearningSourceRelationDto, versionId: string, titleOf: (versionId: string) => string | null): string {
  const outbound = relation.fromVersionId === versionId;
  const other = outbound
    ? (relation.toVersionId ? titleOf(relation.toVersionId) ?? 'another saved source' : relation.toUrl ?? 'a page')
    : titleOf(relation.fromVersionId) ?? 'another saved source';
  if (outbound) return `${RELATION_WORD[relation.kind]} ${other}`;
  return relation.kind === 'cites' || relation.kind === 'cited_for_claim' ? `Cited by ${other}` : `${other} is a copy of this source`;
}

export function trackRecordLine(record: { right: number; wrong: number }): string | null {
  const total = record.right + record.wrong;
  if (total === 0) return null;
  return `Your verdicts: ${record.right} of ${plural(total, 'claim')} it supported held${record.wrong ? `, ${record.wrong} overturned` : ''}`;
}

/** "Trust learn.microsoft.com as primary for: .NET, C#" — one line per site and level. */
export function overrideLines(overrides: LearningTrustOverrideDto[]): string[] {
  const lines: string[] = [];
  const classes = new Map<string, string[]>();
  for (const entry of overrides) {
    if (entry.scope === 'source') lines.push(`This source set to ${LEVEL_WORD[entry.level].toLowerCase()} for this course`);
    else if (entry.scope === 'site') lines.push(`${entry.registrableDomain} treated as ${STANDING_WORD[entry.standing].toLowerCase()} everywhere`);
    else {
      const key = `${entry.registrableDomain}\n${entry.level}`;
      classes.set(key, [...(classes.get(key) ?? []), entry.class.label]);
    }
  }
  for (const [key, labels] of classes) {
    const [domain, level] = key.split('\n') as [string, LearningTrustLevel];
    lines.push(`Trust ${domain} as ${LEVEL_WORD[level].toLowerCase()} for: ${labels.join(', ')}`);
  }
  return lines;
}

export function impactLine(impact: LearningTrustImpactDto, target: 'source' | 'site'): string {
  const subject = target === 'source' ? 'this source' : 'this site';
  if (impact.claims === 0) return `No checked claim rests on ${subject}; later checks use the new level.`;
  return `${plural(impact.claims, 'checked claim')} in ${plural(impact.lessons, 'lesson')} ${impact.claims === 1 ? 'rests' : 'rest'} on ${subject}; ${impact.claims === 1 ? 'it' : 'they'} will be re-decided.`;
}

/** What re-deciding did after an override change: "2 claims verified · 1 reopened for you". */
export function redecidedLine(redecided: LearningTrustRedecidedDto): string {
  const parts = [
    redecided.verified ? `${plural(redecided.verified, 'claim')} verified` : null,
    redecided.reopened ? `${plural(redecided.reopened, 'claim')} reopened for you` : null,
    redecided.stillFailing ? `${plural(redecided.stillFailing, 'claim')} still short of the bar` : null,
    redecided.notLevelled ? `${plural(redecided.notLevelled, 'claim')} left as ${redecided.notLevelled === 1 ? 'it was' : 'they were'}: a source in ${redecided.notLevelled === 1 ? 'it' : 'them'} was never assessed` : null,
  ].filter(Boolean);
  return parts.length ? `${parts.join(' · ')}.` : 'No claim’s decision changed.';
}

/** The lowest level accepted alone for a kind now, in words. */
export function barLine(bar: LearningKindBarDto): string {
  const moved = bar.levels.some((level) => level.basis !== 'static');
  const calibrated = moved ? ' (moved by your verdicts)' : '';
  if (!bar.lowestAccepted) return `No single source is enough for ${KIND_PHRASE[bar.claimKind] ?? 'this kind of claim'} now; it needs independent sources that agree${calibrated}`;
  const accepted = TRUST_LEVELS.filter((level) => levelRank(level) >= levelRank(bar.lowestAccepted!));
  const words = accepted.map((level) => LEVEL_WORD[level].toLowerCase());
  const list = words.length > 1 ? `${words.slice(0, -1).join(', ')} or ${words[words.length - 1]}` : words[0];
  return `Accepted alone: a ${list} source${calibrated}`;
}

/** The best support level among a claim's supports, if any were levelled. */
export function bestLevel(trust: LearningClaimTrustDto): LearningTrustLevel | null {
  return bestOf(trust.supports.map((support) => support.trust.level));
}

/** The best of some levels, if any. */
export function bestOf(levels: readonly LearningTrustLevel[]): LearningTrustLevel | null {
  return levels.reduce<LearningTrustLevel | null>(
    (best, level) => (best === null || levelRank(level) > levelRank(best) ? level : best),
    null,
  );
}

/** How a claim accepted below primary is shown  "Label"). */
const LABEL_PHRASE: Record<string, string> = {
  practice: 'shown as a practice, per its source',
  attributed: 'shown as attributed to its source',
  community_consensus: 'shown as community consensus',
  reported_by: 'shown as reported by whoever measured it',
  secondary: 'per a secondary source, not the owner’s documentation',
  mirror: 'from a verified copy of the official text',
  best_available: 'per the course’s strongest source on this subject',
};

/** A published claim's kind, subject and the label it is shown with. */
export function sufficiencyNote(sufficiency: LearningSufficiencyDto): string {
  return [
    `${kindWord(sufficiency.claimKind)} claim${sufficiency.subject ? ` about ${sufficiency.subject}` : ''}`,
    sufficiency.label ? LABEL_PHRASE[sufficiency.label] ?? sufficiency.label.replace(/_/g, ' ') : null,
  ].filter(Boolean).join(' · ');
}

const AGAINST: Record<LearningTrustLevel, string> = {
  primary: 'primary sources',
  authoritative: 'authoritative sources',
  established: 'established sources',
  community: 'community sources',
  unknown: 'sources of unknown standing',
  distrusted: 'distrusted sources',
};

export function verifiedByLevel(claims: LearningClaimEvidenceDto[]): string[] {
  const counts = new Map<LearningTrustLevel, number>();
  let unweighed = 0;
  for (const claim of claims.filter((item) => item.resolution === 'verified')) {
    const levels = claim.passages.flatMap((passage) => (passage.trust ? [passage.trust.level] : []));
    if (!levels.length) {
      unweighed += 1;
      continue;
    }
    const best = levels.reduce((top, level) => (levelRank(level) > levelRank(top) ? level : top));
    counts.set(best, (counts.get(best) ?? 0) + 1);
  }
  if (counts.size === 0) return [];
  const parts = TRUST_LEVELS.filter((level) => counts.has(level)).map((level, index) => {
    const count = counts.get(level)!;
    return index === 0 ? `${plural(count, 'claim')} verified against ${AGAINST[level]}` : `${count} against ${AGAINST[level]}`;
  });
  if (unweighed) parts.push(`${unweighed} against sources not weighed for standing`);
  return parts;
}
