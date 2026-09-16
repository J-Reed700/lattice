/**
 * Toast Utility Functions - Helper functions for displaying toasts
 *
 * Purpose: Provides convenient utility functions for common toast scenarios
 * Can be used anywhere in the app without requiring React hooks
 *
 * Usage:
 * ```typescript
 * import { showSuccessToast, showErrorToast } from '@/utils/toast';
 *
 * showSuccessToast('File uploaded successfully');
 * showErrorToast('Failed to upload', new Error('Network error'));
 * ```
 */

import { toast } from '../stores/toastStore';

/**
 * Show a success toast
 * @param message - The success message to display
 * @param options - Additional toast options
 */
export function showSuccessToast(
  message: string,
  options?: {
    duration?: number;
    action?: { label: string; onClick: () => void };
  }
): string {
  return toast.success(message, options);
}

/**
 * Show an error toast
 * @param message - The error message to display
 * @param error - Optional error object to extract message from
 * @param options - Additional toast options
 */
export function showErrorToast(
  message: string,
  error?: Error | unknown,
  options?: {
    duration?: number;
    action?: { label: string; onClick: () => void };
  }
): string {
  let errorMessage: string | undefined;

  if (error) {
    if (error instanceof Error) {
      errorMessage = error.message;
    } else if (typeof error === 'string') {
      errorMessage = error;
    } else if (typeof error === 'object' && error !== null && 'message' in error) {
      errorMessage = String(error.message);
    }
  }

  return toast.error(message, {
    ...options,
    message: errorMessage,
  });
}

/**
 * Show a warning toast
 * @param message - The warning message to display
 * @param options - Additional toast options
 */
export function showWarningToast(
  message: string,
  options?: {
    duration?: number;
    message?: string;
    action?: { label: string; onClick: () => void };
  }
): string {
  return toast.warning(message, options);
}

/**
 * Show an info toast
 * @param message - The info message to display
 * @param options - Additional toast options
 */
export function showInfoToast(
  message: string,
  options?: {
    duration?: number;
    message?: string;
    action?: { label: string; onClick: () => void };
  }
): string {
  return toast.info(message, options);
}

/**
 * Show a promise-based toast that updates based on promise state
 * @param promise - The promise to track
 * @param messages - Messages for each state (loading, success, error)
 */
export async function showPromiseToast<T>(
  promise: Promise<T>,
  messages: {
    loading: string;
    success: string | ((data: T) => string);
    error: string | ((error: Error) => string);
  }
): Promise<T> {
  const loadingId = toast.info(messages.loading, { duration: 0 });

  try {
    const result = await promise;
    toast.dismiss(loadingId);

    const successMessage = typeof messages.success === 'function'
      ? messages.success(result)
      : messages.success;

    toast.success(successMessage);

    return result;
  } catch (error) {
    toast.dismiss(loadingId);

    const errorMessage = typeof messages.error === 'function'
      ? messages.error(error as Error)
      : messages.error;

    toast.error(errorMessage, {
      message: error instanceof Error ? error.message : undefined,
    });

    throw error;
  }
}

/**
 * Dismiss a specific toast by ID
 * @param id - The toast ID to dismiss
 */
export function dismissToast(id: string): void {
  toast.dismiss(id);
}

/**
 * Dismiss all active toasts
 */
export function dismissAllToasts(): void {
  toast.dismissAll();
}
