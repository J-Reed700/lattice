/**
 * Skeleton Components
 *
 * Comprehensive skeleton loader system for the Vault desktop app.
 * Replaces spinners with content-aware loading states.
 *
 * Import CSS for animations:
 * import './skeleton.css';
 */

// Core skeleton components
// Import CSS animations
import './skeleton.css';

export {
  Skeleton,
  SkeletonText,
  SkeletonAvatar,
  SkeletonButton,
  SkeletonCard,
} from './Skeleton';

export type {
  SkeletonProps,
  SkeletonTextProps,
  SkeletonAvatarProps,
  SkeletonButtonProps,
} from './Skeleton';

// Specialized skeleton components
export {
  SearchResultSkeleton,
  SearchResultSkeletonCompact,
} from './SearchResultSkeleton';

export type { SearchResultSkeletonProps } from './SearchResultSkeleton';

export {
  DocumentViewerSkeleton,
  DocumentViewerSkeletonMinimal,
} from './DocumentViewerSkeleton';

export type { DocumentViewerSkeletonProps } from './DocumentViewerSkeleton';

export {
  FileBrowserSkeleton,
  FileBrowserSkeletonCompact,
} from './FileBrowserSkeleton';

export type { FileBrowserSkeletonProps } from './FileBrowserSkeleton';

export {
  SettingsSkeleton,
  SettingsSkeletonCompact,
  SettingsDialogSkeleton,
} from './SettingsSkeleton';

export type { SettingsSkeletonProps } from './SettingsSkeleton';

export {
  DashboardSkeleton,
  DashboardSkeletonCompact,
} from './DashboardSkeleton';

export type { DashboardSkeletonProps } from './DashboardSkeleton';
