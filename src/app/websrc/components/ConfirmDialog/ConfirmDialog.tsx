/**
 * ConfirmDialog Component
 *
 * Purpose: Request user confirmation before destructive/irreversible actions
 *
 * Use cases:
 * - Delete files/folders
 * - Clear data
 * - Reset settings
 * - Cancel operations
 *
 * Features:
 * - Multiple severity levels (danger, warning, info)
 * - Keyboard shortcuts (Enter=confirm, Esc=cancel)
 * - Focus management
 * - Loading state during async operations
 *
 * States: Idle, Loading (during async confirm)
 * Accessibility: Focus trap, ARIA dialog, keyboard navigation
 */

import { useEffect, useRef, useState } from 'react';

import { AlertTriangle, Info, AlertCircle } from 'lucide-react';

import { sanitizeFileName } from '@/utils/sanitize';

import { ButtonLoading } from '../LoadingState';

export interface ConfirmDialogProps {
  /** Dialog open state */
  isOpen: boolean;

  /** Dialog title */
  title: string;

  /** Confirmation message/question */
  message: string;

  /** Severity level */
  variant?: 'danger' | 'warning' | 'info';

  /** Confirm button label */
  confirmLabel?: string;

  /** Cancel button label */
  cancelLabel?: string;

  /** Callback when confirmed */
  onConfirm: () => void | Promise<void>;

  /** Callback when cancelled */
  onCancel: () => void;

  /** Require typing confirmation text */
  requireConfirmation?: string;

  /** Additional details/explanation */
  details?: string;

  /** Disable confirm button */
  confirmDisabled?: boolean;
}

