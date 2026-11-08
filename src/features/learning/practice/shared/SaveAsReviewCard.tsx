export interface SaveAsReviewCardProps {
  programId: string;
  lessonId: string;
  promptText: string;
  promptLabel?: string;
  explanation?: string;
  initialAnswer?: string;
}

/**
 * Minimal Stage 1 non-panicking stub for SaveAsReviewCard.
 * Returns an empty container so assertions fail on expected elements.
 */
export function SaveAsReviewCard(_props: SaveAsReviewCardProps): JSX.Element {
  return <div />;
}
