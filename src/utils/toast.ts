import { toast } from '../stores/toastStore';

export function showSuccessToast(
  message: string,
  options?: {
    duration?: number;
    action?: { label: string; onClick: () => void };
  }
): string {
  return toast.success(message, options);
}

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

export function dismissToast(id: string): void {
  toast.dismiss(id);
}

export function dismissAllToasts(): void {
  toast.dismissAll();
}
