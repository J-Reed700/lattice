import type * as Wire from '@/lib/bindings';

export interface CodingSessionProps {
  programId: string;
  sessionId: string;
  onExit: () => void;
  onTryInterview: (setting: Wire.CodingSetting) => void;
}

export function CodingSession(_props: CodingSessionProps): JSX.Element | null {
  return null;
}
