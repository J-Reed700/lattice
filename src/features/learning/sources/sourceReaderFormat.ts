const LEGACY_WEB_EXTRACTION = 'web_reference_v1';

function isFence(block: string) {
  return /^(?:```|~~~)/.test(block.trimStart());
}

function isTableRow(block: string) {
  const line = block.trim();
  return line.startsWith('|') && (line.match(/\|/g)?.length ?? 0) >= 2;
}

function escapeMarkupLikeText(text: string) {
  return text.replace(/<([^>\n]+)>/g, (match, body: string) => (
    /^(?:https?:\/\/|mailto:)/i.test(body) ? match : `&lt;${body}&gt;`
  ));
}

function tableRow(block: string, escapeMarkup: boolean) {
  const line = block
    .split('\n')
    .map((part) => part.trim())
    .filter(Boolean)
    .join(' ')
    .replace(/^\|\s*/, '| ')
    .replace(/\s*\|?$/, ' |');
  return escapeMarkup ? escapeMarkupLikeText(line) : line;
}

function tableColumns(row: string) {
  return Math.max(1, row.split('|').slice(1, -1).length);
}

function splitBlocks(markdown: string) {
  const blocks: string[] = [];
  let lines: string[] = [];
  let fence = '';
  const flush = () => {
    const block = lines.join('\n').trim();
    if (block) blocks.push(block);
    lines = [];
  };

  for (const line of markdown.split('\n')) {
    const marker = line.trimStart().match(/^(`{3,}|~{3,})/)?.[1] ?? '';
    if (fence) {
      lines.push(line);
      if (marker.startsWith(fence[0]) && marker.length >= fence.length) {
        fence = '';
        flush();
      }
    } else if (marker) {
      flush();
      fence = marker;
      lines.push(line);
    } else if (line.trim()) {
      lines.push(line);
    } else {
      flush();
    }
  }
  flush();
  return blocks;
}

/**
 * Early web snapshots retained useful Markdown markers but also retained the
 * source HTML's visual line wrapping and emitted table cells without a GFM
 * separator row. Repair that representation for reading without changing the
 * immutable text used by evidence, search, and hashes.
 *
 * Text is escaped for TiptapViewer, which parses inline HTML, so `<x>` becomes
 * `&lt;x&gt;`. A reader that draws the Markdown itself passes
 * `escapeMarkup: false` to keep `<x>` as written.
 */
export function formatSourceForReader(
  fullText: string,
  extractionVersion: string,
  { escapeMarkup = true }: { escapeMarkup?: boolean } = {},
) {
  const escape = (text: string) => (escapeMarkup ? escapeMarkupLikeText(text) : text);
  const normalized = fullText.replace(/\r\n?/g, '\n').trim();
  if (extractionVersion !== LEGACY_WEB_EXTRACTION) {
    return normalized.replace(/\n{3,}/g, '\n\n');
  }

  const blocks = splitBlocks(normalized);
  const rendered: string[] = [];

  for (let index = 0; index < blocks.length;) {
    const block = blocks[index];
    if (isTableRow(block)) {
      const rows: string[] = [];
      while (index < blocks.length && isTableRow(blocks[index])) {
        rows.push(tableRow(blocks[index], escapeMarkup));
        index += 1;
      }
      const columns = Math.max(...rows.map(tableColumns));
      rendered.push([rows[0], `| ${Array.from({ length: columns }, () => '---').join(' | ')} |`, ...rows.slice(1)].join('\n'));
      continue;
    }

    if (isFence(block)) {
      rendered.push(block);
    } else if (/^(?:#{1,6}\s|[-*+]\s|\d+[.)]\s|>\s)/.test(block)) {
      rendered.push(escape(block.replace(/\n(?!\s*(?:[-*+]\s|\d+[.)]\s|>\s))/g, ' ')));
    } else {
      rendered.push(escape(block.split('\n').map((line) => line.trim()).filter(Boolean).join(' ')));
    }
    index += 1;
  }

  return rendered.join('\n\n');
}
