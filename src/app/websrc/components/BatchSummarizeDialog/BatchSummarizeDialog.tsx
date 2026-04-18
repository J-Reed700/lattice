/**
 * BatchSummarizeDialog Component
 *
 * Dialog for batch summarizing multiple documents.
 * Shows progress and results for batch operations.
 */

import React, { useState } from 'react';

import { X, Sparkles, Loader2, CheckCircle, XCircle, FileText, Shield } from 'lucide-react';

interface BatchSummarizeError {
  index: number;
  error: string;
}

interface BatchSummarizeResults {
  successful: number;
  failed: number;
  total_time: number;
  errors?: BatchSummarizeError[];
}

interface BatchSummarizeDialogProps {
  fileIds: string[];
  onClose: () => void;
  onComplete?: (results: BatchSummarizeResults) => void;
  className?: string;
}

export function BatchSummarizeDialog({
  fileIds,
  onClose,
  onComplete,
  className = ''
}: BatchSummarizeDialogProps) {
  const [summaryType, setSummaryType] = useState('abstractive');
  const [maxWords, setMaxWords] = useState(150);
  const [processing, setProcessing] = useState(false);
  const [progress, setProgress] = useState(0);
  const [results, setResults] = useState<BatchSummarizeResults | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    try {
      setProcessing(true);
      setProgress(0);

      // Start batch task
      const response = await fetch('/api/v1/summarize/batch', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({
          file_ids: fileIds,
          summary_type: summaryType,
          max_words: maxWords
        })
      });

      if (!response.ok) {
        throw new Error('Batch summarization failed');
      }

      const data = await response.json();
      setResults(data);
      setProgress(100);
      onComplete?.(data);
    } catch (error) {
      console.error('Error during batch summarization:', error);
      const message = error instanceof Error ? error.message : String(error);
      alert(`Error: ${message}`);
    } finally {
      setProcessing(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-[hsl(var(--overlay))] bg-opacity-50 flex items-center justify-center z-50 p-4">
      <div className={`bg-[hsl(var(--surface-raised))] rounded-lg shadow-md max-w-2xl w-full max-h-[90vh] overflow-hidden ${className}`}>
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-[hsl(var(--border-subtle))]">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
              <Sparkles className="w-6 h-6 text-[hsl(var(--accent))]" />
            </div>
            <div>
              <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))]">Batch Summarize</h2>
              <p className="text-sm text-[hsl(var(--text-secondary))]">{fileIds.length} documents selected</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-2 hover:bg-[hsl(var(--surface-raised))] rounded-lg transition-colors duration-fast"
            disabled={processing}
          >
            <X className="w-5 h-5 text-[hsl(var(--text-secondary))]" />
          </button>
        </div>

        {/* Privacy Badge */}
        <div className="mx-6 mt-4 flex items-center gap-2 px-3 py-2 bg-[hsl(var(--success-muted))] border border-[hsl(var(--success-muted))] rounded-lg">
          <Shield className="w-4 h-4 text-[hsl(var(--success-fg))]" />
          <span className="text-sm font-medium text-[hsl(var(--success-fg))]">On-Device Processing</span>
          <span className="text-xs text-[hsl(var(--success-fg))]">• All documents processed locally</span>
        </div>

        <div className="p-6 overflow-y-auto">
          {!results ? (
            <form onSubmit={handleSubmit} className="space-y-6">
              {/* Summary Type */}
              <div>
                <label className="block text-sm font-medium text-[hsl(var(--text-secondary))] mb-2">
                  Summary Type
                </label>
                <select
                  value={summaryType}
                  onChange={(e) => setSummaryType(e.target.value)}
                  disabled={processing}
                  className="w-full px-3 py-2 border border-[hsl(var(--border-subtle))] rounded-lg focus:ring-2 focus:ring-[hsl(var(--accent))] focus:border-[hsl(var(--accent))]"
                >
                  <option value="tldr">TL;DR (1-2 sentences)</option>
                  <option value="abstractive">Abstractive Summary</option>
                  <option value="bullet_points">Bullet Points</option>
                  <option value="detailed">Detailed Summary</option>
                </select>
              </div>

              {/* Max Words */}
              <div>
                <label className="block text-sm font-medium text-[hsl(var(--text-secondary))] mb-2">
                  Target Word Count
                </label>
                <input
                  type="range"
                  min="50"
                  max="500"
                  step="50"
                  value={maxWords}
                  onChange={(e) => setMaxWords(parseInt(e.target.value))}
                  disabled={processing}
                  className="w-full"
                />
                <div className="flex justify-between text-sm text-[hsl(var(--text-secondary))] mt-1">
                  <span>50 words</span>
                  <span className="font-medium">{maxWords} words</span>
                  <span>500 words</span>
                </div>
              </div>

              {/* Progress */}
              {processing && (
                <div className="space-y-2">
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-[hsl(var(--text-secondary))]">Processing documents...</span>
                    <span className="font-medium text-[hsl(var(--accent))]">{progress}%</span>
                  </div>
                  <div className="w-full bg-[hsl(var(--surface-raised))] rounded-full h-2">
                    <div
                      className="bg-[hsl(var(--accent))] h-2 rounded-full transition-colors duration-300"
                      style={{ width: `${progress}%` }}
                    />
                  </div>
                  <p className="text-xs text-[hsl(var(--text-secondary))]">
                    This may take several minutes depending on document size and count.
                  </p>
                </div>
              )}

              {/* Estimate */}
              <div className="p-4 bg-[hsl(var(--accent-muted))] border border-[hsl(var(--accent-muted))] rounded-lg">
                <p className="text-sm text-[hsl(var(--accent))]">
                  <strong>Estimated time:</strong>{' '}
                  {Math.ceil(fileIds.length * 10 / 60)} - {Math.ceil(fileIds.length * 20 / 60)} minutes
                </p>
                <p className="text-xs text-[hsl(var(--accent))] mt-1">
                  Processing happens locally on your device for maximum privacy.
                </p>
              </div>

              {/* Actions */}
              <div className="flex justify-end gap-3 pt-4 border-t border-[hsl(var(--border-subtle))]">
                <button
                  type="button"
                  onClick={onClose}
                  disabled={processing}
                  className="px-4 py-2 border border-[hsl(var(--border-subtle))] rounded-lg hover:bg-[hsl(var(--surface))] transition-colors duration-fast disabled:opacity-50"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={processing}
                  className="flex items-center gap-2 px-6 py-2 bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] rounded-lg hover:bg-[hsl(var(--accent))] transition-colors duration-fast disabled:opacity-50"
                >
                  {processing ? (
                    <>
                      <Loader2 className="w-4 h-4 animate-spin" />
                      <span>Processing...</span>
                    </>
                  ) : (
                    <>
                      <Sparkles className="w-4 h-4" />
                      <span>Start Batch Summarization</span>
                    </>
                  )}
                </button>
              </div>
            </form>
          ) : (
            <div className="space-y-6">
              {/* Results Summary */}
              <div className="grid grid-cols-3 gap-4">
                <div className="p-4 bg-[hsl(var(--success-muted))] border border-[hsl(var(--success-muted))] rounded-lg">
                  <div className="flex items-center gap-2 text-[hsl(var(--success-fg))] mb-1">
                    <CheckCircle className="w-5 h-5" />
                    <span className="font-semibold">Success</span>
                  </div>
                  <p className="text-2xl font-bold text-[hsl(var(--success-fg))]">{results.successful}</p>
                </div>
                <div className="p-4 bg-[hsl(var(--danger-muted))] border border-[hsl(var(--danger-muted))] rounded-lg">
                  <div className="flex items-center gap-2 text-[hsl(var(--danger-fg))] mb-1">
                    <XCircle className="w-5 h-5" />
                    <span className="font-semibold">Failed</span>
                  </div>
                  <p className="text-2xl font-bold text-[hsl(var(--danger-fg))]">{results.failed}</p>
                </div>
                <div className="p-4 bg-[hsl(var(--accent-muted))] border border-[hsl(var(--accent-muted))] rounded-lg">
                  <div className="flex items-center gap-2 text-[hsl(var(--accent))] mb-1">
                    <FileText className="w-5 h-5" />
                    <span className="font-semibold">Total Time</span>
                  </div>
                  <p className="text-2xl font-bold text-[hsl(var(--accent))]">
                    {results.total_time.toFixed(0)}s
                  </p>
                </div>
              </div>

              {/* Errors (if any) */}
              {results.errors && results.errors.length > 0 && (
                <div className="space-y-2">
                  <h3 className="font-semibold text-[hsl(var(--text-primary))]">Errors:</h3>
                  <div className="space-y-1 max-h-40 overflow-y-auto">
                    {results.errors.map((error: BatchSummarizeError, i: number) => (
                      <div key={i} className="p-2 bg-[hsl(var(--danger-muted))] border border-[hsl(var(--danger-muted))] rounded text-sm text-[hsl(var(--danger-fg))]">
                        Document {error.index + 1}: {error.error}
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Close Button */}
              <div className="flex justify-end pt-4 border-t border-[hsl(var(--border-subtle))]">
                <button
                  onClick={onClose}
                  className="px-6 py-2 bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] rounded-lg hover:bg-[hsl(var(--accent))] transition-colors duration-fast"
                >
                  Done
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default BatchSummarizeDialog;
