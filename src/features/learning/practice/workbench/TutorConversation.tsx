import type { PracticeBusyState } from '@/features/learning/practice/usePracticeSession';
import type { HintRung } from '@/features/learning/practice/workbench/helpModes';
import type * as Wire from '@/lib/bindings';

type TutorTurn = Wire.LearningPracticeSessionDto['tutorTurns'][number];

export interface TutorConversationProps {
  sessionId: string;
  turns: TutorTurn[];
  mode: Wire.LearningPracticeMode;
  active: boolean;
  canTutor: boolean;
  busyState: PracticeBusyState;
  busy: boolean;
  hintUsed: Wire.LearningPracticeHintLevel[];
  showCitations: boolean;
  sourceOpeningAllowed: boolean;
  onAsk: (kind: 'question' | 'critique', prompt: string) => Promise<boolean>;
  onHint: (hint: HintRung) => Promise<boolean>;
  onOpenCitation: (citation: TutorTurn['citations'][number]) => void;
}

export function TutorConversation(_props: TutorConversationProps) {
  return null;
}
