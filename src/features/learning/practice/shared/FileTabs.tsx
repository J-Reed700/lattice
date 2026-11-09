import type React from 'react';

export interface FileTabItem {
  path: string;
  label?: string;
  badge?: string | null;
  readOnly?: boolean;
}

export interface FileTabsProps {
  files: FileTabItem[];
  activePath: string;
  onSelect: (path: string) => void;
  actions?: React.ReactNode;
  className?: string;
}

export function FileTabs(_props: FileTabsProps) {
  return <></>;
}
