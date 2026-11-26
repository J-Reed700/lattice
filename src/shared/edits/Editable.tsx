import type { ReactElement, ReactNode } from 'react';

import type { ContentRef } from './types';

export interface EditableProps {
  content: ContentRef;
  label: string;
  path?: string;
  source?: string;
  sourceStart?: number;
  className?: string;
  children: ReactNode;
}

export function Editable({ children }: EditableProps): ReactElement {
  return <>{children}</>;
}
