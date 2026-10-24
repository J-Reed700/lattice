import { fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ModelRecommendation } from '@/types/modelCatalog';

import { CATALOG_PAGE_SIZE, ModelCatalogResults } from './ModelCatalogResults';

const rec = (id: string, category: string) => ({ model: { id, name: id, category } }) as unknown as ModelRecommendation;
const renderModel = (model: ModelRecommendation) => <p key={model.model.id}>{model.model.name}</p>;

describe('ModelCatalogResults', () => {
  const scrollIntoView = vi.fn();

  beforeEach(() => {
    Element.prototype.scrollIntoView = scrollIntoView;
  });

  afterEach(() => {
    scrollIntoView.mockReset();
  });

  describe('overview', () => {
    const models = [
      rec('chat-1', 'LLM'), rec('chat-2', 'LLM'), rec('chat-3', 'LLM'), rec('chat-4', 'LLM'),
      rec('embed-1', 'Embedding'),
      rec('listen-1', 'Transcription'),
    ];

    it('previews three models per purpose and hides empty purposes', () => {
      render(<ModelCatalogResults models={models} renderModel={renderModel} overview page={1} onPageChange={vi.fn()} onBrowseCategory={vi.fn()} />);
      const chat = screen.getByRole('region', { name: 'Chat & writing' });
      expect(within(chat).getByText('chat-3')).toBeInTheDocument();
      expect(within(chat).queryByText('chat-4')).not.toBeInTheDocument();
      expect(within(chat).getByText('View all 4')).toBeInTheDocument();
      expect(screen.getByRole('region', { name: 'Search & retrieval' })).toBeInTheDocument();
      expect(screen.getByRole('region', { name: 'Audio transcription' })).toBeInTheDocument();
      expect(screen.queryByRole('region', { name: 'Document reading' })).not.toBeInTheDocument();
    });

    it('opens every model of a purpose', () => {
      const onBrowseCategory = vi.fn();
      render(<ModelCatalogResults models={models} renderModel={renderModel} overview page={1} onPageChange={vi.fn()} onBrowseCategory={onBrowseCategory} />);
      fireEvent.click(screen.getByRole('button', { name: 'View all search & retrieval models' }));
      expect(onBrowseCategory).toHaveBeenCalledWith('Embedding');
    });
  });

  describe('paged list', () => {
    const many = Array.from({ length: CATALOG_PAGE_SIZE * 2 + 3 }, (_v, i) => rec(`model-${i + 1}`, 'LLM'));

    it('shows one page with its range and moves between pages', () => {
      const onPageChange = vi.fn();
      render(<ModelCatalogResults models={many} renderModel={renderModel} overview={false} page={2} onPageChange={onPageChange} onBrowseCategory={vi.fn()} />);
      expect(screen.getByRole('status')).toHaveTextContent(`Showing ${CATALOG_PAGE_SIZE + 1}–${CATALOG_PAGE_SIZE * 2} of ${many.length} models`);
      expect(screen.getByText('Page 2 of 3')).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Previous' }));
      fireEvent.click(screen.getByRole('button', { name: 'Next' }));
      expect(onPageChange.mock.calls).toEqual([[1], [3]]);
      expect(scrollIntoView).toHaveBeenCalledWith({ block: 'nearest', behavior: 'smooth' });
    });

    it('clamps a page past the end and disables Next there', () => {
      render(<ModelCatalogResults models={many} renderModel={renderModel} overview={false} page={9} onPageChange={vi.fn()} onBrowseCategory={vi.fn()} />);
      expect(screen.getByText('Page 3 of 3')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled();
      expect(screen.getByText(`model-${many.length}`)).toBeInTheDocument();
    });

    it('disables Previous on the first page', () => {
      render(<ModelCatalogResults models={many} renderModel={renderModel} overview={false} page={1} onPageChange={vi.fn()} onBrowseCategory={vi.fn()} />);
      expect(screen.getByRole('button', { name: 'Previous' })).toBeDisabled();
    });

    it('has no page controls for a single page', () => {
      render(<ModelCatalogResults models={many.slice(0, 2)} renderModel={renderModel} overview={false} page={1} onPageChange={vi.fn()} onBrowseCategory={vi.fn()} />);
      expect(screen.getByRole('status')).toHaveTextContent('Showing 1–2 of 2 models');
      expect(screen.queryByRole('navigation', { name: 'Catalog pages' })).not.toBeInTheDocument();
    });
  });
});
