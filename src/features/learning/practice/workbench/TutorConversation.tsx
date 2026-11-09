import { Check, Lightbulb, MessageCircle } from 'lucide-react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { CitationCards } from '@/features/learning/practice/PracticeSourceRail';
import { PracticeThread, type ThreadMessage } from '@/features/learning/practice/shared/PracticeThread';
import type { PracticeBusyState } from '@/features/learning/practice/usePracticeSession';
import { HINTS, type HintRung } from '@/features/learning/practice/workbench/helpModes';
import type * as Wire from '@/lib/bindings';
import { useDraft } from '@/shared/drafts/drafts';

type TutorTurn = Wire.LearningPracticeSessionDto['tutorTurns'][number];

export interface TutorConversationProps {
  sessionId: string;
  turns: TutorTurn[];
  mode: Wire.LearningPracticeMode;
  active: boolean;
  canTutor: boolean;
  busyState: PracticeBusyState;
  busy: boolean;
  hintUsed: Wire.LearningPracticeHintLevel[];
  showCitations: boolean;
  sourceOpeningAllowed: boolean;
  onAsk: (kind: 'question' | 'critique', prompt: string) => Promise<boolean>;
  onHint: (hint: HintRung) => Promise<boolean>;
  onOpenCitation: (citation: TutorTurn['citations'][number]) => void;
  error?: { message: string; retry?: () => void } | null;
}

function turnTitle(turn: TutorTurn) {
  if (turn.hintLevel) return HINTS.find((hint) => hint.id === turn.hintLevel)?.title ?? 'Hint';
  return turn.requestKind === 'critique' ? 'Critique' : 'Tutor';
}

function closedReason(active: boolean, mode: Wire.LearningPracticeMode): string {
  if (!active) return 'This attempt is submitted. Start a revision to ask the tutor again.';
  return mode === 'demonstrate' ? 'The tutor is closed with No help.' : 'The tutor is closed.';
}

export function TutorConversation({
  sessionId,
  turns,
  mode,
  active,
  canTutor,
  busyState,
  busy,
  hintUsed,
  showCitations,
  sourceOpeningAllowed,
  onAsk,
  onHint,
  onOpenCitation,
  error = null,
}: TutorConversationProps) {
  const draftKey = `practice-tutor:${sessionId}`;
  const [composer, setComposer] = useDraft(draftKey);
  const turnById = new Map(turns.map((turn) => [turn.id, turn]));
  const messages: ThreadMessage[] = turns.flatMap((turn) => [
    ...(turn.requestKind === 'hint' ? [] : [{ id: `${turn.id}:prompt`, speaker: 'learner' as const, content: turn.prompt }]),
    { id: turn.id, speaker: 'coach' as const, content: turn.response },
  ]);
  const otherBusy = busy && busyState !== 'tutor';
  const critiqueReason = !composer.trim() ? 'Write your question first to request a critique.' : null;
  const disabled = !canTutor
    ? { reason: closedReason(active, mode) }
    : otherBusy
      ? { reason: 'Another action is in progress.' }
      : null;

  return (
    <section aria-labelledby="tutor-conversation-heading" className="rounded-2xl border border-border-default bg-surface p-5 sm:p-6">
      <div className="flex items-center gap-2 text-accent">
        <MessageCircle size={15} aria-hidden="true" />
        <h4 id="tutor-conversation-heading" className="font-serif text-xl text-text-primary">
          Tutor conversation
        </h4>
      </div>
      <div className="mt-4">
        <PracticeThread
          label="Tutor messages"
          messages={messages}
          draftKey={draftKey}
          placeholder={canTutor ? 'Ask about a step, assumption, or source…' : closedReason(active, mode)}
          busy={busyState === 'tutor' ? 'The tutor is responding…' : null}
          disabled={disabled}
          error={error}
          onSend={async (content) => {
            if (!(await onAsk('question', content))) throw new Error('The question was not sent.');
          }}
          renderMessage={(message) => {
            const turn = turnById.get(message.id);
            if (!turn) return <p className="whitespace-pre-wrap text-sm leading-6 text-text-primary">{message.content}</p>;
            return (
              <div className="text-sm leading-6 text-text-secondary">
                <p className="text-xs font-semibold text-accent">
                  {turnTitle(turn)}
                  {turn.modelName ? ` · ${turn.modelName}` : ''}
                </p>
                <TiptapViewer content={turn.response} />
                {showCitations && turn.citations.length > 0 && (
                  <CitationCards disabled={!sourceOpeningAllowed || busy} citations={turn.citations} onOpen={onOpenCitation} />
                )}
              </div>
            );
          }}
        />
      </div>
      {active && (
        <div className="mt-4 space-y-3">
          <div>
            <div className="flex flex-wrap justify-end gap-2">
              <button
                type="button"
                disabled={!canTutor || busy || !composer.trim()}
                onClick={() => {
                  void onAsk('critique', composer).then((ok) => {
                    if (ok) setComposer('');
                  });
                }}
                className="rounded-full border border-border-default px-3 py-1.5 text-xs font-semibold text-text-secondary hover:border-accent hover:text-accent disabled:opacity-40"
              >
                Request critique
              </button>
            </div>
            {canTutor && critiqueReason && <p className="mt-1 text-right text-xs text-text-muted">{critiqueReason}</p>}
          </div>
          <div className="rounded-xl border border-border-default bg-surface-sunken p-3">
            <div className="flex items-center gap-2">
              <Lightbulb size={14} className="text-accent" aria-hidden="true" />
              <span className="text-xs font-semibold text-text-primary">Hint ladder</span>
              <span className="text-xs text-text-muted">Each hint is recorded</span>
            </div>
            <div className="mt-2.5 grid gap-2 sm:grid-cols-2">
              {HINTS.map((hint, index) => {
                const used = hintUsed.includes(hint.id);
                const locked = mode === 'practice' && hint.id === 'worked_explanation';
                return (
                  <button
                    key={hint.id}
                    type="button"
                    disabled={!canTutor || busy || used || locked}
                    onClick={() => void onHint(hint)}
                    className="flex min-w-0 items-center gap-2 rounded-lg border border-border-default bg-surface px-3 py-2 text-left text-xs text-text-secondary hover:border-accent disabled:opacity-60"
                  >
                    <span className="grid h-5 w-5 shrink-0 place-items-center rounded-full bg-accent-muted text-xs font-semibold text-accent">
                      {index + 1}
                    </span>
                    <span className="min-w-0 flex-1">
                      {hint.title}
                      {locked && <span className="block text-xs text-text-muted">Available with Full help</span>}
                    </span>
                    {used && (
                      <span className="flex shrink-0 items-center gap-1 text-success-fg">
                        <Check size={13} aria-hidden="true" /> Used
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
            {!canTutor && <p className="mt-2 text-xs text-text-muted">Hints are closed with No help.</p>}
          </div>
        </div>
      )}
    </section>
  );
}
