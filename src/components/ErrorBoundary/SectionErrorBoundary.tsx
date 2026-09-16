/**
 * SectionErrorBoundary Component
 *
 * Error boundary for major application sections (Search, Files, Settings, etc.).
 * Isolates errors to prevent entire app crashes.
 */

import React from 'react';

import { ErrorBoundary, type ErrorBoundaryProps } from './ErrorBoundary';
import { SectionError } from './SectionError';

interface SectionErrorBoundaryProps {
  children: React.ReactNode;
  sectionName: string;
  icon?: React.ReactNode;
  onError?: ErrorBoundaryProps['onError'];
  onReset?: ErrorBoundaryProps['onReset'];
  resetKeys?: unknown[];
  fallback?: React.ComponentType<{
    error: Error;
    errorInfo?: React.ErrorInfo;
    resetError: () => void;
  }>;
}

export function SectionErrorBoundary({
  children,
  sectionName,
  icon,
  onError,
  onReset,
  resetKeys,
  fallback,
}: SectionErrorBoundaryProps) {
  // Default fallback component
  const DefaultFallback = fallback || (({ error, errorInfo, resetError }) => (
    <SectionError
      error={error}
      errorInfo={errorInfo}
      resetError={resetError}
      sectionName={sectionName}
      icon={icon}
    />
  ));

  const handleError: ErrorBoundaryProps['onError'] = (error, errorInfo) => {
    // Log to console in development
    const isDev = import.meta.env.DEV;
    if (isDev) {
      console.error(`=== ${sectionName.toUpperCase()} ERROR ===`);
      console.error('Error:', error);
      console.error('Error Info:', errorInfo);
      console.error('=========================');
    }

    if (onError) {
      onError(error, errorInfo);
    }
  };

  return (
    <ErrorBoundary
      name={`${sectionName}ErrorBoundary`}
      fallback={DefaultFallback}
      onError={handleError}
      onReset={onReset}
      resetKeys={resetKeys}
      resetOnPropsChange={false}
      isolate
    >
      {children}
    </ErrorBoundary>
  );
}

/**
 * Specific section error boundaries with predefined configurations
 */

export function SearchSectionErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <SectionErrorBoundary sectionName="Search">
      {children}
    </SectionErrorBoundary>
  );
}

export function FilesSectionErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <SectionErrorBoundary sectionName="Library">
      {children}
    </SectionErrorBoundary>
  );
}

export function QASectionErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <SectionErrorBoundary sectionName="Chat">
      {children}
    </SectionErrorBoundary>
  );
}

export function SettingsSectionErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <SectionErrorBoundary sectionName="Settings">
      {children}
    </SectionErrorBoundary>
  );
}

export function DailySectionErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <SectionErrorBoundary sectionName="Journal">
      {children}
    </SectionErrorBoundary>
  );
}
