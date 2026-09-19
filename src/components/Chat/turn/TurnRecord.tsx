import { useEffect, useRef, useState } from 'react';

import type {
  MessageVerificationSummary,
  RetrievalTrace,
  TurnRecord as TurnRecordData,
  TurnStep,
} from '@/types/conversation';

import './turn-record.css';

/**
 * What a turn did, above the answer it produced.
 *
 * One line at rest — what was searched, how much of the answer is backed, which
 * model wrote it, how long it took — and a timeline when opened. While the turn
 * runs it is the same list, live: steps tick off as they finish and the running
 * one carries a clock, so a round that produces no text for minutes is visibly
 * working rather than indistinguishable from a hang.
 *
 * It replaces `RetrievalTrace` and `ActivityNote` and keeps their honesty
 * rules. Chiefly: a turn that did not search did not search *zero* documents,
 * so it says nothing rather than "Searched 0 documents"; and a plain sufficient
 * verdict is the expected case, so it draws nothing. A badge on every ordinary
 * answer is noise, not disclosure.
 */

interface TurnRecordProps {
  /** The retrieval trace, live or persisted. Unchanged source. */
  trace: RetrievalTrace | null;
  /** `null` on every turn persisted before the record existed. */
  record: TurnRecordData | null;
  /** Only while the turn is in flight. */
  liveSteps: TurnStep[] | null;
  isPending: boolean;
  /** For "4 of 5 claims backed". */
  verification: MessageVerificationSummary | null;
}

export function TurnRecord({
  trace,
  record,
  liveSteps,
  isPending,
  verification,
}: TurnRecordProps) {
  const steps = (isPending ? liveSteps : record?.steps) ?? record?.steps ?? [];
  const runningStep = steps.find(step => step.state === 'running') ?? null;

  // The turn opens itself and then gets out of the way: once the model starts
  // writing, the answer is the progress indicator and the timeline folds. A
  // reader who opened it by hand keeps it open.
  const [manuallyOpen, setManuallyOpen] = useState<boolean | null>(null);
  const hasStartedWriting = useRef(false);
  const wasPending = useRef(isPending);
  if (isPending && !wasPending.current) {
    wasPending.current = true;
    hasStartedWriting.current = false;
  }
  if (!isPending) wasPending.current = false;
  if (isPending && steps.some(step => step.kind === 'generate')) {
    hasStartedWriting.current = true;
  }
  const open = manuallyOpen ?? (isPending && !hasStartedWriting.current);

  const summary = summaryLine({ trace, record, verification });
  // Nothing to say and nothing happening: draw nothing at all.
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
        <span className="turn-record-line">
          {summary ?? (runningStep ? runningStep.label : 'Working')}
        </span>
      </button>
      {open && (
        <div className="turn-record-body">
          <TurnNote trace={trace} />
          <ol className="turn-record-steps">
            {steps.map(step => (
              <StepRow key={step.id} step={step} />
            ))}
          </ol>
          {record?.router?.rationale && (
            <p className="turn-record-rationale">
              Routed as {routerActionLabel(record.router.action)} —{' '}
              {record.router.rationale}
            </p>
          )}
        </div>
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

function StepRow({ step }: { step: TurnStep }) {
  const elapsed = useElapsedSeconds(step.state === 'running' ? step.id : null);
  return (
    <li className="turn-record-step" data-state={step.state} data-kind={step.kind}>
      <span className="turn-record-dot" aria-hidden="true" />
      <span className="turn-record-label">
        {step.label}
        {step.detail && <span className="turn-record-detail"> · {step.detail}</span>}
        {step.result && <span className="turn-record-result"> · {step.result}</span>}
      </span>
      <span className="turn-record-time">
        {step.state === 'running'
          ? // A clock, not a spinner: a label alone cannot tell "started a
            // second ago" from "stuck here for six minutes".
            elapsed >= 1
            ? formatDuration(elapsed * 1000)
            : ''
          : typeof step.durationMs === 'number'
            ? formatDuration(step.durationMs)
            : ''}
      </span>
    </li>
  );
}

/**
 * The second line, when retrieval did something worth saying out loud.
 *
 * Only three things qualify: a corrective pass, a skipped planner, and an
 * insufficient verdict with its reasons. A plain sufficient verdict is the
 * expected case and says nothing.
 */
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

/** Track seconds since a step started running, restarting on each new step. */
function useElapsedSeconds(runningStepId: string | null): number {
  const [seconds, setSeconds] = useState(0);
  useEffect(() => {
    if (!runningStepId) {
      setSeconds(0);
      return;
    }
    const startedAt = Date.now();
    setSeconds(0);
    const timer = setInterval(() => {
      setSeconds(Math.floor((Date.now() - startedAt) / 1000));
    }, 1000);
    return () => clearInterval(timer);
  }, [runningStepId]);
  return seconds;
}

/**
 * The one line at rest.
 *
 * Built out of facts the turn actually recorded, each dropped when it has none.
 * Returns null when there is nothing honest to say.
 */
function summaryLine({
  trace,
  record,
  verification,
}: {
  trace: RetrievalTrace | null;
  record: TurnRecordData | null;
  verification: MessageVerificationSummary | null;
}): string | null {
  const parts: string[] = [];
  const retrieval = retrievalSummary(trace);
  if (retrieval) parts.push(retrieval);
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
  if (record?.model) parts.push(record.model.name);
  if (record && record.timing.totalMs > 0) parts.push(formatDuration(record.timing.totalMs));
  return parts.length > 0 ? parts.join(' · ') : null;
}

/**
 * What the trace says, in the words `RetrievalTrace` used.
 *
 * The backend still sends a trace when it could not search at all — that is how
 * the composer learns *why* — but "Searched 0 documents" reads as a search that
 * came up empty, which is a different and untrue claim. So a trace of nothing
 * contributes nothing here.
 */
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
  // With no search to report, the line still needs its verb: "10 web pages"
  // alone does not say whether they were found, read or merely linked.
  const lead = trace.searchedDocuments > 0 ? first : `Retrieved ${first}`;
  if (trace.scope === 'linked') rest.push('this space');
  return [lead, ...rest].join(' · ');
}

/** "4 of 5 claims backed", or nothing when nothing was checked. */
function claimsSummary(verification: MessageVerificationSummary | null): string | null {
  if (!verification?.enabled) return null;
  const evaluated = verification.claimsEvaluated ?? 0;
  if (evaluated === 0) return null;
  const supported = verification.supportedClaims ?? 0;
  return `${supported} of ${evaluated} ${evaluated === 1 ? 'claim' : 'claims'} backed`;
}

/** The router's stable code as a sentence. */
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

/** Sufficiency reason codes as something a reader can act on. */
function readableReason(code: string): string {
  switch (code) {
    // The backend's codes (`retrieval/sufficiency.rs`), in its own words.
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

/**
 * Milliseconds as a duration a reader can compare at a glance.
 *
 * Exported because the same turn appears in more than one place and the two
 * must not disagree about how long it took.
 */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)}s`;
  const whole = Math.round(seconds);
  return `${Math.floor(whole / 60)}m ${String(whole % 60).padStart(2, '0')}s`;
}
