/**
 * Corpus-derived chat starters (BRIEF rank 11, contract §4.7).
 *
 * `starters` is empty whenever no LLM produced usable questions. The empty
 * state renders one plain line in that case and never invents a question.
 */
export interface ChatStarter {
  question: string;
  hint?: string;
}

export interface ChatStarters {
  fingerprint: string;
  generatedAt: string;
  starters: ChatStarter[];
  documentCount: number;
}
