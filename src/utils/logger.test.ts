import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { diagnostics } from './diagnostics';
import { createLogger, logger } from './logger';

const initial = { enabled: logger.isEnabled(), level: logger.getLogLevel() };

beforeEach(() => {
  diagnostics.clear();
  logger.setEnabled(true);
  logger.setLogLevel('debug');
  for (const level of ['debug', 'info', 'warn', 'error'] as const) vi.spyOn(console, level).mockImplementation(() => {});
});
afterEach(() => {
  logger.setEnabled(initial.enabled);
  logger.setLogLevel(initial.level);
  vi.restoreAllMocks();
});

describe('logger', () => {
  it('prints each level to its console method with component and action tags', () => {
    logger.debug('Proofing', { component: 'Oven', action: 'heat' });
    logger.info('Ready');
    logger.warn('Hot', { component: 'Oven', temperature: 250 }, 'extra');
    logger.error('Burnt', { action: 'bake' });
    expect(console.debug).toHaveBeenCalledWith(expect.stringMatching(/DEBUG \[Oven\] \[heat\]: Proofing$/), '');
    expect(console.info).toHaveBeenCalledWith(expect.stringMatching(/INFO: Ready$/), '');
    expect(console.warn).toHaveBeenCalledWith(expect.stringMatching(/WARN \[Oven\]: Hot$/), { temperature: 250 }, 'extra');
    expect(console.error).toHaveBeenCalledWith(expect.stringMatching(/ERROR \[bake\]: Burnt$/), '');
  });

  it('adds an error’s message and stack to the context', () => {
    const failure = new Error('Fire alarm');
    logger.error('Kitchen closed', { component: 'Kitchen' }, failure);
    expect(console.error).toHaveBeenCalledWith(expect.stringContaining('Kitchen closed'), { error: 'Fire alarm', stack: failure.stack });
  });

  it('prints nothing below the set level, but still records it', () => {
    logger.setLogLevel('warn');
    expect(logger.getLogLevel()).toBe('warn');
    logger.info('Quiet');
    logger.warn('Loud');
    expect(console.info).not.toHaveBeenCalled();
    expect(console.warn).toHaveBeenCalled();
    expect(diagnostics.getSnapshot().map((e) => e.message)).toEqual(['Loud', 'Quiet']);
  });

  it('prints nothing while disabled, but still records it', () => {
    logger.setEnabled(false);
    expect(logger.isEnabled()).toBe(false);
    logger.error('Recorded only');
    expect(console.error).not.toHaveBeenCalled();
    expect(diagnostics.getSnapshot()[0]).toMatchObject({ level: 'error', message: 'Recorded only', source: 'Application' });
  });

  it('scopes a logger to a component', () => {
    const scoped = createLogger('Pantry');
    scoped.debug('Counting');
    scoped.info('Stocked', { action: 'restock' });
    scoped.warn('Low on rice');
    scoped.error('Spoiled', {}, new Error('mould'));
    expect(diagnostics.getSnapshot().map((e) => e.source)).toEqual(['Pantry', 'Pantry', 'Pantry', 'Pantry']);
    expect(console.info).toHaveBeenCalledWith(expect.stringMatching(/\[Pantry\] \[restock\]: Stocked$/), '');
    expect(console.error).toHaveBeenCalledWith(expect.stringContaining('Spoiled'), expect.objectContaining({ error: 'mould' }));
  });
});
