import type React from 'react';

export type CriterionState = 'met' | 'partial' | 'missed' | 'insufficient_evidence' | 'pending';

export interface CriterionItem {
  id: string;
  title: string;
  description?: string;
  tag?: string;
  state: CriterionState;
  points?: number | null;
  maxPoints?: number | null;
  observation?: string | null;
  quote?: string | null;
  nextStep?: string | null;
}

export interface CriteriaListProps {
  title?: string;
  subtitle?: string;
  items: CriterionItem[];
  footer?: React.ReactNode;
  className?: string;
}

export function CriteriaList(_props: CriteriaListProps): JSX.Element {
  return <></>;
}
