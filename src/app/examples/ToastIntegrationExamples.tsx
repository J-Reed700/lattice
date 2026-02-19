/**
 * Toast Integration Examples
 *
 * This file demonstrates how to integrate toast notifications
 * throughout the Recall/Vault desktop application.
 *
 * Purpose: Provide reference implementations for common scenarios
 */

import React, { useState } from 'react';
import { useToast } from '../hooks/useToast';
import {
  showSuccessToast,
  showErrorToast,
  showWarningToast,
  showInfoToast,
  showPromiseToast,
} from '../utils/toast';
import VaultAPI from '../lib/api';

/**
 * Example 1: Settings Save
 * Show success/error feedback when saving user settings
 */
export function SettingsSaveExample() {
  const { toast } = useToast();
  const [settings, setSettings] = useState({ theme: 'dark', notifications: true });

  const handleSave = async () => {
    try {
      // Save settings logic here
      await new Promise((resolve) => setTimeout(resolve, 1000)); // Simulate API call

      toast.success('Settings saved successfully');
    } catch (error) {
      toast.error('Failed to save settings', {
        message: error instanceof Error ? error.message : 'Unknown error',
        duration: 5000,
      });
    }
  };

  return (
    <button onClick={handleSave} className="px-4 py-2 bg-blue-500 text-white rounded">
      Save Settings
    </button>
  );
}

/**
 * Example 2: File Upload
 * Show progress, success, and error feedback during file upload
 */
export function FileUploadExample() {
  const { toast } = useToast();

  const handleUpload = async (files: FileList) => {
    const uploadPromise = (async () => {
      // Simulate upload
      await new Promise((resolve) => setTimeout(resolve, 2000));
      return { count: files.length, success: true };
    })();

    try {
      const result = await showPromiseToast(uploadPromise, {
        loading: 'Uploading files...',
        success: (data) => `Uploaded ${data.count} file${data.count > 1 ? 's' : ''} successfully`,
        error: 'Upload failed',
      });
    } catch (error) {
      toast.error('Upload failed', {
        message: 'Please check your connection and try again',
        action: {
          label: 'Retry',
          onClick: () => handleUpload(files),
        },
        duration: 6000,
      });
    }
  };

  return (
    <input
      type="file"
      multiple
      onChange={(e) => e.target.files && handleUpload(e.target.files)}
    />
  );
}

/**
 * Example 3: File Delete with Undo
 * Show success toast with undo action
 */
export function FileDeleteExample({ fileId, fileName }: { fileId: string; fileName: string }) {
  const { toast } = useToast();
  const [deletedFile, setDeletedFile] = useState<{ id: string; data: any } | null>(null);

  const handleDelete = async () => {
    try {
      // Store file data for undo
      const fileData = { /* ... file data ... */ };
      setDeletedFile({ id: fileId, data: fileData });

      // Delete file
      await VaultAPI.deleteFile(fileId);

      toast.success(`"${fileName}" deleted`, {
        action: {
          label: 'Undo',
          onClick: () => handleUndo(fileId, fileData),
        },
        duration: 5000,
      });
    } catch (error) {
      toast.error('Failed to delete file', {
        message: error instanceof Error ? error.message : undefined,
      });
    }
  };

  const handleUndo = async (id: string, data: any) => {
    try {
      // Restore file
      await VaultAPI.restoreFile(id, data);
      setDeletedFile(null);

      toast.success('File restored');
    } catch (error) {
      toast.error('Failed to restore file');
    }
  };

  return (
    <button onClick={handleDelete} className="px-3 py-1 text-red-600 hover:bg-red-50 rounded">
      Delete
    </button>
  );
}

/**
 * Example 4: Indexing Status
 * Show warning when indexing is paused or info about progress
 */
export function IndexingStatusExample() {
  const { toast } = useToast();

  const handlePauseIndexing = () => {
    showWarningToast('Indexing paused', {
      message: 'Resume indexing to keep your vault up to date',
      action: {
        label: 'Resume',
        onClick: () => {
          showInfoToast('Indexing resumed');
        },
      },
      duration: 0, // Don't auto-dismiss
    });
  };

  const handleIndexingComplete = (filesIndexed: number) => {
    showSuccessToast('Indexing complete', {
      message: `${filesIndexed} files indexed successfully`,
    });
  };

  return (
    <div className="space-x-2">
      <button onClick={handlePauseIndexing}>Pause Indexing</button>
      <button onClick={() => handleIndexingComplete(42)}>Complete Indexing</button>
    </div>
  );
}

