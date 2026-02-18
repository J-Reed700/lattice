import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { AITab } from './AITab';
import { VaultAPI } from '../../lib/api';
import { useSettingsStore } from '../../stores/settingsStore';

vi.mock('../../stores/settingsStore');
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));
vi.mock('../../hooks/useDownloadedModels', () => ({
  useDownloadedModels: () => ({
    getActiveModel: vi.fn().mockResolvedValue(undefined),
    getActiveEmbeddingModel: vi.fn().mockResolvedValue(undefined),
  }),
}));
vi.mock('./ModelCatalog', () => ({
  ModelCatalogBrowser: () => <div data-testid="model-catalog-mock">Mocked Catalog</div>,
}));

describe('AITab', () => {
  const mockUpdateAI = vi.fn();
  const mockSettings = {
    embeddingModel: 'bge-m3' as const,
    ocrModel: 'qwen2.5-vl-2b' as const,
    useQuantization: true,
    enableAgenticRAG: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(useSettingsStore).mockImplementation((selector) => {
      const state = {
        settings: { ai: mockSettings },
        updateAI: mockUpdateAI,
      };
      return selector(state as never);
    });
  });

  it('renders AI settings header', () => {
    render(<AITab />);
    expect(screen.getByText('AI Settings')).toBeInTheDocument();
    expect(screen.getByText('Configure chat and downloadable local models')).toBeInTheDocument();
  });

  it('does not render embedding/ocr model selection cards', () => {
    render(<AITab />);

    expect(screen.queryByText('Model Selection')).not.toBeInTheDocument();
    expect(screen.queryByText('BGE-M3')).not.toBeInTheDocument();
    expect(screen.queryByText('Qwen 2.5 VL 2B')).not.toBeInTheDocument();
  });

  describe('Performance Options', () => {
    it('displays quantization checkbox with current state', () => {
      render(<AITab />);

      const checkbox = screen.getByLabelText(/Use Quantization/) as HTMLInputElement;
      expect(checkbox).toBeChecked();
    });

    it('toggles quantization when checkbox clicked', async () => {
      const user = userEvent.setup();
      render(<AITab />);

      const checkbox = screen.getByLabelText(/Use Quantization/);
      await user.click(checkbox);

      expect(mockUpdateAI).toHaveBeenCalledWith({ useQuantization: false });
    });

    it('displays quantization description', () => {
      render(<AITab />);

      expect(
        screen.getByText(/Reduce model size and improve speed with minimal quality loss/)
      ).toBeInTheDocument();
    });
  });

  describe('Agentic RAG Option', () => {
    it('displays agentic RAG checkbox with current state', () => {
      render(<AITab />);

      const checkbox = screen.getByLabelText(/Agentic RAG/) as HTMLInputElement;
      expect(checkbox).not.toBeChecked();
    });

    it('toggles agentic RAG when checkbox clicked', async () => {
      const user = userEvent.setup();
      render(<AITab />);

      const checkbox = screen.getByLabelText(/Agentic RAG/);
      await user.click(checkbox);

      expect(mockUpdateAI).toHaveBeenCalledWith({ enableAgenticRAG: true });
    });

    it('shows experimental badge for agentic RAG', () => {
      render(<AITab />);

      expect(screen.getByText('Experimental')).toBeInTheDocument();
    });

    it('displays agentic RAG description', () => {
      render(<AITab />);

      expect(
        screen.getByText(/AI agents autonomously refine searches/)
      ).toBeInTheDocument();
    });
  });

  it('renders section headers with icons', () => {
    render(<AITab />);

    expect(screen.getByText('Chat Provider')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Download Models' })).toBeInTheDocument();
    expect(screen.getByText('Options')).toBeInTheDocument();
  });

  describe('Ollama Connection', () => {
    it('shows a model dropdown and connection test button', async () => {
      render(<AITab />);

      expect(await screen.findByRole('button', { name: 'Test Connection' })).toBeInTheDocument();
      const ollamaModel = await screen.findByLabelText('Model');
      expect(ollamaModel.tagName).toBe('SELECT');
      expect(ollamaModel).toBeDisabled();
    });

    it('loads model options after a successful connection test', async () => {
      const user = userEvent.setup();
      const testConnectionMock = vi.fn().mockResolvedValueOnce({
        ok: true,
        data: {
          endpoint: '/v1/models',
          models: ['llama3.2:latest', 'qwen2.5:latest'],
        },
      });
      vi.spyOn(VaultAPI, 'testOllamaConnection').mockImplementation(testConnectionMock);

      render(<AITab />);
      const testConnection = await screen.findByRole('button', { name: 'Test Connection' });
      await user.click(testConnection);

      expect(testConnectionMock).toHaveBeenCalled();
      expect(await screen.findByRole('option', { name: 'llama3.2:latest' })).toBeInTheDocument();
      expect(await screen.findByRole('option', { name: 'qwen2.5:latest' })).toBeInTheDocument();
      expect(await screen.findByLabelText('Model')).not.toBeDisabled();
    });
  });
});
