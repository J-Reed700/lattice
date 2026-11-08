import type React from 'react';

export interface PracticeThreadTurn {
  id: string;
  speaker: 'learner' | 'counterpart' | 'coach';
  speakerLabel: string;
  content: string;
  createdAt?: string | number | null;
  badge?: string | null;
}

export interface PracticeThreadProps {
  conversationId?: string | null;
  turns: PracticeThreadTurn[];
  draftKey: string | null;
  placeholder?: string;
  sendLabel?: string;
  busyLabel?: string | null;
  error?: string | null;
  onRetry?: () => void;
  disabled?: boolean;
  disabledReason?: string | null;
  onSend: (content: string, spoken?: boolean) => Promise<void> | void;
  emptyText?: string;
  header?: React.ReactNode;
  footerActions?: React.ReactNode;
}

/**
 * Minimal Stage 1 non-panicking stub for PracticeThread.
 * Returns an empty container so assertions fail on expected elements.
 */
export function PracticeThread(_props: PracticeThreadProps): JSX.Element {
  return <div />;
}
