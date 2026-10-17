import { useMemo, useState } from 'react';

import { useQuery } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-shell';
import { Loader2, AlertCircle, ExternalLink, Link2 } from 'lucide-react';

import { readingKeys, readSourceText } from '@/features/reading/hooks/readSourceFile';
import { useEffectiveTheme } from '@/hooks/useApplyTheme';

import { htmlReadingDocument } from './htmlReadingDocument';

interface HTMLViewerProps {
  htmlPath: string;
  title?: string;
  showTitle?: boolean;
}

const EMPTY_LINKS: string[] = [];

interface WebArchiveMetadata {
  title?: string;
  url?: string;
  site_name?: string;
  links?: string[];
}

interface ArchivedPage {
  html: string;
  fallbackNotice: string | null;
  originalUrl: string | null;
  siteName: string | null;
  links: string[];
}

const escapeHtml = (text: string) =>
  text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');

/** The saved page, or its markdown snapshot when the HTML is gone, with its archive metadata. */
async function loadArchivedPage(htmlPath: string): Promise<ArchivedPage> {
  const page: ArchivedPage = { html: '', fallbackNotice: null, originalUrl: null, siteName: null, links: [] };

  const htmlResult = await readSourceText(htmlPath);
  if (htmlResult.ok) {
    page.html = htmlResult.data;
  } else {
    const lastSlashIndex = htmlPath.lastIndexOf('/');
    const baseDir = lastSlashIndex >= 0 ? htmlPath.slice(0, lastSlashIndex) : htmlPath;
    const fallbackCandidates = [`${baseDir}/article.md`, `${baseDir}/weblink.md`];

    let fallbackMarkdown: string | null = null;
    for (const candidate of fallbackCandidates) {
      const fallbackResult = await readSourceText(candidate);
      if (fallbackResult.ok) {
        fallbackMarkdown = fallbackResult.data;
        break;
      }
    }

    if (!fallbackMarkdown) throw new Error(htmlResult.error || 'Failed to load HTML');
    page.html = `<pre>${escapeHtml(fallbackMarkdown)}</pre>`;
    page.fallbackNotice = 'Archived HTML missing. Showing markdown snapshot.';
  }

  if (htmlPath.endsWith('page.html')) {
    const metadataPath = `${htmlPath.slice(0, -'page.html'.length)}metadata.json`;
    const metadataResult = await readSourceText(metadataPath);
    if (metadataResult.ok) {
      try {
        const metadata = JSON.parse(metadataResult.data) as WebArchiveMetadata;
        if (typeof metadata.url === 'string' && metadata.url.length > 0) page.originalUrl = metadata.url;
        if (typeof metadata.site_name === 'string' && metadata.site_name.length > 0) page.siteName = metadata.site_name;
        if (Array.isArray(metadata.links)) page.links = metadata.links;
      } catch (parseError) {
        console.warn('Failed to parse metadata.json:', parseError);
      }
    }
  }

  return page;
}

/**
 * HTMLViewer - Renders web article HTML in sandboxed iframe
 *
 * Security: Uses sandbox="allow-same-origin" to prevent script execution
 * while allowing styles and images to render.
 */
export function HTMLViewer({ htmlPath, title, showTitle = false }: HTMLViewerProps) {
  const theme = useEffectiveTheme();
  const pageQuery = useQuery({
    queryKey: readingKeys.archivedPage(htmlPath),
    queryFn: () => loadArchivedPage(htmlPath),
    retry: false,
    staleTime: 0,
  });
  const page = pageQuery.data;
  const htmlContent = page?.html ?? '';
  const fallbackNotice = page?.fallbackNotice ?? null;
  const originalUrl = page?.originalUrl ?? null;
  const siteName = page?.siteName ?? null;
  const links = page?.links ?? EMPTY_LINKS;
  const isLoading = pageQuery.isPending;
  const error = pageQuery.isError
    ? pageQuery.error instanceof Error ? pageQuery.error.message : 'Unknown error'
    : null;
  // The link list starts folded for every page.
  const [linksShownFor, setLinksShownFor] = useState<string | null>(null);
  const showLinks = linksShownFor === htmlPath;
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
      {(originalUrl || siteName || links.length > 0) && (
        <div className="px-6 py-3 border-b bg-[hsl(var(--surface))]/10">
          <div className="flex flex-wrap items-center gap-3">
            {originalUrl && (
              <button
                type="button"
                onClick={() => open(originalUrl)}
                className="inline-flex items-center gap-2 rounded-full border border-[hsl(var(--border-subtle))]/70 bg-[hsl(var(--surface))]/90 px-4 py-1.5 text-sm text-primary hover:opacity-90"
              >
                <ExternalLink size={14} />
                Open Original URL
              </button>
            )}
            {siteName && (
              <span className="inline-flex items-center rounded-full border border-[hsl(var(--border-subtle))]/60 bg-[hsl(var(--surface))]/70 px-3 py-1 text-xs font-medium text-[hsl(var(--text-secondary))]">
                {siteName}
              </span>
            )}
            {links.length > 0 && (
              <button
                type="button"
                className="inline-flex items-center gap-2 rounded-full border border-[hsl(var(--border-subtle))]/70 bg-[hsl(var(--surface))]/80 px-3 py-1 text-xs text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]"
                onClick={() => setLinksShownFor(showLinks ? null : htmlPath)}
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
                  onClick={() => open(link)}
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
