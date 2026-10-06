
import { WORKSPACE_NOTES_QUERY_KEY } from '@/features/journal/api/queries';
import { appendToNote, buildSynthesisBlock, resolveWeekPage } from '@/features/journal/model/synthesisTargets';
import { useSynthesisStore, type SynthesisJob } from '@/features/journal/synthesis/synthesisStore';
import { weeklySynthesisCandidatesKey } from '@/hooks/queries/useWeeklySynthesisCandidatesQuery';
import { VaultAPI } from '@/lib/api';
import { flushPendingSaves } from '@/lib/pendingSaves';
import type { SynthesizeJournalEntriesRequest, SynthesizeJournalEntriesResponse } from '@/types/api/conversation';
import { unwrapApiResult } from '@/types/api/result';

import type { QueryClient } from '@tanstack/react-query';

type SynthesisDestination = { kind: 'capture' } | { kind: 'note'; noteId: string } | { kind: 'week'; title: string };
interface SynthesisRequest {
  title: string;
  heading: string;
  request: SynthesizeJournalEntriesRequest;
  destination: SynthesisDestination;
}

/** One shared workflow for the chat menu, palette and Journal. Component unmounts
 * do not discard its result or redirect the user out of their current work. */
export async function runSynthesis(options: SynthesisRequest, client: QueryClient): Promise<boolean> {
  const startedAt = Date.now();
  let generated: SynthesizeJournalEntriesResponse | undefined;
  let block: string | undefined;

  const execute = async (): Promise<boolean> => {
    if (useSynthesisStore.getState().job?.status === 'running') {
      useSynthesisStore.setState({ minimized: false });
      return false;
    }
    const id = crypto.randomUUID();
    const update = (patch: Partial<SynthesisJob>) => {
      useSynthesisStore.setState(state => state.job?.id === id ? { job: { ...state.job, ...patch } } : state);
    };
    useSynthesisStore.setState({
      minimized: false,
      job: {
        id, title: options.title, conversationIds: options.request.conversationIds,
        status: 'running', stage: generated ? 'saving' : 'gathering', startedAt,
        entryCount: generated?.entryCount, chunkCount: generated?.chunkCount,
      },
    });
    try {
      if (!generated) {
        generated = unwrapApiResult(await VaultAPI.synthesizeJournalEntries(options.request, progress => {
          // Ignore delayed channel messages once saving has begun or this attempt ended.
          const current = useSynthesisStore.getState().job;
          if (current?.id !== id || current.status !== 'running' || current.stage === 'saving') return;
          update({
            stage: progress.stage,
            entryCount: progress.entryCount ?? undefined,
            chunkIndex: progress.chunkIndex ?? undefined,
            chunkCount: progress.chunkCount ?? undefined,
          });
        }));
        block = buildSynthesisBlock({
          heading: options.heading, entryCount: generated.entryCount,
          synthesis: generated.synthesis, citations: generated.citations,
        });
      }
      update({ stage: 'saving', entryCount: generated.entryCount, chunkCount: generated.chunkCount });
      const destination = options.destination;
      let noteId: string;
      let noteTitle: string;
      // Flush open editors before appending to a page they may own. Revision
      // checks on the write protect edits made while the request is in flight.
      if (!(await flushPendingSaves())) {
        throw new Error('Some edits could not be saved. Save them, then retry saving this synthesis.');
      }
      if (destination.kind === 'capture') {
        const saved = unwrapApiResult(await VaultAPI.quickCapture(block!, generated.sources ?? [], generated.conversationIds));
        noteId = saved.noteId;
        noteTitle = saved.noteTitle;
      } else {
        const note = destination.kind === 'week'
          ? await resolveWeekPage(destination.title)
          : unwrapApiResult(await VaultAPI.listWorkspaceNotes()).notes.find(page => page.id === destination.noteId);
        if (!note) throw new Error('The destination journal page no longer exists.');
        const saved = await appendToNote(note, block!, generated.sources ?? [], generated.conversationIds);
        noteId = saved.id;
        noteTitle = saved.title;
      }
      // Saving succeeded. Cache refresh failures must never offer to append again.
      update({ status: 'completed', noteId, noteTitle, finishedAt: Date.now() });
      await Promise.allSettled([
        client.invalidateQueries({ queryKey: WORKSPACE_NOTES_QUERY_KEY }),
        client.invalidateQueries({ queryKey: weeklySynthesisCandidatesKey }),
      ]);
      return true;
    } catch (error) {
      update({ status: 'failed', error: error instanceof Error ? error.message : String(error), finishedAt: Date.now(), retry: execute });
      return false;
    }
  };
  return execute();
}
