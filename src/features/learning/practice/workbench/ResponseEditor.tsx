import type { UseAutosaveResult } from '@/features/learning/practice/shared/useAutosave';

export interface ResponseEditorProps {
  text: string;
  dirty: boolean;
  active: boolean;
  autosave: UseAutosaveResult;
  actionError: string;
  onRetryAction: (() => void) | undefined;
  conflict: boolean;
  busy: boolean;
  submitting: boolean;
  onChange: (value: string) => void;
  onReload: () => void;
  onSubmit: () => void;
}

export function ResponseEditor(_props: ResponseEditorProps) {
  return null;
}
