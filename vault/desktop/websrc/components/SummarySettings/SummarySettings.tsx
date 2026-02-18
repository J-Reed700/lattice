/**
 * SummarySettings Component
 *
 * Settings panel for summarization preferences.
 * Model selection, auto-summarize, and cache settings.
 */

import { useState, useEffect } from 'react';

import { Settings, Download, Trash2, Loader2, CheckCircle, HardDrive, Cpu, Shield } from 'lucide-react';

import { ConfirmDialog } from '../ConfirmDialog';

interface ModelInfo {
  name: string;
  full_name: string;
  size_gb: number;
  parameters: number;
  quantization: string;
  languages: string[];
  downloaded: boolean;
  path?: string;
  description?: string;
}

interface SummarySettingsProps {
  onClose?: () => void;
  className?: string;
}

export function SummarySettings({ onClose, className = '' }: SummarySettingsProps) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [defaultModel, setDefaultModel] = useState('phi-3.5-mini');
  const [autoSummarize, setAutoSummarize] = useState(false);
  const [cacheEnabled, setCacheEnabled] = useState(true);
  const [pendingDeleteModel, setPendingDeleteModel] = useState<ModelInfo | null>(null);
  const [isDeletingModel, setIsDeletingModel] = useState(false);

  useEffect(() => {
    loadModels();
    loadSettings();
  }, []);

  const loadModels = async () => {
    try {
      const response = await fetch('/api/v1/summarize/models');
      if (!response.ok) throw new Error('Failed to load models');
      const data = await response.json();
      setModels(data);
    } catch (error) {
      console.error('Error loading models:', error);
      // Error is logged, no need to handle further
    } finally {
      setLoading(false);
    }
  };

  const loadSettings = async () => {
    // Load settings from localStorage or API
    const settings = localStorage.getItem('summarySettings');
    if (settings) {
      const parsed = JSON.parse(settings);
      setDefaultModel(parsed.defaultModel || 'phi-3.5-mini');
      setAutoSummarize(parsed.autoSummarize || false);
      setCacheEnabled(parsed.cacheEnabled !== false);
    }
  };

  const saveSettings = () => {
    const settings = {
      defaultModel,
      autoSummarize,
      cacheEnabled
    };
    localStorage.setItem('summarySettings', JSON.stringify(settings));
  };

  const handleDownloadModel = async (modelName: string) => {
    try {
      setDownloading(modelName);
      const response = await fetch(`/api/v1/summarize/models/download/${modelName}`, {
        method: 'POST'
      });

      if (!response.ok) throw new Error('Download failed');

      // Refresh model list
      await loadModels();
    } catch (error) {
      console.error('Error downloading model:', error);
      const message = error instanceof Error ? error.message : String(error);
      alert(`Failed to download model: ${message}`);
    } finally {
      setDownloading(null);
    }
  };

  const confirmDeleteModel = async () => {
    if (!pendingDeleteModel) return;

    setIsDeletingModel(true);
    try {
      const response = await fetch(`/api/v1/summarize/models/delete/${pendingDeleteModel.name}`, {
        method: 'DELETE'
      });

      if (!response.ok) {
        throw new Error('Delete failed');
      }

      // Refresh model list
      await loadModels();
    } catch (error) {
      console.error('Error deleting model:', error);
      alert(`Failed to delete model: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setIsDeletingModel(false);
      setPendingDeleteModel(null);
    }
  };

  const totalDiskUsage = models
    .filter(m => m.downloaded)
    .reduce((sum, m) => sum + m.size_gb, 0);

  if (loading) {
    return (
      <div className="flex items-center justify-center p-8">
        <Loader2 className="w-6 h-6 animate-spin text-[var(--accent-primary)]" />
      </div>
    );
  }

  return (
    <div className={`space-y-6 ${className}`}>
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="p-2 bg-[var(--accent-light)] rounded-lg">
            <Settings className="w-6 h-6 text-[var(--accent-primary)]" />
          </div>
          <div>
            <h2 className="text-xl font-semibold text-[var(--text-primary)]">Summary Settings</h2>
            <p className="text-sm text-[var(--text-secondary)]">Configure on-device AI summarization</p>
          </div>
        </div>
      </div>

      {/* Privacy Notice */}
      <div className="flex items-start gap-3 p-4 bg-[var(--success-light)] border border-[var(--success-light)] rounded-lg">
        <Shield className="w-5 h-5 text-[var(--success)] mt-0.5" />
        <div>
          <p className="text-sm font-medium text-[var(--success)]">100% Private & Offline</p>
          <p className="text-xs text-[var(--success)] mt-1">
            All models run locally on your device. No data is ever sent to external servers.
          </p>
        </div>
      </div>

      {/* General Settings */}
      <div className="space-y-4">
        <h3 className="font-semibold text-[var(--text-primary)]">General</h3>

        <div className="space-y-3">
          <label className="flex items-center gap-3 p-3 border border-[var(--border-color)] rounded-lg hover:bg-[var(--bg-secondary)] cursor-pointer">
            <input
              type="checkbox"
              checked={autoSummarize}
              onChange={(e) => {
                setAutoSummarize(e.target.checked);
                saveSettings();
              }}
              className="w-4 h-4 text-[var(--accent-primary)] rounded"
            />
            <div>
              <p className="font-medium text-[var(--text-primary)]">Auto-summarize new documents</p>
              <p className="text-sm text-[var(--text-secondary)]">Generate summaries automatically when indexing</p>
            </div>
          </label>

          <label className="flex items-center gap-3 p-3 border border-[var(--border-color)] rounded-lg hover:bg-[var(--bg-secondary)] cursor-pointer">
            <input
              type="checkbox"
              checked={cacheEnabled}
              onChange={(e) => {
                setCacheEnabled(e.target.checked);
                saveSettings();
              }}
              className="w-4 h-4 text-[var(--accent-primary)] rounded"
            />
            <div>
              <p className="font-medium text-[var(--text-primary)]">Cache summaries</p>
              <p className="text-sm text-[var(--text-secondary)]">Store summaries to avoid regeneration</p>
            </div>
          </label>
        </div>
      </div>

      {/* Models */}
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <h3 className="font-semibold text-[var(--text-primary)]">Models</h3>
          <div className="flex items-center gap-2 text-sm text-[var(--text-secondary)]">
            <HardDrive className="w-4 h-4" />
            <span>{totalDiskUsage.toFixed(1)} GB used</span>
          </div>
        </div>

        <div className="space-y-3">
          {models.map((model) => (
            <div
              key={model.name}
              className={`p-4 border rounded-lg ${
                defaultModel === model.name
                  ? 'border-[var(--accent-primary)] bg-[var(--accent-light)]'
                  : 'border-[var(--border-color)] bg-[var(--surface-elevated)]'
              }`}
            >
              <div className="flex items-start justify-between">
                <div className="flex-1">
                  <div className="flex items-center gap-2">
                    <h4 className="font-medium text-[var(--text-primary)]">{model.full_name}</h4>
                    {model.downloaded && (
                      <CheckCircle className="w-4 h-4 text-[var(--success)]" />
                    )}
                  </div>
                  <p className="text-sm text-[var(--text-secondary)] mt-1">{model.description}</p>
                  <div className="flex flex-wrap gap-3 mt-2 text-xs text-[var(--text-secondary)]">
                    <span className="flex items-center gap-1">
                      <Cpu className="w-3 h-3" />
                      {model.parameters}B params
                    </span>
                    <span>•</span>
                    <span>{model.size_gb} GB</span>
                    <span>•</span>
                    <span>{model.quantization}</span>
                    <span>•</span>
                    <span>{model.languages.join(', ')}</span>
                  </div>
                </div>

                <div className="flex gap-2 ml-4">
                  {model.downloaded ? (
                    <>
                      <button
                        onClick={() => {
                          setDefaultModel(model.name);
                          saveSettings();
                        }}
                        className={`px-3 py-1 text-sm rounded-lg transition-colors ${
                          defaultModel === model.name
                            ? 'bg-[var(--accent-primary)] text-white'
                            : 'bg-[var(--surface-elevated)] border border-[var(--border-color)] hover:bg-[var(--bg-secondary)]'
                        }`}
                      >
                        {defaultModel === model.name ? 'Default' : 'Set Default'}
                      </button>
                      <button
                        onClick={() => setPendingDeleteModel(model)}
                        className="p-2 text-[var(--error)] hover:bg-[var(--error-light)] rounded-lg transition-colors"
                        title="Delete model"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </>
                  ) : (
                    <button
                      onClick={() => handleDownloadModel(model.name)}
                      disabled={downloading === model.name}
                      className="flex items-center gap-2 px-3 py-1 text-sm bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-primary)] transition-colors disabled:opacity-50"
                    >
                      {downloading === model.name ? (
                        <>
                          <Loader2 className="w-4 h-4 animate-spin" />
                          Downloading...
                        </>
                      ) : (
                        <>
                          <Download className="w-4 h-4" />
                          Download
                        </>
                      )}
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>

      <ConfirmDialog
        isOpen={pendingDeleteModel !== null}
        title="Delete model?"
        message={`Delete "${pendingDeleteModel?.full_name ?? 'this model'}"? This will remove the model from your device.`}
        confirmLabel={isDeletingModel ? 'Deleting...' : 'Delete'}
        cancelLabel="Cancel"
        variant="danger"
        confirmDisabled={isDeletingModel}
        onConfirm={confirmDeleteModel}
        onCancel={() => setPendingDeleteModel(null)}
      />

      {/* Actions */}
      {onClose && (
        <div className="flex justify-end pt-4 border-t border-[var(--border-color)]">
          <button
            onClick={onClose}
            className="px-6 py-2 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-primary)] transition-colors"
          >
            Done
          </button>
        </div>
      )}
    </div>
  );
}

export default SummarySettings;
