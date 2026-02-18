/**
 * ProgressCard Component
 *
 * Displays individual operation progress with status, actions, and ETA.
 * Supports cancellation for cancellable operations.
 */

import React, { memo, useMemo } from 'react';

import {
  Loader2,
  CheckCircle2,
  XCircle,
  X,
  Upload,
  FileText,
  Search,
  Download,
  Eye,
  AlertCircle,
} from 'lucide-react';

import { ProgressBar } from './ProgressBar';
import { useProgressStore } from '../../stores/progressStore';
import { type ProgressOperation, type OperationType } from '../../types/progress';

export interface ProgressCardProps {
  operation: ProgressOperation;
  /** Show detailed view */
  detailed?: boolean;
  /** Custom className */
  className?: string;
}

// Operation type icons
const operationIcons: Record<OperationType, React.ReactNode> = {
  upload: <Upload className="w-4 h-4" />,
  indexing: <FileText className="w-4 h-4" />,
  search: <Search className="w-4 h-4" />,
  export: <Download className="w-4 h-4" />,
  ocr: <Eye className="w-4 h-4" />,
};

// Format ETA in human-readable form
function formatETA(seconds: number): string {
  if (seconds < 60) {
    return `${Math.round(seconds)}s`;
  } else if (seconds < 3600) {
    const minutes = Math.floor(seconds / 60);
    return `${minutes}m`;
  } else {
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    return `${hours}h ${minutes}m`;
  }
}

// Format elapsed time
function formatElapsed(startTime: Date, endTime?: Date): string {
  const end = endTime || new Date();
  const elapsed = Math.floor((end.getTime() - startTime.getTime()) / 1000);
  return formatETA(elapsed);
}

export const ProgressCard = memo<ProgressCardProps>(
  ({ operation, detailed = false, className = '' }) => {
    const { cancelOperation, removeOperation } = useProgressStore();

    // Status icon and color
    const statusConfig = useMemo(() => {
      switch (operation.status) {
        case 'running':
          return {
            icon: <Loader2 className="w-4 h-4 animate-spin" />,
            color: 'text-[var(--accent-primary)]',
            bgColor: 'bg-[var(--accent-light)]/20',
            borderColor: 'border-[var(--accent-light)]',
          };
        case 'completed':
          return {
            icon: <CheckCircle2 className="w-4 h-4" />,
            color: 'text-[var(--success)]',
            bgColor: 'bg-[var(--success-light)]/20',
            borderColor: 'border-[var(--success-light)]',
          };
        case 'failed':
          return {
            icon: <XCircle className="w-4 h-4" />,
            color: 'text-[var(--error)]',
            bgColor: 'bg-[var(--error-light)]/20',
            borderColor: 'border-[var(--error-light)]',
          };
        case 'cancelled':
          return {
            icon: <AlertCircle className="w-4 h-4" />,
            color: 'text-[var(--text-secondary)]',
            bgColor: 'bg-[var(--bg-secondary)]/20',
            borderColor: 'border-[var(--border-color)]',
          };
        case 'pending':
        default:
          return {
            icon: <Loader2 className="w-4 h-4" />,
            color: 'text-[var(--text-secondary)]',
            bgColor: 'bg-[var(--bg-secondary)]/20',
            borderColor: 'border-[var(--border-color)]',
          };
      }
    }, [operation.status]);

    const handleCancel = () => {
      if (operation.cancellable) {
        cancelOperation(operation.id);
      }
    };

    const handleRemove = () => {
      removeOperation(operation.id);
    };

    const isActive = operation.status === 'running' || operation.status === 'pending';
    const isDone =
      operation.status === 'completed' ||
      operation.status === 'failed' ||
      operation.status === 'cancelled';

    return (
      <div
        className={`
          ${statusConfig.bgColor}
          border ${statusConfig.borderColor}
          rounded-lg p-3
          transition-all duration-200
          ${className}
        `}
      >
        {/* Header */}
        <div className="flex items-start justify-between gap-2 mb-2">
          <div className="flex items-center gap-2 flex-1 min-w-0">
            <div className={statusConfig.color}>{operationIcons[operation.type]}</div>
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium text-[var(--text-primary)] capitalize">
                  {operation.type}
                </span>
                <div className={statusConfig.color}>{statusConfig.icon}</div>
              </div>
              {operation.total > 0 && (
                <div className="text-xs text-[var(--text-secondary)]">
                  {operation.current} / {operation.total}
                </div>
              )}
            </div>
          </div>

          <div className="flex items-center gap-1">
            {isActive && operation.cancellable && (
              <button
                onClick={handleCancel}
                className="p-1 hover:bg-[var(--surface-elevated)] rounded transition-colors"
                aria-label="Cancel operation"
                title="Cancel"
              >
                <X className="w-4 h-4 text-[var(--text-tertiary)]" />
              </button>
            )}
            {isDone && (
              <button
                onClick={handleRemove}
                className="p-1 hover:bg-[var(--surface-elevated)] rounded transition-colors"
                aria-label="Remove operation"
                title="Remove"
              >
                <X className="w-4 h-4 text-[var(--text-tertiary)]" />
              </button>
            )}
          </div>
        </div>

        {/* Message */}
        <p className="text-sm text-[var(--text-secondary)] mb-2 truncate" title={operation.message}>
          {operation.message}
        </p>

        {/* Progress Bar */}
        {isActive && (
          <ProgressBar
            progress={operation.progress}
            variant={operation.status === 'running' ? 'default' : 'indeterminate'}
            height={6}
            className="mb-2"
          />
        )}

        {/* Footer Info */}
        <div className="flex items-center justify-between text-xs text-[var(--text-secondary)]">
          <div className="flex items-center gap-3">
            {operation.progress > 0 && operation.progress < 100 && (
              <span>{Math.round(operation.progress)}%</span>
            )}
            {operation.eta && operation.eta > 0 && (
              <span>ETA: {formatETA(operation.eta)}</span>
            )}
            {operation.endTime && (
              <span>Took {formatElapsed(operation.startTime, operation.endTime)}</span>
            )}
          </div>
          {detailed && operation.errors && operation.errors.length > 0 && (
            <span className="text-[var(--error)]">{operation.errors.length} errors</span>
          )}
        </div>

        {/* Errors (detailed view only) */}
        {detailed && operation.errors && operation.errors.length > 0 && (
          <div className="mt-2 pt-2 border-t border-[var(--border-color)]">
            <div className="text-xs text-[var(--error)] space-y-1">
              {operation.errors.slice(0, 3).map((error, idx) => (
                <div key={idx} className="truncate" title={error}>
                  {error}
                </div>
              ))}
              {operation.errors.length > 3 && (
                <div className="text-[var(--text-tertiary)]">+{operation.errors.length - 3} more</div>
              )}
            </div>
          </div>
        )}
      </div>
    );
  }
);

ProgressCard.displayName = 'ProgressCard';
