/**
 * Finds references to files, with or without lines, in the many ways a model
 * writes them. The Explorer prompt asks for `path:10-24` in backticks, but a
 * folder's own instructions, a different model or plain habit produce all of:
 *
 *   src/main.rs:12   src/main.rs:10-24   src/main.rs:10–24   src/main.rs:12:5
 *   src/main.rs#L12  src/main.rs#L10-L24 src/main.rs:L12     src/main.rs(12)
 *   src/main.rs (lines 10–24)   src/main.rs, line 12   src/main.rs L12
 *   lines 10–24 of src/main.rs  [a.rs:1; b.rs:2-3]  src/main.rs:9, 64-67
 *   **src/main.rs**:12   `src/main.rs`:12   src/lib/api.ts   package.json
 *   /abs/path/main.rs:12   src\main.rs:12   file:///abs/main.rs#L3
 *
 * so this reads them all, and leaves alone the things that only look alike:
 * times (10:30), ratios (16:9), hosts and URLs (localhost:5173,
 * https://x.dev:443), versions (v1.2.3), and dotted names like `self.status`
 * or "Node.js".
 */

export interface LineRange {
  /** 1-based, inclusive. */
  startLine: number;
  endLine: number;
}

export interface FileRefMatch {
  /** Offsets into the scanned text: the part that reads as the reference. */
  start: number;
  end: number;
  path: string;
  /** `null` for a file named without lines. */
  lines: LineRange | null;
}

/** Extensions a file named without a folder must have to count as one. */
const FILE_EXTENSIONS = new Set(
  (
    'rs ts tsx js jsx mjs cjs mts cts json jsonc json5 toml yaml yml md mdx markdown txt lock log ' +
    'py pyi pyx ipynb go c h cc cpp cxx c++ hpp hh hxx ipp inl m mm swift kt kts java scala groovy gradle ' +
    'rb php cs fs fsx vb sh bash zsh fish ps1 psm1 bat cmd sql html htm css scss sass less ' +
    'vue svelte astro xml xsd plist ini cfg conf env properties cmake mk make proto graphql gql ' +
    'lua dart ex exs erl hrl clj cljs cljc edn elm hs lhs ml mli r jl zig nim sol tf tfvars hcl ' +
    'wgsl glsl hlsl metal cu cuh csv tsv rst adoc tex bib svg wasm wat pbxproj xcconfig entitlements ' +
    'storyboard xib nix dhall prisma http rest jinja j2 hbs ejs pug njk liquid tmpl tpl'
  ).split(' ')
);

/** Files with no extension that are still plainly files. */
const EXTENSIONLESS = new Set([
  'Makefile',
  'makefile',
  'GNUmakefile',
  'Dockerfile',
  'Containerfile',
  'Justfile',
  'justfile',
  'Rakefile',
  'Gemfile',
  'Procfile',
  'Vagrantfile',
  'Brewfile',
  'Podfile',
  'Jenkinsfile',
  'LICENSE',
  'README',
  'CODEOWNERS',
  'CHANGELOG',
]);

/** The first segment of `github.com/x/y.ts` is a host, not a folder. */
const DOMAIN = /^(?:www\.)?[a-z0-9-]+(?:\.[a-z0-9-]+)*\.(?:com|org|net|io|dev|ai|app|co|gov|edu|me|so|xyz|info|cloud|page|site|us|uk|de|cn|jp)$/i;

/** A run that could be a path. It ends on a character a sentence does not, so a closing period stays out. */
const TOKEN = /(?:file:\/\/)?[A-Za-z0-9_@.+~\-/\\]*[A-Za-z0-9_@+-]/g;
const PATH_CHAR = /[A-Za-z0-9_@.+~\-/\\:]/;

