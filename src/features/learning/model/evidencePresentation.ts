import type { LearningClaimEvidenceDto, LearningEvidencePassageDto, LearningLessonEvidenceDto, LearningOutlineCitationDto, LearningOutlineCitationTarget, LearningOutlineEvidenceDto } from '@/lib/bindings';

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

/** The passage a statement stands on: the one that states its first stated part, else the one holding its supporting quote, else the first that is a source. */
function standsOn(claim: LearningClaimEvidenceDto): LearningEvidencePassageDto | undefined {
  const stated = claim.parts?.find((part) => part.kind === 'stated' && part.sourceVersionId);
  const quote = claim.supportingQuote;
  return claim.passages.find((passage) => passage.sourceVersionId === stated?.sourceVersionId)
    ?? (quote ? claim.passages.find((passage) => passage.retrievalKind !== 'execution' && passage.text.includes(quote)) : undefined)
    ?? claim.passages.find((passage) => passage.retrievalKind !== 'execution');
}

function listed(items: string[]): string {
  if (items.length > 3) return `${items.slice(0, 2).join(', ')} and ${items.length - 2} more`;
  return items.length > 1 ? `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}` : items.join('');
}

/** How a statement was checked, naming its source: "According to <source>" for a Sourced statement the gate labelled so, "Consistent with <sources>" for a Reasoning one. A Common statement has no source to name. */
export function checkWords(claim: LearningClaimEvidenceDto): string | null {
  if (claim.class === 'sourced') {
    const source = standsOn(claim)?.title;
    if (!source) return null;
    const words = claim.label ? SOURCE_WORDS[claim.label] : undefined;
    return words ? words(source) : `Stated by ${source}`;
  }
  if (claim.class === 'reasoning') {
    const titles = [...new Set(claim.passages.filter((passage) => passage.retrievalKind !== 'execution').map((passage) => passage.title))];
    return titles.length ? `Consistent with ${listed(titles)}` : null;
  }
  return null;
}

/** What a Sourced statement says beyond its passage that is common knowledge: the parts no passage states. */
export function commonParts(claim: LearningClaimEvidenceDto): string[] {
  return (claim.parts ?? []).filter((part) => part.kind === 'common').map((part) => part.part);
}

/** One numbering scheme for whole-lesson and section-level evidence. */
export function passageNumbersFor(data: Pick<LearningLessonEvidenceDto, 'teachingClaims'> | null | undefined): Map<string, number> {
  const numbers = new Map<string, number>();
  for (const claim of data?.teachingClaims ?? []) {
    for (const passage of claim.passages) {
      const key = passageKey(passage);
      if (!numbers.has(key)) numbers.set(key, numbers.size + 1);
    }
  }
  return numbers;
}

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
