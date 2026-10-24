import { open } from '@tauri-apps/plugin-dialog';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { filesApi } from './client';
import { documentApi } from './documents';

describe('filesApi pickers', () => {
  beforeEach(() => {
    vi.mocked(open).mockReset();
    vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  it('carries every document command', () => {
    expect(filesApi.indexFile).toBe(documentApi.indexFile);
  });

  it('asks for one file and returns its path', async () => {
    vi.mocked(open).mockResolvedValueOnce('/recipes/bread.md');
    await expect(filesApi.selectFile()).resolves.toBe('/recipes/bread.md');
    expect(open).toHaveBeenCalledWith({ directory: false, multiple: false, title: 'Select File' });
  });

  it('returns null when the file dialog fails', async () => {
    vi.mocked(open).mockRejectedValueOnce(new Error('dialog closed'));
    await expect(filesApi.selectFile()).resolves.toBeNull();
  });

  it('returns every chosen path for a multi-select', async () => {
    vi.mocked(open).mockResolvedValueOnce(['/a.md', '/b.md'] as never);
    await expect(filesApi.selectMultipleFiles()).resolves.toEqual(['/a.md', '/b.md']);
    expect(open).toHaveBeenCalledWith({ directory: false, multiple: true, title: 'Select Files' });
  });

  it('returns no paths when a multi-select is cancelled', async () => {
    vi.mocked(open).mockResolvedValueOnce(null);
    await expect(filesApi.selectMultipleFiles()).resolves.toEqual([]);
  });

  it('returns no paths when a multi-select fails', async () => {
    vi.mocked(open).mockRejectedValueOnce(new Error('denied'));
    await expect(filesApi.selectMultipleFiles()).resolves.toEqual([]);
  });

  it('asks for a folder', async () => {
    vi.mocked(open).mockResolvedValueOnce('/field-notes');
    await expect(filesApi.selectFolder()).resolves.toBe('/field-notes');
    expect(open).toHaveBeenCalledWith({ directory: true, multiple: false, title: 'Select Folder' });
  });

  it('returns null when the folder dialog fails', async () => {
    vi.mocked(open).mockRejectedValueOnce(new Error('denied'));
    await expect(filesApi.selectFolder()).resolves.toBeNull();
  });
});
