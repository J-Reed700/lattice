import React from 'react';

import { SectionErrorBoundary } from './SectionErrorBoundary';

export function StudioSectionErrorBoundary({ children }: { children: React.ReactNode }) {
  return <SectionErrorBoundary sectionName="Studio">{children}</SectionErrorBoundary>;
}
