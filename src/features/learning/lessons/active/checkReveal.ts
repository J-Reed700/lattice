/**
 * A check section's two halves and the review card it becomes (learning
 * science lessons §3.3). The split mirrors `split_check` in
 * `lessons/generation/lesson_rules.rs`: exactly one line that is the heading
 * "Check your thinking" (level two to four) separates the case the learner
 * reasons through from the answer and reasoning revealed after they commit.
 */
import type { LearningRecallContentDto } from '@/lib/bindings';

export type CheckParts = { question: string; reveal: string };

const REVEAL_LINE = /^\s*#{2,4}\s*check your thinking\s*$/i;

/** The case and its reveal, or `null` when the body is not a well-formed check. */
export function splitCheck(body: string): CheckParts | null {
  const lines = body.split('\n');
  const reveals = lines.flatMap((line, index) => (REVEAL_LINE.test(line) ? [index] : []));
  if (reveals.length !== 1) return null;
  const at = reveals[0];
  const question = lines.slice(0, at).join('\n').trim();
  const reveal = lines.slice(at + 1).join('\n').trim();
  return question && reveal ? { question, reveal } : null;
}

/**
 * `text` within `limit` characters, cut at the last sentence or line end that
 * fits, so a card never stops mid-word. The recall card's own bounds
 * (`recall_content.rs`) are 2,000 for a prompt, 1,000 for an answer and
 * 3,000 for an explanation.
 */
export function clip(text: string, limit: number): string {
  const trimmed = text.trim();
  if (trimmed.length <= limit) return trimmed;
  const room = trimmed.slice(0, limit - 1);
  const end = Math.max(room.lastIndexOf('. '), room.lastIndexOf('\n'), room.lastIndexOf('? '), room.lastIndexOf('! '));
  return `${(end > limit / 2 ? room.slice(0, end + 1) : room).trimEnd()}…`;
}

/**
 * The review card a check becomes: the case is the prompt, the reveal's first
 * paragraph the answer, and the rest of the reasoning the explanation.
 */
export function checkCardContent({ question, reveal }: CheckParts): LearningRecallContentDto {
  const [first = '', ...rest] = reveal.split(/\n\s*\n/);
  return {
    prompt: clip(question, 2000),
    answer: clip(first, 1000),
    explanation: clip(rest.join('\n\n'), 3000),
    options: [],
    correctOptionIndex: null,
    language: null,
    clozeDeletions: [],
  };
}

/** Lattice's namespace for check cards: a fixed, random UUID. */
const CHECK_NAMESPACE = '6f1d3c52-2b8e-4c1a-9a43-7d0e8f5b2c61';

function bytesOf(uuid: string): Uint8Array {
  const hex = uuid.replace(/-/g, '');
  return Uint8Array.from({ length: 16 }, (_, index) => parseInt(hex.slice(index * 2, index * 2 + 2), 16));
}

/**
 * The check's review card id: a name-based (version 5) UUID of the lesson,
 * the section and its text. The same check always names the same card, so
 * answering it again finds the card it already made; a rewritten check names
 * a new one.
 */
export async function checkCardId(lessonId: string, section: number, body: string): Promise<string> {
  const name = new TextEncoder().encode(`${lessonId}\n${section}\n${body}`);
  const input = new Uint8Array(16 + name.length);
  input.set(bytesOf(CHECK_NAMESPACE));
  input.set(name, 16);
  const hash = new Uint8Array(await globalThis.crypto.subtle.digest('SHA-1', input)).slice(0, 16);
  hash[6] = (hash[6] & 0x0f) | 0x50;
  hash[8] = (hash[8] & 0x3f) | 0x80;
  const hex = Array.from(hash, (byte) => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
