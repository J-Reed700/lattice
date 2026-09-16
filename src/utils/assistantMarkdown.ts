const ZERO_WIDTH_REGEX = /[\u200B-\u200D\uFEFF]/g;
const BULLET_MARKER_REGEX = /^(\s{0,3})[•◦▪●○■□▸▹►‣⁃∙·]\s+/u;
const DASH_MARKER_REGEX = /^(\s{0,3})[–—−]\s+/u;
const PAREN_NUMBERED_LIST_REGEX = /^(\s*)(\d+)[)\uFF09]\s*/u;
const CODE_FENCE_REGEX = /^\s*```/;

/**
 * Normalize assistant markdown into a stable, parser-friendly format.
 *
 * This standardizes common LLM formatting variants (unicode bullets, dash bullets,
 * and parenthesized numbered lists) before rendering.
 */
export function normalizeAssistantMarkdown(content: string): string {
  if (!content) return content;

  const normalized = content
    .replace(/\r\n?/g, '\n')
    .replace(ZERO_WIDTH_REGEX, '')
    .replace(/\u00A0/g, ' ');

  const lines = normalized.split('\n');
  let inCodeFence = false;

  const transformed = lines.map((rawLine) => {
    const line = rawLine.replace(/[ \t]+$/g, '');

    if (CODE_FENCE_REGEX.test(line)) {
      inCodeFence = !inCodeFence;
      return line;
    }

    if (inCodeFence) {
      return line;
    }

    return line
      .replace(BULLET_MARKER_REGEX, '$1- ')
      .replace(DASH_MARKER_REGEX, '$1- ')
      .replace(PAREN_NUMBERED_LIST_REGEX, '$1$2. ');
  });

  return transformed
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trimEnd();
}
