import type { ReactNode } from 'react';

import type { RelationDto } from '@/lib/bindings';

export interface CitationCardProps {
  number: number;
  title: string;
  site?: string | null;
  relation?: RelationDto | null;
  /** The cited sentences, in the order they stand in the source. */
  excerpts: readonly string[];
  /** The saved passage, shown clamped when there are no excerpts. */
  passage?: string | null;
  /** The headings above the cited text, outermost first. */
  trail?: readonly string[];
  footer?: ReactNode;
  onOpen?: () => void;
}

/** The one card that says what a citation is: how it is used, where it is from, and the sentences behind it. */
export function CitationCard(_props: CitationCardProps) {
  return <div />;
}
