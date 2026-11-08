type Props = {
  programId: string;
  lessonId: string;
  /** Zero-based. */
  questionIndex: number;
  open: boolean;
  onClose: () => void;
  /** Called after a save, before the editor closes. */
  onSaved?: () => void;
};

/** Edits one question of a published lesson: its prompt, choices, right answer and explanation. */
export function LessonQuestionEditor(_props: Props) {
  return null;
}
