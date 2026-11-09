import type * as Wire from '@/lib/bindings';

export type HelpLabel = 'Full help' | 'Hints only' | 'No help';

export interface HelpMode {
  id: Wire.LearningPracticeMode;
  title: string;
  help: HelpLabel;
  description: string;
}

export interface HintRung {
  id: Wire.LearningPracticeHintLevel;
  title: string;
  prompt: string;
}

export const HELP_MODES: HelpMode[] = [
  { id: 'explore', title: 'Explore', help: 'Full help', description: 'Ask questions, use any hint, and reveal the worked solution after a deliberate confirmation.' },
  { id: 'practice', title: 'Practice', help: 'Hints only', description: 'Ask questions and use scaffolded hints. Worked explanations and the solution stay closed.' },
  { id: 'demonstrate', title: 'Demonstrate', help: 'No help', description: 'Write and submit independently. Source opening, tutor hints, and solutions are closed during the attempt.' },
];

export const HINTS: HintRung[] = [
  { id: 'orienting_question', title: 'Orienting question', prompt: 'Give me an orienting question that helps me notice where to begin, without giving away the answer.' },
  { id: 'concept_or_source', title: 'Concept or source', prompt: 'Point me toward a relevant concept or exact saved source, without solving the task.' },
  { id: 'partial_strategy', title: 'Partial strategy', prompt: 'Offer one partial strategy step, leaving the key reasoning for me.' },
  { id: 'worked_explanation', title: 'Worked explanation', prompt: 'Give a worked explanation. Clearly label it as substantial assistance.' },
];

export const helpModeOf = (id: Wire.LearningPracticeMode): HelpMode | undefined => HELP_MODES.find((mode) => mode.id === id);
