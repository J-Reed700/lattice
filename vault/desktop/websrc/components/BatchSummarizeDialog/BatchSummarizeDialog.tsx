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
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50 p-4">
      <div className={`bg-[var(--surface-elevated)] rounded-lg shadow-xl max-w-2xl w-full max-h-[90vh] overflow-hidden ${className}`}>
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-[var(--border-color)]">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-[var(--accent-light)] rounded-lg">
              <Sparkles className="w-6 h-6 text-[var(--accent-primary)]" />
            </div>
            <div>
              <h2 className="text-xl font-semibold text-[var(--text-primary)]">Batch Summarize</h2>
              <p className="text-sm text-[var(--text-secondary)]">{fileIds.length} documents selected</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-2 hover:bg-[var(--bg-tertiary)] rounded-lg transition-colors"
            disabled={processing}
          >
            <X className="w-5 h-5 text-[var(--text-secondary)]" />
          </button>
        </div>

        {/* Privacy Badge */}
        <div className="mx-6 mt-4 flex items-center gap-2 px-3 py-2 bg-[var(--success-light)] border border-[var(--success-light)] rounded-lg">
          <Shield className="w-4 h-4 text-[var(--success)]" />
          <span className="text-sm font-medium text-[var(--success)]">On-Device Processing</span>
          <span className="text-xs text-[var(--success)]">• All documents processed locally</span>
        </div>

        <div className="p-6 overflow-y-auto">
          {!results ? (
            <form onSubmit={handleSubmit} className="space-y-6">
              {/* Summary Type */}
              <div>
                <label className="block text-sm font-medium text-[var(--text-secondary)] mb-2">
                  Summary Type
                </label>
                <select
                  value={summaryType}
                  onChange={(e) => setSummaryType(e.target.value)}
                  disabled={processing}
                  className="w-full px-3 py-2 border border-[var(--border-color)] rounded-lg focus:ring-2 focus:ring-[var(--accent-primary)] focus:border-[var(--accent-primary)]"
                >
                  <option value="tldr">TL;DR (1-2 sentences)</option>
                  <option value="abstractive">Abstractive Summary</option>
                  <option value="bullet_points">Bullet Points</option>
                  <option value="detailed">Detailed Summary</option>
                </select>
              </div>

              {/* Max Words */}
              <div>
                <label className="block text-sm font-medium text-[var(--text-secondary)] mb-2">
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
                <div className="flex justify-between text-sm text-[var(--text-secondary)] mt-1">
                  <span>50 words</span>
                  <span className="font-medium">{maxWords} words</span>
                  <span>500 words</span>
                </div>
              </div>

              {/* Progress */}
              {processing && (
                <div className="space-y-2">
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-[var(--text-secondary)]">Processing documents...</span>
                    <span className="font-medium text-[var(--accent-primary)]">{progress}%</span>
                  </div>
                  <div className="w-full bg-[var(--bg-tertiary)] rounded-full h-2">
                    <div
                      className="bg-[var(--accent-primary)] h-2 rounded-full transition-all duration-300"
                      style={{ width: `${progress}%` }}
                    />
                  </div>
                  <p className="text-xs text-[var(--text-secondary)]">
                    This may take several minutes depending on document size and count.
                  </p>
                </div>
              )}

              {/* Estimate */}
              <div className="p-4 bg-[var(--accent-light)] border border-[var(--accent-light)] rounded-lg">
                <p className="text-sm text-[var(--accent-primary)]">
                  <strong>Estimated time:</strong>{' '}
                  {Math.ceil(fileIds.length * 10 / 60)} - {Math.ceil(fileIds.length * 20 / 60)} minutes
                </p>
                <p className="text-xs text-[var(--accent-primary)] mt-1">
                  Processing happens locally on your device for maximum privacy.
                </p>
              </div>

              {/* Actions */}
              <div className="flex justify-end gap-3 pt-4 border-t border-[var(--border-color)]">
                <button
                  type="button"
                  onClick={onClose}
                  disabled={processing}
                  className="px-4 py-2 border border-[var(--border-color)] rounded-lg hover:bg-[var(--bg-secondary)] transition-colors disabled:opacity-50"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={processing}
                  className="flex items-center gap-2 px-6 py-2 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-primary)] transition-colors disabled:opacity-50"
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
                <div className="p-4 bg-[var(--success-light)] border border-[var(--success-light)] rounded-lg">
                  <div className="flex items-center gap-2 text-[var(--success)] mb-1">
                    <CheckCircle className="w-5 h-5" />
                    <span className="font-semibold">Success</span>
                  </div>
                  <p className="text-2xl font-bold text-[var(--success)]">{results.successful}</p>
                </div>
                <div className="p-4 bg-[var(--error-light)] border border-[var(--error-light)] rounded-lg">
                  <div className="flex items-center gap-2 text-[var(--error)] mb-1">
                    <XCircle className="w-5 h-5" />
                    <span className="font-semibold">Failed</span>
                  </div>
                  <p className="text-2xl font-bold text-[var(--error)]">{results.failed}</p>
                </div>
                <div className="p-4 bg-[var(--accent-light)] border border-[var(--accent-light)] rounded-lg">
                  <div className="flex items-center gap-2 text-[var(--accent-primary)] mb-1">
                    <FileText className="w-5 h-5" />
                    <span className="font-semibold">Total Time</span>
                  </div>
                  <p className="text-2xl font-bold text-[var(--accent-primary)]">
                    {results.total_time.toFixed(0)}s
                  </p>
                </div>
              </div>

              {/* Errors (if any) */}
              {results.errors && results.errors.length > 0 && (
                <div className="space-y-2">
                  <h3 className="font-semibold text-[var(--text-primary)]">Errors:</h3>
                  <div className="space-y-1 max-h-40 overflow-y-auto">
                    {results.errors.map((error: BatchSummarizeError, i: number) => (
                      <div key={i} className="p-2 bg-[var(--error-light)] border border-[var(--error-light)] rounded text-sm text-[var(--error)]">
                        Document {error.index + 1}: {error.error}
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Close Button */}
              <div className="flex justify-end pt-4 border-t border-[var(--border-color)]">
                <button
                  onClick={onClose}
                  className="px-6 py-2 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-primary)] transition-colors"
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
