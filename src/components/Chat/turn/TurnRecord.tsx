import { useRef, useState } from 'react';

import type {
  MessageVerificationSummary,
  RetrievalTrace,
  TurnRecord as TurnRecordData,
  TurnStep,
} from '@/types/conversation';

import { hasWebResearch } from './research';
import { ResearchDeck } from './ResearchDeck';
import { roundCount, roundOf } from './rounds';
import { formatDuration } from './StepRow';

import './turn-record.css';

export { formatDuration };

/**
 * What a turn did, above the answer it produced.
 *
 * One line at rest — what was searched, how much of the answer is backed, which
 * model wrote it, how long it took — and a timeline when opened. While the turn
 * runs it is the same list, live: steps tick off as they finish and the running
 * one carries a clock, so a round that produces no text for minutes is visibly
 * working rather than indistinguishable from a hang.
 *
 * A model with tools can go back for more after reading what retrieval found.
 * Each trip is a named round, and a round that starts after the record has
 * folded still shows on the one line, so the second and third look are as
 * visible as the first. A trip to the web is more than a row: it opens as a
 * card naming what was searched and every page that came back (`ResearchDeck`).
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
  /**
   * The answer has text on screen. A generation step alone does not mean that:
   * a tool round generates for minutes and writes nothing.
   */
  isWriting: boolean;
  /** For "4 of 5 claims backed". */
  verification: MessageVerificationSummary | null;
}

export function TurnRecord({
  trace,
  record,
  liveSteps,
  isPending,
  isWriting,
  verification,
}: TurnRecordProps) {
  const steps = (isPending ? liveSteps : record?.steps) ?? record?.steps ?? [];
  // Pages are read side by side, so several steps run at once; the newest is
  // the one that says where the turn has got to.
  const runningStep = isPending
    ? ([...steps].reverse().find(step => step.state === 'running') ?? null)
    : null;

  // The turn opens itself and then gets out of the way: once the answer has
  // text, the answer is the progress indicator and the timeline folds. Until
  // then it stays open through every round — folding at the first generation
  // step hid exactly the rounds worth watching, since a tool round generates
  // without writing. A reader who opened it by hand keeps it open.
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

  const summary = summaryLine({ trace, record, verification, rounds: roundCount(steps) });
  const runningRound = runningStep ? roundOf(steps, runningStep.id) : null;
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
          {runningStep ? (
            // Folded or not, a running turn says what it is doing right now.
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
        // Cards float; a sunken box around them would pin them back down.
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
  rounds,
}: {
  trace: RetrievalTrace | null;
  record: TurnRecordData | null;
  verification: MessageVerificationSummary | null;
  rounds: number;
}): string | null {
  const parts: string[] = [];
  const retrieval = retrievalSummary(trace);
  if (retrieval) parts.push(retrieval);
  // Only worth a clause when the model went back: every turn that retrieves
  // has one round, and saying so would be noise.
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
