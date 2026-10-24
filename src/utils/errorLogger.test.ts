import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { diagnostics } from './diagnostics';
import {
  clearErrorQueue,
  downloadErrorLog,
  exportErrorLog,
  getErrorQueue,
  getRecentErrors,
  logComponentError,
  logDatabaseError,
  logError,
  logNetworkError,
} from './errorLogger';

beforeEach(() => diagnostics.clear());
afterEach(() => vi.restoreAllMocks());

describe('errorLogger', () => {
  it('records errors under their component, else their action, else the application', () => {
    logError(new Error('Oven offline'), { component: 'Timer' });
    logError(new Error('Dough collapsed'), { action: 'proof' });
    logError(new Error('Out of flour'));
    expect(diagnostics.getSnapshot().map((e) => [e.source, e.message])).toEqual([
      ['Application', 'Out of flour'], ['proof', 'Dough collapsed'], ['Timer', 'Oven offline'],
    ]);
  });

  it('maps warning and info levels', () => {
    logError(new Error('Salt is low'), undefined, 'warning');
    logError(new Error('Bread is ready'), undefined, 'info');
    expect(diagnostics.getSnapshot().map((e) => e.level)).toEqual(['info', 'warn']);
  });

  it('lists only errors as recent errors and in the queue', () => {
    logError(new Error('Kettle failed'), { component: 'Kitchen' });
    logError(new Error('Just a note'), undefined, 'info');
    expect(getRecentErrors()).toEqual([{ message: 'Kettle failed', type: 'Kitchen', timestamp: expect.any(String) }]);
    expect(getErrorQueue()).toEqual([expect.objectContaining({ message: 'Kettle failed', errorType: 'Kitchen', level: 'error', url: '', userAgent: '' })]);
    clearErrorQueue();
    expect(getRecentErrors()).toEqual([]);
  });

  it('records component, network and database errors with their context', () => {
    logComponentError(new Error('Render failed'), { componentStack: '\n at RecipeCard' } as React.ErrorInfo, 'RecipeCard');
    logNetworkError(new Error('Timed out'), '/api/recipes', 504);
    logDatabaseError(new Error('Locked'), 'insert', 'recipes');
    const [database, network, component] = diagnostics.getSnapshot();
    expect(component).toMatchObject({ source: 'RecipeCard', message: 'Render failed' });
    expect(component.details).toContain('RecipeCard');
    expect(network).toMatchObject({ source: 'network_request', message: 'Timed out' });
    expect(network.details).toContain('504');
    expect(database).toMatchObject({ source: 'database_operation', message: 'Locked' });
    expect(database.details).toContain('recipes');
  });

  it('exports the log and downloads it as a JSON file', () => {
    logError(new Error('Burnt'));
    expect(JSON.parse(exportErrorLog()).entries[0]).toMatchObject({ message: 'Burnt' });

    const createObjectURL = vi.fn(() => 'blob:log');
    const revokeObjectURL = vi.fn();
    Object.assign(URL, { createObjectURL, revokeObjectURL });
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    downloadErrorLog();
    expect(createObjectURL).toHaveBeenCalledWith(expect.any(Blob));
    const anchor = click.mock.contexts[0] as HTMLAnchorElement;
    expect(anchor.download).toMatch(/^lattice-error-log-.*\.json$/);
    expect(anchor.href).toBe('blob:log');
    expect(anchor.isConnected).toBe(false);
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:log');
  });
});
