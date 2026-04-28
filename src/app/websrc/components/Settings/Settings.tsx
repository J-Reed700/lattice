/**
 * Settings Component - Main Container
 *
 * Tabbed settings interface with persistent storage,
 * keyboard shortcuts (Cmd/Ctrl + ,), and export/import functionality.
 */

import { useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { save, open } from '@tauri-apps/plugin-dialog';
import { writeTextFile, readTextFile } from '@tauri-apps/plugin-fs';
import { Settings as SettingsIcon, Search, Database, MessageSquare, Brain, Palette, Shield, RotateCcw, Download, Upload, HardDrive, FileText, Settings2, Wrench } from 'lucide-react';

import { AIModelsTab } from './AIModelsTab';
import { ChatTab, ModelsTab, PromptsTab, TuningTab, ToolsTab, LlmSettingsProvider } from './AITab';
import { DisplayTab } from './DisplayTab';
import { IndexingTab } from './IndexingTab';
import { PrivacyTab } from './PrivacyTab';
import { SearchTab } from './SearchTab';
import { CONFIG_QUERY_KEY } from '../../hooks/queries/useConfigQuery';
import { SETTINGS_QUERY_KEY } from '../../hooks/queries/useSettingsQuery';
import VaultAPI from '../../lib/api';
import { toast } from '../../stores/toastStore';

type SettingsTab = 'search' | 'indexing' | 'chat' | 'models' | 'downloaded-models' | 'prompts' | 'tuning' | 'tools' | 'display' | 'privacy';

interface Tab {
  id: SettingsTab;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
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
      { id: 'search', label: 'Search', icon: Search, component: SearchTab },
      { id: 'indexing', label: 'Indexing', icon: Database, component: IndexingTab },
      { id: 'display', label: 'Display', icon: Palette, component: DisplayTab },
      { id: 'privacy', label: 'Privacy', icon: Shield, component: PrivacyTab },
    ],
  },
  {
    label: 'AI',
    tabs: [
      { id: 'chat', label: 'Chat', icon: MessageSquare, component: ChatTab },
      { id: 'models', label: 'Models', icon: Brain, component: ModelsTab },
      { id: 'downloaded-models', label: 'Downloaded', icon: HardDrive, component: AIModelsTab },
      { id: 'prompts', label: 'Prompts', icon: FileText, component: PromptsTab },
      { id: 'tuning', label: 'Tuning', icon: Settings2, component: TuningTab },
      { id: 'tools', label: 'Tools', icon: Wrench, component: ToolsTab },
    ],
  },
];

const tabs: Tab[] = tabGroups.flatMap((group) => group.tabs);

