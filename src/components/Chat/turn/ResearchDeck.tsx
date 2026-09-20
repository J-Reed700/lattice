import { useMemo, useState, type CSSProperties } from 'react';

import type { TurnStep } from '@/types/conversation';
import { openExternalUrl } from '@/utils/openExternalUrl';

import {
  deckOf,
  hostHue,
  roundLine,
  type DeckPage,
  type DeckRound,
  type DeckThinking,
} from './research';
import { roundSummary } from './rounds';
import { StepRow, formatDuration, useElapsedSeconds } from './StepRow';

import './research-deck.css';

/**
 * The body of the turn record: every step the turn took, with each trip to the
 * web lifted out of the list into a card of its own.
 *
 * A card is a window onto one round — what was searched for, the pages that
 * came back, and each page's fate as it is read. It sits on a raised surface,
 * apart from the rows around it, because it is a different kind of thing: the
 * rows say what the model did, the card shows where it went. When the model
 * decides it has not seen enough, that is said in words and another card
 * arrives under it; the earlier one folds to its one line so the newest is
 * always the one in front.
 *
 * Nothing here is drawn for a turn that did not go to the web: those steps
 * render as the plain rows they always were.
 */
export function ResearchDeck({ steps, live }: { steps: readonly TurnStep[]; live: boolean }) {
  const deck = useMemo(() => deckOf(steps), [steps]);
  // Only what the reader chose. The default — newest card open — is derived,
  // so a card that arrives mid-turn takes the front without an effect.
  const [chosen, setChosen] = useState<ReadonlyMap<number, boolean>>(new Map());
  const newestCard = deck.items.reduce(
    (latest, item) => (item.type === 'round' && isCard(item) ? item.round : latest),
    0
  );
  const numbered = deck.rounds >= 2;

  return (
    <ol className="deck" data-live={live}>
      {deck.deepResearch && (
        // Said once, up top: it is why this turn is long, and a reader who
        // forgot flipping the switch should not have to wonder.
        <li className="deck-mode">
          <span className="deck-mode-tag">Deep research</span>
          {deck.modeNote && <span className="deck-mode-note">{deck.modeNote}</span>}
        </li>
      )}
      {deck.items.map(item => {
        if (item.type === 'step') return <StepRow key={item.step.id} step={item.step} />;
        if (item.type === 'thinking') {
          return <Thinking key={item.step.id} item={item} deepResearch={deck.deepResearch} />;
        }
        if (!isCard(item)) {
          return <PlainRound key={`round-${item.round}`} round={item} numbered={numbered} />;
        }
        const open = chosen.get(item.round) ?? item.round === newestCard;
        return (
          <RoundCard
            key={`round-${item.round}`}
            round={item}
            open={open}
            title={numbered ? `Round ${item.round}` : deck.deepResearch ? 'Deep research' : 'Web search'}
            onToggle={() => setChosen(previous => new Map(previous).set(item.round, !open))}
          />
        );
      })}
    </ol>
  );
}

/** A round earns a card by having gone somewhere a reader can follow. */
function isCard(round: DeckRound): boolean {
  return round.pages.length > 0 || round.queries.length > 0;
}

/** Documents and tools: the rule-down-the-side grouping the timeline always had. */
function PlainRound({ round, numbered }: { round: DeckRound; numbered: boolean }) {
  if (!numbered) {
    return (
      <>
        {round.otherSteps.map(step => (
          <StepRow key={step.id} step={step} />
        ))}
      </>
    );
  }
  const summary = roundSummary(round.otherSteps);
  return (
    <li className="turn-record-round">
      <p className="turn-record-round-heading">
        Round {round.round}
        {summary && <span className="turn-record-detail"> · {summary}</span>}
      </p>
      <ol className="turn-record-steps">
        {round.otherSteps.map(step => (
          <StepRow key={step.id} step={step} />
        ))}
      </ol>
    </li>
  );
}

