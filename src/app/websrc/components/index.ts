/**
 * Component Library
 *
 * All application components organized by category
 */

// UI Components
export { Button } from './ui/button';
export { Input } from './ui/input';
export { Card, CardHeader, CardTitle, CardDescription, CardContent } from './ui/card';
export { Switch } from './ui/switch';
export { Select } from './ui/select';
export { Checkbox } from './ui/Checkbox';
export { Dialog } from './ui/dialog';
export { Toast, ToastContainer, useToast } from './ui/Toast';
export type { ToastType, ToastVariant } from './ui/Toast';
export { Badge } from './ui/badge';
export { Tabs, TabsList, TabsTrigger, TabsContent } from './ui/tabs';
export { ScrollArea } from './ui/ScrollArea';

// Layout & Utility
export { Layout } from './Layout';
export { ThemeToggle } from './ThemeToggle';
export { ErrorBoundary } from './ErrorBoundary';
export { ErrorToast } from './ErrorToast';

// Search Components
export { SearchInterface } from './SearchInterface';
export { SearchResult } from './SearchResult';
export { SearchBar } from './SearchBar';
export { SearchView } from './SearchView';
export { ResultsList } from './ResultsList';
export { DocumentViewer } from './DocumentViewer';
export type { DocumentViewerProps, FileData } from './DocumentViewer';

// Dashboard Components (NEW - v1.0)
export { Dashboard } from './Dashboard';
export { DashboardStats } from './Dashboard';
export { RecentActivity } from './Dashboard';
export { QuickActions } from './Dashboard';

// Panel Components
export { QAPanel } from './QAPanel';
export { BackupPanel } from './BackupPanel';
export { LogViewer } from './LogViewer';
export { IndexingPanel } from './IndexingPanel';
export { SummaryPanel } from './SummaryPanel';
export { StorageDashboard } from './StorageDashboard';

// Navigation Components
export { FileTree } from './FileTree';
export { FolderList } from './FolderList';
export { DocumentList } from './DocumentList';
export { CommandPalette } from './CommandPalette';
export { CommandItem } from './CommandItem';

// Modal/Dialog Components
export { KeyboardShortcutsModal } from './KeyboardShortcutsModal';
export { ShortcutsManager } from './ShortcutsManager';
export { ExportDialog } from './ExportDialog';
export { BatchSummarizeDialog } from './BatchSummarizeDialog';
export { Settings } from './Settings';

// Onboarding Components
export { WelcomeScreen } from './WelcomeScreen';
export { FirstFolderPicker } from './FirstFolderPicker';
export { QuickTour } from './QuickTour';
export { ModelDownloadScreen } from './ModelDownloadScreen';
export { ModelDownloadProgress } from './ModelDownloadProgress';

// Tag Components
export { TagBadge } from './TagBadge';
export { TagManager } from './TagManager';
export type { Tag } from './TagBadge';

// Miscellaneous Components
export { Upload } from './Upload';
export { IndexProgress } from './IndexProgress';
export { SummarizeButton } from './SummarizeButton';
export { SummarySettings } from './SummarySettings';

// State Components (NEW - v1.0 Completion)
export {
  EmptyState,
  NoSearchResults,
  NoFiles,
  NoTags,
  NoRecentDocuments,
  FirstTimeDaily,
  NoBacklinks,
} from './EmptyState';
export type { EmptyStateProps } from './EmptyState';

export {
  LoadingState,
  InlineSpinner,
  SkeletonCard,
  SkeletonList,
  FullPageLoading,
  SectionLoading,
  ButtonLoading,
} from './LoadingState';
export type { LoadingStateProps } from './LoadingState';

// User Feedback Components (NEW - v1.0 Completion)
export {
  ConfirmDialog,
  DeleteFileDialog,
  ClearDataDialog,
  ResetSettingsDialog,
} from './ConfirmDialog';
export type { ConfirmDialogProps } from './ConfirmDialog';

export { HelpOverlay } from './HelpOverlay';
export type { HelpOverlayProps } from './HelpOverlay';

// Recent Documents (NEW - v1.0 Completion)
export {
  RecentDocuments,
  addToRecentDocuments,
  clearRecentDocuments,
} from './RecentDocuments';
export type { RecentDocumentsProps } from './RecentDocuments';

// Header Component (NEW - v1.0 Completion)
export { Header, CompactHeader } from './Header';
export type { HeaderProps } from './Header';

// Favorites & Recent Documents System (NEW)
export { FavoriteButton } from './FavoriteButton';
export { FavoritesPanel } from './FavoritesPanel';
export { RecentDocumentsPanel } from './RecentDocumentsPanel';
