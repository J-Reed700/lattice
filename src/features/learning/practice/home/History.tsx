export type HistoryFilter = 'all' | 'explain' | 'spot' | 'code' | 'mock' | 'role';

export interface HistoryItem {
  id: string;
  when: string;
  timestamp?: number;
  what: string;
  kind: 'explain' | 'spot' | 'code' | 'mock' | 'role' | string;
  kindLabel?: string;
  settingOrHelp: string;
  result: string;
  resultBadge?: string;
  details?: string;
}

export interface HistoryProps {
  programId: string;
  items?: HistoryItem[];
  onOpenItem?: (item: HistoryItem) => void;
  className?: string;
}

/** Stage 1 minimal non-panicking stub: returns null. */
export function History(_props: HistoryProps) {
  return null;
}
