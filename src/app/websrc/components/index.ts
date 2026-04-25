/**
 * Component Library barrel.
 *
 * Re-exports the components that other call sites consume by name.
 * Components imported only by direct path are NOT listed here; the
 * barrel is for convenience, not completeness.
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

// Dashboard
export { Dashboard } from './Dashboard';

// Navigation Components
export { FileTree } from './FileTree';
export { FolderList } from './FolderList';
export { CommandPalette } from './CommandPalette';
export { CommandItem } from './CommandItem';

// Modal/Dialog Components
export { KeyboardShortcutsModal } from './KeyboardShortcutsModal';
export { Settings } from './Settings';

// Tag Components
export { TagBadge } from './TagBadge';
export { TagManager } from './TagManager';
export type { Tag } from './TagBadge';

// Misc
export { IndexProgress } from './IndexProgress';

// State Components
export { EmptyState } from './EmptyState';
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

// User Feedback Components
export {
  ConfirmDialog,
  DeleteFileDialog,
  ClearDataDialog,
  ResetSettingsDialog,
} from './ConfirmDialog';
export type { ConfirmDialogProps } from './ConfirmDialog';

// Header
export { Header } from './Header';
export type { HeaderProps } from './Header';
