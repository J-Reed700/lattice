export function toError(error: unknown): Error {
  if (error instanceof Error) {
    return error;
  }

  if (typeof error === 'string') {
    return new Error(error);
  }

  if (typeof error === 'object' && error !== null) {
    if ('message' in error && typeof error.message === 'string') {
      const err = new Error(error.message);
      if ('stack' in error && typeof error.stack === 'string') {
        err.stack = error.stack;
      }
      return err;
    }

    try {
      return new Error(JSON.stringify(error));
    } catch {
      return new Error('Unknown error object');
    }
  }

  return new Error(String(error));
}

export function isError(value: unknown): value is Error {
  return value instanceof Error;
}

export function isString(value: unknown): value is string {
  return typeof value === 'string';
}

export function isNumber(value: unknown): value is number {
  return typeof value === 'number' && !isNaN(value);
}

export function isDefined<T>(value: T | null | undefined): value is T {
  return value !== null && value !== undefined;
}