function Thinking({ item, deepResearch }: { item: DeckThinking; deepResearch: boolean }) {
  const { step, wentBack } = item;
  const elapsed = useElapsedSeconds(step.state === 'running' ? step.id : null);
  return (
    <>
      {step.state === 'running' ? (
        <li className="deck-thinking" data-kind={step.kind} data-state="running">
          <span className="deck-orb" aria-hidden="true" />
          <span className="deck-shimmer">{step.label}…</span>
          <span className="turn-record-time">
            {elapsed >= 1 ? formatDuration(elapsed * 1000) : ''}
          </span>
        </li>
      ) : (
        <StepRow step={step} />
      )}
      {wentBack && (
        // The model's own decision, in words. Without it a second card reads as
        // the first search still going; with it, as a choice to dig further.
        <li className="deck-again">
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M13 8a5 5 0 1 1-1.6-3.7M13 2.5v2.4h-2.4" />
          </svg>
          {deepResearch
            ? 'Not enough yet — starting another round of deep research'
            : 'Not enough yet — going back for another look'}
        </li>
      )}
    </>
  );
}

function RoundCard({
  round,
  title,
  open,
  onToggle,
}: {
  round: DeckRound;
  title: string;
  open: boolean;
  onToggle: () => void;
}) {
  const line = roundLine(round);
  return (
    <li className="deck-round" data-active={round.active} data-open={open}>
      <section className="deck-card" aria-label={title}>
        <button type="button" className="deck-card-head" aria-expanded={open} onClick={onToggle}>
          <span className="deck-card-title">
            {round.active && <span className="deck-live" aria-hidden="true" />}
            {title}
          </span>
          {line && <span className="deck-card-line">{line}</span>}
          <svg className="deck-card-chevron" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 6.5 8 10.5 12 6.5" />
          </svg>
        </button>
        {open && (
          <div className="deck-card-body" data-other-first={round.otherStepsCameFirst}>
            {round.queries.length > 0 && (
              <ul className="deck-queries" aria-label="Searches">
                {round.queries.map(query => (
                  <li
                    key={query.id}
                    className="deck-query"
                    data-running={query.running}
                    data-failed={query.failure !== null}
                  >
                    <svg viewBox="0 0 16 16" aria-hidden="true">
                      <circle cx="7" cy="7" r="4.25" />
                      <path d="m10.25 10.25 3 3" />
                    </svg>
                    <span>{query.text}</span>
                    {query.failure && <span className="deck-query-failure">{query.failure}</span>}
                  </li>
                ))}
              </ul>
            )}
            {round.pages.length > 0 ? (
              <ul className="deck-pages" aria-label="Pages">
                {round.pages.map((page, index) => (
                  <PageRow key={page.url} page={page} index={index} />
                ))}
              </ul>
            ) : (
              round.active && <p className="deck-waiting">Waiting for results…</p>
            )}
            {round.otherSteps.length > 0 && (
              <ol className="turn-record-steps deck-card-steps">
                {round.otherSteps.map(step => (
                  <StepRow key={step.id} step={step} />
                ))}
              </ol>
            )}
          </div>
        )}
      </section>
    </li>
  );
}

const STATE_WORD: Record<DeckPage['state'], string> = {
  found: 'found',
  reading: 'reading',
  read: 'read',
  failed: 'could not be read',
};

function PageRow({ page, index }: { page: DeckPage; index: number }) {
  const style = { '--i': index, '--hue': hostHue(page.host) } as CSSProperties;
  return (
    <li className="deck-page" data-state={page.state} style={style}>
      <button
        type="button"
        className="deck-page-link"
        title={page.url}
        onClick={() => void openExternalUrl(page.url)}
      >
        <span className="deck-page-tile" aria-hidden="true">
          {page.host.charAt(0).toUpperCase()}
        </span>
        <span className="deck-page-text">
          <span className="deck-page-title">{page.title ?? page.host}</span>
          <span className="deck-page-host">{page.host}</span>
        </span>
        <span className="deck-page-state">
          {/* The word, not only the colour: a refused page has to be readable
              as refused by someone who cannot tell the dots apart. */}
          {page.state === 'read' || page.state === 'failed'
            ? (page.note ?? STATE_WORD[page.state])
            : STATE_WORD[page.state]}
        </span>
      </button>
    </li>
  );
}
