import { useState, useEffect } from 'react';

import {
  Download,
  FileJson,
  FileSpreadsheet,
  FileText,
  Archive,
  CheckCircle,
  XCircle,
  Loader2,
  Filter,
} from 'lucide-react';

import { handleAsyncEvent } from '../../utils/promiseHandlers';
import { Button } from '../ui/button';
import { Checkbox } from '../ui/Checkbox';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../ui/dialog';
import { Input } from '../ui/input';
import { Select } from '../ui/select';
import { useToast } from '../ui/Toast';

interface ExportDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

type ExportFormat = 'json' | 'csv' | 'markdown' | 'zip';
type ExportScope = 'full' | 'filtered' | 'selected';

interface ExportJob {
  export_id: string;
  status: 'pending' | 'in_progress' | 'completed' | 'failed' | 'cancelled';
  format: ExportFormat;
  scope: ExportScope;
  file_count: number;
  total_size_bytes: number;
  progress_percent: number;
  created_at: string;
  completed_at?: string;
  error_message?: string;
  output_path?: string;
}

export function ExportDialog({ isOpen, onClose }: ExportDialogProps) {
  const { success, error: showError } = useToast();

  const [step, setStep] = useState<'config' | 'progress'>('config');
  const [format, setFormat] = useState<ExportFormat>('json');
  const [scope, setScope] = useState<ExportScope>('full');
  const [includeEmbeddings, setIncludeEmbeddings] = useState(false);
  const [includeOriginalFiles, setIncludeOriginalFiles] = useState(true);
  const [compress, setCompress] = useState(true);

  const [dateFrom, setDateFrom] = useState('');
  const [dateTo, setDateTo] = useState('');
  const [fileTypes, setFileTypes] = useState<string[]>([]);
  const [tags, setTags] = useState('');

  const [currentJob, setCurrentJob] = useState<ExportJob | null>(null);
  const [isExporting, setIsExporting] = useState(false);

  useEffect(() => {
    let intervalId: NodeJS.Timeout;

    if (currentJob?.status === 'in_progress') {
      intervalId = setInterval(() => {
        void (async () => {
          try {
            const response = await fetch(
              `http://localhost:8000/api/v1/export/${currentJob.export_id}/status`
            );

            if (response.ok) {
              const updated = await response.json();
              setCurrentJob(updated);

              if (updated.status === 'completed') {
                success('Export completed successfully');
                clearInterval(intervalId);
              } else if (updated.status === 'failed') {
              showError(`Export didn't finish: ${updated.error_message}`);
              clearInterval(intervalId);
            }
          }
        } catch (err) {
          console.error('Failed to check export status:', err);
        }
        })();
      }, 2000);
    }

    return () => {
      if (intervalId) clearInterval(intervalId);
    };
  }, [currentJob, success, showError]);

  const handleStartExport = async () => {
    setIsExporting(true);

    try {
      let url = 'http://localhost:8000/api/v1/export';
      let body: Record<string, unknown> | null = null;

      if (scope === 'full') {
        url += `/full?format=${format}&include_embeddings=${includeEmbeddings}&include_original_files=${includeOriginalFiles}&compress=${compress}`;
      } else if (scope === 'filtered') {
        url += '/filtered';
        body = {
          format,
          scope: 'filtered',
          filters: {
            date_from: dateFrom || undefined,
            date_to: dateTo || undefined,
            file_types: fileTypes.length > 0 ? fileTypes : undefined,
            tags: tags ? tags.split(',').map((t) => t.trim()) : undefined,
          },
          include_embeddings: includeEmbeddings,
          include_original_files: includeOriginalFiles,
          compress,
        };
      }

      const response = await fetch(url, {
        method: 'POST',
        headers: body ? { 'Content-Type': 'application/json' } : undefined,
        body: body ? JSON.stringify(body) : undefined,
      });

      if (!response.ok) {
        const errorData = await response.json();
        throw new Error(errorData.detail || 'Export failed');
      }

      const job = await response.json();
      setCurrentJob(job);
      setStep('progress');
      success('Export started');
    } catch (err) {
      showError(`Couldn't start export: ${  String(err)}`);
    } finally {
      setIsExporting(false);
    }
  };

  const handleDownload = async () => {
    if (!currentJob?.export_id) return;

    try {
      const response = await fetch(
        `http://localhost:8000/api/v1/export/${currentJob.export_id}/download`
      );

      if (!response.ok) {
        throw new Error('Download failed');
      }

      const blob = await response.blob();
      const url = window.URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `vault_export_${currentJob.export_id}.${getFileExtension(currentJob.format)}`;
      document.body.appendChild(a);
      a.click();
      window.URL.revokeObjectURL(url);
      document.body.removeChild(a);

      success('Export downloaded successfully');
    } catch (err) {
      showError(`Couldn't download export: ${  String(err)}`);
    }
  };

  const handleCancel = async () => {
    if (!currentJob?.export_id) return;

    try {
      const response = await fetch(
        `http://localhost:8000/api/v1/export/${currentJob.export_id}`,
        { method: 'DELETE' }
      );

      if (response.ok || response.status === 204) {
        success('Export cancelled');
        onClose();
        resetDialog();
      }
    } catch (err) {
      showError(`Couldn't cancel export: ${  String(err)}`);
    }
  };

  const resetDialog = () => {
    setStep('config');
    setFormat('json');
    setScope('full');
    setIncludeEmbeddings(false);
    setIncludeOriginalFiles(true);
    setCompress(true);
    setDateFrom('');
    setDateTo('');
    setFileTypes([]);
    setTags('');
    setCurrentJob(null);
  };

  const getFileExtension = (fmt: ExportFormat): string => {
    const extensions = {
      json: compress ? 'json.gz' : 'json',
      csv: compress ? 'csv.gz' : 'csv',
      markdown: 'zip',
      zip: 'zip',
    };
    return extensions[fmt];
  };

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${Math.round(bytes / Math.pow(k, i) * 100) / 100  } ${  sizes[i]}`;
  };

  const formatOptions = [
    {
      value: 'json',
      label: 'JSON',
      icon: FileJson,
      description: 'Complete structured export with all metadata',
    },
    {
      value: 'csv',
      label: 'CSV',
      icon: FileSpreadsheet,
      description: 'Tabular format for spreadsheet analysis',
    },
    {
      value: 'markdown',
      label: 'Markdown',
      icon: FileText,
      description: 'Human-readable format with folder structure',
    },
    {
      value: 'zip',
      label: 'ZIP Archive',
      icon: Archive,
      description: 'Bundle of original files plus metadata',
    },
  ];

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) {
          onClose();
          if (step === 'config') resetDialog();
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {step === 'config' ? 'Export knowledge base' : 'Export progress'}
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-6">
        {step === 'config' && (
          <>
            <div>
              <label className="block text-sm font-medium text-[hsl(var(--text-secondary))] mb-3">
                Export format
              </label>
              <div className="grid grid-cols-2 gap-3">
                {formatOptions.map((option) => {
                  const Icon = option.icon;
                  const isSelected = format === option.value;

                  return (
                    <button
                      key={option.value}
                      onClick={() => setFormat(option.value as ExportFormat)}
                      className={`
                        p-4 rounded-md border-2 text-left transition-colors duration-fast
                        ${
                          isSelected
                            ? 'border-[hsl(var(--accent))] bg-[hsl(var(--accent-muted))]'
                            : 'border-[hsl(var(--border-subtle))] hover:border-[hsl(var(--border-default))]'
                        }
                      `}
                    >
                      <div className="flex items-start gap-3">
                        <Icon
                          className={`w-4 h-4 flex-shrink-0 ${
                            isSelected
                              ? 'text-[hsl(var(--accent))]'
                              : 'text-[hsl(var(--text-tertiary))]'
                          }`}
                          strokeWidth={1.75}
                        />
                        <div className="flex-1 min-w-0">
                          <div
                            className={`font-medium ${
                              isSelected
                                ? 'text-[hsl(var(--accent))]'
                                : 'text-[hsl(var(--text-primary))]'
                            }`}
                          >
                            {option.label}
                          </div>
                          <div className="text-xs text-[hsl(var(--text-secondary))] mt-1">
                            {option.description}
                          </div>
                        </div>
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>

            <div>
              <label className="block text-sm font-medium text-[hsl(var(--text-secondary))] mb-2">
                Export scope
              </label>
              <Select
                value={scope}
                onValueChange={(value) => setScope(value as ExportScope)}
              >
                <option value="full">Full export - all files in knowledge base</option>
                <option value="filtered">Filtered export - apply filters below</option>
              </Select>
            </div>

            {scope === 'filtered' && (
              <div className="space-y-4 p-4 bg-[hsl(var(--surface))] rounded-md border border-[hsl(var(--border-subtle))]">
                <h3 className="text-sm font-medium text-[hsl(var(--text-primary))] flex items-center gap-2">
                  <Filter className="w-4 h-4" strokeWidth={1.75} />
                  Filters
                </h3>

                <div className="grid grid-cols-2 gap-4">
                  <Input
                    type="date"
                    label="Date from"
                    value={dateFrom}
                    onChange={(e) => setDateFrom(e.target.value)}
                  />
                  <Input
                    type="date"
                    label="Date to"
                    value={dateTo}
                    onChange={(e) => setDateTo(e.target.value)}
                  />
                </div>

                <div>
                  <label className="block text-sm font-medium text-[hsl(var(--text-secondary))] mb-2">
                    File types (select multiple)
                  </label>
                  <div className="space-y-2">
                    {['pdf', 'docx', 'txt', 'md'].map((type) => (
                      <Checkbox
                        key={type}
                        label={type.toUpperCase()}
                        checked={fileTypes.includes(type)}
                        onCheckedChange={(checked) => {
                          if (checked) {
                            setFileTypes([...fileTypes, type]);
                          } else {
                            setFileTypes(fileTypes.filter((t) => t !== type));
                          }
                        }}
                      />
                    ))}
                  </div>
                </div>

                <Input
                  label="Tags (comma-separated)"
                  placeholder="work, important, research"
                  value={tags}
                  onChange={(e) => setTags(e.target.value)}
                />
              </div>
            )}

            <div className="space-y-3 p-4 bg-[hsl(var(--surface))] rounded-md border border-[hsl(var(--border-subtle))]">
              <h3 className="text-sm font-medium text-[hsl(var(--text-primary))]">Options</h3>

              <Checkbox
                label="Include vector embeddings"
                description="Export AI embeddings (increases file size significantly)"
                checked={includeEmbeddings}
                onCheckedChange={setIncludeEmbeddings}
              />

              {format === 'zip' && (
                <Checkbox
                  label="Include original files"
                  description="Bundle original files in ZIP archive"
                  checked={includeOriginalFiles}
                  onCheckedChange={setIncludeOriginalFiles}
                />
              )}

              <Checkbox
                label="Compress output"
                description="Compress export file to reduce size"
                checked={compress}
                onCheckedChange={setCompress}
              />
            </div>

            <div className="flex justify-end gap-3 pt-4 border-t border-[hsl(var(--border-subtle))]">
              <Button variant="secondary" onClick={onClose}>
                Cancel
              </Button>
              <Button
                onClick={handleAsyncEvent(handleStartExport)}
                disabled={isExporting}
              >
                {isExporting ? (
                  <>
                    <Loader2 className="w-4 h-4 animate-spin mr-2" />
                    Starting...
                  </>
                ) : (
                  <>
                    <Download className="w-4 h-4 mr-2" strokeWidth={1.75} />
                    Start export
                  </>
                )}
              </Button>
            </div>
          </>
        )}

        {step === 'progress' && currentJob && (
          <>
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  {currentJob.status === 'completed' ? (
                    <CheckCircle className="w-5 h-5 text-[hsl(var(--success-fg))]" strokeWidth={1.75} />
                  ) : currentJob.status === 'failed' ? (
                    <XCircle className="w-5 h-5 text-[hsl(var(--danger-fg))]" strokeWidth={1.75} />
                  ) : (
                    <Loader2 className="w-5 h-5 text-[hsl(var(--accent))] animate-spin" />
                  )}
                  <div>
                    <h3 className="font-medium text-[hsl(var(--text-primary))]">
                      {currentJob.status === 'completed'
                        ? 'Export complete'
                        : currentJob.status === 'failed'
                        ? "Export didn't finish"
                        : 'Exporting...'}
                    </h3>
                    <p className="text-sm text-[hsl(var(--text-secondary))]">
                      {currentJob.format.toUpperCase()} • {currentJob.scope}
                    </p>
                  </div>
                </div>
                <span
                  className={`px-3 py-1 text-sm font-medium rounded-full ${
                    currentJob.status === 'completed'
                      ? 'bg-[hsl(var(--success-muted))] text-[hsl(var(--success-fg))]'
                      : currentJob.status === 'failed'
                      ? 'bg-[hsl(var(--danger-muted))] text-[hsl(var(--danger-fg))]'
                      : 'bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))]'
                  }`}
                >
                  {Math.round(currentJob.progress_percent)}%
                </span>
              </div>

              <div className="w-full bg-[hsl(var(--surface-raised))] rounded-full h-2 overflow-hidden">
                <div
                  className={`h-full transition-[width] duration-base ease-out ${
                    currentJob.status === 'completed'
                      ? 'bg-[hsl(var(--success-fg))]'
                      : currentJob.status === 'failed'
                      ? 'bg-[hsl(var(--danger-fg))]'
                      : 'bg-[hsl(var(--accent))]'
                  }`}
                  style={{ width: `${currentJob.progress_percent}%` }}
                />
              </div>

              <div className="grid grid-cols-2 gap-4 p-4 bg-[hsl(var(--surface))] rounded-md">
                <div>
                  <div className="text-sm text-[hsl(var(--text-secondary))]">Files</div>
                  <div className="text-lg font-semibold text-[hsl(var(--text-primary))]">
                    {currentJob.file_count.toLocaleString()}
                  </div>
                </div>
                <div>
                  <div className="text-sm text-[hsl(var(--text-secondary))]">Size</div>
                  <div className="text-lg font-semibold text-[hsl(var(--text-primary))]">
                    {formatBytes(currentJob.total_size_bytes)}
                  </div>
                </div>
              </div>

              {currentJob.error_message && (
                <div className="p-4 bg-[hsl(var(--danger-muted))] border border-[hsl(var(--danger-muted))] rounded-md">
                  <p className="text-sm text-[hsl(var(--danger-fg))]">{currentJob.error_message}</p>
                </div>
              )}
            </div>

            <div className="flex justify-end gap-3 pt-4 border-t border-[hsl(var(--border-subtle))]">
              {currentJob.status === 'in_progress' && (
                <Button variant="secondary" onClick={handleAsyncEvent(handleCancel)}>
                  Cancel export
                </Button>
              )}
              {currentJob.status === 'completed' && (
                <>
                  <Button variant="secondary" onClick={() => { onClose(); resetDialog(); }}>
                    Close
                  </Button>
                  <Button onClick={handleAsyncEvent(handleDownload)}>
                    <Download className="w-4 h-4 mr-2" strokeWidth={1.75} />
                    Download export
                  </Button>
                </>
              )}
              {currentJob.status === 'failed' && (
                <Button onClick={() => { onClose(); resetDialog(); }}>
                  Close
                </Button>
              )}
            </div>
          </>
        )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
