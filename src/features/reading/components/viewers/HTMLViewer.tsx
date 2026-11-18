import { useMemo, useState } from 'react';

import { useQuery } from '@tanstack/react-query';
import { Loader2, AlertCircle, ExternalLink, Link2 } from 'lucide-react';

import { useApi } from '@/contexts/ApiContext';
import { readingKeys, readSourceText } from '@/features/reading/hooks/readSourceFile';
import { useEffectiveTheme } from '@/hooks/useApplyTheme';
import type { ApiClient } from '@/lib/api';
import type { SavedWebArticleDto } from '@/lib/bindings';
import { openExternalUrl } from '@/utils/openExternalUrl';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import { htmlReadingDocument } from './htmlReadingDocument';

type HTMLViewerProps =
  | { htmlPath: string; title?: string; showTitle?: boolean }
  | { article: SavedWebArticleDto; showTitle?: boolean };

const EMPTY_LINKS: string[] = [];

interface LoadedPage {
  html: string;
  fallbackNotice: string | null;
}

const escapeHtml = (text: string) =>
  text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');

/** An HTML file, read as it is. */
async function loadHtmlFile(htmlPath: string, api: ApiClient): Promise<LoadedPage> {
  const result = await readSourceText(htmlPath, api);
  if (!result.ok) throw new Error(result.error || 'Failed to load HTML');
  return { html: result.data, fallbackNotice: null };
}

/** The saved page of an article, or its text when the page is gone or cannot be read. */
async function loadSavedArticle(article: SavedWebArticleDto, api: ApiClient): Promise<LoadedPage> {
  if (article.pagePath !== null) {
    const page = await readSourceText(article.pagePath, api);
    if (page.ok) return { html: page.data, fallbackNotice: null };
  }
  const text = await readSourceText(article.textPath, api);
  if (!text.ok) throw new Error(text.error || 'Failed to load HTML');
  return {
    html: `<pre>${escapeHtml(text.data)}</pre>`,
    fallbackNotice: 'Archived HTML missing. Showing markdown snapshot.',
  };
}

/**
 * HTMLViewer - Renders web article HTML in sandboxed iframe
 *
 * Security: Uses sandbox="allow-same-origin" to prevent script execution
 * while allowing styles and images to render.
 */
export function HTMLViewer(props: HTMLViewerProps) {
  const api = useApi();
  if ('article' in props) {
    const { article } = props;
    return (
      <ReadingPage
        readPath={article.pagePath ?? article.textPath}
        load={() => loadSavedArticle(article, api)}
        title={article.title}
        showTitle={props.showTitle}
        originalUrl={article.url}
        links={article.links}
      />
    );
  }
  const { htmlPath } = props;
  return (
    <ReadingPage
      readPath={htmlPath}
      load={() => loadHtmlFile(htmlPath, api)}
      title={props.title ?? null}
      showTitle={props.showTitle}
      originalUrl={null}
      links={EMPTY_LINKS}
    />
  );
}

interface ReadingPageProps {
  /** The file the page is read from; the query is keyed by it. */
  readPath: string;
  load: () => Promise<LoadedPage>;
  title: string | null;
  showTitle?: boolean;
  originalUrl: string | null;
  links: string[];
}

function ReadingPage({ readPath, load, title, showTitle = false, originalUrl, links }: ReadingPageProps) {
  const theme = useEffectiveTheme();
  const pageQuery = useQuery({
    queryKey: readingKeys.archivedPage(readPath),
    queryFn: load,
    retry: false,
    staleTime: 0,
  });
  const page = pageQuery.data;
  const htmlContent = page?.html ?? '';
  const fallbackNotice = page?.fallbackNotice ?? null;
  const isLoading = pageQuery.isPending;
  const error = pageQuery.isError
    ? pageQuery.error instanceof Error ? pageQuery.error.message : 'Unknown error'
    : null;
  // The link list starts folded for every page.
  const [linksShownFor, setLinksShownFor] = useState<string | null>(null);
  const showLinks = linksShownFor === readPath;
  const readingDocument = useMemo(() => htmlReadingDocument(htmlContent, theme), [htmlContent, theme]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-8 h-8 animate-spin text-primary" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full p-8">
        <div className="text-center space-y-2">
          <AlertCircle className="w-12 h-12 text-[hsl(var(--danger))] mx-auto" />
          <p className="text-[hsl(var(--danger))] font-medium">Failed to load article</p>
          <p className="text-sm text-[hsl(var(--text-secondary))]">{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full min-h-0 flex flex-col bg-[hsl(var(--bg))]">
      {showTitle && title && (
        <div className="px-6 py-4 border-b">
          <h1 className="m-0 text-xl font-semibold text-[hsl(var(--text-primary))]">{title}</h1>
        </div>
      )}
      {(originalUrl || links.length > 0) && (
        <div className="px-6 py-3 border-b bg-[hsl(var(--surface))]/10">
          <div className="flex flex-wrap items-center gap-3">
            {originalUrl && (
              <button
                type="button"
                onClick={handleAsyncEvent(() => openExternalUrl(originalUrl))}
                className="inline-flex items-center gap-2 rounded-full border border-[hsl(var(--border-subtle))]/70 bg-[hsl(var(--surface))]/90 px-4 py-1.5 text-sm text-primary hover:opacity-90"
              >
                <ExternalLink size={14} />
                Open Original URL
              </button>
            )}
            {links.length > 0 && (
              <button
                type="button"
                className="inline-flex items-center gap-2 rounded-full border border-[hsl(var(--border-subtle))]/70 bg-[hsl(var(--surface))]/80 px-3 py-1 text-xs text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]"
                onClick={() => setLinksShownFor(showLinks ? null : readPath)}
              >
                <Link2 size={12} />
                {showLinks ? 'Hide' : 'Show'} links ({links.length})
              </button>
            )}
          </div>
          {showLinks && links.length > 0 && (
            <div className="mt-3 flex flex-col gap-2 max-h-36 overflow-auto rounded-xl border border-[hsl(var(--border-subtle))]/70 bg-[hsl(var(--surface))]/55 p-3">
              {links.slice(0, 20).map((link) => (
                <button
                  key={link}
                  type="button"
                  className="text-left text-sm text-primary hover:underline break-all"
                  onClick={handleAsyncEvent(() => openExternalUrl(link))}
                >
                  {link}
                </button>
              ))}
              {links.length > 20 && (
                <p className="text-xs text-[hsl(var(--text-secondary))]">
                  Showing 20 of {links.length} links
                </p>
              )}
            </div>
          )}
        </div>
      )}
      {fallbackNotice && (
        <div className="px-6 py-2 border-b bg-[hsl(var(--warning-muted))] text-[hsl(var(--warning-fg))] text-xs">
          {fallbackNotice}
        </div>
      )}
      <div className="flex-1 min-h-0 rounded-xl border border-border-subtle bg-surface shadow-sheet">
        <iframe
          srcDoc={readingDocument}
          className="h-full w-full border-0"
          sandbox="allow-same-origin"
          title={title || 'Web Article'}
        />
      </div>
    </div>
  );
}
