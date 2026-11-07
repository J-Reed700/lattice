import { afterEach, expect, it, vi } from 'vitest';

import { SANITIZED_MESSAGE, SENSITIVE_MESSAGE } from './__tests__/sensitiveMessage';
import { diagnostics } from './diagnostics';
import { installGlobalDiagnostics } from './globalDiagnostics';
import { toast } from '../stores/toastStore';

vi.mock('../stores/toastStore', () => ({ toast: { error: vi.fn() } }));
let dispose = () => {};
afterEach(() => { dispose(); diagnostics.clear(); vi.restoreAllMocks(); });
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
