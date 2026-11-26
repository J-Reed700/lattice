import type { ArticleBlock } from '@/features/reading/model/articleBlocks';
import { inlineRuns } from '@/features/reading/model/inlineMarkdown';

/** The headings above `offset` of an article, outermost first, as the reader draws their words. */
export function headingTrail(text: string, blocks: readonly ArticleBlock[], offset: number): string[] {
  const stack: { level: number; words: string }[] = [];
  for (const block of blocks) {
    if (block.start > offset) break;
    if (block.kind !== 'heading') continue;
    const words = inlineRuns(text, block.start, block.end)
      .map((run) => run.text)
      .join('')
      .trim();
    if (!words) continue;
    const level = block.level ?? 1;
    while (stack.length > 0 && stack[stack.length - 1]!.level >= level) stack.pop();
    stack.push({ level, words });
  }
  return stack.map((heading) => heading.words);
}
