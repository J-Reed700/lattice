import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';
import type { DocumentMetadata } from '@/types/fileBrowser';

import { useLibraryPaletteCommands } from './useLibraryPaletteCommands';

const h = vi.hoisted(() => ({ commands: [] as PaletteCommand[] }));
vi.mock('@/features/palette/hooks/useRegisterPaletteCommands', () => ({
  useRegisterPaletteCommands: (commands: PaletteCommand[]) => { h.commands = commands; },
}));

const local = { id: 'd1', fileName: 'herbal.pdf', filePath: '/lib/herbal.pdf', category: 'document', fileType: 'pdf' } as DocumentMetadata;
const web = { id: 'w1', fileName: 'Bread', filePath: 'https://example.org/bread', category: 'web', fileType: 'html' } as DocumentMetadata;

const setup = (overrides: Partial<Parameters<typeof useLibraryPaletteCommands>[0]> = {}) => {
  const props = {
    focusedDoc: local as DocumentMetadata | null,
    askAbout: vi.fn(),
    reindexDocument: vi.fn().mockResolvedValue(undefined),
    removeFromIndex: vi.fn().mockResolvedValue(undefined),
    isNeighborhoodOpen: false,
    toggleNeighborhood: vi.fn(),
    hasRunThemes: false,
    themesEnabled: true,
    isFindingThemes: false,
    handleFindThemes: vi.fn(),
    ...overrides,
  };
  renderHook(() => useLibraryPaletteCommands(props));
  const command = (id: string) => h.commands.find((entry) => entry.id === id)!;
  return { props, command };
};

describe('useLibraryPaletteCommands', () => {
  beforeEach(() => { h.commands = []; });

  it('acts on the focused document', () => {
    const { props, command } = setup();
    command('library.askAboutDocument').run();
    command('library.reindexFile').run();
    command('library.removeFromIndex').run();
    command('library.toggleRelated').run();
    command('library.findThemes').run();
    expect(props.askAbout).toHaveBeenCalledWith(local);
    expect(props.reindexDocument).toHaveBeenCalledWith(local);
    expect(props.removeFromIndex).toHaveBeenCalledWith(local);
    expect(props.toggleNeighborhood).toHaveBeenCalled();
    expect(props.handleFindThemes).toHaveBeenCalled();
    expect(h.commands.every((entry) => entry.enabled)).toBe(true);
    expect(command('library.toggleRelated').label).toBe('Show related documents');
    expect(command('library.findThemes').label).toBe('Find themes in vault');
  });

  it('cannot reindex or unindex a web page', () => {
    const { command } = setup({ focusedDoc: web });
    expect(command('library.reindexFile').enabled).toBe(false);
    expect(command('library.removeFromIndex').enabled).toBe(false);
  });

  it('disables document commands with nothing focused, and they do nothing', () => {
    const { props, command } = setup({ focusedDoc: null, isNeighborhoodOpen: true, hasRunThemes: true, isFindingThemes: true });
    for (const id of ['library.askAboutDocument', 'library.reindexFile', 'library.removeFromIndex', 'library.toggleRelated', 'library.findThemes']) {
      expect(command(id).enabled).toBe(false);
    }
    command('library.askAboutDocument').run();
    command('library.reindexFile').run();
    command('library.removeFromIndex').run();
    expect(props.askAbout).not.toHaveBeenCalled();
    expect(props.reindexDocument).not.toHaveBeenCalled();
    expect(props.removeFromIndex).not.toHaveBeenCalled();
    expect(command('library.toggleRelated').label).toBe('Hide related documents');
    expect(command('library.findThemes').label).toBe('Refresh themes');
  });
});
