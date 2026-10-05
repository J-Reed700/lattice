/**
 * "Is the caret in the middle of a `/command` or an `@document`?", answered
 * from the composer's plain text and the caret offset.
 *
 * The composer stays a `<textarea>`, so there is no document model to ask — the
 * text before the caret is the whole truth. Kept pure and separate from the
 * popup so the rules are testable without a DOM.
 */

export type SuggestKind = 'slash' | 'mention';

export interface SuggestTrigger {
  kind: SuggestKind;
  /** What has been typed after the trigger character; '' the moment it is typed. */
  query: string;
  /** Index of the trigger character itself. */
  start: number;
  /** Index just past the query — where the caret is. */
  end: number;
}

/** A `/` at the start of a word, with up to 24 more non-space characters. */
const SLASH = /(?:^|\s)\/([^\s/]{0,24})$/;
/** An `@` at the start of a word. File names have spaces; the query may not. */
const MENTION = /(?:^|\s)@([^\s@]{0,48})$/;

/**
 * The trigger the caret sits in, or null. A caret with a selection behind it
 * is not typing a command, so callers pass the selection start and end alike
 * and get null when they differ.
 */
export function readTrigger(
  value: string,
  caret: number,
  selectionEnd: number = caret
): SuggestTrigger | null {
  if (caret !== selectionEnd) return null;
  if (caret < 0 || caret > value.length) return null;

  const before = value.slice(0, caret);

  const mention = MENTION.exec(before);
  if (mention) {
    const query = mention[1];
    return { kind: 'mention', query, start: caret - query.length - 1, end: caret };
  }

  const slash = SLASH.exec(before);
  if (slash) {
    const query = slash[1];
    return { kind: 'slash', query, start: caret - query.length - 1, end: caret };
  }

  return null;
}

/**
 * Take the typed token back out. Accepting a command leaves no `/deep` behind
 * to be sent as a question, and accepting a document leaves no `@halv`.
 */
export function replaceTrigger(
  value: string,
  trigger: SuggestTrigger,
  replacement: string
): { value: string; caret: number } {
  const next = value.slice(0, trigger.start) + replacement + value.slice(trigger.end);
  return { value: next, caret: trigger.start + replacement.length };
}
