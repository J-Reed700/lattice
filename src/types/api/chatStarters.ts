/**
 * Corpus-derived chat starters.
 *
 * `starters` is empty whenever no LLM produced usable questions. The empty
 * state renders one plain line in that case and never invents a question.
 */
export type ChatStarter = import('../../lib/bindings').ChatStarterDto;

export type ChatStarters = import('../../lib/bindings').ChatStartersDto;
