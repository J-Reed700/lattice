/** What private text looks like, decided in one place for logs and error screens. */

export function redactSecrets(text: string): string {
  return text
    .replace(/("(?:api[_-]?key|token|password|secret|authorization|content|prompt|query|body)"\s*:\s*)"(?:\\.|[^"\\])*"/gi, '$1"[REDACTED]"')
    .replace(/\b(Bearer\s+)[^\s"']+/gi, '$1[REDACTED]')
    .replace(/((?:api[_-]?key|token|password|secret|authorization)\s*[=:]\s*)[^\s,;"']+/gi, '$1[REDACTED]')
    .replace(/([?&](?:token|key|api_key|secret|password)=)[^&#\s]+/gi, '$1[REDACTED]');
}

export function redact(text: string): string {
  return redactSecrets(text)
    .replace(/(?:\/Users\/|\/home\/)[^/\s]+/g, '/[USER]')
    .replace(/[A-Z]:\\Users\\[^\\\s]+/gi, '[USER]');
}

// A path character, then the same without separators. A path may hold spaces,
// so it runs on past them and ends on its last non-space character.
const PATH_CHAR = String.raw`\p{L}\p{M}\p{N}_.~+,'()&#%!$=/\\\-`;
const NAME_CHAR = String.raw`\p{L}\p{M}\p{N}_.~+,'()&#%!$=\-`;
const PATH_REST = String.raw`(?:[${PATH_CHAR} ]*[${PATH_CHAR}])?`;
const WORD_START = String.raw`(^|[\s"\[{:;])`;

const RULES: ReadonlyArray<readonly [RegExp, string]> = [
  [/\b[a-z][a-z0-9+.-]{0,31}:\/\/[^\s"'`)\]}>]+/gi, '[URL]'],
  [/(^|[^\p{L}\p{N}._%+-])[\p{L}\p{N}._%+-]+@[\p{L}\p{N}-]+(?:\.[\p{L}\p{N}-]+)+/gu, '$1[EMAIL]'],
  [/\b\d{1,3}(?:\.\d{1,3}){3}\b/g, '[IP]'],
  [/(^|[^A-Za-z0-9_-])(?:0x)?[A-Fa-f0-9]{32,}(?![A-Za-z0-9_-])/g, '$1[TOKEN]'],
  [/(^|[^A-Za-z0-9_-])[A-Za-z0-9_-]{40,}(?![A-Za-z0-9_-])/g, '$1[TOKEN]'],
  [new RegExp(String.raw`(^|[^\p{L}\p{N}])[\\/]?\[USER\][\\/][${PATH_CHAR}]${PATH_REST}`, 'gu'), '$1[PATH]'],
  [new RegExp(String.raw`(^|[^\p{L}\p{N}])[A-Za-z]:[\\/]${PATH_REST}`, 'gu'), '$1[PATH]'],
  [new RegExp(String.raw`(^|[^\p{L}\p{N}\\])\\\\[${NAME_CHAR}]${PATH_REST}`, 'gu'), '$1[PATH]'],
  [new RegExp(String.raw`(^|[\s"'(\[{=,:;])~?/[${PATH_CHAR}]${PATH_REST}`, 'gu'), '$1[PATH]'],
  [new RegExp(String.raw`${WORD_START}[${NAME_CHAR}]+(?:/[${NAME_CHAR}]+){2,}${PATH_REST}`, 'gu'), '$1[PATH]'],
  [new RegExp(String.raw`${WORD_START}[${NAME_CHAR}]+(?:\\[${NAME_CHAR}]+)+${PATH_REST}`, 'gu'), '$1[PATH]'],
];

/** An error message made safe to show: secrets, URLs, emails, IPs, tokens and paths removed. */
export function sanitizeErrorMessage(message: unknown): string {
  if (typeof message !== 'string') return '';
  let text = redactSecrets(message);
  for (const [pattern, replacement] of RULES) text = text.replace(pattern, replacement);
  return text;
}

const FALLBACK_MESSAGE = 'Something unexpected happened.';

function messageOf(error: unknown): unknown {
  try {
    if (typeof error === 'string') return error;
    if (typeof error === 'object' && error !== null) return (error as { message?: unknown }).message;
  } catch {
    return undefined;
  }
  return undefined;
}

export function getUserFriendlyMessage(error: unknown): string {
  return sanitizeErrorMessage(messageOf(error)).trim() || FALLBACK_MESSAGE;
}
