/**
 * ErrorBoundary Components - Central Export
 *
 * Comprehensive error boundary system for graceful error handling
 */

// Base error boundary
export { ErrorBoundary, withErrorBoundary } from './ErrorBoundary';
export type { ErrorBoundaryProps, ErrorFallbackProps } from './ErrorBoundary';

// Root-level error boundary
export { RootErrorBoundary } from './RootErrorBoundary';

// Section-level error boundaries
export {
  SectionErrorBoundary,
  SearchSectionErrorBoundary,
  FilesSectionErrorBoundary,
  QASectionErrorBoundary,
  SettingsSectionErrorBoundary,
  DailySectionErrorBoundary,
} from './SectionErrorBoundary';

// Fallback UI components
export { FullPageError } from './FullPageError';
export type { FullPageErrorProps } from './FullPageError';

export { SectionError } from './SectionError';
export type { SectionErrorProps } from './SectionError';

export { InlineError, CompactError } from './InlineError';
export type { InlineErrorProps } from './InlineError';

// Legacy exports for backwards compatibility
export {
  SearchErrorBoundary,
  FilesErrorBoundary,
  QAErrorBoundary,
  SettingsErrorBoundary,
} from './FeatureErrorBoundary';
