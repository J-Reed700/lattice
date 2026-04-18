/**
 * ProgressManager Component
 *
 * Main progress overlay that shows all active operations.
 * Collapsible, positioned at bottom-right, with keyboard support.
 */

import { memo, useEffect, useMemo, useState } from 'react';

import { ChevronDown, ChevronUp, CheckCircle2 } from 'lucide-react';

import { ProgressCard } from './ProgressCard';
import { useProgressStore} from '../../stores/progressStore';

export interface ProgressManagerProps {
  /** Custom className */
  className?: string;
}

export const ProgressManager = memo<ProgressManagerProps>(({ className = '' }) => {
  const operations = useProgressStore((state) => Array.from(state.operations.values()));
  const isCollapsed = useProgressStore((state) => state.isCollapsed);
  const toggleCollapsed = useProgressStore((state) => state.toggleCollapsed);
  const clearCompleted = useProgressStore((state) => state.clearCompleted);
  const [isVisible, setIsVisible] = useState(false);

  useEffect(() => {
    if (operations.length > 0) {
      const timer = setTimeout(() => setIsVisible(true), 10);
      return () => clearTimeout(timer);
    } else {
      setIsVisible(false);
    }
  }, [operations.length]);

  const activeOperations = useMemo(
    () => operations.filter((op) => op.status === 'running' || op.status === 'pending'),
    [operations]
  );

  const completedOperations = useMemo(
    () =>
      operations.filter(
        (op) =>
          op.status === 'completed' || op.status === 'failed' || op.status === 'cancelled'
      ),
    [operations]
  );

  const hasOperations = operations.length > 0;
  const hasActive = activeOperations.length > 0;
  const hasCompleted = completedOperations.length > 0;

  // Keyboard support: Esc to collapse
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !isCollapsed && hasOperations) {
        toggleCollapsed();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isCollapsed, hasOperations, toggleCollapsed]);

  // Auto-expand when new operations start
  useEffect(() => {
    if (hasActive && operations.length === 1) {
      useProgressStore.getState().setCollapsed(false);
    }
  }, [hasActive, operations.length]);

  if (!hasOperations) {
    return null;
  }

  return (
    <div
      className={`
        fixed bottom-4 right-4 z-50
        w-96 max-w-[calc(100vw-2rem)]
        transition-colors duration-fast ease-out
        ${isVisible ? 'opacity-100 translate-y-0' : 'opacity-0 translate-y-24'}
        ${className}
      `}
      role="region"
      aria-label="Progress indicator"
    >
        <div className="bg-[hsl(var(--surface-raised))] rounded-lg shadow-md border border-[hsl(var(--border-subtle))] overflow-hidden">
          {/* Header */}
          <div className="flex items-center justify-between p-3 bg-[hsl(var(--surface))]/50 border-b border-[hsl(var(--border-subtle))]">
            <div className="flex items-center gap-2">
              <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">
                Operations
              </h3>
              {hasActive && (
                <span className="px-2 py-0.5 text-xs font-medium bg-[hsl(var(--accent-muted))]/30 text-[hsl(var(--accent))] rounded-full">
                  {activeOperations.length} active
                </span>
              )}
              {hasCompleted && !hasActive && (
                <span className="px-2 py-0.5 text-xs font-medium bg-[hsl(var(--success-muted))]/30 text-[hsl(var(--success-fg))] rounded-full">
                  {completedOperations.length} completed
                </span>
              )}
            </div>

            <div className="flex items-center gap-1">
              {hasCompleted && !hasActive && (
                <button
                  onClick={clearCompleted}
                  className="p-1.5 hover:bg-[hsl(var(--surface-raised))] rounded transition-colors"
                  aria-label="Clear completed"
                  title="Clear completed"
                >
                  <CheckCircle2 className="w-4 h-4 text-[hsl(var(--text-tertiary))]" />
                </button>
              )}
              <button
                onClick={toggleCollapsed}
                className="p-1.5 hover:bg-[hsl(var(--surface-raised))] rounded transition-colors"
                aria-label={isCollapsed ? 'Expand' : 'Collapse'}
                title={isCollapsed ? 'Expand' : 'Collapse'}
              >
                {isCollapsed ? (
                  <ChevronUp className="w-4 h-4 text-[hsl(var(--text-tertiary))]" />
                ) : (
                  <ChevronDown className="w-4 h-4 text-[hsl(var(--text-tertiary))]" />
                )}
              </button>
            </div>
          </div>

          {/* Content */}
          {!isCollapsed && (
            <div className="overflow-hidden animate-in slide-in-from-top duration-200">
              <div className="max-h-96 overflow-y-auto p-3 space-y-2">
                  {/* Active Operations */}
                  {activeOperations.map((operation) => (
                    <ProgressCard key={operation.id} operation={operation} detailed />
                  ))}

                  {/* Completed Operations */}
                  {hasCompleted && (
                    <>
                      {hasActive && (
                        <div className="pt-2 pb-1">
                          <div className="text-xs font-medium text-[hsl(var(--text-secondary))] uppercase tracking-wide">
                            Recent
                          </div>
                        </div>
                      )}
                      {completedOperations.map((operation) => (
                        <ProgressCard key={operation.id} operation={operation} />
                      ))}
                    </>
                  )}
                </div>
            </div>
          )}

          {/* Collapsed Summary */}
          {isCollapsed && (
            <div className="p-3">
              <div className="text-sm text-[hsl(var(--text-secondary))]">
                {hasActive
                  ? `${activeOperations.length} operation${
                      activeOperations.length > 1 ? 's' : ''
                    } in progress...`
                  : `${completedOperations.length} operation${
                      completedOperations.length > 1 ? 's' : ''
                    } completed`}
              </div>
              {hasActive && (
                <div className="mt-2 space-y-1">
                  {activeOperations.slice(0, 2).map((op) => (
                    <div
                      key={op.id}
                      className="text-xs text-[hsl(var(--text-secondary))] truncate"
                    >
                      {op.type}: {Math.round(op.progress)}%
                    </div>
                  ))}
                  {activeOperations.length > 2 && (
                    <div className="text-xs text-[hsl(var(--text-tertiary))]">
                      +{activeOperations.length - 2} more
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </div>
    </div>
  );
});

ProgressManager.displayName = 'ProgressManager';
