import { invoke } from '@tauri-apps/api/core';
import { expect, it, vi } from 'vitest';

import { diagnostics } from '../utils/diagnostics';

const { VaultAPI } = await vi.importActual<typeof import('./api')>('./api');
it('records backend error details without logging request contents and keeps the failure contract', async () => {
  diagnostics.clear();
  vi.mocked(invoke).mockRejectedValueOnce(JSON.stringify({ code: 'DATABASE_ERROR', message: 'Database is locked', details: { operation: 'search' } }));
  const result = await VaultAPI.searchSemantic('private search text');
  expect(result).toMatchObject({ ok: false, error: 'Database is locked' });
  expect(diagnostics.getSnapshot()[0].message).toBe('Database is locked');
  expect(diagnostics.export()).toContain('DATABASE_ERROR');
  expect(diagnostics.export()).not.toContain('private search text');
});
