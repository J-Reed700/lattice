import { format, startOfWeek } from 'date-fns';

import VaultAPI from '@/lib/api';
import type { SynthesisCitationDto } from '@/types/api/conversation';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

/** How many source lines a synthesis block prints before it summarises. */
const MAX_SOURCE_LINES = 20;

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
): Promise<WorkspaceNote> {
  const current = note.content?.trim() ?? '';
  const next: WorkspaceNote = {
    ...note,
    content: current ? `${current}\n\n${block}` : block,
  };
  const saved = await VaultAPI.updateWorkspaceNote(next);
  if (!saved.ok) {
    throw new Error(saved.error);
  }
  return saved.data;
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
    for (const citation of citations.slice(0, MAX_SOURCE_LINES)) {
      const label = CITATION_KIND_LABELS[citation.kind] ?? citation.kind;
      // An untitled conversation would otherwise print "- — conversation".
      const title = citation.title.trim() || UNTITLED_BY_KIND[citation.kind] || 'Untitled';
      lines.push(`- ${title} — ${label}`);
    }
    if (citations.length > MAX_SOURCE_LINES) {
      lines.push(`- …and ${citations.length - MAX_SOURCE_LINES} more`);
    }
    lines.push('');
  }

  return lines.join('\n');
}
