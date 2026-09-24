import { beforeEach, describe, expect, it, vi } from 'vitest';

import { diagnostics } from './diagnostics';

describe('local diagnostics', () => {
  beforeEach(() => diagnostics.clear());
  it('captures structured errors, causes, and redacts nested secrets and private content', () => {
    const cause = new Error('Disk full');
    diagnostics.capture({ code: 'FILE_WRITE_ERROR', message: 'Cannot save', cause }, 'API · save', { token: 'secret123', prompt: 'private document' });
    const exported = diagnostics.export();
    expect(exported).toContain('FILE_WRITE_ERROR');
    expect(exported).toContain('Disk full');
    expect(exported).not.toContain('secret123');
    expect(exported).not.toContain('private document');
  });
  it('redacts secrets embedded in console messages', () => {
    diagnostics.record('error', 'failed {"token":"supersecret"} Bearer abc123 /Users/josh/private', 'Console');
    expect(diagnostics.export()).not.toMatch(/supersecret|abc123|josh/);
  });
  it('coalesces consecutive repeats while retaining occurrence counts', () => {
    diagnostics.capture('offline', 'network');
    diagnostics.capture('offline', 'network');
    expect(diagnostics.getSnapshot()).toHaveLength(1);
    expect(diagnostics.getSnapshot()[0].count).toBe(2);
  });
  it('bounds storage, survives circular details, and clears persistence', () => {
    const circular: Record<string, unknown> = {}; circular.self = circular;
    for (let i = 0; i < 305; i++) diagnostics.record('info', `event ${i}`, 'test', circular);
    expect(diagnostics.getSnapshot()).toHaveLength(300);
    expect(diagnostics.getSnapshot()[0].details).toContain('[Circular]');
    diagnostics.flush();
    expect(JSON.parse(localStorage.getItem('lattice-diagnostics-v1')!)).toHaveLength(300);
    diagnostics.clear();
    expect(localStorage.getItem('lattice-diagnostics-v1')).toBe('[]');
  });
  it('does not throw if persistence is unavailable or a subscriber fails', () => {
    const spy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('quota'); });
    const unsubscribe = diagnostics.subscribe(() => { throw new Error('broken listener'); });
    expect(() => { diagnostics.capture('failure', 'test'); diagnostics.flush(); }).not.toThrow();
    expect(diagnostics.getSnapshot()[0].message).toBe('failure');
    unsubscribe(); spy.mockRestore();
  });
});