export function Settings() {
  const [activeTab, setActiveTab] = useState<SettingsTab>('search');
  const [showResetDialog, setShowResetDialog] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [isResetting, setIsResetting] = useState(false);

  const queryClient = useQueryClient();

  // Invalidate React Query caches whenever settings change on the backend
  // (reset/import). Tabs that consume those caches refetch automatically.
  const invalidateSettingsCaches = () => {
    void queryClient.invalidateQueries({ queryKey: SETTINGS_QUERY_KEY });
    void queryClient.invalidateQueries({ queryKey: CONFIG_QUERY_KEY });
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
  const isWideContentTab = activeTab === 'models' || activeTab === 'downloaded-models' || activeTab === 'tools';

  return (
    <div className="flex h-full bg-[hsl(var(--bg))]">
      {/* Sidebar */}
      <div className="w-60 bg-[hsl(var(--surface))] border-r border-[hsl(var(--border-subtle))] flex flex-col">
        {/* Header */}
        <div className="p-4 border-b border-[hsl(var(--border-subtle))]">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
              <SettingsIcon className="w-5 h-5 text-[hsl(var(--accent))]" />
            </div>
            <div>
              <h1 className="text-lg font-semibold text-[hsl(var(--text-primary))]">Settings</h1>
            </div>
          </div>
        </div>

        {/* Tabs */}
        <nav className="flex-1 p-3 space-y-4 overflow-y-auto">
          {tabGroups.map((group) => (
            <div key={group.label}>
              <div className="px-3 pb-1.5 text-[10px] font-semibold uppercase tracking-wider text-[hsl(var(--text-tertiary))]">
                {group.label}
              </div>
              <div className="space-y-1">
                {group.tabs.map((tab) => {
                  const Icon = tab.icon;
                  return (
                    <button
                      key={tab.id}
                      onClick={() => setActiveTab(tab.id)}
                      className={`
                        w-full flex items-center gap-3 px-3 py-2.5 text-sm rounded-md transition-colors duration-fast
                        ${
                          activeTab === tab.id
                            ? 'bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))] font-semibold'
                            : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]'
                        }
                      `}
                    >
                      <Icon className="w-4 h-4 flex-shrink-0" />
                      <span className="font-medium">{tab.label}</span>
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </nav>

        {/* Actions */}
        <div className="p-3 border-t border-[hsl(var(--border-subtle))] space-y-2">
          <button
            onClick={handleExport}
            disabled={isExporting}
            className="w-full flex items-center gap-2 px-3 py-2 text-sm text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] rounded-md transition-colors duration-fast disabled:opacity-50"
          >
            <Download className="w-4 h-4" strokeWidth={1.75} />
            {isExporting ? 'Exporting...' : 'Export'}
          </button>

          <button
            onClick={handleImport}
            disabled={isImporting}
            className="w-full flex items-center gap-2 px-3 py-2 text-sm text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] rounded-md transition-colors duration-fast disabled:opacity-50"
          >
            <Upload className="w-4 h-4" strokeWidth={1.75} />
            {isImporting ? 'Importing...' : 'Import'}
          </button>

          <button
            onClick={() => setShowResetDialog(true)}
            className="w-full flex items-center gap-2 px-3 py-2 text-sm text-[hsl(var(--danger-fg))] hover:bg-[hsl(var(--danger-muted))] rounded-md transition-colors duration-fast"
          >
            <RotateCcw className="w-4 h-4" strokeWidth={1.75} />
            Reset all
          </button>
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 overflow-y-auto">
        <div
          className={
            isWideContentTab
              ? 'mx-auto w-full max-w-[1700px] px-6 py-8 lg:px-10'
              : 'max-w-3xl mx-auto p-8'
          }
        >
          <LlmSettingsProvider>
            <ActiveTabComponent />
          </LlmSettingsProvider>
        </div>
      </div>

      {/* Reset Confirmation Dialog */}
      {showResetDialog && (
        <div className="fixed inset-0 bg-[hsl(var(--overlay))] flex items-center justify-center z-50 p-4">
          <div className="bg-[hsl(var(--surface-raised))] rounded-lg shadow-md max-w-md w-full p-6 border border-[hsl(var(--border-subtle))]">
            <div className="flex items-center gap-3 mb-4">
              <div className="p-2 bg-[hsl(var(--danger-muted))] rounded-md">
                <RotateCcw className="w-4 h-4 text-[hsl(var(--danger-fg))]" strokeWidth={1.75} />
              </div>
              <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))]">
                Reset all settings?
              </h2>
            </div>

            <p className="text-sm text-[hsl(var(--text-secondary))] mb-6">
              This will restore all settings to their default values. This action cannot be
              undone. Consider exporting your settings first.
            </p>

            <div className="flex gap-3 justify-end">
              <button
                onClick={() => setShowResetDialog(false)}
                className="px-4 py-2 text-sm font-medium text-[hsl(var(--text-primary))] bg-[hsl(var(--surface-raised))] hover:bg-[hsl(var(--surface))] rounded-md transition-colors duration-fast border border-[hsl(var(--border-subtle))]"
              >
                Cancel
              </button>
              <button
                onClick={() => {
                  void handleReset();
                }}
                disabled={isResetting}
                className="px-4 py-2 text-sm font-medium text-[hsl(var(--accent-fg))] bg-[hsl(var(--danger))] hover:opacity-90 rounded-md transition-opacity duration-fast disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {isResetting ? 'Resetting...' : 'Reset settings'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Auto-save indicator */}
      <div className="fixed bottom-4 right-4 px-3 py-2 bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded-md shadow-sm flex items-center gap-2">
        <div className="w-2 h-2 bg-[hsl(var(--success-fg))] rounded-full animate-pulse" />
        <span className="text-xs text-[hsl(var(--text-secondary))]">Settings saved automatically</span>
      </div>
    </div>
  );
}

Settings.displayName = 'Settings';
