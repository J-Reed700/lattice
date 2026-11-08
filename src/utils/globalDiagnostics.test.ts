import { afterEach, expect, it, vi } from 'vitest';

import { SANITIZED_MESSAGE, SENSITIVE_MESSAGE } from './__tests__/sensitiveMessage';
import { diagnostics } from './diagnostics';
import { installGlobalDiagnostics } from './globalDiagnostics';
import { toast } from '../stores/toastStore';

vi.mock('../stores/toastStore', () => ({ toast: { error: vi.fn() } }));
let dispose = () => {};
afterEach(() => { dispose(); diagnostics.clear(); vi.clearAllMocks(); vi.restoreAllMocks(); });
it('captures uncaught exceptions and rejections with accurate messages and throttled notices', () => {
  diagnostics.clear();
  dispose = installGlobalDiagnostics();
  window.dispatchEvent(new ErrorEvent('error', { error: new Error('Render callback failed') }));
  const rejection = new Event('unhandledrejection');
  Object.defineProperty(rejection, 'reason', { value: { code: 'NETWORK_ERROR', message: 'Service offline' } });
  window.dispatchEvent(rejection);
  expect(diagnostics.getSnapshot().map(entry => entry.message)).toEqual(['Service offline', 'Render callback failed']);
  expect(toast.error).toHaveBeenCalledTimes(1);
  dispose();
  expect(diagnostics.getSnapshot()).toHaveLength(2);
});
it('captures legacy console failures once and restores console on cleanup', () => {
  diagnostics.clear();
  const original = vi.spyOn(console, 'error').mockImplementation(() => {});
  dispose = installGlobalDiagnostics();
  installGlobalDiagnostics();
  console.error('Could not open file', { code: 'PERMISSION_DENIED' });
  expect(diagnostics.getSnapshot()).toHaveLength(1);
  expect(diagnostics.getSnapshot()[0].details).toContain('PERMISSION_DENIED');
  expect(original).toHaveBeenCalledTimes(1);
  dispose();
  expect(console.error).toBe(original);
});
it('shows the sanitized message in the toast but keeps the log entry', () => {
  diagnostics.clear();
  dispose = installGlobalDiagnostics();
  window.dispatchEvent(new ErrorEvent('error', { error: new Error(SENSITIVE_MESSAGE) }));
  expect(toast.error).toHaveBeenCalledWith('An unexpected error occurred', { message: SANITIZED_MESSAGE, duration: 8000 });
  expect(diagnostics.getSnapshot()[0].message).toContain('alice@example.com');
});
it('points to the logs when the toast message is empty', () => {
  diagnostics.clear();
  dispose = installGlobalDiagnostics();
  window.dispatchEvent(new ErrorEvent('error', { error: new Error('') }));
  expect(toast.error).toHaveBeenCalledWith('An unexpected error occurred', {
    message: 'Open Settings → Logs for details.',
    duration: 8000,
  });
});
it('logs the browser\'s resize-loop notice as a warning and does not tell the user about it', () => {
  diagnostics.clear();
  dispose = installGlobalDiagnostics();
  window.dispatchEvent(new ErrorEvent('error', { message: 'ResizeObserver loop completed with undelivered notifications.' }));
  window.dispatchEvent(new ErrorEvent('error', { message: 'ResizeObserver loop limit exceeded' }));
  expect(toast.error).not.toHaveBeenCalled();
  expect(diagnostics.getSnapshot().map(entry => [entry.level, entry.message])).toEqual([
    ['warn', 'ResizeObserver loop limit exceeded'],
    ['warn', 'ResizeObserver loop completed with undelivered notifications.'],
  ]);
});
it('still tells the user about the next error after a resize-loop notice', () => {
  diagnostics.clear();
  dispose = installGlobalDiagnostics();
  window.dispatchEvent(new ErrorEvent('error', { message: 'ResizeObserver loop completed with undelivered notifications.' }));
  window.dispatchEvent(new ErrorEvent('error', { error: new Error('Render callback failed') }));
  expect(vi.mocked(toast.error).mock.calls).toEqual([
    ['An unexpected error occurred', { message: 'Render callback failed', duration: 8000 }],
  ]);
});
it('treats a thrown error as an error even when its text is the resize-loop notice', () => {
  diagnostics.clear();
  dispose = installGlobalDiagnostics();
  window.dispatchEvent(new ErrorEvent('error', { error: new Error('ResizeObserver loop completed with undelivered notifications.') }));
  expect(toast.error).toHaveBeenCalledTimes(1);
  expect(diagnostics.getSnapshot()[0].level).toBe('error');
});
