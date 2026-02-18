/**
 * SummaryPanel Component
 *
 * Displays document summaries with on-device AI indicator.
 * Shows different summary types and allows regeneration.
 */

import { useState, useEffect } from 'react';

import { Loader2, Sparkles, FileText, List, Zap, BookOpen, Shield, RefreshCw } from 'lucide-react';

interface Summary {
  id: string;
  text: string;
  summary_type: string;
  source_length: number;
  summary_length: number;
  compression_ratio: number;
  model: string;
  language: string;
  generation_time: number;
  tokens_per_second: number;
  created_at: string;
  cache_hit: boolean;
}

interface SummaryPanelProps {
  fileId: string;
  className?: string;
  onError?: (error: string) => void;
}

const SUMMARY_TYPE_ICONS: Record<string, typeof Zap> = {
  tldr: Zap,
  abstractive: FileText,
  bullet_points: List,
  detailed: BookOpen,
  extractive: FileText,
  custom: FileText
};

const SUMMARY_TYPE_LABELS = {
  tldr: 'TL;DR',
  abstractive: 'Summary',
  bullet_points: 'Key Points',
  detailed: 'Detailed',
  extractive: 'Extractive',
  custom: 'Custom'
};

export function SummaryPanel({ fileId, className = '', onError }: SummaryPanelProps) {
  const [summaries, setSummaries] = useState<Summary[]>([]);
  const [loading, setLoading] = useState(true);
  const [generating, setGenerating] = useState<string | null>(null);
  const [selectedType, setSelectedType] = useState<string>('abstractive');

  const loadSummaries = async () => {
    try {
      setLoading(true);
      const response = await fetch(`/api/v1/summarize/document/${fileId}/summaries`);

      if (!response.ok) {
        throw new Error('Failed to load summaries');
      }

      const data = await response.json();
      setSummaries(data);

      // Select first available summary type
      if (data.length > 0) {
        setSelectedType(data[0].summary_type);
      }
    } catch (error) {
      console.error('Error loading summaries:', error);
      const message = error instanceof Error ? error.message : String(error);
      onError?.(message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadSummaries();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fileId]);

  const generateSummary = async (summaryType: string, maxWords: number = 150) => {
    try {
      setGenerating(summaryType);

      const response = await fetch(
        `/api/v1/summarize/document/${fileId}?summary_type=${summaryType}&max_words=${maxWords}`,
        { method: 'POST' }
      );

      if (!response.ok) {
        throw new Error('Failed to generate summary');
      }

      const summary = await response.json();

      // Add or update summary in list
      setSummaries(prev => {
        const filtered = prev.filter(s => s.summary_type !== summaryType);
        return [...filtered, summary];
      });

      setSelectedType(summaryType);
    } catch (error) {
      console.error('Error generating summary:', error);
      const message = error instanceof Error ? error.message : String(error);
      onError?.(message);
    } finally {
      setGenerating(null);
    }
  };

  const currentSummary = summaries.find(s => s.summary_type === selectedType);

  if (loading) {
    return (
      <div className={`flex items-center justify-center p-8 ${className}`}>
        <Loader2 className="w-6 h-6 animate-spin text-[var(--accent-primary)]" />
        <span className="ml-2 text-[var(--text-secondary)]">Loading summaries...</span>
      </div>
    );
  }

  return (
    <div className={`space-y-4 ${className}`}>
      {/* On-Device AI Badge */}
      <div className="flex items-center gap-2 px-3 py-2 bg-[var(--success-light)] border border-[var(--success-light)] rounded-lg">
        <Shield className="w-4 h-4 text-[var(--success)]" />
        <span className="text-sm font-medium text-[var(--success)]">On-Device AI</span>
        <span className="text-xs text-[var(--success)]">• 100% Private • No Cloud</span>
      </div>

      {/* Summary Type Tabs */}
      <div className="flex gap-2 border-b border-[var(--border-color)]">
        {Object.entries(SUMMARY_TYPE_LABELS).map(([type, label]) => {
          const Icon = SUMMARY_TYPE_ICONS[type];
          const hasSummary = summaries.some(s => s.summary_type === type);
          const isGenerating = generating === type;

          return (
            <button
              key={type}
              onClick={() => {
                if (hasSummary) {
                  setSelectedType(type);
                } else {
                  const maxWords = type === 'tldr' ? 50 : type === 'detailed' ? 300 : 150;
                  generateSummary(type, maxWords);
                }
              }}
              className={`
                flex items-center gap-2 px-4 py-2 border-b-2 transition-colors
                ${selectedType === type
                  ? 'border-[var(--accent-primary)] text-[var(--accent-primary)]'
                  : 'border-transparent text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
                }
                ${!hasSummary && !isGenerating ? 'opacity-60' : ''}
              `}
              disabled={isGenerating}
            >
              {isGenerating ? (
                <Loader2 className="w-4 h-4 animate-spin" />
              ) : (
                <Icon className="w-4 h-4" />
              )}
              <span className="text-sm font-medium">{label}</span>
              {!hasSummary && !isGenerating && (
                <Sparkles className="w-3 h-3 text-[var(--accent-light)]" />
              )}
            </button>
          );
        })}
      </div>

      {/* Summary Content */}
      {currentSummary ? (
        <div className="space-y-4">
          {/* Summary Text */}
          <div className="prose max-w-none">
            <div className="whitespace-pre-wrap text-[var(--text-primary)] leading-relaxed">
              {currentSummary.text}
            </div>
          </div>

          {/* Summary Metadata */}
          <div className="flex flex-wrap gap-4 p-3 bg-[var(--bg-secondary)] rounded-lg text-sm">
            <div className="flex items-center gap-2">
              <span className="text-[var(--text-secondary)]">Model:</span>
              <span className="font-medium text-[var(--text-primary)]">{currentSummary.model}</span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-[var(--text-secondary)]">Compression:</span>
              <span className="font-medium text-[var(--text-primary)]">
                {currentSummary.compression_ratio.toFixed(1)}x
              </span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-[var(--text-secondary)]">Generated:</span>
              <span className="font-medium text-[var(--text-primary)]">
                {currentSummary.generation_time.toFixed(1)}s
              </span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-[var(--text-secondary)]">Speed:</span>
              <span className="font-medium text-[var(--text-primary)]">
                {currentSummary.tokens_per_second.toFixed(1)} tok/s
              </span>
            </div>
            {currentSummary.cache_hit && (
              <div className="flex items-center gap-2 text-[var(--success)]">
                <Zap className="w-4 h-4" />
                <span className="font-medium">Cached</span>
              </div>
            )}
          </div>

          {/* Regenerate Button */}
          <button
            onClick={() => {
              const maxWords = selectedType === 'tldr' ? 50 : selectedType === 'detailed' ? 300 : 150;
              generateSummary(selectedType, maxWords);
            }}
            disabled={generating !== null}
            className="flex items-center gap-2 px-4 py-2 bg-[var(--surface-elevated)] border border-[var(--border-color)] rounded-lg hover:bg-[var(--bg-secondary)] transition-colors disabled:opacity-50"
          >
            <RefreshCw className={`w-4 h-4 ${generating === selectedType ? 'animate-spin' : ''}`} />
            <span className="text-sm font-medium">Regenerate</span>
          </button>
        </div>
      ) : generating ? (
        <div className="flex flex-col items-center justify-center p-12 space-y-4">
          <Loader2 className="w-8 h-8 animate-spin text-[var(--accent-primary)]" />
          <div className="text-center">
            <p className="text-[var(--text-primary)] font-medium">Generating summary...</p>
            <p className="text-sm text-[var(--text-secondary)] mt-1">
              This may take 5-15 seconds depending on document length
            </p>
          </div>
        </div>
      ) : (
        <div className="flex flex-col items-center justify-center p-12 space-y-4 text-center">
          <Sparkles className="w-12 h-12 text-[var(--text-tertiary)]" />
          <div>
            <p className="text-[var(--text-primary)] font-medium">No summary generated yet</p>
            <p className="text-sm text-[var(--text-secondary)] mt-1">
              Click a tab above to generate a summary
            </p>
          </div>
        </div>
      )}
    </div>
  );
}

export default SummaryPanel;
