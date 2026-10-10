import { useRef } from 'react';
import type { ReactNode } from 'react';

import { AudioLines, ChevronLeft, ChevronRight, FileText, MessageSquare, Search } from 'lucide-react';

import type { ModelCategory, ModelRecommendation } from '@/types/modelCatalog';

import { CATALOG_TEXT_BUTTON_CLASS } from './catalogUtils';
import { SECONDARY_BUTTON_CLASS } from '../settingsStyles';


export const CATALOG_PAGE_SIZE = 8;
const PREVIEW_SIZE = 3;

const SECTIONS = [
  { category: 'LLM', title: 'Chat & writing', description: 'Conversations, drafting, and reasoning.', icon: MessageSquare },
  { category: 'Embedding', title: 'Search & retrieval', description: 'Find related ideas across your library.', icon: Search },
  { category: 'OCR', title: 'Document reading', description: 'Extract text from images and scans.', icon: FileText },
  { category: 'Transcription', title: 'Audio transcription', description: 'Turn recordings into searchable text.', icon: AudioLines },
] as const;

export interface CatalogResultsNavigation {
  overview: boolean;
  page: number;
  onPageChange: (page: number) => void;
  onBrowseCategory: (category: ModelCategory) => void;
}

interface ModelCatalogResultsProps extends CatalogResultsNavigation {
  models: ModelRecommendation[];
  renderModel: (model: ModelRecommendation) => ReactNode;
}

/** Layout only. The parent owns download state and preserves the page while a detail is open. */
export function ModelCatalogResults({
  models, renderModel, overview, page, onPageChange, onBrowseCategory,
}: ModelCatalogResultsProps) {
  const resultsRef = useRef<HTMLDivElement>(null);

  if (overview) {
    return (
      <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,22rem),1fr))] gap-5">
        {SECTIONS.map(({ category, title, description, icon: Icon }) => {
          const categoryModels = models.filter(({ model }) => model.category === category);
          if (categoryModels.length === 0) return null;
          return (
            <section key={category} aria-label={title} data-category={category} className="model-category min-w-0 overflow-hidden rounded-xl border border-border-default bg-surface shadow-sm">
              <div className="model-category-heading flex items-center gap-3 border-b border-border-subtle px-4 py-4">
                <span className="model-category-icon flex h-9 w-9 shrink-0 items-center justify-center rounded-lg"><Icon className="h-[18px] w-[18px]" aria-hidden="true" /></span>
                <div className="min-w-0 flex-1">
                  <h4 className="text-sm font-semibold text-text-primary">{title}</h4>
                  <p className="mt-1 text-xs leading-relaxed text-text-secondary">{description}</p>
                </div>
                <span className="text-xs tabular-nums text-text-muted">{categoryModels.length}</span>
              </div>
              <div className="px-4">{categoryModels.slice(0, PREVIEW_SIZE).map(renderModel)}</div>
              <button
                type="button"
                onClick={() => onBrowseCategory(category)}
                aria-label={`View all ${title.toLowerCase()} models`}
                className={`${CATALOG_TEXT_BUTTON_CLASS} w-full justify-between px-4 py-5 font-medium hover:bg-surface-raised`}
              >
                View all {categoryModels.length}
                <ChevronRight className="h-3.5 w-3.5" aria-hidden="true" />
              </button>
            </section>
          );
        })}
      </div>
    );
  }

  const pageCount = Math.max(1, Math.ceil(models.length / CATALOG_PAGE_SIZE));
  const currentPage = Math.min(page, pageCount);
  const start = (currentPage - 1) * CATALOG_PAGE_SIZE;
  const changePage = (nextPage: number) => {
    onPageChange(nextPage);
    resultsRef.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  };

  return (
    <div ref={resultsRef}>
      <p role="status" className="mb-3 text-xs tabular-nums text-text-muted">
        Showing {start + 1}–{Math.min(start + CATALOG_PAGE_SIZE, models.length)} of {models.length} models
      </p>
      <div className="border-t border-border-subtle">
        {models.slice(start, start + CATALOG_PAGE_SIZE).map(renderModel)}
      </div>
      {pageCount > 1 ? (
        <nav aria-label="Catalog pages" className="mt-4 flex items-center justify-between gap-3">
          <button type="button" disabled={currentPage === 1} onClick={() => changePage(currentPage - 1)} className={SECONDARY_BUTTON_CLASS}>
            <ChevronLeft className="h-4 w-4" aria-hidden="true" /> Previous
          </button>
          <span className="text-xs tabular-nums text-text-muted">Page {currentPage} of {pageCount}</span>
          <button type="button" disabled={currentPage === pageCount} onClick={() => changePage(currentPage + 1)} className={SECONDARY_BUTTON_CLASS}>
            Next <ChevronRight className="h-4 w-4" aria-hidden="true" />
          </button>
        </nav>
      ) : null}
    </div>
  );
}
