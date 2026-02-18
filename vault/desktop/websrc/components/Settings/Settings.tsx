/**
 * Settings Component - Main Container
 *
 * Tabbed settings interface with persistent storage,
 * keyboard shortcuts (Cmd/Ctrl + ,), and export/import functionality.
 */

import { useState } from 'react';

import { save, open } from '@tauri-apps/plugin-dialog';
import { writeTextFile, readTextFile } from '@tauri-apps/plugin-fs';
import { Settings as SettingsIcon, Search, Database, Brain, Palette, Shield, RotateCcw, Download, Upload, HardDrive } from 'lucide-react';

import { AITab } from './AITab';
import { DisplayTab } from './DisplayTab';
import { DownloadedModelsTab } from './DownloadedModelsTab';
import { IndexingTab } from './IndexingTab';
import { PrivacyTab } from './PrivacyTab';
import { SearchTab } from './SearchTab';
import { useSettingsStore } from '../../stores/settingsStore';
import { toast } from '../../stores/toastStore';

type SettingsTab = 'search' | 'indexing' | 'ai' | 'downloaded-models' | 'display' | 'privacy';

interface Tab {
  id: SettingsTab;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  component: React.ComponentType;
}

const tabs: Tab[] = [
  { id: 'search', label: 'Search', icon: Search, component: SearchTab },
  { id: 'indexing', label: 'Indexing', icon: Database, component: IndexingTab },
  { id: 'ai', label: 'AI Models', icon: Brain, component: AITab },
  { id: 'downloaded-models', label: 'Downloaded Models', icon: HardDrive, component: DownloadedModelsTab },
  { id: 'display', label: 'Display', icon: Palette, component: DisplayTab },
  { id: 'privacy', label: 'Privacy', icon: Shield, component: PrivacyTab },
];

export function Settings() {
  const [activeTab, setActiveTab] = useState<SettingsTab>('search');
  const [showResetDialog, setShowResetDialog] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [isImporting, setIsImporting] = useState(false);

  const settings = useSettingsStore((state) => state.settings);
  const resetToDefaults = useSettingsStore((state) => state.resetToDefaults);
  const exportSettings = useSettingsStore((state) => state.exportSettings);
  const importSettings = useSettingsStore((state) => state.importSettings);

  const handleReset = () => {
    resetToDefaults();
    setShowResetDialog(false);
    toast.success('Settings reset to defaults', { duration: 3000 });
  };

  const handleExport = async () => {
    setIsExporting(true);
    try {
      const path = await save({
        defaultPath: 'recall-settings.json',
        filters: [
          {
            name: 'JSON',
            extensions: ['json'],
          },
        ],
      });

      if (path) {
        const json = exportSettings();
        await writeTextFile(path, json);
        toast.success('Settings exported successfully', { duration: 3000 });
      }
    } catch (error) {
      console.error('Export failed:', error);
      toast.error('Export failed', { message: String(error), duration: 5000 });
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
        const success = importSettings(json);

        if (success) {
          toast.success('Settings imported successfully', { duration: 3000 });
        } else {
          toast.error('Invalid settings file', { duration: 5000 });
        }
      }
    } catch (error) {
      console.error('Import failed:', error);
      toast.error('Import failed', { message: String(error), duration: 5000 });
    } finally {
      setIsImporting(false);
    }
  };

  const ActiveTabComponent = tabs.find((tab) => tab.id === activeTab)?.component || SearchTab;
  const isWideContentTab = activeTab === 'ai' || activeTab === 'downloaded-models';

  return (
    <div className="flex h-full bg-[var(--bg-primary)]">
      {/* Sidebar */}
      <div className="w-60 bg-[var(--bg-secondary)] border-r border-[var(--border-color)] flex flex-col">
        {/* Header */}
        <div className="p-4 border-b border-[var(--border-color)]">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-[var(--accent-light)] rounded-lg">
              <SettingsIcon className="w-5 h-5 text-[var(--accent-primary)]" />
            </div>
            <div>
              <h1 className="text-lg font-semibold text-[var(--text-primary)]">Settings</h1>
              <p className="text-xs text-[var(--text-tertiary)]">v{settings.version}</p>
            </div>
          </div>
        </div>

        {/* Tabs */}
        <nav className="flex-1 p-3 space-y-1 overflow-y-auto">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`
                  w-full flex items-center gap-3 px-3 py-2.5 text-sm rounded-lg transition-all duration-200
                  ${
                    activeTab === tab.id
                      ? 'bg-[var(--accent-primary)]/10 text-[var(--accent-primary)] font-semibold'
                      : 'text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]'
                  }
                `}
              >
                <Icon className="w-4 h-4 flex-shrink-0" />
                <span className="font-medium">{tab.label}</span>
              </button>
            );
          })}
        </nav>

        {/* Actions */}
        <div className="p-3 border-t border-[var(--border-color)] space-y-2">
          <button
            onClick={handleExport}
            disabled={isExporting}
            className="w-full flex items-center gap-2 px-3 py-2 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] rounded-lg transition-colors disabled:opacity-50"
          >
            <Download className="w-4 h-4" />
            {isExporting ? 'Exporting...' : 'Export'}
          </button>

          <button
            onClick={handleImport}
            disabled={isImporting}
            className="w-full flex items-center gap-2 px-3 py-2 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] rounded-lg transition-colors disabled:opacity-50"
          >
            <Upload className="w-4 h-4" />
            {isImporting ? 'Importing...' : 'Import'}
          </button>

          <button
            onClick={() => setShowResetDialog(true)}
            className="w-full flex items-center gap-2 px-3 py-2 text-sm text-[var(--error)] hover:bg-[var(--error-light)] rounded-lg transition-colors"
          >
            <RotateCcw className="w-4 h-4" />
            Reset All
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
          <ActiveTabComponent />
        </div>
      </div>

      {/* Reset Confirmation Dialog */}
      {showResetDialog && (
        <div className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-4">
          <div className="bg-[var(--surface-elevated)] rounded-2xl shadow-2xl max-w-md w-full p-6 border border-[var(--border-color)]">
            <div className="flex items-center gap-3 mb-4">
              <div className="p-2 bg-[var(--error-light)] rounded-lg">
                <RotateCcw className="w-5 h-5 text-[var(--error)]" />
              </div>
              <h2 className="text-xl font-semibold text-[var(--text-primary)]">
                Reset All Settings?
              </h2>
            </div>

            <p className="text-sm text-[var(--text-secondary)] mb-6">
              This will restore all settings to their default values. This action cannot be
              undone. Consider exporting your settings first.
            </p>

            <div className="flex gap-3 justify-end">
              <button
                onClick={() => setShowResetDialog(false)}
                className="px-4 py-2 text-sm font-medium text-[var(--text-primary)] bg-[var(--surface-hover)] hover:bg-[var(--surface-active)] rounded-lg transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleReset}
                className="px-4 py-2 text-sm font-medium text-white bg-[var(--error)] hover:opacity-90 rounded-lg transition-opacity"
              >
                Reset Settings
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Auto-save indicator */}
      <div className="fixed bottom-4 right-4 px-3 py-2 bg-[var(--surface-elevated)] backdrop-blur-xl border border-[var(--border-color)] rounded-lg shadow-lg flex items-center gap-2">
        <div className="w-2 h-2 bg-[var(--success)] rounded-full animate-pulse" />
        <span className="text-xs text-[var(--text-secondary)]">Settings saved automatically</span>
      </div>
    </div>
  );
}

Settings.displayName = 'Settings';
