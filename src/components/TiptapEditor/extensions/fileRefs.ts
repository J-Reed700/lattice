export interface LineRange {
  /** 1-based, inclusive. */
  startLine: number;
  endLine: number;
}

export interface FileRef {
  path: string;
  /** `null` for a file named without lines. */
  lines: LineRange | null;
}

/** `#L12`, `#L10-L24`, `:12`, `:10-24` at the end of an address. */
const LINES_IN_ADDRESS = /(?:#L|:)(\d+)(?:[-–]L?(\d+))?$/;
/** A last path segment with an extension. */
const NAMES_FILE = /\.[A-Za-z][A-Za-z0-9_+]{0,11}$/;

function range(first: string, second: string | undefined): LineRange | null {
  const a = Number(first);
  const b = second === undefined ? a : Number(second);
  if (!Number.isInteger(a) || !Number.isInteger(b) || a < 1 || b < 1) return null;
  // A reversed range still names the same lines.
  return { startLine: Math.min(a, b), endLine: Math.max(a, b) };
}

/** `file://`, backslashes and `./` dropped; the rest is the folder's own spelling. */
function normalizePath(raw: string): string {
  let path = raw.replace(/^file:\/\//i, '').replace(/\\/g, '/').replace(/\/{2,}/g, '/');
  while (path.startsWith('./')) path = path.slice(2);
  return path;
}

/** A link's address, when it points into the folder rather than at the web. */
export function fileRefFromHref(href: string): FileRef | null {
  let address = href.trim();
  if (/^[a-z][a-z0-9+.-]*:/i.test(address) && !/^file:\/\//i.test(address)) return null;
  try {
    address = decodeURI(address);
  } catch {
    // Kept as written.
  }
  const found = LINES_IN_ADDRESS.exec(address);
  const lines = found ? range(found[1], found[2]) : null;
  const path = normalizePath(lines && found ? address.slice(0, found.index) : address);
  const last = path.split('/').pop() ?? '';
  return NAMES_FILE.test(last) ? { path, lines } : null;
}
