/**
 * useToast Hook - Easy-to-use hook for displaying toast notifications
 *
 * Purpose: Provides a convenient API for showing toasts from any component
 *
 * Usage:
 * ```tsx
 * const { toast } = useToast();
 *
 * toast.success('File uploaded successfully');
 * toast.error('Failed to delete file', { duration: 5000 });
 * toast.warning('Indexing paused');
 * toast.info('New update available', {
 *   action: {
 *     label: 'Update Now',
 *     onClick: () => handleUpdate()
 *   }
 * });
 * ```
 */

import { useCallback } from 'react';

import { toast as toastAPI, type Toast } from '../stores/toastStore';

export interface UseToastReturn {
  toast: {
    success: (_title: string, _options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>) => string;
    error: (_title: string, _options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>) => string;
    warning: (_title: string, _options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>) => string;
    info: (_title: string, _options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>) => string;
    dismiss: (_id: string) => void;
    dismissAll: () => void;
  };
}

/**
 * Hook for displaying toast notifications
 * Returns toast API with methods for each toast type
 */
export function useToast(): UseToastReturn {
  const success = useCallback((
    title: string,
    options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>
  ) => toastAPI.success(title, options), []);

  const error = useCallback((
    title: string,
    options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>
  ) => toastAPI.error(title, options), []);

  const warning = useCallback((
    title: string,
    options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>
  ) => toastAPI.warning(title, options), []);

  const info = useCallback((
    title: string,
    options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>
  ) => toastAPI.info(title, options), []);

  const dismiss = useCallback((id: string) => {
    toastAPI.dismiss(id);
  }, []);

  const dismissAll = useCallback(() => {
    toastAPI.dismissAll();
  }, []);

  return {
    toast: {
      success,
      error,
      warning,
      info,
      dismiss,
      dismissAll,
    },
  };
}
