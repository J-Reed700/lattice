import { useRef, useState } from 'react';

import { hasWebResearch } from '@/features/chat/components/turn/research';
import { ResearchDeck } from '@/features/chat/components/turn/ResearchDeck';
import { roundCount, roundOf } from '@/features/chat/components/turn/rounds';
import { TurnClockContext, formatDuration } from '@/features/chat/components/turn/StepRow';
import type {
  MessageVerificationSummary,
  RetrievalTrace,
  TurnModel,
  TurnRecord as TurnRecordData,
  TurnStep,
} from '@/types/conversation';
import { runningOnCpuSentence } from '@/utils/chatModelSelection';

import '@/features/chat/components/turn/turn-record.css';

export { formatDuration };

interface TurnRecordProps {
  trace: RetrievalTrace | null;
  record: TurnRecordData | null;
  liveSteps: TurnStep[] | null;
  liveModel?: TurnModel | null;
  turnStartedAt?: number | null;
  isPending: boolean;
  isWriting: boolean;
  verification: MessageVerificationSummary | null;
}

export function TurnRecord({
  trace,
  record,
  liveSteps,
  liveModel = null,
  turnStartedAt = null,
  isPending,
  isWriting,
  verification,
}: TurnRecordProps) {
  const steps = (isPending ? liveSteps : record?.steps) ?? record?.steps ?? [];
  const runningStep = isPending
    ? ([...steps].reverse().find(step => step.state === 'running') ?? null)
    : null;

  const [manuallyOpen, setManuallyOpen] = useState<boolean | null>(null);
  const hasStartedWriting = useRef(false);
  const wasPending = useRef(isPending);
  if (isPending && !wasPending.current) {
    wasPending.current = true;
    hasStartedWriting.current = false;
  }
  if (!isPending) wasPending.current = false;
  if (isPending && isWriting) hasStartedWriting.current = true;
  const open = manuallyOpen ?? (isPending && !hasStartedWriting.current);

  const answeringWith = isPending ? liveModel : null;
  const summary = summaryLine({ trace, record, answeringWith, verification, rounds: roundCount(steps) });
  const runningRound = runningStep ? roundOf(steps, runningStep.id) : null;
  if (!summary && steps.length === 0) return null;

  return (
    <div className="turn-record">
      <button
        type="button"
        className="turn-record-summary"
        aria-expanded={open}
        onClick={() => setManuallyOpen(!open)}
      >
        <Chevron open={open} />
        <span
          className="turn-record-line"
          title={answeringWith?.degraded ? runningOnCpuSentence(answeringWith.name, answeringWith.degraded) : undefined}
        >
          {runningStep ? (
            <>
              <span className="turn-record-live">
                {runningRound !== null && runningRound > 1 && `Round ${runningRound} · `}
                {runningStep.label}
              </span>
              {summary && <span> · {summary}</span>}
            </>
          ) : (
            (summary ?? 'Working')
          )}
        </span>
      </button>
      {open && (
        <TurnClockContext.Provider value={isPending ? turnStartedAt : null}>
          <div className="turn-record-body" data-deck={hasWebResearch(steps)}>
            <TurnNote trace={trace} />
            <ResearchDeck steps={steps} live={isPending} />
            {record?.router?.rationale && (
              <p className="turn-record-rationale">
                Routed as {routerActionLabel(record.router.action)} —{' '}
                {record.router.rationale}
              </p>
            )}
          </div>
        </TurnClockContext.Provider>
      )}
    </div>
  );
}

function Chevron({ open }: { open: boolean }) {
  return (
    <svg
      className="turn-record-chevron"
      data-open={open}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M6 3.5 10.5 8 6 12.5" />
    </svg>
  );
}

function TurnNote({ trace }: { trace: RetrievalTrace | null }) {
  if (!trace) return null;
  const notes: string[] = [];
  const retried = (trace.kbCorrectiveRetries ?? 0) > 0;
  if (retried && trace.kbPlannerSkipped === true) {
    notes.push('Reused the previous topic, then searched again for more support');
  } else if (retried) {
    notes.push('First results looked thin, so it searched again');
  } else if (trace.kbPlannerSkipped === true) {
    notes.push('Reused the previous topic instead of planning a new search');
  }
  if (trace.kbSufficient === false) {
    const reasons = trace.sufficiencyReasons ?? [];
    notes.push(
      reasons.length > 0
        ? `Evidence judged thin: ${reasons.map(readableReason).join(', ')}`
        : 'Evidence judged thin'
    );
  }
  if (trace.unavailableReason) {
    notes.push(`Could not search your documents: ${trace.unavailableReason}`);
  }
  if (notes.length === 0) return null;
  return (
    <>
      {notes.map(note => (
        <p key={note} className="turn-record-note">
          {note}
        </p>
      ))}
    </>
  );
}

