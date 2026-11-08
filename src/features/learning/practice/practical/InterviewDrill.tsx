import { useEffect, useState } from "react";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Layers, LoaderCircle, X } from "lucide-react";

import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { useApi } from "@/contexts/ApiContext";
import { FlashcardSession } from "@/features/learning/recall/flashcards/FlashcardSession";
import type {
  CreateInterviewStudyDeckRequestDto,
  LearningPracticalActivityDto,
  StudyDeckDto,
} from "@/lib/bindings";
import { practiceHomeApi } from "../home/practiceHomeApi";

/**
 * Turns an interview's questions and private answer guides into a review
 * deck. The mock interview is the realistic rehearsal; the review deck is where each
 * answer gets retrieved from memory again and again, spaced to fit the
 * interview date, with the guide shown only after each attempt.
 */
export function InterviewDrill(props: {
  programId: string;
  activity: LearningPracticalActivityDto;
}) {
  return <DrillForm {...props} />;
}

function DrillForm({
  programId,
  activity,
}: {
  programId: string;
  activity: LearningPracticalActivityDto;
}) {
  const api = useApi();
  const queryClient = useQueryClient();

  const profileQuery = useQuery({
    queryKey: ["learning-practice-profile", programId],
    queryFn: () => practiceHomeApi.getProfile(programId),
  });

  const [when, setWhen] = useState("");
  const [deck, setDeck] = useState<StudyDeckDto | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);

  useEffect(() => {
    if (profileQuery.data?.interviewAt && !when) {
      const dt = new Date(profileQuery.data.interviewAt);
      const iso = new Date(dt.getTime() - dt.getTimezoneOffset() * 60_000)
        .toISOString()
        .slice(0, 16);
      setWhen(iso);
    }
  }, [profileQuery.data?.interviewAt, when]);

  const create = useMutation({
    mutationFn: async (request: CreateInterviewStudyDeckRequestDto) => {
      const res = await api.createInterviewStudyDeck(request);
      if (!res.ok) throw new Error(res.error);
      return res.data;
    },
    onSuccess: (newDeck) => {
      setDeck(newDeck);
      setDrawerOpen(true);
      void queryClient.invalidateQueries({ queryKey: ["study"] });
    },
  });

  const questions = activity.interviewPlan?.questions?.length ?? 0;
  const targetAt = when ? new Date(when).getTime() : NaN;
  const validTarget = Number.isFinite(targetAt) && targetAt > Date.now();

  if (!questions) return null;

  return (
    <>
      <section
        aria-labelledby={`interview-drill-${activity.id}`}
        className="rounded-2xl border border-border-default bg-surface p-4 sm:p-5 shadow-xs"
      >
        <div className="flex items-start gap-3">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent-muted text-accent">
            <Layers size={18} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-xs font-semibold uppercase tracking-[.14em] text-accent">
              Spaced review
            </div>
            <h3
              id={`interview-drill-${activity.id}`}
              className="mt-1 font-serif text-xl text-text-primary"
            >
              Review these {questions} {questions === 1 ? "question" : "questions"} until they stick
            </h3>
            <p className="mt-1 text-xs leading-5 text-text-secondary">
              Each question becomes a card. You answer it aloud from memory, then
              tick off the points a strong answer covers. Misses come back in
              the same session, and reviews are spaced to land before your
              interview. Making it again opens the same review deck, reviews kept,
              with the updated date.
            </p>
          </div>
        </div>

        <form
          className="mt-4 flex flex-wrap items-end gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            if (create.isPending) return;
            create.mutate({
              programId,
              activityId: activity.id,
              targetAt: validTarget ? targetAt : null,
            });
          }}
        >
          <label className="text-xs font-medium text-text-secondary">
            Interview date and time
            <input
              type="datetime-local"
              value={when}
              onChange={(event) => setWhen(event.target.value)}
              placeholder="Set interview date"
              className="mt-1.5 block rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
            />
          </label>
          <button
            type="submit"
            disabled={create.isPending}
            className="inline-flex min-h-[44px] cursor-pointer items-center gap-2 rounded-full bg-accent px-5 py-2.5 text-xs font-semibold text-accent-fg transition-opacity hover:opacity-90 disabled:opacity-45 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          >
            {create.isPending ? (
              <LoaderCircle size={14} className="animate-spin" />
            ) : (
              <Layers size={14} />
            )}
            {create.isPending ? "Preparing review deck…" : "Make review deck"}
          </button>
          {!when && (
            <span className="text-xs text-text-muted">
              Choose your interview date to pace spaced reviews before it.
            </span>
          )}
          {when && !validTarget && (
            <span className="text-xs text-text-muted">
              That time has passed; the deck will use its standard schedule.
            </span>
          )}
        </form>

        {create.isError && (
          <p
            role="alert"
            className="mt-3 rounded-lg bg-danger-muted p-3 text-xs text-danger-fg"
          >
            {create.error.message}
          </p>
        )}
      </section>

      {/* In-course review deck drawer */}
      {drawerOpen && deck && (
        <Dialog open={drawerOpen} onOpenChange={setDrawerOpen}>
          <DialogContent className="max-h-[92dvh] max-w-3xl overflow-y-auto rounded-2xl border border-border-default bg-surface p-6 shadow-xl">
            <div className="flex items-center justify-between border-b border-border-default pb-3">
              <DialogTitle className="font-serif text-xl text-text-primary">
                {deck.title} · Review deck
              </DialogTitle>
              <button
                type="button"
                onClick={() => setDrawerOpen(false)}
                className="rounded-lg p-1.5 text-text-muted hover:bg-surface-raised hover:text-text-primary"
                aria-label="Close review deck drawer"
              >
                <X size={18} />
              </button>
            </div>
            <div className="mt-4">
              <FlashcardSession
                cards={deck.cards}
                mode="flashcard"
                title={deck.title}
                targetAt={validTarget ? targetAt : deck.targetAt}
                onClose={() => setDrawerOpen(false)}
              />
            </div>
          </DialogContent>
        </Dialog>
      )}
    </>
  );
}
