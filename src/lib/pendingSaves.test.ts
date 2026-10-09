import { afterEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';

import { flushPendingSaves, registerPendingSave } from './pendingSaves';

const unregister: Array<() => void> = [];
function register(save: () => Promise<boolean>) {
  const remove = registerPendingSave(save);
  unregister.push(remove);
  return remove;
}
afterEach(() => { unregister.splice(0).forEach(remove => remove()); });

describe('pending save barrier', () => {
  it('allows navigation when no editor has pending work', async () => {
    expect(await flushPendingSaves()).toBe(true);
  });

  it('starts all saves and waits for the slowest acknowledgement', async () => {
    const first = deferred<boolean>();
    const last = deferred<boolean>();
    const a = vi.fn(() => first.promise);
    const b = vi.fn(() => last.promise);
    register(a); register(b);
    const settled = vi.fn();
    const flushing = flushPendingSaves().then(settled);
    await Promise.resolve();
    expect(a).toHaveBeenCalledOnce();
    expect(b).toHaveBeenCalledOnce();
    first.resolve(true);
    await Promise.resolve();
    expect(settled).not.toHaveBeenCalled();
    last.resolve(true);
    await flushing;
    expect(settled).toHaveBeenCalledWith(true);
  });

  it.each(['refused', 'rejected', 'thrown'] as const)('blocks navigation on a %s save without skipping other editors', async failure => {
    register(() => {
      if (failure === 'thrown') throw new Error('Disk full');
      return failure === 'refused' ? Promise.resolve(false) : Promise.reject(new Error('Disk full'));
    });
    const sibling = vi.fn(async () => true);
    register(sibling);
    expect(await flushPendingSaves()).toBe(false);
    expect(sibling).toHaveBeenCalledOnce();
  });

  it('retains a failed save for an explicit retry', async () => {
    const save = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    register(save);
    expect(await flushPendingSaves()).toBe(false);
    expect(await flushPendingSaves()).toBe(true);
    expect(save).toHaveBeenCalledTimes(2);
  });

  it('unregisters idempotently and does not call an unmounted editor', async () => {
    const save = vi.fn(async () => true);
    const remove = register(save);
    remove(); remove();
    expect(await flushPendingSaves()).toBe(true);
    expect(save).not.toHaveBeenCalled();
  });

  it('takes a stable snapshot when an editor registers during a flush', async () => {
    const later = vi.fn(async () => true);
    register(async () => { register(later); return true; });
    expect(await flushPendingSaves()).toBe(true);
    expect(later).not.toHaveBeenCalled();
    expect(await flushPendingSaves()).toBe(true);
    expect(later).toHaveBeenCalledOnce();
  });
});