function summaryLine({
  trace,
  record,
  answeringWith,
  verification,
  rounds,
}: {
  trace: RetrievalTrace | null;
  record: TurnRecordData | null;
  answeringWith: TurnModel | null;
  verification: MessageVerificationSummary | null;
  rounds: number;
}): string | null {
  const parts: string[] = [];
  if (answeringWith) {
    parts.push(`Answering with ${answeringWith.name}${answeringWith.degraded ? ' · on CPU' : ''}`);
  }
  const retrieval = retrievalSummary(trace);
  if (retrieval) parts.push(retrieval);
  if (rounds >= 2) parts.push(`${rounds} rounds`);
  if (typeof trace?.focusedDocuments === 'number') {
    parts.push(
      trace.focusedDocuments === 0
        ? 'pinned to documents outside this space'
        : `pinned to ${trace.focusedDocuments} ${
            trace.focusedDocuments === 1 ? 'document' : 'documents'
          }`
    );
  }
  const claims = claimsSummary(verification);
  if (claims) parts.push(claims);
  if (record?.model) parts.push(modelSummary(record.model));
  if (record && record.timing.totalMs > 0) parts.push(formatDuration(record.timing.totalMs));
  return parts.length > 0 ? parts.join(' · ') : null;
}

function retrievalSummary(trace: RetrievalTrace | null): string | null {
  if (!trace) return null;
  if (
    trace.searchedDocuments === 0 &&
    trace.passages === 0 &&
    trace.files === 0 &&
    !trace.webPages
  ) {
    return null;
  }
  const parts: string[] = [];
  if (trace.searchedDocuments > 0) {
    parts.push(
      `Searched ${trace.searchedDocuments.toLocaleString()} ${
        trace.searchedDocuments === 1 ? 'document' : 'documents'
      }`
    );
  }
  if (trace.passages > 0) {
    parts.push(
      `${trace.passages} ${trace.passages === 1 ? 'passage' : 'passages'} from ${trace.files} ${
        trace.files === 1 ? 'file' : 'files'
      }`
    );
  } else if (trace.files > 0) {
    parts.push(`${trace.files} ${trace.files === 1 ? 'file' : 'files'}`);
  }
  if (trace.webPages) {
    parts.push(`${trace.webPages} ${trace.webPages === 1 ? 'web page' : 'web pages'}`);
  }
  const [first, ...rest] = parts;
  if (first === undefined) return null;
  const lead = trace.searchedDocuments > 0 ? first : `Retrieved ${first}`;
  if (trace.scope === 'linked') rest.push('this space');
  return [lead, ...rest].join(' · ');
}

function modelSummary(model: TurnModel): string {
  const origin = model.origin ? ORIGIN_LABELS[model.origin] : null;
  const degraded = model.degraded ? `ran on CPU: ${model.degraded}` : null;
  return [model.name, origin, degraded].filter(Boolean).join(' · ');
}

const ORIGIN_LABELS: Record<NonNullable<TurnModel['origin']>, string> = {
  default: 'app default',
  chat_override: 'chosen for this chat',
  request_override: 'one-off',
  learning_role: 'Learning model',
};

function claimsSummary(verification: MessageVerificationSummary | null): string | null {
  if (!verification?.enabled) return null;
  const evaluated = verification.claimsEvaluated ?? 0;
  if (evaluated === 0) return null;
  const supported = verification.supportedClaims ?? 0;
  return `${supported} of ${evaluated} ${evaluated === 1 ? 'claim' : 'claims'} backed`;
}

function routerActionLabel(action: string): string {
  switch (action) {
    case 'use_last_document':
      return 'a follow-up on the last document';
    case 'new_search':
      return 'a fresh search';
    case 'clarify':
      return 'a question back';
    default:
      return action;
  }
}

function readableReason(code: string): string {
  switch (code) {
    case 'no_results':
      return 'nothing came back';
    case 'low_top_score':
      return 'nothing scored well';
    case 'flat_rerank_spread':
      return 'no passage stood out from the rest';
    case 'low_term_coverage':
      return 'the passages missed most of your terms';
    case 'too_few_results':
      return 'too few passages came back';
    case 'no_reranker':
      return 'no reranker to tell the passages apart';
    case 'ordered_reading':
      return 'the question wants the document read in order';
    default:
      return code.replace(/_/g, ' ');
  }
}
