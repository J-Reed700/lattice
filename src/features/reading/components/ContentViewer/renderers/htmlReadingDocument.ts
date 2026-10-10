import DOMPurify from 'dompurify';

/** Give archived pages a readable palette, including legacy snapshots with
 * inline colors. The original file stays intact; this is the reading preview. */
export function htmlReadingDocument(html: string, theme: 'light' | 'dark'): string {
  const content = DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ['style', 'link', 'meta'],
    FORBID_ATTR: ['style', 'color', 'bgcolor', 'background'],
  });
  return `<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<style>
:root { color-scheme: ${theme}; color: CanvasText; background: Canvas; }
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; max-width: 800px; margin: 32px auto; padding: 0 24px; line-height: 1.65; overflow-wrap: anywhere; }
h1 { margin-top: 0; }
img, video { max-width: 100%; height: auto; }
a { color: LinkText; text-underline-offset: 3px; }
a:visited { color: VisitedText; }
pre, code { background: color-mix(in srgb, CanvasText 6%, Canvas); }
pre { padding: 16px; overflow-x: auto; white-space: pre-wrap; }
blockquote { margin-inline: 0; padding-left: 20px; border-left: 3px solid color-mix(in srgb, CanvasText 25%, Canvas); }
table { border-collapse: collapse; max-width: 100%; }
th, td { padding: 8px; text-align: left; border: 1px solid color-mix(in srgb, CanvasText 25%, Canvas); }
.metadata { font-size: 0.9em; padding-bottom: 12px; margin-bottom: 28px; border-bottom: 1px solid color-mix(in srgb, CanvasText 25%, Canvas); }
</style>
</head>
<body>${content}</body>
</html>`;
}
