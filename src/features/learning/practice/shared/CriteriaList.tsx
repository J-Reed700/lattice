export type EvidenceView =
  | { kind: 'quote'; text: string }
  | { kind: 'lines'; path: string; start: number; end: number }
  | { kind: 'run'; label: string }
  | { kind: 'example'; label: string }
  | { kind: 'hint'; label: string }
  | { kind: 'text'; text: string };

export interface CriterionView {
  id: string;
  title: string;
  points: number | null;
  maxPoints: number;
  evidence: EvidenceView[];
  feedback?: string;
  nextStep?: string;
}

export function CriteriaList(_props: { criteria: CriterionView[]; heading?: string }) {
  return null;
}
