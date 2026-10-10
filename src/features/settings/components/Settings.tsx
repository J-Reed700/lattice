/**
 * Settings — shell.
 *
 * Icon-led navigation and grouped settings in a responsive reading column.
 */

import { useEffect, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { save, open } from '@tauri-apps/plugin-dialog';
import { writeTextFile, readTextFile } from '@tauri-apps/plugin-fs';
import { Archive, Boxes, ChevronRight, Download, FileText, FolderOpen, Layers3, MessageCircle, Palette, RotateCcw, ScanLine, Search, ShieldCheck, SlidersHorizontal, Upload, Wrench, type LucideIcon } from 'lucide-react';
import { useSearchParams } from 'react-router';

import { SidebarHeader } from '@/components/ui';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import VaultAPI from '@/lib/api';
import { cn } from '@/lib/utils';
import { toast } from '@/stores/toastStore';

import { AIModelsTab } from './AIModelsTab';
import { ChatTab, ModelsTab, PromptsTab, TuningTab, ToolsTab, LlmSettingsProvider } from './AITab';
import { DisplayTab } from './DisplayTab';
import { IndexingTab } from './IndexingTab';
import { LogsTab } from './LogsTab';
import { PrivacyTab } from './PrivacyTab';
import { SearchTab } from './SearchTab';
import { SpacesTab } from './SpacesTab';
import { VaultTab } from './VaultTab';
import { SETTINGS_QUERY_KEY } from '../hooks/useSettingsQuery';
import './settings.css';

type SettingsTab =
  | 'search'
  | 'indexing'
  | 'vault'
  | 'spaces'
  | 'chat'
  | 'models'
  | 'downloaded-models'
  | 'prompts'
  | 'tuning'
  | 'tools'
  | 'display'
  | 'privacy'
  | 'logs';

interface Tab {
  id: SettingsTab;
  label: string;
  component: React.ComponentType;
  icon: LucideIcon;
}

type TabGroup = {
  label: string;
  tabs: Tab[];
};

const tabGroups: TabGroup[] = [
  {
    label: 'General',
    tabs: [
      { id: 'search', label: 'Search', icon: Search, component: SearchTab },
      { id: 'indexing', label: 'Indexing', icon: ScanLine, component: IndexingTab },
      { id: 'vault', label: 'Vault', icon: FolderOpen, component: VaultTab },
      { id: 'spaces', label: 'Spaces', icon: Layers3, component: SpacesTab },
      { id: 'display', label: 'Display', icon: Palette, component: DisplayTab },
      { id: 'privacy', label: 'Privacy', icon: ShieldCheck, component: PrivacyTab },
      { id: 'logs', label: 'Logs', icon: FileText, component: LogsTab },
    ],
  },
  {
    label: 'AI',
    tabs: [
      { id: 'chat', label: 'Chat', icon: MessageCircle, component: ChatTab },
      { id: 'models', label: 'Models', icon: Boxes, component: ModelsTab },
      { id: 'downloaded-models', label: 'Downloaded', icon: Download, component: AIModelsTab },
      { id: 'prompts', label: 'Prompts', icon: FileText, component: PromptsTab },
      { id: 'tuning', label: 'Tuning', icon: SlidersHorizontal, component: TuningTab },
      { id: 'tools', label: 'Tools', icon: Wrench, component: ToolsTab },
    ],
  },
];

const tabs: Tab[] = tabGroups.flatMap((group) => group.tabs);
const tabIds = new Set<string>(tabs.map((tab) => tab.id));

const footerButtonClass =
  'flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-xs text-text-secondary transition-colors duration-fast hover:bg-surface-raised hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50';

export function Settings() {
  const [searchParams] = useSearchParams();
  const requestedTab = searchParams.get('tab');
  const [activeTab, setActiveTab] = useState<SettingsTab>(() =>
    requestedTab && tabIds.has(requestedTab) ? requestedTab as SettingsTab : 'search'
  );
  useEffect(() => {
    if (requestedTab && tabIds.has(requestedTab)) setActiveTab(requestedTab as SettingsTab);
  }, [requestedTab]);
  const [showResetDialog, setShowResetDialog] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [isResetting, setIsResetting] = useState(false);

  const queryClient = useQueryClient();

  // Other panes ask to jump here (e.g. "Browse catalog" from an empty
  // Downloaded list). Without this the request went nowhere.
  useEffect(() => {
    const handleNavigate = (event: Event) => {
      const detail = (event as CustomEvent<{ tab?: string }>).detail;
      if (detail?.tab && tabIds.has(detail.tab)) {
        setActiveTab(detail.tab as SettingsTab);
      }
    };
    window.addEventListener('settings:navigate-tab', handleNavigate);
    return () => window.removeEventListener('settings:navigate-tab', handleNavigate);
  }, []);

  // Invalidate React Query caches whenever settings change on the backend
  // (reset/import). Tabs that consume that cache refetch automatically.
  const invalidateSettingsCaches = () => {
    void queryClient.invalidateQueries({ queryKey: SETTINGS_QUERY_KEY });
  };

  const handleReset = async () => {
    setIsResetting(true);
    const result = await VaultAPI.resetSettings();
    setIsResetting(false);
    setShowResetDialog(false);

    if (result.ok) {
      invalidateSettingsCaches();
      toast.success('Settings reset to defaults', { duration: 3000 });
    } else {
      toast.error("Couldn't reset settings", { message: result.error, duration: 5000 });
    }
  };

  const handleExport = async () => {
    setIsExporting(true);
    try {
      const path = await save({
        defaultPath: 'lattice-settings.json',
        filters: [
          {
            name: 'JSON',
            extensions: ['json'],
          },
        ],
      });

      if (path) {
        const result = await VaultAPI.exportSettings();
        if (!result.ok) {
          throw new Error(result.error);
        }
        await writeTextFile(path, result.data);
        toast.success('Settings exported successfully', { duration: 3000 });
      }
    } catch (error) {
      console.error("Couldn't export settings:", error);
      toast.error("Couldn't export settings", { message: String(error), duration: 5000 });
    } finally {
      setIsExporting(false);
    }
  };

  const handleImport = async () => {
    setIsImporting(true);
    try {
      const path = await open({
        multiple: false,
        filters: [
          {
            name: 'JSON',
            extensions: ['json'],
          },
        ],
      });

      if (path && typeof path === 'string') {
        const json = await readTextFile(path);
        const result = await VaultAPI.importSettings(json, false);

        if (result.ok) {
          invalidateSettingsCaches();
          toast.success('Settings imported successfully', { duration: 3000 });
        } else {
          toast.error('Invalid settings file', { message: result.error, duration: 5000 });
        }
      }
    } catch (error) {
      console.error("Couldn't import settings:", error);
      toast.error("Couldn't import settings", { message: String(error), duration: 5000 });
    } finally {
      setIsImporting(false);
    }
  };

  const ActiveTabComponent = tabs.find((tab) => tab.id === activeTab)?.component || SearchTab;
  const isWideContentTab =
    activeTab === 'models' || activeTab === 'downloaded-models' || activeTab === 'tools';

  return (
    <div className="settings-workspace flex h-full min-w-0 bg-bg">
      {/* Sidebar */}
      <div className="settings-navigation flex w-[208px] shrink-0 flex-col border-r border-border-subtle bg-surface-sunken">
        <SidebarHeader title="Settings" />

        {/* Tab list — scrolls so the footer never clips it */}
        <nav aria-label="Settings sections" className="min-h-0 flex-1 overflow-y-auto px-2 py-3">
          {tabGroups.map((group) => (
            <div key={group.label} className="mb-4 last:mb-0">
              <div className="px-3 pb-2 text-xxs font-semibold uppercase tracking-[0.1em] text-text-muted">
                {group.label}
              </div>
              {group.tabs.map((tab) => {
                const isActive = activeTab === tab.id;
                const Icon = tab.icon;
                return (
                  <button
                    key={tab.id}
                    type="button"
                    onClick={() => setActiveTab(tab.id)}
                    aria-current={isActive ? 'page' : undefined}
                    className={cn(
                      'relative mb-0.5 flex min-h-8 w-full items-center gap-2.5 rounded-md px-3 py-1.5 text-left text-ui transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                      isActive
                        ? 'bg-accent-muted font-semibold text-accent shadow-[inset_0_0_0_1px_hsl(var(--accent)/0.12)]'
                        : 'text-text-secondary hover:bg-surface-raised hover:text-text-primary',
                    )}
                  >
                    <Icon className="h-4 w-4 shrink-0" strokeWidth={1.7} aria-hidden="true" />
                    {tab.label}
                    {isActive ? <ChevronRight className="ml-auto h-3.5 w-3.5" aria-hidden="true" /> : null}
                  </button>
                );
              })}
            </div>
          ))}
        </nav>

        {/* Footer — never scrolls, never clips the list above it */}
        <div className="shrink-0 border-t border-border-subtle p-2">
          <button type="button" onClick={handleExport} disabled={isExporting} className={footerButtonClass}>
            <Archive className="h-3.5 w-3.5" aria-hidden="true" />
            {isExporting ? 'Exporting…' : 'Export'}
          </button>
          <button type="button" onClick={handleImport} disabled={isImporting} className={footerButtonClass}>
            <Upload className="h-3.5 w-3.5" aria-hidden="true" />
            {isImporting ? 'Importing…' : 'Import'}
          </button>
          <button
            type="button"
            onClick={() => setShowResetDialog(true)}
            className={cn(footerButtonClass, 'text-danger-fg hover:bg-danger-muted hover:text-danger-fg')}
          >
            <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
            Reset all
          </button>
        </div>
      </div>

      {/* Main content — reading column */}
      <main className="h-full min-h-0 min-w-0 flex-1 overflow-y-auto bg-bg" aria-label="Settings content">
        <div
          className={cn(
            'settings-content mx-auto w-full px-6 pb-16 pt-8',
            isWideContentTab ? 'max-w-[1100px]' : 'max-w-[760px]',
          )}
        >
          <LlmSettingsProvider>
            <ActiveTabComponent />
          </LlmSettingsProvider>
        </div>
      </main>

      {/* Reset confirmation */}
      <Dialog open={showResetDialog} onOpenChange={setShowResetDialog}>
        <DialogContent
          hideClose
          className="block max-w-sm rounded-md border border-border-subtle bg-surface-raised p-5 shadow-md"
          onInteractOutside={(event) => event.preventDefault()}
        >
          <DialogTitle asChild>
            <h2 className="text-base font-medium text-text-primary">
              Reset all settings?
            </h2>
          </DialogTitle>
          <DialogDescription asChild>
            <p className="mt-2 text-sm text-text-secondary">
              Every setting goes back to its default. Export first if you want a copy.
            </p>
          </DialogDescription>
          <div className="mt-5 flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setShowResetDialog(false)}
              className="h-8 rounded-sm border border-border-default bg-surface px-3 text-sm text-text-primary transition-colors duration-fast hover:bg-surface-raised"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => {
                void handleReset();
              }}
              disabled={isResetting}
              className="h-8 rounded-sm bg-danger px-3 text-sm font-medium text-accent-fg transition-opacity duration-fast hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isResetting ? 'Resetting…' : 'Reset'}
            </button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

Settings.displayName = 'Settings';
