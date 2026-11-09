import type React from 'react';

export interface ThreadMessage {
  id: string;
  speaker: 'learner' | 'counterpart' | 'coach';
  content: string;
  lines?: number[];
  name?: string;
}

export interface PracticeThreadProps {
  label: string;
  messages: ThreadMessage[];
  onSend: (content: string, spoken: boolean) => Promise<void>;
  busy?: string | null;
  error?: { message: string; retry?: () => void } | null;
  placeholder?: string;
  draftKey?: string;
  voice?: boolean;
  suggestion?: string | null;
  disabled?: { reason: string } | null;
  renderMessage?: (message: ThreadMessage) => React.ReactNode;
  composerTop?: React.ReactNode;
}

export function PracticeThread(_props: PracticeThreadProps) {
  return null;
}
