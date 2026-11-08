export interface ChoiceOption {
  value: string;
  label: string;
  description?: string;
}

export interface ChoiceSlot {
  id: string;
  label: string;
  value: string;
  options: ChoiceOption[];
  ariaDescribedBy?: string;
}

export interface StartSentenceProps {
  programId: string;
  cardId: 'explain' | 'spot' | 'code' | 'mock' | 'role';
  slots: ChoiceSlot[];
  sentencePrefix?: string;
  sentenceSuffix?: string;
  onChange?: (slotId: string, value: string) => void;
  className?: string;
}

/** Stage 1 minimal non-panicking stub: returns null. */
export function StartSentence(_props: StartSentenceProps): JSX.Element | null {
  return null;
}
