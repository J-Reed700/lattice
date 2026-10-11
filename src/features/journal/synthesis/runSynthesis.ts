import { WORKSPACE_NOTES_QUERY_KEY } from '@/features/journal/api/queries';
import { weeklySynthesisCandidatesKey } from '@/features/journal/hooks/useWeeklySynthesisCandidatesQuery';
import { appendToNote, buildSynthesisBlock, resolveWeekPage } from '@/features/journal/model/synthesisTargets';
import { VaultAPI } from '@/lib/api';
import type { SynthesizeJournalEntriesRequestDto, SynthesizeJournalEntriesResponseDto } from '@/lib/bindings';
import { flushPendingSaves } from '@/lib/pendingSaves';
import { unwrapApiResult } from '@/types/api/result';

import {
  isSynthesisLive,
  markSynthesisApplied,
  rememberSynthesis,
  startSynthesis,
  synthesesQueryOptions,
  synthesisKeys,
  synthesisResult,
  type JournalSynthesisDto,
  type SynthesisDestinationDto,
} from './api';
import { setSaving, useSynthesisPanel } from './synthesisPanel';

import type { QueryClient } from '@tanstack/react-query';

interface SynthesisRequest {
  title: string;
  heading: string;
  request: SynthesizeJournalEntriesRequestDto;
  destination: SynthesisDestinationDto;
}

/**
 * One shared start for the chat menu, palette and Journal. The synthesis runs
 * as a job, so leaving the page, or closing the app, does not lose it; this
 * window saves it to its destination as soon as it finishes. False when one
 * is already running: the panel is shown instead.
 */
export async function runSynthesis(options: SynthesisRequest, client: QueryClient): Promise<boolean> {
  const syntheses = await client.ensureQueryData(synthesesQueryOptions());
  if (syntheses.some(isSynthesisLive)) {
    useSynthesisPanel.setState({ minimized: false });
    return false;
  }
  const synthesis = await startSynthesis({
    request: options.request,
    destination: options.destination,
    title: options.title,
    heading: options.heading,
  });
  useSynthesisPanel.setState((panel) => ({
    minimized: false,
    saved: null,
    autoSave: [...panel.autoSave, synthesis.job.id],
  }));
  client.setQueryData<JournalSynthesisDto[]>(synthesisKeys.list, (list) => rememberSynthesis(list, synthesis));
  return true;
}

async function saveTo(destination: SynthesisDestinationDto, block: string, result: SynthesizeJournalEntriesResponseDto) {
  if (destination.kind === 'capture') {
    const saved = unwrapApiResult(await VaultAPI.quickCapture(block, result.sources, result.conversationIds));
    return { noteId: saved.noteId, noteTitle: saved.noteTitle };
  }
  const note = destination.kind === 'week'
    ? await resolveWeekPage(destination.title)
    : unwrapApiResult(await VaultAPI.listWorkspaceNotes()).notes.find(page => page.id === destination.noteId);
  if (!note) throw new Error('The destination journal page no longer exists.');
  const saved = await appendToNote(note, block, result.sources, result.conversationIds);
  return { noteId: saved.id, noteTitle: saved.title };
}

const applying = new Set<string>();

/**
 * Saves a finished synthesis to the destination it was started with, after
 * open editors have saved, then marks it applied so it is saved once.
 */
export async function applySynthesis(synthesis: JournalSynthesisDto, client: QueryClient): Promise<boolean> {
  const jobId = synthesis.job.id;
  if (applying.has(jobId)) return false;
  applying.add(jobId);
  setSaving(jobId, {});
  try {
    // Flush open editors before appending to a page they may own. Revision
    // checks on the write protect edits made while the request is in flight.
    if (!(await flushPendingSaves())) {
      throw new Error('Some edits could not be saved. Save them, then retry saving this synthesis.');
    }
    const result = await synthesisResult(jobId);
    const block = buildSynthesisBlock({
      heading: synthesis.heading, entryCount: result.entryCount,
      synthesis: result.synthesis, citations: result.citations,
    });
    const { noteId, noteTitle } = await saveTo(synthesis.destination, block, result);
    await markSynthesisApplied(jobId);
    // Saving succeeded. Cache refresh failures must never offer to save again.
    client.setQueryData<JournalSynthesisDto[]>(synthesisKeys.list, (list) => list?.filter(item => item.job.id !== jobId));
    useSynthesisPanel.setState((panel) => ({
      autoSave: panel.autoSave.filter(id => id !== jobId),
      saved: {
        jobId, title: synthesis.title, noteId, noteTitle, entryCount: result.entryCount,
        startedAt: synthesis.job.startedAt ?? synthesis.job.createdAt,
        finishedAt: synthesis.job.finishedAt ?? Date.now(),
      },
    }));
    setSaving(jobId, null);
    await Promise.allSettled([
      client.invalidateQueries({ queryKey: WORKSPACE_NOTES_QUERY_KEY }),
      client.invalidateQueries({ queryKey: weeklySynthesisCandidatesKey }),
    ]);
    return true;
  } catch (error) {
    setSaving(jobId, { error: error instanceof Error ? error.message : String(error) });
    return false;
  } finally {
    applying.delete(jobId);
  }
}
