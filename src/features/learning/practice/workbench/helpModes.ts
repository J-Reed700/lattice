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

export const HELP_MODES: HelpMode[] = [];
export const HINTS: HintRung[] = [];
