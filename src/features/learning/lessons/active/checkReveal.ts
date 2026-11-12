/** The review card a check becomes, and the id that names it (learning science lessons §3.3). */
import type { LearningCheckDto, LearningRecallContentDto } from '@/lib/bindings';

/**
 * The review card a check becomes: the case is the prompt, the reveal's first
 * paragraph the answer, and the rest of the reasoning the explanation.
 */
export function checkCardContent({ question, reveal }: LearningCheckDto): LearningRecallContentDto {
  const [first = '', ...rest] = reveal.split(/\n\s*\n/);
  return {
    prompt: question.trim(),
    answer: first.trim(),
    explanation: rest.join('\n\n').trim(),
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
