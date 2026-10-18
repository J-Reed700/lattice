import type { ConversationTangentDto } from '@/lib/bindings';

export type TangentSort = 'conversation' | 'recent';

/** What the tangent list shows. Kept by the sidebar, so it survives opening a tangent and coming back. */
export interface TangentListView {
  query: string;
  sort: TangentSort;
  /** Only the tangents opened from this reply (the reply's "N tangents" marker). */
  focusMessageId: string | null;
}

export const DEFAULT_TANGENT_VIEW: TangentListView = { query: '', sort: 'conversation', focusMessageId: null };

export interface TangentNode {
  tangent: ConversationTangentDto;
  /** Tangents opened inside this one, oldest first. */
  children: TangentNode[];
}

/** Where a group's reply lives, which decides its header and its action. */
export type TangentGroupSource =
  | { kind: 'reply'; messageId: string; excerpt: string }
  | { kind: 'tangent'; tangent: ConversationTangentDto }
  | { kind: 'missing' };

export interface TangentGroup {
  key: string;
  source: TangentGroupSource;
  nodes: TangentNode[];
}

interface LoadedMessage { id: string; content: string }

const collapse = (value: string) => value.replace(/\s+/g, ' ').trim().toLowerCase();
const time = (value: string) => Date.parse(value) || 0;
const oldestFirst = (a: ConversationTangentDto, b: ConversationTangentDto) => time(a.createdAt) - time(b.createdAt);

/** Case-insensitive, across the title and the passage; line breaks in a passage count as spaces. */
export function matchesTangent(tangent: ConversationTangentDto, query: string): boolean {
  const needle = collapse(query);
  return !needle || collapse(tangent.title).includes(needle) || collapse(tangent.selectedText).includes(needle);
}

/** The reply as one plain line for a group header; the header clamps it. */
export function replyExcerpt(content: string): string {
  return content.slice(0, 600)
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[#>*_`~|]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

/** The tangent this one was opened inside, when that tangent is still listed. */
function nestedUnder(tangent: ConversationTangentDto, byId: ReadonlyMap<string, ConversationTangentDto>): string | null {
  return tangent.sourceConversationId !== tangent.parentConversationId && byId.has(tangent.sourceConversationId)
    ? tangent.sourceConversationId : null;
}

function childrenBySource(tangents: readonly ConversationTangentDto[], byId: ReadonlyMap<string, ConversationTangentDto>) {
  const children = new Map<string, ConversationTangentDto[]>();
  for (const tangent of tangents) {
    const source = nestedUnder(tangent, byId);
    if (source) children.set(source, [...(children.get(source) ?? []), tangent]);
  }
  for (const list of children.values()) list.sort(oldestFirst);
  return children;
}

/** The tangents the view starts from: every top-level one, or those opened from the focused reply. */
function scopeRoots(tangents: readonly ConversationTangentDto[], byId: ReadonlyMap<string, ConversationTangentDto>, focusMessageId: string | null) {
  return tangents.filter(tangent => (focusMessageId ? tangent.sourceMessageId === focusMessageId : !nestedUnder(tangent, byId)));
}

/**
 * Conversation order: one group per source reply, in the order the replies sit
 * in the conversation, with tangents opened inside a tangent nested under it. A
 * filter keeps a tangent when it or anything nested in it matches. Replies that
 * are no longer loaded (deleted, or inside a removed tangent) share one group at
 * the end.
 */
export function groupTangents(tangents: readonly ConversationTangentDto[], messages: readonly LoadedMessage[], view: TangentListView): TangentGroup[] {
  const byId = new Map(tangents.map(tangent => [tangent.conversationId, tangent]));
  const children = childrenBySource(tangents, byId);
  const position = new Map(messages.map((message, index) => [message.id, index]));
  const build = (tangent: ConversationTangentDto, seen: Set<string>): TangentNode | null => {
    if (seen.has(tangent.conversationId)) return null;
    const next = new Set(seen).add(tangent.conversationId);
    const nested = (children.get(tangent.conversationId) ?? []).flatMap(child => build(child, next) ?? []);
    return matchesTangent(tangent, view.query) || nested.length ? { tangent, children: nested } : null;
  };

  const groups = new Map<string, TangentGroup & { order: number; since: number }>();
  for (const root of scopeRoots(tangents, byId, view.focusMessageId).sort(oldestFirst)) {
    const node = build(root, new Set());
    if (!node) continue;
    const index = position.get(root.sourceMessageId);
    const sourceTangent = root.sourceConversationId !== root.parentConversationId ? byId.get(root.sourceConversationId) : undefined;
    const key = index !== undefined || sourceTangent ? root.sourceMessageId : 'missing';
    const existing = groups.get(key);
    if (existing) { existing.nodes.push(node); continue; }
    const source: TangentGroupSource = index !== undefined
      ? { kind: 'reply', messageId: root.sourceMessageId, excerpt: replyExcerpt(messages[index].content) }
      : sourceTangent ? { kind: 'tangent', tangent: sourceTangent } : { kind: 'missing' };
    const order = index ?? (sourceTangent ? Number.MAX_SAFE_INTEGER - 1 : Number.MAX_SAFE_INTEGER);
    groups.set(key, { key, source, nodes: [node], order, since: time(root.createdAt) });
  }
  return [...groups.values()]
    .sort((a, b) => a.order - b.order || a.since - b.since)
    .map(({ key, source, nodes }) => ({ key, source, nodes }));
}

/** Recent activity: every tangent in scope, nested ones included, newest activity first. */
export function recentTangents(tangents: readonly ConversationTangentDto[], view: TangentListView): ConversationTangentDto[] {
  let scope: readonly ConversationTangentDto[] = tangents;
  if (view.focusMessageId) {
    const byId = new Map(tangents.map(tangent => [tangent.conversationId, tangent]));
    const children = childrenBySource(tangents, byId);
    const found = new Map<string, ConversationTangentDto>();
    const visit = (tangent: ConversationTangentDto) => {
      if (found.has(tangent.conversationId)) return;
      found.set(tangent.conversationId, tangent);
      children.get(tangent.conversationId)?.forEach(visit);
    };
    scopeRoots(tangents, byId, view.focusMessageId).forEach(visit);
    scope = [...found.values()];
  }
  return scope.filter(tangent => matchesTangent(tangent, view.query))
    .sort((a, b) => time(b.updatedAt) - time(a.updatedAt));
}
