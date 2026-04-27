import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import { handleTauriError, getUserFriendlyErrorMessage, TauriError } from './errorHandler';

describe('errorHandler', () => {
  let consoleErrorSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(() => {
    consoleErrorSpy.mockRestore();
  });

  describe('handleTauriError', () => {
    it('should handle Python bridge errors', () => {
      const error = new Error('Python bridge connection failed');
      const result = handleTauriError(error);

      expect(result).toBe('AI features unavailable. Python backend may not be running.');
      expect(consoleErrorSpy).toHaveBeenCalledWith('Tauri error:', error);
    });

    it('should handle Python backend errors', () => {
      const error = new Error('Failed to connect to Python backend');
      const result = handleTauriError(error);

      expect(result).toBe('AI features unavailable. Python backend may not be running.');
    });

    it('should handle Ollama errors', () => {
      const error = new Error('Ollama service not available');
      const result = handleTauriError(error);

      expect(result).toBe('LLM unavailable. Please start Ollama (run "ollama serve" in terminal).');
    });

    it('should handle database errors', () => {
      const error = new Error('Database connection failed');
      const result = handleTauriError(error);

      expect(result).toBe('Database error. Please restart the application.');
    });

    it('should handle SQLite errors', () => {
      const error = new Error('SQLite database is locked');
      const result = handleTauriError(error);

      expect(result).toBe('Database error. Please restart the application.');
    });

    it('should handle connection refused errors', () => {
      const error = new Error('Connection refused to localhost:8080');
      const result = handleTauriError(error);

      expect(result).toBe('Connection failed. Please check if required services are running.');
    });

    it('should handle timeout errors', () => {
      const error = new Error('Operation timeout after 30s');
      const result = handleTauriError(error);

      expect(result).toBe('Operation timed out. Please try again.');
    });

    it('should handle permission denied errors', () => {
      const error = new Error('Permission denied: cannot write to file');
      const result = handleTauriError(error);

      expect(result).toBe('Permission denied. Please check file/folder permissions.');
    });

    it('should handle access denied errors', () => {
      const error = new Error('Access denied to resource');
      const result = handleTauriError(error);

      expect(result).toBe('Permission denied. Please check file/folder permissions.');
    });

    it('should handle not found errors', () => {
      const error = new Error('File not found: /path/to/file');
      const result = handleTauriError(error);

      expect(result).toBe('Resource not found. It may have been moved or deleted.');
    });

    it('should handle no such file errors', () => {
      const error = new Error('No such file or directory');
      const result = handleTauriError(error);

      expect(result).toBe('Resource not found. It may have been moved or deleted.');
    });

    it('should return original message for unknown errors', () => {
      const error = new Error('Some specific error message');
      const result = handleTauriError(error);

      expect(result).toBe('Some specific error message');
    });

    it('should handle string errors', () => {
      const result = handleTauriError('String error message');

      expect(result).toBe('String error message');
    });

    it('should handle unknown error types', () => {
      const result = handleTauriError({ unknown: 'error' });

      expect(result).toBe('An unexpected error occurred. Please try again.');
    });

    it('should handle null/undefined errors', () => {
      const resultNull = handleTauriError(null);
      const resultUndefined = handleTauriError(undefined);

      expect(resultNull).toBe('An unexpected error occurred. Please try again.');
      expect(resultUndefined).toBe('An unexpected error occurred. Please try again.');
    });

    it('should be case-insensitive for error matching', () => {
      const error1 = new Error('PYTHON BRIDGE UNAVAILABLE');
      const error2 = new Error('Ollama Connection Failed');
      const error3 = new Error('DATABASE ERROR');

      expect(handleTauriError(error1)).toContain('AI features unavailable');
      expect(handleTauriError(error2)).toContain('LLM unavailable');
      expect(handleTauriError(error3)).toContain('Database error');
    });
  });

  describe('getUserFriendlyErrorMessage', () => {
    it('should return user-friendly message for known errors', () => {
      const error = new Error('Python bridge unavailable');
      const result = getUserFriendlyErrorMessage(error);

      expect(result).toBe('AI features unavailable. Python backend may not be running.');
    });

    it('should use fallback message when provided', () => {
      const error = { unknown: 'type' };
      const result = getUserFriendlyErrorMessage(error, 'Custom fallback message');

      expect(result).toBe('Custom fallback message');
    });

    it('should use default fallback when no custom fallback provided', () => {
      const error = { unknown: 'type' };
      const result = getUserFriendlyErrorMessage(error);

      expect(result).toBe('An error occurred');
    });

    it('should prioritize error message over fallback for known errors', () => {
      const error = new Error('Ollama service down');
      const result = getUserFriendlyErrorMessage(error, 'Fallback message');

      expect(result).not.toBe('Fallback message');
      expect(result).toContain('Ollama');
    });
  });

  describe('TauriError', () => {
    it('should create TauriError with message', () => {
      const error = new TauriError('Test error message');

      expect(error).toBeInstanceOf(Error);
      expect(error).toBeInstanceOf(TauriError);
      expect(error.message).toBe('Test error message');
      expect(error.name).toBe('TauriError');
    });

    it('should create TauriError with code', () => {
      const error = new TauriError('Test error', 'ERR_CODE');

      expect(error.code).toBe('ERR_CODE');
    });

    it('should have undefined code when not provided', () => {
      const error = new TauriError('Test error');

      expect(error.code).toBeUndefined();
    });
  });

  describe('Error Pattern Matching', () => {
    it('should match multiple keywords in error messages', () => {
      const errors = [
        new Error('The Python bridge service is not responding'),
        new Error('Cannot connect to the Python backend server'),
        new Error('Python bridge initialization failed'),
      ];

      errors.forEach((error) => {
        expect(handleTauriError(error)).toContain('AI features unavailable');
      });
    });

    it('should prioritize more specific error messages', () => {
      const specificError = new Error('Python bridge unavailable');
      const genericError = new Error('Something went wrong');

      const specificResult = handleTauriError(specificError);
      const genericResult = handleTauriError(genericError);

      expect(specificResult).not.toBe(genericResult);
      expect(specificResult).toContain('Python backend');
      expect(genericResult).toBe('Something went wrong');
    });

    it('should handle errors with stack traces', () => {
      const error = new Error('Database error');
      error.stack = 'Error: Database error\n    at someFunction (file.ts:10:5)';

      const result = handleTauriError(error);

      expect(result).toBe('Database error. Please restart the application.');
    });
  });

  describe('Console Logging', () => {
    it('should always log errors to console', () => {
      const error = new Error('Test error');
      handleTauriError(error);

      expect(consoleErrorSpy).toHaveBeenCalledWith('Tauri error:', error);
    });

    it('should log non-Error objects', () => {
      const notAnError = { message: 'Not an error object' };
      handleTauriError(notAnError);

      expect(consoleErrorSpy).toHaveBeenCalledWith('Tauri error:', notAnError);
    });
  });
});
