import { format, startOfWeek } from 'date-fns';

import VaultAPI from '@/lib/api';
import type { SourceDto } from '@/lib/bindings';
import type { SynthesisCitationDto } from '@/types/api/conversation';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

const CITATION_KIND_LABELS: Record<SynthesisCitationDto['kind'], string> = {
  conversation: 'conversation',
  reference: 'saved passage',
  note: 'journal page',
};

/** What a source with no title of its own is called in the sources list. */
const UNTITLED_BY_KIND: Record<SynthesisCitationDto['kind'], string> = {
  conversation: 'Untitled conversation',
  reference: 'Untitled passage',
  note: 'Untitled page',
};

/**
 * Monday-anchored page title, e.g. "Week of Sep 1". Stable for a whole week,
 * so a second run in the same week appends to the same page.
 */
export function weekPageTitle(reference: Date = new Date()): string {
  return `Week of ${format(startOfWeek(reference, { weekStartsOn: 1 }), 'MMM d')}`;
}

/**
 * Finds the page with this exact title, or creates it. Mirrors the shape of
 * `utils/chatReferenceCapture.ts::resolveTargetNote`.
 */
export async function resolveWeekPage(title: string): Promise<WorkspaceNote> {
  const notesResult = await VaultAPI.listWorkspaceNotes();
  if (!notesResult.ok) {
    throw new Error(notesResult.error);
  }
  const notes = Array.isArray(notesResult.data.notes) ? notesResult.data.notes : [];
  const existing = notes.find((note) => note.title.trim() === title);
  if (existing) return existing;

  const created = await VaultAPI.createWorkspaceNote(title);
  if (!created.ok) {
    throw new Error(created.error);
  }
  return created.data;
}

/** Appends a markdown block to a page and persists it. Returns the saved note. */
export async function appendToNote(
  note: WorkspaceNote,
  block: string,
  incomingSources: readonly SourceDto[] = [],
  conversationIds: readonly string[] = [],
): Promise<WorkspaceNote> {
  const next = appendSynthesisContent(note, block, incomingSources, conversationIds);
  const saved = await VaultAPI.updateWorkspaceNote(next);
  if (!saved.ok) {
    throw new Error(saved.error);
  }
  return saved.data;
}

/** Merge a synthesis block and its sources while keeping page citation IDs unique. */
export function appendSynthesisContent(
  note: WorkspaceNote,
  block: string,
  incomingSources: readonly SourceDto[] = [],
  conversationIds: readonly string[] = [],
): WorkspaceNote {
  const existingSources = note.sources ?? [];
  const usedIds = new Set(
    existingSources.flatMap((source) => source.citationId == null ? [] : [source.citationId]),
  );
  let nextId = Math.max(0, ...usedIds) + 1;
  const remappedIds = new Map<number, number>();
  const additions = incomingSources.map((source) => {
    if (source.citationId == null) return source;
    const oldId = source.citationId;
    let newId = oldId;
    if (usedIds.has(newId)) {
      while (usedIds.has(nextId)) nextId += 1;
      newId = nextId++;
    }
    usedIds.add(newId);
    remappedIds.set(oldId, newId);
    return { ...source, citationId: newId };
  });
  const rewrittenBlock = remappedIds.size === 0
    ? block
    : block.replace(/\[(\d+)\]/g, (mark, digits: string) => {
        const mapped = remappedIds.get(Number(digits));
        return mapped === undefined ? mark : `[${mapped}]`;
      });
  const current = note.content?.trim() ?? '';
  return {
    ...note,
    content: current ? `${current}\n\n${rewrittenBlock}` : rewrittenBlock,
    ...(conversationIds.length > 0
      ? { linkedConversationIds: [...new Set([...note.linkedConversationIds, ...conversationIds])] }
      : {}),
    ...(additions.length > 0 ? { sources: [...existingSources, ...additions] } : {}),
  };
}

export interface SynthesisBlockInput {
  heading: string;
  entryCount: number;
  synthesis: string;
  citations?: SynthesisCitationDto[];
  generatedAt?: Date;
}

/**
 * The markdown a synthesis result is written as. Shared by the Journal and the
 * Chat sidebar so both produce identical blocks.
 */
export function buildSynthesisBlock(input: SynthesisBlockInput): string {
  const { heading, entryCount, synthesis, citations, generatedAt } = input;
  const stamp = (generatedAt ?? new Date()).toLocaleString();
  const lines = [
    `## Journal Synthesis · ${heading}`,
    `_Generated ${stamp} from ${entryCount} ${entryCount === 1 ? 'entry' : 'entries'}._`,
    '',
    synthesis.trim(),
    '',
  ];

  if (citations && citations.length > 0) {
    lines.push('### Sources');
    for (const citation of citations) {
      const label = CITATION_KIND_LABELS[citation.kind] ?? citation.kind;
      // An untitled conversation would otherwise print "- — conversation".
      const title = citation.title.trim() || UNTITLED_BY_KIND[citation.kind] || 'Untitled';
      lines.push(`- ${title} — ${label}`);
    }
    lines.push('');
  }

  return lines.join('\n');
}
