import { useState } from 'react';

import { Search, FolderPlus, Settings, FileText, Globe } from 'lucide-react';

import VaultAPI from '../../lib/api';
import Button from '../ui/Button/Button';
import { WebUrlDialog } from '../WebUrlDialog';

/**
 * QuickActions
 *
 * Purpose: Provide fast access to common Vault operations
 *
 * Features:
 * - Four primary actions (Search, Add Folder, View Files, Settings)
 * - Icon-based buttons with clear labels
 * - Keyboard shortcuts displayed
 * - Folder picker integration
 * - Two variants: normal (horizontal) and large (vertical grid)
 *
 * States: default, loading (for Add Folder)
 * Accessibility: WCAG AA, keyboard navigation, ARIA labels
 */

interface QuickActionsProps {
  onNavigate?: (view: 'search' | 'files' | 'settings') => void;
  variant?: 'normal' | 'large';
}

export const QuickActions = ({ onNavigate, variant = 'normal' }: QuickActionsProps) => {
  const [isWebUrlDialogOpen, setIsWebUrlDialogOpen] = useState(false);

  const handleAddFolder = async () => {
    const folderPath = await VaultAPI.selectFolder();
    if (folderPath) {
      const result = await VaultAPI.startIndexing(folderPath, true);
      if (result.ok) {
        // Refresh dashboard after indexing starts
        window.location.reload();
      }
    }
  };

  const handleWebUrlSuccess = () => {
    // Refresh dashboard after successful web ingestion
    window.location.reload();
  };

  const actions = [
    {
      label: 'Search Documents',
      icon: Search,
      onClick: () => onNavigate?.('search'),
      variant: 'primary' as const,
      shortcut: 'Ctrl+1',
      description: 'Find documents by content',
    },
    {
      label: 'Add Folder',
      icon: FolderPlus,
      onClick: handleAddFolder,
      variant: 'secondary' as const,
      shortcut: null,
      description: 'Index a new folder',
    },
    {
      label: 'Add Web URL',
      icon: Globe,
      onClick: () => setIsWebUrlDialogOpen(true),
      variant: 'secondary' as const,
      shortcut: null,
      description: 'Ingest web content',
    },
    {
      label: 'Browse Files',
      icon: FileText,
      onClick: () => onNavigate?.('files'),
      variant: 'secondary' as const,
      shortcut: 'Ctrl+2',
      description: 'View all indexed files',
    },
    {
      label: 'Settings',
      icon: Settings,
      onClick: () => onNavigate?.('settings'),
      variant: 'ghost' as const,
      shortcut: 'Ctrl+,',
      description: 'Configure Vault',
    },
  ];

  if (variant === 'large') {
    return (
      <>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 md:gap-4 max-w-2xl mx-auto">
          {actions.map((action, index) => {
            const Icon = action.icon;
            return (
              <button
                key={index}
                onClick={action.onClick}
                className="flex flex-col items-center justify-center p-4 md:p-6 min-h-[120px] bg-[hsl(var(--surface-raised))] rounded-lg border-2 border-[hsl(var(--border-subtle))] hover:border-[hsl(var(--accent))] hover:shadow-md transition-colors duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))] focus-visible:ring-offset-2"
                aria-label={action.label}
              >
                <div className="w-10 h-10 md:w-12 md:h-12 bg-[hsl(var(--accent-muted))]/20 rounded-full flex items-center justify-center mb-2 md:mb-3">
                  <Icon className="w-5 h-5 md:w-6 md:h-6 text-[hsl(var(--accent))]" />
                </div>
                <span className="text-base md:text-lg font-semibold text-[hsl(var(--text-primary))] mb-1">
                  {action.label}
                </span>
                <span className="text-xs md:text-sm text-[hsl(var(--text-secondary))] text-center">
                  {action.description}
                </span>
                {action.shortcut && (
                  <span className="mt-2 text-xs text-[hsl(var(--text-tertiary))] font-mono bg-[hsl(var(--bg))] px-2 py-1 rounded">
                    {action.shortcut}
                  </span>
                )}
              </button>
            );
          })}
        </div>

        <WebUrlDialog
          open={isWebUrlDialogOpen}
          onOpenChange={setIsWebUrlDialogOpen}
          onSuccess={handleWebUrlSuccess}
        />
      </>
    );
  }

  return (
    <>
      <div className="bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))] p-3 md:p-4">
        <h3 className="text-xs md:text-sm font-semibold text-[hsl(var(--text-primary))] mb-2 md:mb-3">Quick Actions</h3>
        <div className="flex flex-wrap gap-2 md:gap-3">
          {actions.map((action, index) => {
            const Icon = action.icon;
            return (
              <Button
                key={index}
                variant={action.variant}
                onClick={action.onClick}
                leftIcon={<Icon className="w-4 h-4" />}
                size="md"
                aria-label={action.label}
              >
                {action.label}
                {action.shortcut && (
                  <span className="ml-2 text-xs opacity-60 font-mono">
                    {action.shortcut}
                  </span>
                )}
              </Button>
            );
          })}
        </div>
      </div>

      <WebUrlDialog
        open={isWebUrlDialogOpen}
        onOpenChange={setIsWebUrlDialogOpen}
        onSuccess={handleWebUrlSuccess}
      />
    </>
  );
};

export default QuickActions;
