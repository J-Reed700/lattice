import type { LearningEvidencePassageDto, LearningLessonEvidenceDto, LearningOutlineCitationDto, LearningOutlineCitationTarget, LearningOutlineEvidenceDto } from '@/lib/bindings';

export function passageKey(passage: LearningEvidencePassageDto): string {
  return `${passage.sourceVersionId}:${passage.startByte}:${passage.endByte}`;
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
