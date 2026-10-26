import { MarkdownViewer } from '@/features/reading/components/viewers/MarkdownViewer';
import { cn } from '@/lib/utils';

/**
 * A review card's prompt, answer or explanation as written. Cards made from
 * a lesson's checks carry the lesson's Markdown (code, lists, emphasis;
 * learning science lessons §3.3), so they render like the lesson, never as
 * raw symbols.
 */
export function CardText({ text, className }: { text: string; className?: string }) {
  return <div className={cn('min-w-0 wrap-break-word', className)}><MarkdownViewer content={text} /></div>;
}

/**
 * One line of a card's text for a compact row: its first line of prose,
 * without Markdown syntax. A card that opens with code shows its first line
 * of code.
 */
export function cardPreview(text: string): string {
  const lines = text.split('\n');
  let inFence = false;
  let firstCode = '';
  for (const raw of lines) {
    const line = raw.trim();
    if (/^(```|~~~)/.test(line)) { inFence = !inFence; continue; }
    if (!line) continue;
    if (inFence) { firstCode ||= line; continue; }
    const prose = line
      .replace(/^#{1,6}\s+/, '')
      .replace(/^>\s?/, '')
      .replace(/^([-*+]|\d+[.)])\s+/, '');
    // Inline code is kept byte for byte (`get_item_by_id` is not emphasis),
    // and emphasis may still wrap it (**`code`**).
    const code: string[] = [];
    const held = prose.replace(/`([^`]+)`/g, (_, inner: string) => `\uE000${code.push(inner) - 1}\uE000`);
    return plain(held).replace(/\uE000(\d+)\uE000/g, (_, index: string) => code[Number(index)] ?? '');
  }
  return firstCode;
}

/** Prose without link and emphasis syntax. Underscores inside a word (snake_case) and a lone `*` (2 * 3) are text. */
function plain(text: string): string {
  return text
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\*\*(\S(?:.*?\S)?)\*\*/g, '$1')
    .replace(/\*(\S(?:.*?\S)?)\*/g, '$1')
    .replace(/(^|[^\p{L}\p{N}_])__(\S(?:.*?\S)?)__(?![\p{L}\p{N}_])/gu, '$1$2')
    .replace(/(^|[^\p{L}\p{N}_])_(\S(?:.*?\S)?)_(?![\p{L}\p{N}_])/gu, '$1$2');
}