export function ConfirmDialog({
  isOpen,
  title,
  message,
  variant = 'info',
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  onConfirm,
  onCancel,
  requireConfirmation,
  details,
  confirmDisabled = false,
}: ConfirmDialogProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [confirmText, setConfirmText] = useState('');
  const confirmButtonRef = useRef<HTMLButtonElement>(null);
  const cancelButtonRef = useRef<HTMLButtonElement>(null);

  // Focus management
  useEffect(() => {
    if (isOpen) {
      // Focus cancel button by default (safer)
      cancelButtonRef.current?.focus();
      setConfirmText(''); // Reset confirmation text
    }
  }, [isOpen]);

  // Keyboard shortcuts
  useEffect(() => {
    if (!isOpen) return;

    const canConfirm =
      !confirmDisabled &&
      !isLoading &&
      (!requireConfirmation || confirmText === requireConfirmation);

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
      } else if (e.key === 'Enter' && e.metaKey) {
        // Cmd/Ctrl+Enter to confirm
        e.preventDefault();
        if (canConfirm) {
          handleConfirm();
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen, onCancel, confirmText, requireConfirmation, confirmDisabled, isLoading]);

  const handleConfirm = async () => {
    setIsLoading(true);
    try {
      await onConfirm();
      onCancel(); // Close dialog after successful confirmation
    } catch (err) {
      console.error('Error during confirmation:', err);
      // Keep dialog open on error
    } finally {
      setIsLoading(false);
    }
  };

  const canConfirm =
    !confirmDisabled &&
    !isLoading &&
    (!requireConfirmation || confirmText === requireConfirmation);

  if (!isOpen) return null;

  const variantStyles = {
    danger: {
      icon: <AlertTriangle className="w-6 h-6" strokeWidth={1.75} />,
      iconBg: 'bg-[hsl(var(--danger-muted))]',
      iconColor: 'text-[hsl(var(--danger-fg))]',
      buttonBg: 'bg-[hsl(var(--danger))] hover:opacity-90',
      buttonText: 'text-[hsl(var(--accent-fg))]',
    },
    warning: {
      icon: <AlertCircle className="w-6 h-6" strokeWidth={1.75} />,
      iconBg: 'bg-[hsl(var(--warning-muted))]',
      iconColor: 'text-[hsl(var(--warning-fg))]',
      buttonBg: 'bg-[hsl(var(--warning))] hover:opacity-90',
      buttonText: 'text-[hsl(var(--accent-fg))]',
    },
    info: {
      icon: <Info className="w-6 h-6" strokeWidth={1.75} />,
      iconBg: 'bg-[hsl(var(--accent-muted))]',
      iconColor: 'text-[hsl(var(--accent))]',
      buttonBg: 'bg-[hsl(var(--accent))] hover:bg-[hsl(var(--accent-hover))]',
      buttonText: 'text-[hsl(var(--accent-fg))]',
    },
  };

  const styles = variantStyles[variant];

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-[hsl(var(--overlay))] z-50 animate-in fade-in duration-base"
        onClick={onCancel}
        aria-hidden="true"
      />

      {/* Dialog */}
      <div
        className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 z-50 w-full max-w-md animate-in zoom-in-95 duration-base"
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
        aria-describedby="confirm-dialog-description"
      >
        <div className="bg-[hsl(var(--surface-raised))] rounded-lg shadow-md border border-[hsl(var(--border-subtle))] p-6">
          {/* Icon and Title */}
          <div className="flex items-start gap-4 mb-4">
            <div
              className={`flex-shrink-0 w-12 h-12 rounded-full ${styles.iconBg} ${styles.iconColor} flex items-center justify-center`}
            >
              {styles.icon}
            </div>
            <div className="flex-1 pt-1">
              <h2
                id="confirm-dialog-title"
                className="text-lg font-semibold text-[hsl(var(--text-primary))]"
              >
                {title}
              </h2>
            </div>
          </div>

          {/* Message */}
          <div className="mb-6 pl-16">
            <p
              id="confirm-dialog-description"
              className="text-sm text-[hsl(var(--text-secondary))] mb-2"
            >
              {message}
            </p>

            {/* Details */}
            {details && (
              <p className="text-xs text-[hsl(var(--text-tertiary))] mt-2">
                {details}
              </p>
            )}

            {/* Confirmation Input */}
            {requireConfirmation && (
              <div className="mt-4">
                <label
                  htmlFor="confirm-text"
                  className="block text-xs font-medium text-[hsl(var(--text-secondary))] mb-2"
                >
                  Type <span className="font-mono font-bold">{requireConfirmation}</span> to
                  confirm:
                </label>
                <input
                  id="confirm-text"
                  type="text"
                  value={confirmText}
                  onChange={(e) => setConfirmText(e.target.value)}
                  className="w-full px-3 py-2 border border-[hsl(var(--border-default))] rounded-md bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))] text-sm focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                  placeholder={requireConfirmation}
                  disabled={isLoading}
                  autoComplete="off"
                />
              </div>
            )}
          </div>

          {/* Actions */}
          <div className="flex items-center gap-3 justify-end">
            <button
              ref={cancelButtonRef}
              onClick={onCancel}
              disabled={isLoading}
              className="px-4 py-2 text-sm font-medium text-[hsl(var(--text-secondary))] bg-[hsl(var(--surface))] border border-[hsl(var(--border-subtle))] rounded-md hover:bg-[hsl(var(--surface-raised))] transition-colors duration-fast disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {cancelLabel}
            </button>
            <button
              ref={confirmButtonRef}
              onClick={handleConfirm}
              disabled={!canConfirm}
              className={`px-4 py-2 text-sm font-medium rounded-md transition-colors duration-fast disabled:opacity-50 disabled:cursor-not-allowed ${styles.buttonBg} ${styles.buttonText}`}
            >
              {isLoading ? (
                <ButtonLoading>{confirmLabel}</ButtonLoading>
              ) : (
                <>
                  {confirmLabel}
                  {!requireConfirmation && (
                    <span className="ml-2 text-xs opacity-60">⌘↵</span>
                  )}
                </>
              )}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}

/**
 * Preset confirmation dialogs for common actions
 */

export function DeleteFileDialog({
  isOpen,
  fileName,
  onConfirm,
  onCancel,
}: {
  isOpen: boolean;
  fileName: string;
  onConfirm: () => void | Promise<void>;
  onCancel: () => void;
}) {
  return (
    <ConfirmDialog
      isOpen={isOpen}
      variant="danger"
      title="Delete file?"
      message={`Are you sure you want to delete "${sanitizeFileName(fileName)}"? This action cannot be undone.`}
      confirmLabel="Delete"
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}

export function ClearDataDialog({
  isOpen,
  dataType,
  onConfirm,
  onCancel,
}: {
  isOpen: boolean;
  dataType: string;
  onConfirm: () => void | Promise<void>;
  onCancel: () => void;
}) {
  return (
    <ConfirmDialog
      isOpen={isOpen}
      variant="warning"
      title={`Clear ${dataType}?`}
      message={`This will permanently delete all ${dataType}. This action cannot be undone.`}
      confirmLabel="Clear"
      onConfirm={onConfirm}
      onCancel={onCancel}
      requireConfirmation="CLEAR"
    />
  );
}

export function ResetSettingsDialog({
  isOpen,
  onConfirm,
  onCancel,
}: {
  isOpen: boolean;
  onConfirm: () => void | Promise<void>;
  onCancel: () => void;
}) {
  return (
    <ConfirmDialog
      isOpen={isOpen}
      variant="warning"
      title="Reset all settings?"
      message="This will restore all settings to their default values. Your documents will not be affected."
      details="You may need to restart the application for all changes to take effect."
      confirmLabel="Reset"
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}