/**
 * Example 5: Form Validation Errors
 * Show multiple validation errors
 */
export function FormValidationExample() {
  const { toast } = useToast();

  const handleSubmit = (formData: { name: string; email: string }) => {
    const errors: string[] = [];

    if (!formData.name) errors.push('Name is required');
    if (!formData.email) errors.push('Email is required');
    if (formData.email && !formData.email.includes('@')) errors.push('Invalid email format');

    if (errors.length > 0) {
      errors.forEach((error) => {
        toast.error(error, { duration: 3000 });
      });
      return;
    }

    toast.success('Form submitted successfully');
  };

  return null; // Form component would be here
}

/**
 * Example 6: Update Available
 * Show info toast with action to update
 */
export function UpdateAvailableExample() {
  const { toast } = useToast();

  const checkForUpdates = () => {
    const updateAvailable = true; // Check for updates

    if (updateAvailable) {
      showInfoToast('New update available', {
        message: 'Version 1.2.0 is ready to install',
        action: {
          label: 'Update Now',
          onClick: () => {
            showPromiseToast(
              new Promise((resolve) => setTimeout(resolve, 3000)),
              {
                loading: 'Installing update...',
                success: 'Update installed successfully. Please restart.',
                error: 'Update failed',
              }
            );
          },
        },
        duration: 0, // Don't auto-dismiss
      });
    }
  };

  return <button onClick={checkForUpdates}>Check for Updates</button>;
}

/**
 * Example 7: Batch Operations
 * Show progress for batch operations with multiple files
 */
export function BatchOperationsExample() {
  const { toast } = useToast();

  const handleBatchSummarize = async (fileIds: string[]) => {
    const toastId = toast.info(`Summarizing ${fileIds.length} files...`, {
      duration: 0,
    });

    try {
      let completed = 0;

      for (const fileId of fileIds) {
        await VaultAPI.summarizeFile(fileId);
        completed++;

        // Update progress (dismiss old, show new)
        toast.dismiss(toastId);
        toast.info(`Summarizing files... (${completed}/${fileIds.length})`);
      }

      toast.dismiss(toastId);
      toast.success(`Successfully summarized ${completed} files`);
    } catch (error) {
      toast.dismiss(toastId);
      toast.error('Batch summarization failed', {
        message: 'Some files may not have been processed',
        action: {
          label: 'View Details',
          onClick: () => {
            // Show error details
          },
        },
      });
    }
  };

  return null;
}

/**
 * Example 8: Copy to Clipboard
 * Show quick success feedback for clipboard operations
 */
export function CopyToClipboardExample({ text }: { text: string }) {
  const { toast } = useToast();

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success('Copied to clipboard', { duration: 2000 });
    } catch (error) {
      toast.error('Failed to copy', { duration: 2000 });
    }
  };

  return (
    <button onClick={handleCopy} className="px-3 py-1 text-sm hover:bg-gray-100 rounded">
      Copy
    </button>
  );
}

/**
 * Example 9: Network Connection Status
 * Show warning when offline, info when back online
 */
export function NetworkStatusExample() {
  const { toast } = useToast();

  React.useEffect(() => {
    const handleOnline = () => {
      showSuccessToast('Connection restored', { duration: 3000 });
    };

    const handleOffline = () => {
      showWarningToast('No internet connection', {
        message: 'Some features may be unavailable',
        duration: 0,
      });
    };

    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);

    return () => {
      window.removeEventListener('online', handleOnline);
      window.removeEventListener('offline', handleOffline);
    };
  }, []);

  return null;
}

/**
 * Example 10: Keyboard Shortcut Feedback
 * Show info toast when user uses keyboard shortcuts
 */
export function KeyboardShortcutExample() {
  const { toast } = useToast();

  const handleShortcut = (action: string) => {
    toast.info(action, { duration: 1500 });
  };

  React.useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'd') {
        e.preventDefault();
        handleShortcut('Quick capture opened');
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  return null;
}
