/**
 * Rename Dialog Component
 *
 * Purpose: Allow users to rename a document's display name
 *
 * Features:
 * - Simple input field with current name
 * - Validation (non-empty, reasonable length)
 * - Cancel and Save buttons
 * - Toast notifications for feedback
 * - Auto-close on completion
 */

import { useState, useEffect, useRef } from 'react';

import { X } from 'lucide-react';

import VaultAPI from '../../lib/api';
import { toast } from '../../stores/toastStore';
import { type DocumentMetadata } from '../../types/fileBrowser';
import Button from '../ui/Button/Button';
import { Input } from '../ui/input';

interface RenameDialogProps {
  document: DocumentMetadata;
  onClose: () => void;
  onSuccess: () => void;
}

export function RenameDialog({ document, onClose, onSuccess }: RenameDialogProps) {
  const [newName, setNewName] = useState(document.fileName);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus input and select text on mount
  useEffect(() => {
    if (inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, []);

  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };

    window.document.addEventListener('keydown', handleEscape);
    return () => {
      window.document.removeEventListener('keydown', handleEscape);
    };
  }, [onClose]);

  const validateName = (name: string): string | null => {
    const trimmed = name.trim();

    if (trimmed.length === 0) {
      return 'Document name cannot be empty';
    }

    if (trimmed.length > 255) {
      return 'Document name cannot exceed 255 characters';
    }

    if (trimmed.includes('/') || trimmed.includes('\\')) {
      return 'Document name cannot contain path separators (/ or \\)';
    }

    return null;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    const validationError = validateName(newName);
    if (validationError) {
      setError(validationError);
      return;
    }

    if (newName.trim() === document.fileName) {
      onClose();
      return;
    }

    setIsSaving(true);
    setError(null);

    try {
      const result = await VaultAPI.renameDocument(document.id, newName.trim());

      if (result.ok) {
        toast.success('Document renamed successfully', {
          message: result.data.message,
        });
        onSuccess();
        onClose();
      } else {
        setError(result.error || 'Failed to rename document');
        toast.error('Failed to rename document', {
          message: result.error,
        });
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setError(errorMessage);
      toast.error('Failed to rename document', {
        message: errorMessage,
      });
    } finally {
      setIsSaving(false);
    }
  };

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setNewName(e.target.value);
    setError(null); // Clear error on input change
  };

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-[hsl(var(--overlay))]/50 z-50"
        onClick={onClose}
        aria-hidden="true"
      />

      {/* Dialog */}
      <div
        className="fixed top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 z-50 w-full max-w-md"
        role="dialog"
        aria-labelledby="rename-dialog-title"
        aria-modal="true"
      >
        <div className="bg-[hsl(var(--surface-raised))] rounded-lg shadow-md border border-[hsl(var(--border-subtle))] overflow-hidden">
          {/* Header */}
          <div className="flex items-center justify-between px-6 py-4 border-b border-[hsl(var(--border-subtle))]">
            <h2 id="rename-dialog-title" className="text-lg font-semibold">
              Rename Document
            </h2>
            <button
              onClick={onClose}
              className="p-1 rounded hover:bg-[hsl(var(--surface-raised))] transition-colors duration-fast"
              aria-label="Close dialog"
              disabled={isSaving}
            >
              <X className="w-5 h-5" />
            </button>
          </div>

          {/* Form */}
          <form onSubmit={handleSubmit}>
            <div className="p-6 space-y-4">
              <div>
                <label
                  htmlFor="document-name"
                  className="block text-sm font-medium mb-2"
                >
                  Document Name
                </label>
                <Input
                  ref={inputRef}
                  id="document-name"
                  type="text"
                  value={newName}
                  onChange={handleInputChange}
                  placeholder="Enter document name"
                  disabled={isSaving}
                  className={error ? 'border-[hsl(var(--danger-fg))]' : ''}
                  autoComplete="off"
                />
                {error && (
                  <p className="mt-2 text-sm text-[hsl(var(--danger-fg))]" role="alert">
                    {error}
                  </p>
                )}
              </div>

              <div className="text-sm text-[hsl(var(--text-secondary))]">
                <p>Current name: {document.fileName}</p>
              </div>
            </div>

            {/* Footer */}
            <div className="flex items-center justify-end gap-3 px-6 py-4 bg-[hsl(var(--surface))] border-t border-[hsl(var(--border-subtle))]">
              <Button
                type="button"
                variant="ghost"
                onClick={onClose}
                disabled={isSaving}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                variant="primary"
                disabled={isSaving || !!error}
              >
                {isSaving ? 'Saving...' : 'Save'}
              </Button>
            </div>
          </form>
        </div>
      </div>
    </>
  );
}
