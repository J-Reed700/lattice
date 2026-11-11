import type * as Wire from '@/lib/bindings';

export type HelpLabel = 'Full help' | 'Hints only' | 'No help';

export interface HelpMode {
  id: Wire.LearningPracticeMode;
  help: HelpLabel;
  description: string;
}

export interface HintRung {
  id: Wire.LearningPracticeHintLevel;
  title: string;
  prompt: string;
}

const HELP_WORDS: Record<Wire.LearningPracticeMode, HelpLabel> = { explore: 'Full help', practice: 'Hints only', demonstrate: 'No help' };

const DESCRIPTIONS: Record<Wire.LearningPracticeMode, string> = {
  explore: 'Ask questions, use any hint, and reveal the worked solution after a deliberate confirmation.',
  practice: 'Ask questions and use scaffolded hints. Worked explanations and the solution stay closed.',
  demonstrate: 'Write and submit independently. Source opening, tutor hints, and solutions are closed during the attempt.',
};

export const HELP_MODES: HelpMode[] = (['explore', 'practice', 'demonstrate'] as const).map((id) => ({
  id,
  help: HELP_WORDS[id],
  description: DESCRIPTIONS[id],
}));

export const HINTS: HintRung[] = [
  { id: 'orienting_question', title: 'Orienting question', prompt: 'Give me an orienting question that helps me notice where to begin, without giving away the answer.' },
  { id: 'concept_or_source', title: 'Concept or source', prompt: 'Point me toward a relevant concept or exact saved source, without solving the task.' },
  { id: 'partial_strategy', title: 'Partial strategy', prompt: 'Offer one partial strategy step, leaving the key reasoning for me.' },
  { id: 'worked_explanation', title: 'Worked explanation', prompt: 'Give a worked explanation. Clearly label it as substantial assistance.' },
];

export const helpWord = (id: Wire.LearningPracticeMode): HelpLabel => HELP_WORDS[id];
export const helpDescription = (id: Wire.LearningPracticeMode): string => DESCRIPTIONS[id];
