import { speechSentences } from '@/features/speech/model/speechText';

/** Sentences after the first gather up to this many characters, as read-aloud's do. */
const GATHER_CHARS = 220;

/**
 * Tells which sentences of a reply are ready to say while it is being written.
 * The last sentence seen is held back, since more of it may still be coming.
 */
export class ReplyReader {
  private said = 0;

  /** The sentences of `markdown`, the reply so far, not yet handed out. */
  update(markdown: string): string[] {
    const whole = speechSentences(markdown).slice(0, -1);
    const fresh = whole.slice(this.said);
    this.said += fresh.length;
    return fresh;
  }

  /** The rest of the finished reply. */
  finish(markdown: string): string[] {
    const fresh = speechSentences(markdown).slice(this.said);
    this.said += fresh.length;
    return fresh;
  }
}

/**
 * A reply's sentences on their way to the voice. The first goes alone so speech
 * starts as soon as it is ready; while the voice is busy the next ones queue up
 * and go together, as a long answer's do when it is read aloud.
 */
export class SpokenPieces implements AsyncIterable<string> {
  private queue: string[] = [];
  private closed = false;
  private wake: (() => void) | null = null;
  private first = true;

  push(sentences: readonly string[]) {
    if (this.closed || sentences.length === 0) return;
    this.queue.push(...sentences);
    this.wake?.();
  }

  /** Nothing more is coming; what is queued still gets said. */
  close() {
    this.closed = true;
    this.wake?.();
  }

  /** Nothing more is coming and what is queued is dropped. */
  abandon() {
    this.queue = [];
    this.close();
  }

  private next(): string {
    let piece = this.queue.shift() ?? '';
    if (this.first) {
      this.first = false;
      return piece;
    }
    while (this.queue.length > 0 && piece.length + this.queue[0].length + 1 <= GATHER_CHARS) {
      piece = `${piece} ${this.queue.shift() ?? ''}`;
    }
    return piece;
  }

  async *[Symbol.asyncIterator](): AsyncGenerator<string> {
    for (;;) {
      if (this.queue.length > 0) {
        yield this.next();
      } else if (this.closed) {
        return;
      } else {
        await new Promise<void>((resolve) => {
          this.wake = resolve;
        });
        this.wake = null;
      }
    }
  }
}
