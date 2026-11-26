import type { ArticleBlock } from '@/features/reading/model/articleBlocks';

/** The headings above `offset` of an article, outermost first, as the reader draws their words. */
export function headingTrail(_text: string, _blocks: readonly ArticleBlock[], _offset: number): string[] {
  return [];
}
