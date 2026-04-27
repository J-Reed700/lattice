export class TauriError extends Error {
  constructor(message: string, public readonly code?: string) {
    super(message);
    this.name = 'TauriError';
  }
}

export function handleTauriError(err: unknown): string {
  console.error('Tauri error:', err);

  if (err instanceof Error) {
    const message = err.message.toLowerCase();

    if (message.includes('python bridge') || message.includes('python backend')) {
      return 'AI features unavailable. Python backend may not be running.';
    }

    if (message.includes('ollama')) {
      return 'LLM unavailable. Please start Ollama (run "ollama serve" in terminal).';
    }

    if (message.includes('database') || message.includes('sqlite')) {
      return 'Database error. Please restart the application.';
    }

    if (message.includes('connection refused') || message.includes('failed to connect')) {
      return 'Connection failed. Please check if required services are running.';
    }

    if (message.includes('timeout')) {
      return 'Operation timed out. Please try again.';
    }

    if (message.includes('permission denied') || message.includes('access denied')) {
      return 'Permission denied. Please check file/folder permissions.';
    }

    if (message.includes('not found') || message.includes('no such file')) {
      return 'Resource not found. It may have been moved or deleted.';
    }

    return err.message;
  }

  if (typeof err === 'string') {
    return err;
  }

  return 'An unexpected error occurred. Please try again.';
}

export function getUserFriendlyErrorMessage(err: unknown, fallback?: string): string {
  const message = handleTauriError(err);
  if (message === 'An unexpected error occurred. Please try again.') {
    return fallback || 'An error occurred';
  }
  return message || fallback || 'An error occurred';
}

// Additional helper functions for error handling
export function formatErrorDisplay(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  if (typeof error === 'object' && error !== null && 'message' in error) {
    return String((error as any).message);
  }
  return 'An unexpected error occurred';
}

export function shouldRetry(error: unknown): boolean {
  if (error instanceof Error) {
    const message = error.message.toLowerCase();
    return message.includes('timeout') ||
           message.includes('connection refused') ||
           message.includes('network') ||
           message.includes('temporarily unavailable');
  }
  return false;
}

export function getErrorCategory(error: unknown): 'network' | 'validation' | 'permission' | 'unknown' {
  if (error instanceof Error) {
    const message = error.message.toLowerCase();

    if (message.includes('network') || message.includes('connection') || message.includes('timeout')) {
      return 'network';
    }
    if (message.includes('validation') || message.includes('invalid') || message.includes('required')) {
      return 'validation';
    }
    if (message.includes('permission') || message.includes('denied') || message.includes('unauthorized')) {
      return 'permission';
    }
  }
  return 'unknown';
}
