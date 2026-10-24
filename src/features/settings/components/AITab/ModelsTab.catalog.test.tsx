import { open } from '@tauri-apps/plugin-dialog';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';
import { deferred } from '@/tests/deferred';

import { ModelsTab } from './ModelsTab';

const h = vi.hoisted(() => ({
  llm: { llmSettings: null as null | Record<string, any>, saveLlmUpdates: vi.fn() },
  path: { modelDownloadPath: '/Users/ada/models', isLoadingModelDownloadPath: false, loadModelDownloadPath: vi.fn() },
  selectedModel: null as unknown,
  commands: [] as PaletteCommand[],
  catalogProps: {} as Record<string, any>,
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

vi.mock('./useLlmSettings', () => ({ useLlmSettings: () => h.llm }));
vi.mock('@/features/model/hooks/useModelDownloadPath', () => ({ useModelDownloadPath: () => h.path }));
vi.mock('@/features/model/stores/modelCatalogStore', () => ({
  useModelCatalogStore: (selector: (state: unknown) => unknown) => selector({ selectedModel: h.selectedModel }),
}));
vi.mock('@/features/palette/hooks/useRegisterPaletteCommands', () => ({
  useRegisterPaletteCommands: (commands: PaletteCommand[]) => { h.commands = commands; },
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('../ModelCatalog', () => ({
  ModelCatalogBrowser: (props: Record<string, any>) => { h.catalogProps = props; return <div data-testid="catalog" />; },
}));
vi.mock('../HuggingFaceSettings', () => ({ HuggingFaceSettings: () => <div data-testid="hf-settings" /> }));

const router = { model: 'tiny-router', enabled: true };

describe('ModelsTab catalog and folders', () => {
  const scrollIntoView = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    Element.prototype.scrollIntoView = scrollIntoView;
    h.llm.llmSettings = { router, externalModelDirectories: ['/Volumes/Archive/gguf'] };
    h.llm.saveLlmUpdates.mockResolvedValue(true);
    h.path = { modelDownloadPath: '/Users/ada/models', isLoadingModelDownloadPath: false, loadModelDownloadPath: vi.fn() };
    h.selectedModel = null;
  });

  it('shows the catalog with the router model and hides it on request', () => {
    render(<ModelsTab />);
    expect(screen.getByTestId('catalog')).toBeInTheDocument();
    expect(h.catalogProps.routerModelId).toBe('tiny-router');
    fireEvent.click(screen.getByRole('button', { name: 'Hide catalog' }));
    expect(screen.queryByTestId('catalog')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Browse catalog' })).toHaveAttribute('aria-expanded', 'false');
  });

  it('reopens the catalog from the palette', () => {
    render(<ModelsTab />);
    fireEvent.click(screen.getByRole('button', { name: 'Hide catalog' }));
    act(() => h.commands.find((command) => command.id === 'models.browseCatalog')!.run());
    expect(screen.getByTestId('catalog')).toBeInTheDocument();
    expect(scrollIntoView).toHaveBeenCalledWith({ block: 'start', behavior: 'smooth' });
  });

  it('scrolls to the catalog when a model is chosen', () => {
    h.selectedModel = { model: { id: 'saga' } };
    render(<ModelsTab />);
    expect(scrollIntoView).toHaveBeenCalled();
  });

  it('sets the router model from the catalog', async () => {
    render(<ModelsTab />);
    await act(() => h.catalogProps.onSetRouterModel('saga-1b'));
    expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ router: { ...router, model: 'saga-1b' } });
  });

  it('does not set a router model before settings load', async () => {
    h.llm.llmSettings = null;
    render(<ModelsTab />);
    expect(h.catalogProps.routerModelId).toBeUndefined();
    await act(() => h.catalogProps.onSetRouterModel('saga-1b'));
    expect(h.llm.saveLlmUpdates).not.toHaveBeenCalled();
    expect(screen.getByText('Already have model files? Add a folder to make them available in Lattice.')).toBeInTheDocument();
  });

  it('opens the downloaded models tab', () => {
    const listener = vi.fn();
    window.addEventListener('settings:navigate-tab', listener);
    render(<ModelsTab />);
    fireEvent.click(screen.getByRole('button', { name: 'Downloaded models' }));
    expect((listener.mock.calls[0][0] as CustomEvent).detail).toEqual({ tab: 'downloaded-models' });
    window.removeEventListener('settings:navigate-tab', listener);
  });

  it('shows the downloads folder while it loads', () => {
    h.path = { ...h.path, isLoadingModelDownloadPath: true };
    render(<ModelsTab />);
    expect(screen.getByText('Loading…')).toBeInTheDocument();
  });

  describe('external folders', () => {
    it('lists saved folders and removes one', async () => {
      render(<ModelsTab />);
      expect(screen.getByText('/Volumes/Archive/gguf')).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Remove external folder /Volumes/Archive/gguf' }));
      await waitFor(() => expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ externalModelDirectories: [] }));
    });

    it('adds a picked folder', async () => {
      const picked = deferred<string | null>();
      vi.mocked(open).mockReturnValueOnce(picked.promise as never);
      render(<ModelsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Add folder' }));
      expect(screen.getByRole('button', { name: 'Adding…' })).toBeDisabled();
      await act(async () => { picked.resolve('/Volumes/New/models'); await picked.promise; });
      await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('External model folder added'));
      expect(open).toHaveBeenCalledWith({ directory: true, multiple: false, title: 'Select Model Folder' });
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ externalModelDirectories: ['/Volumes/Archive/gguf', '/Volumes/New/models'] });
      expect(screen.getByRole('button', { name: 'Add folder' })).toBeEnabled();
    });

    it('ignores a cancelled pick and a folder already added', async () => {
      vi.mocked(open).mockResolvedValueOnce(null);
      render(<ModelsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Add folder' }));
      await waitFor(() => expect(screen.getByRole('button', { name: 'Add folder' })).toBeEnabled());
      vi.mocked(open).mockResolvedValueOnce('/Volumes/Archive/gguf');
      fireEvent.click(screen.getByRole('button', { name: 'Add folder' }));
      await waitFor(() => expect(h.toast.info).toHaveBeenCalledWith('Folder already added'));
      expect(h.llm.saveLlmUpdates).not.toHaveBeenCalled();
    });

    it('reports a picker failure', async () => {
      vi.mocked(open).mockRejectedValueOnce('dialog unavailable');
      render(<ModelsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Add folder' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't add that folder", { message: 'dialog unavailable' }));
    });

    it('does not save folders before settings load', async () => {
      h.llm.llmSettings = null;
      vi.mocked(open).mockResolvedValueOnce('/Volumes/New/models');
      render(<ModelsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Add folder' }));
      await waitFor(() => expect(h.toast.success).toHaveBeenCalled());
      expect(h.llm.saveLlmUpdates).not.toHaveBeenCalled();
    });
  });
});
