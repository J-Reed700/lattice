/**
 * ProgressProvider Component
 *
 * Provides progress tracking functionality to the app.
 * Includes ProgressManager overlay and ProgressNotifications.
 * Automatically listens to Tauri progress events.
 */

import { type ReactNode } from 'react';

import { ProgressManager } from './ProgressManager';
import { ProgressNotifications } from './ProgressNotification';
import { useProgressListener } from '../../hooks/useProgressListener';
import './progress.css';

export interface ProgressProviderProps {
  children: ReactNode;
}

export function ProgressProvider({ children }: ProgressProviderProps) {
  // Listen to all progress event types
  useProgressListener({
    types: ['upload', 'indexing', 'search', 'export', 'ocr'],
    autoCreate: true,
  });

  return (
    <>
      {children}
      <ProgressManager />
      <ProgressNotifications position="top-right" />
    </>
  );
}
