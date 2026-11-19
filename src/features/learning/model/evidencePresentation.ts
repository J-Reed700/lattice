import type { LearningCitationDto, LearningClaimEvidenceDto, LearningEvidencePassageDto, LearningLessonEvidenceDto, LearningOutlineCitationDto, LearningOutlineCitationTarget, LearningOutlineEvidenceDto } from '@/lib/bindings';
import type { ClaimVerdict } from '@/types/conversation';

export function passageKey(passage: LearningEvidencePassageDto): string {
  return `${passage.sourceVersionId}:${passage.startByte}:${passage.endByte}`;
}

/** How a source is named for the label the provenance gate gave the statement. */
const SOURCE_WORDS: Record<string, (source: string) => string> = {
  attributed: (source) => `According to ${source}`,
  practice: (source) => `A practice, per ${source}`,
  community_consensus: (source) => `Community consensus, per ${source}`,
  reported_by: (source) => `Reported by whoever measured it, per ${source}`,
  secondary: (source) => `Per ${source}, not the owner’s documentation`,
  mirror: (source) => `Per ${source}, a verified copy of the official text`,
  best_available: (source) => `Per ${source}, the strongest source the course has on this`,
};

function listed(items: string[]): string {
  if (items.length > 3) return `${items.slice(0, 2).join(', ')} and ${items.length - 2} more`;
  return items.length > 1 ? `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}` : items.join('');
}

/** How a statement was checked, naming its source: "According to <source>" for a Sourced statement the gate labelled so, "Consistent with <sources>" for a Reasoning one. A Common statement has no source to name. */
export function checkWords(claim: LearningClaimEvidenceDto, citations: ReadonlyMap<number, LearningCitationDto>): string | null {
  const cited = (claim.cites ?? []).flatMap((number) => citations.get(number)?.passage.title ?? []);
  if (claim.class === 'sourced') {
    const source = cited[0];
    if (!source) return null;
    const words = claim.label ? SOURCE_WORDS[claim.label] : undefined;
    return words ? words(source) : `Stated by ${source}`;
  }
  if (claim.class === 'reasoning') {
    const titles = [...new Set(cited)];
    return titles.length ? `Consistent with ${listed(titles)}` : null;
  }
  return null;
}

/** What a Sourced statement says beyond its passage, of one kind: the common knowledge or the lesson's own figures no passage states. */
export function partsOfKind(claim: LearningClaimEvidenceDto, kind: 'common' | 'own'): string[] {
  return (claim.parts ?? []).filter((part) => part.kind === kind).map((part) => part.part);
}

type ReportCitations = Pick<LearningLessonEvidenceDto, 'citations'> | null | undefined;

/** The report's passages by the number the lesson's chips carry. */
export function citationsByNumber(report: ReportCitations): Map<number, LearningCitationDto> {
  return new Map((report?.citations ?? []).map((citation) => [citation.number, citation]));
}

/** The number of each passage, by its saved version and span. */
export function citationNumbers(report: ReportCitations): Map<string, number> {
  return new Map((report?.citations ?? []).map((citation) => [passageKey(citation.passage), citation.number]));
}

/** How a statement is drawn in the lesson: confirmed and common statements are never marked as verified (§5.6). */
export function claimMarkVerdict(claim: LearningClaimEvidenceDto): ClaimVerdict['verdict'] {
  if (claim.resolution === 'verified') return claim.class === 'common' ? 'unverified' : 'supported';
  if (claim.resolution !== 'not_verified') return 'unverified';
  if (claim.unverifiedReason === 'contradicted') return 'contradicted';
  return claim.unverifiedReason === 'unsupported' ? 'unsupported' : 'unverified';
}

/** How the claim was settled, in words (§5.6): confirmed never reads as verified. */
export function resolutionText(claim: LearningClaimEvidenceDto): string {
  if (claim.resolution === 'not_verified') return 'Not verified';
  if (claim.resolution === 'confirmed_by_you') return 'Confirmed by you';
  if (claim.resolution === 'source_removed') return 'Source removed · review';
  if (claim.class === 'common') return 'Common knowledge · not checked against a source';
  if (claim.class === 'reasoning') return 'Consistent';
  return claim.verdict || 'checked';
}

export const UNSTATED_PARTS = [
  { kind: 'common', label: 'Common knowledge, not in the source: ' },
  { kind: 'own', label: "The lesson's own figures, not in the source: " },
] as const;

export function findOutlineCitation(
  evidence: LearningOutlineEvidenceDto | null | undefined,
  target: LearningOutlineCitationTarget,
  moduleId: string,
  visibleClaim: string,
  itemIndex?: number,
  lessonId?: string,
): LearningOutlineCitationDto | undefined {
  return evidence?.citations.find((citation) =>
    citation.target === target
    && citation.moduleId === moduleId
    && citation.claim.trim() === visibleClaim.trim()
    && (itemIndex === undefined || citation.itemIndex === itemIndex)
    && (lessonId === undefined || citation.lessonId === lessonId)
  );
}
