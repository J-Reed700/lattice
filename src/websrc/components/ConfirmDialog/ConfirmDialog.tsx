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

  const confirmButtonClass =
    variant === 'danger'
      ? 'bg-danger text-accent-fg hover:opacity-90'
      : variant === 'warning'
        ? 'bg-warning text-accent-fg hover:opacity-90'
        : 'bg-accent text-accent-fg hover:bg-accent-hover';

  return (
    <>
      <div
        className="fixed inset-0 z-50 bg-overlay animate-in fade-in duration-fast"
        onClick={onCancel}
        aria-hidden="true"
      />

      <div
        className="fixed left-1/2 top-1/2 z-50 w-full max-w-sm -translate-x-1/2 -translate-y-1/2 animate-in fade-in zoom-in-95 duration-fast"
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
        aria-describedby="confirm-dialog-description"
      >
        <div className="rounded-lg border border-border-subtle bg-surface-raised p-5 shadow-md">
          <h2 id="confirm-dialog-title" className="font-serif text-base font-semibold text-text-primary">
            {title}
          </h2>
          <p id="confirm-dialog-description" className="mt-2 text-sm text-text-secondary">
            {message}
          </p>
          {details && <p className="mt-1 text-xs text-text-muted">{details}</p>}

          {requireConfirmation && (
            <div className="mt-4">
              <label htmlFor="confirm-text" className="block text-xs text-text-secondary">
                Type <span className="font-mono text-text-primary">{requireConfirmation}</span> to confirm
              </label>
              <input
                id="confirm-text"
                type="text"
                value={confirmText}
                onChange={(e) => setConfirmText(e.target.value)}
                className="mt-1.5 h-8 w-full rounded-sm border border-border-default bg-bg px-2.5 text-sm text-text-primary outline-none transition-colors duration-fast focus:border-accent"
                placeholder={requireConfirmation}
                disabled={isLoading}
                autoComplete="off"
              />
            </div>
          )}

          <div className="mt-6 flex items-center justify-end gap-2">
            <button
              ref={cancelButtonRef}
              type="button"
              onClick={onCancel}
              disabled={isLoading}
              className="inline-flex h-8 items-center rounded-md px-3 text-sm text-text-secondary transition-colors duration-fast hover:bg-surface hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-50"
            >
              {cancelLabel}
            </button>
            <button
              ref={confirmButtonRef}
              type="button"
              onClick={handleConfirm}
              disabled={!canConfirm}
              className={`inline-flex h-8 items-center gap-2 rounded-md px-3 text-sm font-medium transition-colors duration-fast disabled:cursor-not-allowed disabled:opacity-50 ${confirmButtonClass}`}
            >
              {isLoading ? (
                <ButtonLoading>{confirmLabel}</ButtonLoading>
              ) : (
                <>
                  {confirmLabel}
                  {!requireConfirmation && <kbd className="font-mono text-xxs opacity-70">⌘↵</kbd>}
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
