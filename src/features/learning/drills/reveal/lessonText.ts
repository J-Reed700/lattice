/** The code of a lesson sentence that is a whole fenced block, without its fence lines; none for any other text. */
export function fencedCode(text: string): string | null {
  const body = /^```[^`\n]*\n([\s\S]*?)\n?```$/.exec(text.trim())?.[1];
  return body === undefined || /^```/m.test(body) ? null : body;
}