// Lines written straight after the path. Sticky: each is tried exactly where the path ends.
const DASH = String.raw`\s?[-–—]\s?`;
/** `:12`, `:10-24`, `:12:5`, `#L12`, `#L10-L24`, `:L12`. */
const COLON_LINES = new RegExp(String.raw`(?::|#)L?(\d+)(?::\d+)?(?:${DASH}L?(\d+)(?::\d+)?)?(?!\d)`, 'y');
/** `(12)` and `(12,5)`, as compilers print them. */
const PAREN_LINES = /\((\d+)(?:[,:]\s*\d+)?\)/y;
/** ` (lines 10–24)`, `, line 12`, ` at line 12`, ` L12`, `: lines 3 to 9`. */
const WORD_LINES =
  /(\s?\(\s?|,\s?|:\s?|\s[-–—]\s|\s)(?:(?:at|on|from)\s+)?(?:lines?\s*|ln\.?\s*|L(?=\d))(\d+)(?:\s*(?:[-–—]|to|through|thru)\s*L?(\d+))?/iy;
/** `, 64-67`, `; 70`, ` and 5`, `, L80-L90`: more lines in the same file. */
const MORE_LINES = /(\s?(?:,|;|&|\band\b)\s?(?:lines?\s*)?L?)(\d+)(?:\s*[-–—]\s*L?(\d+))?/iy;
/** What may follow a lone continuation number, so `a.rs:12, 2024 was…` is not read as line 2024. */
const AFTER_LONE_NUMBER = /^\s?(?:[,;)\]]|and\b|&|\.(?:\s|$)|$)/;
/** `line 12 of`, `lines 10-24 in the file`, just before the path. */
const PREFIX_LINES =
  /\b(?:lines?|ln\.?)\s*(\d+)(?:\s*(?:[-–—]|to|through|thru)\s*(\d+))?\s+(?:of|in|from)\s+(?:the\s+)?(?:file\s+)?[`"'*_[(]*$/i;

function range(first: string, second: string | undefined): LineRange | null {
  const a = Number(first);
  const b = second === undefined ? a : Number(second);
  if (!Number.isInteger(a) || !Number.isInteger(b) || a < 1 || b < 1) return null;
  // A reversed range still names the same lines.
  return { startLine: Math.min(a, b), endLine: Math.max(a, b) };
}

/** `file://`, backslashes and `./` dropped; the rest is the folder's own spelling. */
export function normalizePath(raw: string): string {
  let path = raw.replace(/^file:\/\//i, '').replace(/\\/g, '/').replace(/\/{2,}/g, '/');
  while (path.startsWith('./')) path = path.slice(2);
  return path;
}

/**
 * Whether a path names a file. A path with a folder in it needs only an
 * extension; a bare name needs one this knows, so `dbResult.ok`, "e.g." and
 * `example.com` stay text. In prose a capitalised `.js` name is a project
 * ("Node.js"), not a file.
 */
function namesFile(path: string, inCode: boolean, hasLines: boolean): boolean {
  const segments = path.split('/').filter(Boolean);
  const last = segments[segments.length - 1];
  if (!last) return false;
  const hasFolder = segments.length > 1;
  if (hasFolder && !path.startsWith('/') && DOMAIN.test(segments[0])) return false;
  if (EXTENSIONLESS.has(last)) return hasFolder || inCode || hasLines;
  const dot = last.lastIndexOf('.');
  if (dot === 0) return (hasFolder || inCode) && /^\.[A-Za-z][\w.-]*$/.test(last);
  if (dot < 0) return false;
  const extension = last.slice(dot + 1);
  if (!/^[A-Za-z][A-Za-z0-9_+]{0,11}$/.test(extension)) return false;
  if (hasFolder) return true;
  if (!FILE_EXTENSIONS.has(extension.toLowerCase())) return false;
  return inCode || hasLines || !/^[A-Z][a-z0-9]*\.js$/.test(last);
}

/** The lines written straight after a path, and where they end. */
function linesAfter(text: string, at: number): { lines: LineRange; end: number } | null {
  for (const pattern of [COLON_LINES, PAREN_LINES]) {
    pattern.lastIndex = at;
    const match = pattern.exec(text);
    const lines = match && range(match[1], match[2]);
    if (match && lines) return { lines, end: at + match[0].length };
  }
  WORD_LINES.lastIndex = at;
  const words = WORD_LINES.exec(text);
  const lines = words && range(words[2], words[3]);
  if (words && lines) {
    let end = at + words[0].length;
    // `(lines 10–24)` keeps its closing parenthesis; a bare `)` belongs to the sentence.
    if (words[1].includes('(')) {
      const close = /^\s?\)/.exec(text.slice(end));
      if (close) end += close[0].length;
    }
    return { lines, end };
  }
  return null;
}

/**
 * Every file reference in `text`, in order. `inCode(i)` says whether offset
 * `i` sits in inline code, where bare names like `main.rs` are clearly files.
 */
export function scanFileRefs(text: string, inCode: (index: number) => boolean = () => false): FileRefMatch[] {
  const found: FileRefMatch[] = [];
  TOKEN.lastIndex = 0;
  let token: RegExpExecArray | null;
  while ((token = TOKEN.exec(text))) {
    const start = token.index;
    const raw = token[0];
    // Mid-word, or the rest of a URL after its scheme (`https://…`).
    if (start > 0 && PATH_CHAR.test(text[start - 1]) && !/^file:\/\//i.test(raw)) continue;
    // A host with a port or a path after it is a URL: `example.com:443/x`.
    if (text.slice(start + raw.length, start + raw.length + 1) === ':' && /^\d+\//.test(text.slice(start + raw.length + 1))) {
      continue;
    }
    const path = normalizePath(raw);
    const code = inCode(start);
    const tokenEnd = start + raw.length;
    // `**src/main.rs**:12` and `` `src/main.rs`:12 ``: the lines sit past the closing marks.
    let linesAt = tokenEnd;
    const closer = /^(?:\*\*|__|\*|_|`)/.exec(text.slice(tokenEnd));
    if (closer && /^(?::|#)L?\d/.test(text.slice(tokenEnd + closer[0].length))) linesAt += closer[0].length;
    const after = linesAfter(text, linesAt);

    let refStart = start;
    let lines = after?.lines ?? null;
    if (!lines) {
      const before = PREFIX_LINES.exec(text.slice(Math.max(0, start - 80), start));
      const prefixed = before && range(before[1], before[2]);
      if (before && prefixed) {
        lines = prefixed;
        refStart = start - before[0].length;
      }
    }
    if (!namesFile(path, code, lines !== null)) continue;

    const refEnd = after ? after.end : tokenEnd;
    found.push({ start: refStart, end: refEnd, path, lines });
    TOKEN.lastIndex = refEnd;

    // More lines in the same file: `main.rs:9, 64-67`, `lines 3 and 5`.
    let at = refEnd;
    while (lines) {
      MORE_LINES.lastIndex = at;
      const more = MORE_LINES.exec(text);
      if (!more) break;
      const extra = range(more[2], more[3]);
      const end = at + more[0].length;
      if (!extra || (more[3] === undefined && !AFTER_LONE_NUMBER.test(text.slice(end)))) break;
      found.push({ start: at + more[1].length, end, path, lines: extra });
      at = end;
      TOKEN.lastIndex = end;
    }
  }
  return found;
}

/** A link's address, when it points into the folder rather than at the web. */
export function fileRefFromHref(href: string): FileRefMatch | null {
  let address = href.trim();
  if (/^[a-z][a-z0-9+.-]*:/i.test(address) && !/^file:\/\//i.test(address)) return null;
  try {
    address = decodeURI(address);
  } catch {
    // Kept as written.
  }
  const [ref] = scanFileRefs(address, () => true);
  if (ref?.start !== 0 || ref.end !== address.length) return null;
  return ref;
}
