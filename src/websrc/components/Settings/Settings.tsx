/**
 * Settings — shell.
 *
 * Plain-text tab list on the left (no icons, no icon box), reading column
 * on the right. Every tab renders exactly one PageHeader whose title is a
 * noun. See `.design/UX-OVERHAUL-BRIEF.md` §3.
 */

import { useEffect, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { save, open } from '@tauri-apps/plugin-dialog';
import { writeTextFile, readTextFile } from '@tauri-apps/plugin-fs';

import { cn } from '@/lib/utils';

import { AIModelsTab } from './AIModelsTab';
import { ChatTab, ModelsTab, PromptsTab, TuningTab, ToolsTab, LlmSettingsProvider } from './AITab';
import { DisplayTab } from './DisplayTab';
import { IndexingTab } from './IndexingTab';
import { PrivacyTab } from './PrivacyTab';
import { SearchTab } from './SearchTab';
import { VaultTab } from './VaultTab';
import { SETTINGS_QUERY_KEY } from '../../hooks/queries/useSettingsQuery';
import VaultAPI from '../../lib/api';
import { toast } from '../../stores/toastStore';
import { SidebarHeader } from '../ui';

type SettingsTab =
  | 'search'
  | 'indexing'
  | 'vault'
  | 'chat'
  | 'models'
  | 'downloaded-models'
  | 'prompts'
  | 'tuning'
  | 'tools'
  | 'display'
  | 'privacy';

interface Tab {
  id: SettingsTab;
  label: string;
  component: React.ComponentType;
}

type TabGroup = {
  label: string;
  tabs: Tab[];
};

const tabGroups: TabGroup[] = [
  {
    label: 'General',
    tabs: [
      { id: 'search', label: 'Search', component: SearchTab },
      { id: 'indexing', label: 'Indexing', component: IndexingTab },
      { id: 'vault', label: 'Vault', component: VaultTab },
      { id: 'display', label: 'Display', component: DisplayTab },
      { id: 'privacy', label: 'Privacy', component: PrivacyTab },
    ],
  },
  {
    label: 'AI',
    tabs: [
      { id: 'chat', label: 'Chat', component: ChatTab },
      { id: 'models', label: 'Models', component: ModelsTab },
      { id: 'downloaded-models', label: 'Downloaded', component: AIModelsTab },
      { id: 'prompts', label: 'Prompts', component: PromptsTab },
      { id: 'tuning', label: 'Tuning', component: TuningTab },
      { id: 'tools', label: 'Tools', component: ToolsTab },
    ],
  },
];

const tabs: Tab[] = tabGroups.flatMap((group) => group.tabs);
const tabIds = new Set<string>(tabs.map((tab) => tab.id));

const footerButtonClass =
  'flex w-full items-center rounded-sm px-2.5 py-1.5 text-sm text-text-secondary transition-colors duration-fast hover:bg-surface-raised hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-50';

export function Settings() {
  const [activeTab, setActiveTab] = useState<SettingsTab>('search');
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
    <div className="flex h-full bg-bg">
      {/* Sidebar */}
      <div className="flex w-[240px] shrink-0 flex-col border-r border-border-subtle bg-surface">
        <SidebarHeader title="Settings" />

        {/* Tab list — scrolls so the footer never clips it */}
        <nav aria-label="Settings sections" className="min-h-0 flex-1 overflow-y-auto px-2 py-3">
          {tabGroups.map((group) => (
            <div key={group.label} className="mb-5 last:mb-0">
              <div className="px-2.5 pb-1.5 text-xxs uppercase tracking-[0.08em] text-text-muted">
                {group.label}
              </div>
              {group.tabs.map((tab) => {
                const isActive = activeTab === tab.id;
                return (
                  <button
                    key={tab.id}
                    type="button"
                    onClick={() => setActiveTab(tab.id)}
                    aria-current={isActive ? 'page' : undefined}
                    className={cn(
                      'relative flex w-full items-center rounded-sm px-2.5 py-1.5 text-left text-sm transition-colors duration-fast',
                      isActive
                        ? 'bg-surface-raised text-text-primary'
                        : 'text-text-secondary hover:bg-surface-raised hover:text-text-primary',
                    )}
                  >
                    {isActive ? (
                      <span aria-hidden className="absolute inset-y-0 left-0 w-0.5 bg-accent" />
                    ) : null}
                    {tab.label}
                  </button>
                );
              })}
            </div>
          ))}
        </nav>

        {/* Footer — never scrolls, never clips the list above it */}
        <div className="shrink-0 border-t border-border-subtle p-2">
          <button type="button" onClick={handleExport} disabled={isExporting} className={footerButtonClass}>
            {isExporting ? 'Exporting…' : 'Export'}
          </button>
          <button type="button" onClick={handleImport} disabled={isImporting} className={footerButtonClass}>
            {isImporting ? 'Importing…' : 'Import'}
          </button>
          <button
            type="button"
            onClick={() => setShowResetDialog(true)}
            className="flex w-full items-center rounded-sm px-2.5 py-1.5 text-left text-sm text-danger-fg transition-colors duration-fast hover:bg-danger-muted"
          >
            Reset all
          </button>
        </div>
      </div>

      {/* Main content — reading column */}
      <main className="h-full flex-1 overflow-y-auto bg-bg">
        <div
          className={cn(
            'mx-auto w-full px-6 pb-16 pt-10',
            isWideContentTab ? 'max-w-[1100px]' : 'max-w-[760px]',
          )}
        >
          <LlmSettingsProvider>
            <ActiveTabComponent />
          </LlmSettingsProvider>
        </div>
      </main>

      {/* Reset confirmation */}
      {showResetDialog && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-overlay p-4">
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="settings-reset-title"
            className="w-full max-w-sm rounded-md border border-border-subtle bg-surface-raised p-5 shadow-md"
          >
            <h2 id="settings-reset-title" className="text-base font-medium text-text-primary">
              Reset all settings?
            </h2>
            <p className="mt-2 text-sm text-text-secondary">
              Every setting goes back to its default. Export first if you want a copy.
            </p>
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
          </div>
        </div>
      )}
    </div>
  );
}

Settings.displayName = 'Settings';
