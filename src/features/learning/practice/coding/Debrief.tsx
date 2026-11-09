import type * as Wire from '@/lib/bindings';

export interface DebriefProps {
  title: string;
  debrief: Wire.CodingDebriefDto;
  onExit: () => void;
  onTryInterview: () => void;
}

export function Debrief(_props: DebriefProps): JSX.Element | null {
  return null;
}
